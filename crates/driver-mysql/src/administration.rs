use super::*;
use crate::catalog::{columns, database, show_create};
impl MysqlSession {
    pub(super) async fn load_roles(&self) -> Result<RolesSnapshot, DbError> {
        let mut conn = self.fresh().await?;
        let account_query = if self.maria {
            "SELECT User,Host,(COALESCE(JSON_VALUE(Priv,'$.is_role'),'false') NOT IN ('1','true','TRUE') AND COALESCE(JSON_VALUE(Priv,'$.account_locked'),'false') NOT IN ('1','true','TRUE')),CONCAT(QUOTE(User),'@',QUOTE(Host)) FROM mysql.global_priv ORDER BY User,Host"
        } else {
            "SELECT User,Host,(account_locked='N'),CONCAT(QUOTE(User),'@',QUOTE(Host)) FROM mysql.user ORDER BY User,Host"
        };
        let accounts: Vec<(String, String, u8, String)> = match conn.query(account_query).await {
            Ok(r) => r,
            Err(e) => {
                let e = error(e);
                if matches!(e.kind, ErrorKind::Auth) {
                    return Ok(RolesSnapshot{supported:false,reason:Some("Listing users requires SELECT on mysql.user; credentials and authentication hashes are never selected or returned".into()),..Default::default()});
                }
                return Err(e);
            }
        };
        let mut roles = vec![];
        let mut grantee_names = std::collections::HashMap::new();
        for (user, host, login, grantee) in accounts {
            let name = format!("{user}@{host}");
            grantee_names.insert(grantee, name.clone());
            let grants: Vec<String> = conn
                .query(format!("SHOW GRANTS FOR {}@{}", quote(&user), quote(&host)))
                .await
                .map_err(error)?;
            // MariaDB may put an authentication clause in SHOW GRANTS; never return that clause.
            let summary = grants
                .into_iter()
                .map(|g| {
                    let upper = g.to_ascii_uppercase();
                    upper
                        .find(" IDENTIFIED ")
                        .map(|i| g[..i].to_owned())
                        .unwrap_or(g)
                })
                .collect::<Vec<_>>();
            roles.push(RoleInfo {
                name,
                login: login != 0,
                superuser: summary.iter().any(|s| {
                    s.strip_prefix("GRANT ")
                        .and_then(|s| s.split_once(" ON *.* TO "))
                        .is_some_and(|(rights, _)| {
                            rights == "ALL PRIVILEGES"
                                || rights.split(',').any(|p| p.trim() == "SUPER")
                        })
                }),
                member_of: vec![],
                summary,
            });
        }
        let grants:Vec<(String,String,String,String)>=conn.query("SELECT GRANTEE,TABLE_SCHEMA,TABLE_NAME,PRIVILEGE_TYPE FROM information_schema.TABLE_PRIVILEGES ORDER BY GRANTEE,TABLE_SCHEMA,TABLE_NAME,PRIVILEGE_TYPE").await.map_err(error)?;
        let matrix = grants
            .into_iter()
            .map(|(role, namespace, object, privilege)| ObjectPrivilege {
                role: grantee_names.get(&role).cloned().unwrap_or(role),
                namespace,
                object,
                privilege,
                allowed: true,
            })
            .collect();
        Ok(RolesSnapshot{supported:true,reason:Some("Table grants are explicit grants; role inheritance, global and schema grants are described in SHOW GRANTS, not expanded into effective access".into()),roles,matrix,..Default::default()})
    }
    pub(super) async fn grants(&self, changes: &[GrantChange]) -> Result<GrantPlan, DbError> {
        validate_grants(changes)?;
        let snapshot = self.load_roles().await?;
        if !snapshot.supported {
            return Err(DbError::unsupported(snapshot.reason.unwrap_or_default()));
        }
        let mut statements = vec![];
        for c in changes {
            if ![
                "SELECT",
                "INSERT",
                "UPDATE",
                "DELETE",
                "REFERENCES",
                "INDEX",
                "ALTER",
                "CREATE",
                "DROP",
            ]
            .contains(&c.privilege.to_ascii_uppercase().as_str())
                || c.namespace.is_empty()
                || c.object.is_empty()
            {
                return Err(DbError::query(
                    "Grant requires an allowlisted table privilege and database/table",
                ));
            }
            let account = snapshot
                .roles
                .iter()
                .find(|r| r.name == c.role)
                .ok_or_else(|| DbError::query("Choose an existing MySQL user@host account"))?;
            let (user, host) = account
                .name
                .rsplit_once('@')
                .ok_or_else(|| DbError::query("Invalid user@host"))?;
            statements.push(format!(
                "{} {} ON {} {} {}@{}",
                if c.revoke { "REVOKE" } else { "GRANT" },
                c.privilege.to_ascii_uppercase(),
                qualified(&c.namespace, &c.object),
                if c.revoke { "FROM" } else { "TO" },
                quote(user),
                quote(host)
            ));
        }
        Ok(GrantPlan{statements,warnings:vec!["MySQL GRANT/REVOKE implicitly commits and cannot be rollback-previewed; execute an explicitly approved plan manually.".into()],apply_supported:false})
    }
    pub(super) async fn dump(
        &self,
        cancel: CancelToken,
        sink: &dyn BackupSink,
    ) -> Result<BackupSummary, DbError> {
        let mut gate = tokio::select! {s=self.conn.lock()=>s,_=cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"Backup cancelled"))};
        let mut conn = self.operation_connection(&mut gate).await?;
        let db = database(&mut conn, None).await?;
        conn.query_drop("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .await
            .map_err(error)?;
        conn.query_drop("START TRANSACTION WITH CONSISTENT SNAPSHOT")
            .await
            .map_err(error)?;
        let id = conn.id();
        let work = async {
            management::visible_privilege(&mut conn, &db, None, "backup metadata").await?;
            let unsupported:Option<u64>=conn.exec_first("SELECT (SELECT COUNT(*) FROM information_schema.TABLES WHERE TABLE_SCHEMA=? AND TABLE_TYPE='BASE TABLE' AND (ENGINE IS NULL OR ENGINE<>'InnoDB'))+(SELECT COUNT(*) FROM information_schema.TRIGGERS WHERE TRIGGER_SCHEMA=?)+(SELECT COUNT(*) FROM information_schema.ROUTINES WHERE ROUTINE_SCHEMA=?)+(SELECT COUNT(*) FROM information_schema.EVENTS WHERE EVENT_SCHEMA=?)",(&db,&db,&db,&db)).await.map_err(error)?;
            if unsupported != Some(0) {
                return Err(DbError::unsupported("Logical backup refuses non-InnoDB tables, triggers, routines and events; use a complete server backup for this database"));
            }
            let tables:Vec<(String,String)>=conn.exec("SELECT TABLE_NAME,TABLE_TYPE FROM information_schema.TABLES WHERE TABLE_SCHEMA=? ORDER BY (TABLE_TYPE='VIEW'),TABLE_NAME",(&db,)).await.map_err(error)?;
            let mut summary=BackupSummary{format:"mysql-sql-v1".into(),counts:vec![],warnings:vec!["Consistent InnoDB data snapshot; concurrent DDL is not supported. Accounts, grants and server configuration are excluded; restore is not provided by this driver version.".into()]};
            record(
                sink,
                b'S',
                b"SET NAMES utf8mb4; SET time_zone='+00:00'; SET FOREIGN_KEY_CHECKS=0",
                &cancel,
            )
            .await?;
            let mut views = vec![];
            for (table, ty) in tables {
                let definition = show_create(&mut conn, &db, &table).await?;
                if ty == "VIEW" {
                    views.push(definition);
                    continue;
                }
                let cols = columns(&mut conn, &db, &table, self.maria).await?;
                // Acquiring a metadata lock before streaming prevents this table changing mid-dump.
                conn.query_drop(format!("SELECT 1 FROM {} LIMIT 0", qualified(&db, &table)))
                    .await
                    .map_err(error)?;
                record(sink, b'S', definition.as_bytes(), &cancel).await?;
                let names = cols
                    .iter()
                    .filter(|c| !c.generated)
                    .map(|c| quote(&c.name))
                    .collect::<Vec<_>>();
                let select = format!("SELECT {} FROM {}", names.join(","), qualified(&db, &table));
                let prefix = format!(
                    "INSERT INTO {} ({}) VALUES (",
                    quote(&table),
                    names.join(",")
                );
                let mut result = conn.query_iter(select).await.map_err(error)?;
                let metadata = result.columns_ref().to_vec();
                let mut count = 0;
                while let Some(row) = result.next().await.map_err(error)? {
                    let mut sql = prefix.clone();
                    for (i, value) in row.unwrap_raw().into_iter().enumerate() {
                        let v = value.ok_or_else(|| {
                            DbError::internal("MySQL backup row contains a missing value")
                        })?;
                        if i > 0 {
                            sql.push(',');
                        }
                        match v {
                            mysql_async::Value::NULL => sql.push_str("NULL"),
                            mysql_async::Value::Bytes(bytes) => {
                                let literal = format!("X'{}'", value::hex(&bytes));
                                if metadata[i].character_set() == 63
                                    && matches!(metadata[i].column_type(),mysql_async::consts::ColumnType::MYSQL_TYPE_BIT|mysql_async::consts::ColumnType::MYSQL_TYPE_GEOMETRY|mysql_async::consts::ColumnType::MYSQL_TYPE_TINY_BLOB|mysql_async::consts::ColumnType::MYSQL_TYPE_MEDIUM_BLOB|mysql_async::consts::ColumnType::MYSQL_TYPE_LONG_BLOB|mysql_async::consts::ColumnType::MYSQL_TYPE_BLOB|mysql_async::consts::ColumnType::MYSQL_TYPE_VAR_STRING|mysql_async::consts::ColumnType::MYSQL_TYPE_STRING|mysql_async::consts::ColumnType::MYSQL_TYPE_VARCHAR)
                                {
                                    sql.push_str(&literal);
                                } else {
                                    sql.push_str(&format!("CONVERT({literal} USING utf8mb4)"));
                                }
                            }
                            _ => return Err(DbError::query("Expected text-protocol backup value")),
                        }
                    }
                    sql.push(')');
                    record(sink, b'S', sql.as_bytes(), &cancel).await?;
                    count += 1;
                }
                result.drop_result().await.map_err(error)?;
                summary.counts.push((table, count));
            }
            for view in views {
                record(sink, b'S', view.as_bytes(), &cancel).await?;
            }
            record(sink, b'S', b"SET FOREIGN_KEY_CHECKS=1", &cancel).await?;
            record(sink, b'Z', &[], &cancel).await?;
            Ok(summary)
        };
        let r = self.guarded(id, cancel.clone(), 3600000, work).await;
        self.cleanup(&mut conn, "ROLLBACK").await?;
        r
    }
}

async fn record(
    sink: &dyn BackupSink,
    tag: u8,
    bytes: &[u8],
    cancel: &CancelToken,
) -> Result<(), DbError> {
    tokio::select! {biased;_ =cancel.cancelled()=>Err(DbError::new(ErrorKind::Cancelled,"Backup cancelled")),r=backup_record(sink,tag,bytes)=>r}
}
