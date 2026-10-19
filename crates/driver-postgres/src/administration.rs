use super::*;
use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
struct Dump {
    before: Vec<String>,
    tables: Vec<(String, Vec<String>, String)>,
    after: Vec<String>,
}
impl PgSession {
    pub(super) async fn native_backup(&self, cancel: CancelToken) -> Result<BackupData, DbError> {
        let state = self.state.lock().await;
        if state.explicit {
            return Err(DbError::unsupported(
                "Finish the existing transaction before backup",
            ));
        }
        self.client
            .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .await
            .map_err(|e| self.error(e))?;
        let result = async {
            let unsupported: bool = self.client.query_one("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname NOT LIKE 'pg_%' AND n.nspname<>'information_schema' AND (c.relkind IN ('p','f') OR c.relrowsecurity OR c.reloptions IS NOT NULL OR c.relpersistence<>'p')) OR EXISTS(SELECT 1 FROM pg_inherits) OR EXISTS(SELECT 1 FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE NOT t.tgisinternal AND n.nspname NOT LIKE 'pg_%') OR EXISTS(SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname NOT LIKE 'pg_%' AND n.nspname<>'information_schema') OR EXISTS(SELECT 1 FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname NOT LIKE 'pg_%' AND n.nspname<>'information_schema' AND t.typtype IN ('e','d'))",&[]).await.map_err(|e|self.error(e))?.get(0);
            if unsupported { return Err(DbError::unsupported("Native backup currently supports ordinary tables, sequences, indexes, constraints and views; custom routines/types/triggers/partitioned/foreign tables require another backup strategy")); }
            let snapshot = self.snapshot_unlocked(None).await?;
            let mut dump=Dump{before:vec![],tables:vec![],after:vec![]};
            let schemas=self.client.query("SELECT nspname FROM pg_namespace WHERE nspname NOT LIKE 'pg_%' AND nspname<>'information_schema' ORDER BY nspname",&[]).await.map_err(|e|self.error(e))?;
            for row in schemas { dump.before.push(format!("CREATE SCHEMA IF NOT EXISTS {}",quote_ident(row.get(0)))); }
            let sequences=self.client.query("SELECT schemaname,sequencename,start_value,min_value,max_value,increment_by,cycle,cache_size,data_type::text FROM pg_sequences WHERE schemaname NOT LIKE 'pg_%' ORDER BY schemaname,sequencename",&[]).await.map_err(|e|self.error(e))?;
            for row in sequences {
                let target=format!("{}.{}",quote_ident(row.get(0)),quote_ident(row.get(1)));
                dump.before.push(format!("CREATE SEQUENCE {target} AS {} START {} MINVALUE {} MAXVALUE {} INCREMENT {} {} CACHE {}",row.get::<_,String>(8),row.get::<_,i64>(2),row.get::<_,i64>(3),row.get::<_,i64>(4),row.get::<_,i64>(5),if row.get::<_,bool>(6){"CYCLE"}else{"NO CYCLE"},row.get::<_,i64>(7)));
                let value=self.client.query_one(&format!("SELECT last_value,is_called FROM {target}"),&[]).await.map_err(|e|self.error(e))?;
                dump.after.push(format!("SELECT setval('{}',{}, {})",target.replace('\'',"''"),value.get::<_,i64>(0),value.get::<_,bool>(1)));
            }
            let ownership=self.client.query("SELECT ns.nspname,s.relname,nt.nspname,t.relname,a.attname FROM pg_depend d JOIN pg_class s ON s.oid=d.objid JOIN pg_namespace ns ON ns.oid=s.relnamespace JOIN pg_class t ON t.oid=d.refobjid JOIN pg_namespace nt ON nt.oid=t.relnamespace JOIN pg_attribute a ON a.attrelid=t.oid AND a.attnum=d.refobjsubid WHERE s.relkind='S' AND d.deptype='a' AND ns.nspname NOT LIKE 'pg_%' ORDER BY ns.nspname,s.relname",&[]).await.map_err(|e|self.error(e))?;
            for r in ownership {dump.after.push(format!("ALTER SEQUENCE {}.{} OWNED BY {}.{}.{}",quote_ident(r.get(0)),quote_ident(r.get(1)),quote_ident(r.get(2)),quote_ident(r.get(3)),quote_ident(r.get(4))));}
            let mut counts=vec![];
            for object in &snapshot.objects {
                if cancel.is_cancelled(){return Err(DbError::new(ErrorKind::Cancelled,"Backup cancelled"));}
                if object.kind!="table" {continue;}
                let target=format!("{}.{}",quote_ident(&object.namespace),quote_ident(&object.name));
                dump.before.push(object.definition.clone());
                let generated: bool=self.client.query_one("SELECT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=$1::text::regclass AND attnum>0 AND (attgenerated<>'' OR attidentity<>''))",&[&target]).await.map_err(|e|self.error(e))?.get(0);
                if generated {return Err(DbError::unsupported("Generated and identity columns are not yet supported by native backup"));}
                let stream=self.client.copy_out(&format!("COPY {target} TO STDOUT")).await.map_err(|e|self.error(e))?;
                pin_mut!(stream);let mut data=Vec::new();
                while let Some(chunk)=tokio::select!{_ = cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"Backup cancelled")),chunk=stream.next()=>chunk}{let chunk=chunk.map_err(|e|self.error(e))?;if data.len()+chunk.len()>BACKUP_MAX_BYTES{return Err(DbError::query("Backup exceeds 8 MiB payload limit"));}data.extend_from_slice(&chunk);}
                let count=self.client.query_one(&format!("SELECT count(*) FROM {target}"),&[]).await.map_err(|e|self.error(e))?.get::<_,i64>(0) as u64;
                counts.push((target.clone(),count));
                dump.tables.push((target,object.columns.iter().map(|c|c.name.clone()).collect(),String::from_utf8(data).map_err(|_|DbError::unsupported("UTF-8 backup required"))?));
            }
            for kind in ["constraint","index","view","materialized view"] {for object in &snapshot.objects {if object.kind==kind {dump.after.push(object.definition.clone());}}}
            let data=serde_json::to_vec(&dump).map_err(|_|DbError::query("Cannot encode backup"))?;
            let backup=BackupData{format:"postgres-copy-v1".into(),data,counts};backup.validate("postgres-copy-v1")?;Ok(backup)
        }.await;
        if self.client.batch_execute("ROLLBACK").await.is_err() {
            self.connection.abort();
            self.closed.store(true, Ordering::Release);
            return Err(DbError::new(
                ErrorKind::Connection,
                "Backup cleanup failed; session closed",
            ));
        }
        result
    }
    pub(super) async fn native_restore(
        &self,
        backup: &BackupData,
        overwrite: bool,
        cancel: CancelToken,
    ) -> Result<(), DbError> {
        backup.validate("postgres-copy-v1")?;
        let dump: Dump = serde_json::from_slice(&backup.data)
            .map_err(|_| DbError::query("Invalid PostgreSQL backup"))?;
        let state = self.state.lock().await;
        if state.explicit {
            return Err(DbError::unsupported(
                "Finish the existing transaction before restore",
            ));
        }
        self.client
            .batch_execute("BEGIN")
            .await
            .map_err(|e| self.error(e))?;
        let work=async {
            let schemas=self.client.query("SELECT nspname FROM pg_namespace WHERE nspname NOT LIKE 'pg_%' AND nspname<>'information_schema' ORDER BY nspname",&[]).await.map_err(|e|self.error(e))?;
            let nonempty:bool=self.client.query_one("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname NOT LIKE 'pg_%' AND n.nspname<>'information_schema') OR EXISTS(SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname NOT LIKE 'pg_%' AND n.nspname<>'information_schema') OR EXISTS(SELECT 1 FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname NOT LIKE 'pg_%' AND n.nspname<>'information_schema' AND t.typtype IN ('e','d'))",&[]).await.map_err(|e|self.error(e))?.get(0);
            if nonempty&&!overwrite{return Err(DbError::new(ErrorKind::Conflict,"Restore requires an empty database"));}
            if overwrite {for row in schemas {self.client.batch_execute(&format!("DROP SCHEMA {} CASCADE",quote_ident(row.get(0)))).await.map_err(|e|self.error(e))?;}}
            for sql in &dump.before {check_cancel(&cancel)?;self.client.batch_execute(sql).await.map_err(|e|self.error(e))?;}
            for (target,columns,data) in &dump.tables {
                check_cancel(&cancel)?;
                let sink=self.client.copy_in(&format!("COPY {target} ({}) FROM STDIN",columns.iter().map(|c|quote_ident(c)).collect::<Vec<_>>().join(","))).await.map_err(|e|self.error(e))?;
                pin_mut!(sink);sink.as_mut().send(bytes::Bytes::copy_from_slice(data.as_bytes())).await.map_err(|e|self.error(e))?;sink.as_mut().finish().await.map_err(|e|self.error(e))?;
            }
            for sql in &dump.after {check_cancel(&cancel)?;self.client.batch_execute(sql).await.map_err(|e|self.error(e))?;}
            check_cancel(&cancel)
        };
        let result=tokio::select!{r=work=>r,_=cancel.cancelled()=>{let _=self.client.cancel_token().cancel_query(tokio_postgres::NoTls).await;Err(DbError::new(ErrorKind::Cancelled,"Restore cancelled"))}};
        let cleanup = tokio::time::timeout(Duration::from_secs(3),self.client.batch_execute(if result.is_ok() { "COMMIT" } else { "ROLLBACK" })).await;
        if !matches!(cleanup,Ok(Ok(()))) {
            self.connection.abort();
            self.closed.store(true, Ordering::Release);
            return Err(DbError::new(
                ErrorKind::Connection,
                "Restore outcome unknown; session closed",
            ));
        }
        result
    }
    pub(super) async fn native_roles(&self) -> Result<RolesSnapshot, DbError> {
        let _state = self.state.lock().await;
        let rows=self.client.query("SELECT rolname,rolcanlogin,rolsuper,rolcreatedb,rolcreaterole,rolbypassrls FROM pg_roles ORDER BY rolname LIMIT 1001",&[]).await.map_err(|e|self.error(e))?;
        if rows.len() > 1000 {
            return Err(DbError::query("Role list exceeds 1000 roles"));
        }
        let mut result = RolesSnapshot {
            supported: true,
            ..Default::default()
        };
        for row in rows {
            let name: String = row.get(0);
            let memberships=self.client.query("SELECT r.rolname FROM pg_auth_members m JOIN pg_roles r ON r.oid=m.roleid JOIN pg_roles u ON u.oid=m.member WHERE u.rolname=$1 ORDER BY r.rolname",&[&name]).await.map_err(|e|self.error(e))?.iter().map(|r|r.get(0)).collect();
            let mut summary = vec![];
            for (i, label) in [
                (2, "Superuser"),
                (3, "Create databases"),
                (4, "Manage roles"),
                (5, "Bypass row-level security"),
            ] {
                if row.get::<_, bool>(i) {
                    summary.push(label.into());
                }
            }
            result.roles.push(RoleInfo {
                name,
                login: row.get(1),
                superuser: row.get(2),
                member_of: memberships,
                summary,
            });
        }
        let rows=self.client.query("SELECT r.rolname,n.nspname,c.relname,p.privilege,has_schema_privilege(r.oid,n.oid,'USAGE') AND has_table_privilege(r.oid,c.oid,p.privilege) FROM pg_roles r CROSS JOIN pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace CROSS JOIN (VALUES ('SELECT'),('INSERT'),('UPDATE'),('DELETE'),('TRUNCATE'),('REFERENCES'),('TRIGGER')) p(privilege) WHERE c.relkind IN ('r','p','v','m') AND n.nspname NOT LIKE 'pg_%' AND n.nspname<>'information_schema' LIMIT 50001",&[]).await.map_err(|e|self.error(e))?;
        if rows.len() > 50000 {
            return Err(DbError::query("Privilege matrix exceeds 50000 cells"));
        }
        for r in rows {
            result.matrix.push(ObjectPrivilege {
                role: r.get(0),
                namespace: r.get(1),
                object: r.get(2),
                privilege: r.get(3),
                allowed: r.get(4),
            });
        }
        for role in &mut result.roles {
            let allowed = result
                .matrix
                .iter()
                .filter(|p| p.role == role.name && p.allowed)
                .count();
            role.summary.push(format!("{allowed} effective table privileges, including memberships and PUBLIC; row policies and column privileges can further affect access"));
        }
        Ok(result)
    }
    pub(super) async fn native_grants(
        &self,
        changes: &[GrantChange],
    ) -> Result<GrantPlan, DbError> {
        validate_grants(changes)?;
        let _state = self.state.lock().await;
        let mut statements = vec![];
        for c in changes {
            if ![
                "SELECT",
                "INSERT",
                "UPDATE",
                "DELETE",
                "TRUNCATE",
                "REFERENCES",
                "TRIGGER",
            ]
            .contains(&c.privilege.as_str())
            {
                return Err(DbError::query("Unsupported table privilege"));
            }
            let exists:bool=self.client.query_one("SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=$1) AND EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$2 AND c.relname=$3 AND c.relkind IN ('r','p','v','m'))",&[&c.role,&c.namespace,&c.object]).await.map_err(|e|self.error(e))?.get(0);
            if !exists {
                return Err(DbError::query("Unknown role or table"));
            }
            statements.push(format!(
                "{} {} ON TABLE {}.{} {} {}",
                if c.revoke { "REVOKE" } else { "GRANT" },
                c.privilege,
                quote_ident(&c.namespace),
                quote_ident(&c.object),
                if c.revoke { "FROM" } else { "TO" },
                quote_ident(&c.role)
            ));
        }
        Ok(GrantPlan{statements,warnings:vec!["REVOKE does not remove privileges inherited through membership, PUBLIC, or ownership. Schema USAGE and row policies still apply.".into()],apply_supported:true})
    }
}
fn check_cancel(cancel: &CancelToken) -> Result<(), DbError> {
    if cancel.is_cancelled() {
        Err(DbError::new(ErrorKind::Cancelled, "Restore cancelled"))
    } else {
        Ok(())
    }
}
impl PgSession {
    pub(super) async fn copy_import(
        &self,
        req: &EditRequest,
        cancel: CancelToken,
        sink: &dyn ProgressSink,
    ) -> Result<ImportOutput, DbError> {
        if req.edits.is_empty() || req.edits.len() > IMPORT_MAX_ROWS {
            return Err(DbError::query("Import requires 1..10000 inserts"));
        }
        let state = self.state.lock().await;
        if state.explicit {
            return Err(DbError::unsupported(
                "Finish the existing transaction before importing",
            ));
        }
        let path = parse_path(&req.node_id)?;
        if path.len() != 4 || path[0] != "table" || path[2] != "tables" {
            return Err(DbError::query("Invalid import table"));
        }
        let table = format!("{}.{}", quote_ident(&path[1]), quote_ident(&path[3]));
        self.client
            .batch_execute("BEGIN")
            .await
            .map_err(|e| self.error(e))?;
        let work = async {
            if let Some(sql) = dbcore::import_create_sql(req, &table, "postgres")? {
                self.client
                    .batch_execute(&sql)
                    .await
                    .map_err(|e| self.error(e))?;
            }
            let columns=self.client.query("SELECT a.attname FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname=$2 AND c.relkind IN ('r','p') AND a.attnum>0 AND NOT a.attisdropped",&[&path[1],&path[3]]).await.map_err(|e|self.error(e))?.into_iter().map(|r|r.get::<_,String>(0)).collect::<Vec<_>>();
            dbcore::edits::plan(req, &table, &columns, &[], quote_ident, |i, _, _| {
                format!("${i}")
            })?;
            let RowEdit::Insert { values: first } = &req.edits[0] else {
                return Err(DbError::query("Import supports inserts only"));
            };
            if first.is_empty() {
                return Err(DbError::query("COPY requires at least one mapped column"));
            }
            let fields = first.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
            let sql = format!(
                "COPY {table} ({}) FROM STDIN",
                fields
                    .iter()
                    .map(|n| quote_ident(n))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let copy = self.client.copy_in(&sql).await.map_err(|e| self.error(e))?;
            pin_mut!(copy);
            let total = req.edits.len() as u64;
            sink.progress(0, total).await?;
            for (batch_index, batch) in req.edits.chunks(100).enumerate() {
                check_cancel(&cancel)?;
                let mut data = String::new();
                for edit in batch {
                    let RowEdit::Insert { values } = edit else {
                        return Err(DbError::query("Import supports inserts only"));
                    };
                    if values.len() != fields.len()
                        || values
                            .iter()
                            .zip(&fields)
                            .any(|((name, _), expected)| name != expected)
                    {
                        return Err(DbError::query(
                            "COPY import requires identical column mapping for every row",
                        ));
                    }
                    for (i, (_, value)) in values.iter().enumerate() {
                        if i > 0 {
                            data.push('\t');
                        }
                        match dbcore::edits::text(value)? {
                            None => data.push_str("\\N"),
                            Some(text) => {
                                if text.contains('\0') {
                                    return Err(DbError::query(
                                        "PostgreSQL text must not contain NUL",
                                    ));
                                }
                                data.push_str(
                                    &text
                                        .replace('\\', "\\\\")
                                        .replace('\t', "\\t")
                                        .replace('\n', "\\n")
                                        .replace('\r', "\\r"),
                                );
                            }
                        }
                    }
                    data.push('\n');
                }
                copy.as_mut()
                    .send(bytes::Bytes::from(data))
                    .await
                    .map_err(|e| self.error(e))?;
                sink.progress(((batch_index + 1) * 100).min(req.edits.len()) as u64, total)
                    .await?;
            }
            let affected = copy.as_mut().finish().await.map_err(|e| self.error(e))?;
            check_cancel(&cancel)?;
            Ok(ImportOutput::Applied(EditResult {
                affected: vec![affected],
                statements: vec![sql],
            }))
        };
        let result = tokio::select! {r=work=>r,_=cancel.cancelled()=>Err(DbError::new(ErrorKind::Cancelled,"COPY import cancelled")),_=tokio::time::sleep(Duration::from_millis(req.timeout_ms.max(1)))=>Err(DbError::new(ErrorKind::Timeout,"COPY import timed out"))};
        let cleanup = tokio::time::timeout(
            Duration::from_secs(3),
            self.client
                .batch_execute(if result.is_ok() { "COMMIT" } else { "ROLLBACK" }),
        )
        .await;
        if !matches!(cleanup, Ok(Ok(()))) {
            self.connection.abort();
            self.closed.store(true, Ordering::Release);
            return Err(DbError::new(
                ErrorKind::Connection,
                "Import outcome unknown; session closed",
            ));
        }
        result
    }
}
