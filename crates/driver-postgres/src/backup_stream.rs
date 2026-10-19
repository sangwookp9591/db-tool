use super::*;
use futures_util::SinkExt;
use std::collections::{BTreeMap, BTreeSet};
const BACKUP_SETTINGS:&str="SET LOCAL search_path=pg_catalog; SET LOCAL check_function_bodies=false; SET LOCAL standard_conforming_strings=on; SET LOCAL DateStyle='ISO,YMD'; SET LOCAL IntervalStyle=postgres; SET LOCAL extra_float_digits=3; SET LOCAL bytea_output=hex; SET LOCAL TimeZone='UTC'";

struct Definition {
    class: String,
    oid: String,
    sql: String,
    deps: BTreeSet<(String, String)>,
}
fn sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn qname(schema: &str, name: &str) -> String {
    format!("{}.{}", quote_ident(schema), quote_ident(name))
}
async fn sql_record(sink: &dyn BackupSink, sql: &str) -> Result<(), DbError> {
    backup_record(sink, b'S', sql.as_bytes()).await
}
impl PgSession {
    pub(super) async fn stream_backup(
        &self,
        cancel: CancelToken,
        sink: &dyn BackupSink,
    ) -> Result<BackupSummary, DbError> {
        let state = self.state.lock().await;
        if state.explicit {
            return Err(DbError::unsupported(
                "Finish the existing transaction before backup",
            ));
        }
        self.client
            .batch_execute(&format!(
                "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY; {BACKUP_SETTINGS}"
            ))
            .await
            .map_err(|e| self.error(e))?;
        let work = async {
            let unsupported:bool=self.client.query_one("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname !~ '^pg_' AND n.nspname<>'information_schema' AND (c.relrowsecurity OR c.relpersistence='t'))",&[]).await.map_err(|e|self.error(e))?.get(0);
            if unsupported {
                return Err(DbError::unsupported(
                    "Row policies and temporary tables require a separate backup strategy",
                ));
            }
            let mut summary = BackupSummary {
                format: "postgres-copy-v2".into(),
                ..Default::default()
            };
            for r in self.client.query("SELECT 'Extension requires recreation: '||extname FROM pg_extension WHERE extname<>'plpgsql' UNION ALL SELECT 'Foreign table requires recreation: '||quote_ident(n.nspname)||'.'||quote_ident(c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE c.relkind='f' AND n.nspname !~ '^pg_' ORDER BY 1",&[]).await.map_err(|e|self.error(e))? {summary.warnings.push(r.get(0));}
            for row in self.client.query("SELECT nspname FROM pg_namespace WHERE nspname !~ '^pg_' AND nspname<>'information_schema' ORDER BY nspname",&[]).await.map_err(|e|self.error(e))? {sql_record(sink,&format!("CREATE SCHEMA IF NOT EXISTS {}",quote_ident(row.get(0)))).await?;}
            sql_record(sink, BACKUP_SETTINGS).await?;
            let mut definitions = vec![];
            let mut refreshes = BTreeMap::new();
            let mut refresh_order = vec![];
            let mut aliases = BTreeMap::new();
            let mut after = vec![];
            // Identity sequences are created by their columns; all sequence states are restored after COPY.
            for r in self.client.query("SELECT s.schemaname,s.sequencename,s.start_value,s.min_value,s.max_value,s.increment_by,s.cycle,s.cache_size,s.data_type::text,c.oid::text,EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_class'::regclass AND d.objid=c.oid AND d.deptype='i') FROM pg_sequences s JOIN pg_namespace n ON n.nspname=s.schemaname JOIN pg_class c ON c.relnamespace=n.oid AND c.relname=s.sequencename WHERE n.nspname !~ '^pg_' AND n.nspname<>'information_schema' AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_class'::regclass AND d.objid=c.oid AND d.deptype='e') ORDER BY 1,2",&[]).await.map_err(|e|self.error(e))? {
                let target=qname(r.get(0),r.get(1));
                if !r.get::<_,bool>(10) {definitions.push(Definition{class:"pg_class".into(),oid:r.get(9),sql:format!("CREATE SEQUENCE {target} AS {} START {} MINVALUE {} MAXVALUE {} INCREMENT {} {} CACHE {}",r.get::<_,String>(8),r.get::<_,i64>(2),r.get::<_,i64>(3),r.get::<_,i64>(4),r.get::<_,i64>(5),if r.get::<_,bool>(6){"CYCLE"}else{"NO CYCLE"},r.get::<_,i64>(7)),deps:Default::default()});}
                let value=self.client.query_one(&format!("SELECT last_value,is_called FROM {target}"),&[]).await.map_err(|e|self.error(e))?;
                after.push(format!("SELECT pg_catalog.setval({}::regclass,{}, {})",sql_literal(&target),value.get::<_,i64>(0),value.get::<_,bool>(1)));
            }
            for r in self.client.query("SELECT t.oid::text,n.nspname,t.typname,t.typtype::text,t.typrelid::text,format_type(t.typbasetype,t.typtypmod),t.typnotnull,t.typdefault FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace LEFT JOIN pg_class c ON c.oid=t.typrelid WHERE n.nspname !~ '^pg_' AND n.nspname<>'information_schema' AND (t.typtype IN ('e','d') OR t.typtype='c' AND c.relkind='c') AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_type'::regclass AND d.objid=t.oid AND d.deptype='e') ORDER BY 2,3",&[]).await.map_err(|e|self.error(e))? {
                let oid:String=r.get(0);let target=qname(r.get(1),r.get(2));
                let sql=match r.get::<_,String>(3).as_str(){
                    "e"=>{let values=self.client.query("SELECT enumlabel FROM pg_enum WHERE enumtypid=$1::text::oid ORDER BY enumsortorder",&[&oid]).await.map_err(|e|self.error(e))?.into_iter().map(|r|sql_literal(r.get(0))).collect::<Vec<_>>();format!("CREATE TYPE {target} AS ENUM ({})",values.join(","))},
                    "c"=>{let rel:String=r.get(4);let columns=self.columns(&rel,false).await?;aliases.insert(("pg_class".into(),rel),("pg_type".into(),oid.clone()));format!("CREATE TYPE {target} AS ({})",columns.0.join(","))},
                    "d"=>{let constraints=self.client.query("SELECT conname,pg_get_constraintdef(oid,false) FROM pg_constraint WHERE contypid=$1::text::oid ORDER BY conname",&[&oid]).await.map_err(|e|self.error(e))?.into_iter().map(|r|format!(" CONSTRAINT {} {}",quote_ident(r.get(0)),r.get::<_,String>(1))).collect::<String>();format!("CREATE DOMAIN {target} AS {}{}{}{}",r.get::<_,String>(5),r.get::<_,Option<String>>(7).map(|v|format!(" DEFAULT {v}")).unwrap_or_default(),if r.get::<_,bool>(6){" NOT NULL"}else{""},constraints)},
                    _=>unreachable!()
                };
                definitions.push(Definition{class:"pg_type".into(),oid,sql,deps:Default::default()});
            }
            for r in self.client.query("SELECT p.oid::text,pg_get_functiondef(p.oid),p.prosecdef,pg_get_userbyid(p.proowner),n.nspname,p.proname,pg_get_function_identity_arguments(p.oid),p.prokind::text FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname !~ '^pg_' AND n.nspname<>'information_schema' AND p.prokind IN ('f','p') AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_proc'::regclass AND d.objid=p.oid AND d.deptype='e') ORDER BY n.nspname,p.proname,p.oid",&[]).await.map_err(|e|self.error(e))? {
                let mut sql:String=r.get(1);
                if r.get::<_,bool>(2){
                    let owner:String=r.get(3);
                    sql+=&format!("; ALTER {} {}({}) OWNER TO {}",if r.get::<_,String>(7)=="p"{"PROCEDURE"}else{"FUNCTION"},qname(r.get(4),r.get(5)),r.get::<_,String>(6),quote_ident(&owner));
                    summary.warnings.push(format!("Security-definer routine requires its original owner role: {owner}"));
                }
                definitions.push(Definition{class:"pg_proc".into(),oid:r.get(0),sql,deps:Default::default()});
            }
            let tables=self.client.query("WITH RECURSIVE tree AS (SELECT c.oid,0 AS depth FROM pg_class c WHERE NOT EXISTS(SELECT 1 FROM pg_inherits i WHERE i.inhrelid=c.oid) UNION ALL SELECT i.inhrelid,t.depth+1 FROM tree t JOIN pg_inherits i ON i.inhparent=t.oid) SELECT c.oid::text,n.nspname,c.relname,c.relkind::text,c.relispartition,pg_get_expr(c.relpartbound,c.oid),pg_get_partkeydef(c.oid),c.reltype::text,c.relpersistence::text,c.reloptions FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace JOIN tree t ON t.oid=c.oid WHERE n.nspname !~ '^pg_' AND n.nspname<>'information_schema' AND c.relkind IN ('r','p','v','m') AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_class'::regclass AND d.objid=c.oid AND d.deptype='e') ORDER BY t.depth,n.nspname,c.relname",&[]).await.map_err(|e|self.error(e))?;
            let mut copies = vec![];
            for r in &tables {
                let oid: String = r.get(0);
                let target = qname(r.get(1), r.get(2));
                let kind: String = r.get(3);
                aliases.insert(
                    ("pg_type".into(), r.get::<_, String>(7)),
                    ("pg_class".into(), oid.clone()),
                );
                let mut deps = BTreeSet::new();
                let sql = if kind == "r" || kind == "p" {
                    let (columns, copy_columns) = self.columns(&oid, true).await?;
                    if kind == "r" {
                        copies.push((target.clone(), copy_columns));
                    }
                    let parent=self.client.query_opt("SELECT i.inhparent::text,n.nspname,c.relname FROM pg_inherits i JOIN pg_class c ON c.oid=i.inhparent JOIN pg_namespace n ON n.oid=c.relnamespace WHERE i.inhrelid=$1::text::oid",&[&oid]).await.map_err(|e|self.error(e))?;
                    let partition = r
                        .get::<_, Option<String>>(6)
                        .map(|v| format!(" PARTITION BY {v}"))
                        .unwrap_or_default();
                    let options = r
                        .get::<_, Option<Vec<String>>>(9)
                        .map(|v| format!(" WITH ({})", v.join(",")))
                        .unwrap_or_default();
                    if r.get::<_, bool>(4) {
                        let p = parent.ok_or_else(|| DbError::query("Missing partition parent"))?;
                        deps.insert(("pg_class".into(), p.get(0)));
                        let mut sql = format!(
                            "CREATE TABLE {target} PARTITION OF {} {}{partition}{options}",
                            qname(p.get(1), p.get(2)),
                            r.get::<_, String>(5)
                        );
                        for column in self.client.query("SELECT a.attname,pg_get_expr(d.adbin,d.adrelid) FROM pg_attribute a LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum WHERE a.attrelid=$1::text::oid AND a.attnum>0 AND NOT a.attisdropped AND a.attidentity='' AND a.attgenerated='' AND NOT EXISTS(SELECT 1 FROM pg_inherits i JOIN pg_attribute p ON p.attrelid=i.inhparent AND p.attname=a.attname WHERE i.inhrelid=a.attrelid AND p.attidentity<>'') ORDER BY a.attnum",&[&oid]).await.map_err(|e|self.error(e))? {
                            sql+=&format!("; ALTER TABLE ONLY {target} ALTER COLUMN {} {}",quote_ident(column.get(0)),column.get::<_,Option<String>>(1).map(|value|format!("SET DEFAULT {value}")).unwrap_or_else(||"DROP DEFAULT".into()));
                        }
                        sql
                    } else {
                        if parent.is_some() {
                            return Err(DbError::unsupported(
                                "Legacy table inheritance requires a separate backup strategy",
                            ));
                        }
                        format!(
                            "CREATE {}TABLE {target} ({}){partition}{options}",
                            if r.get::<_, String>(8) == "u" {
                                "UNLOGGED "
                            } else {
                                ""
                            },
                            columns.join(",")
                        )
                    }
                } else {
                    let def: String = self
                        .client
                        .query_one("SELECT pg_get_viewdef($1::text::oid,false)", &[&oid])
                        .await
                        .map_err(|e| self.error(e))?
                        .get(0);
                    if kind == "m" {
                        refreshes.insert(
                            ("pg_class".into(), oid.clone()),
                            format!("REFRESH MATERIALIZED VIEW {target}"),
                        );
                    }
                    format!(
                        "CREATE {} {target} AS {}{}",
                        if kind == "m" {
                            "MATERIALIZED VIEW"
                        } else {
                            "VIEW"
                        },
                        def.trim_end().trim_end_matches(';'),
                        if kind == "m" { " WITH NO DATA" } else { "" }
                    )
                };
                definitions.push(Definition {
                    class: "pg_class".into(),
                    oid,
                    sql,
                    deps,
                });
            }
            // SERIAL ownership is applied after COPY; including its automatic edge makes creation cyclic.
            for def in &mut definitions {
                for r in self.client.query("SELECT d.refclassid::regclass::text,d.refobjid::text FROM pg_depend d WHERE d.deptype<>'a' AND ((d.classid=$1::text::regclass AND d.objid=$2::text::oid) OR (d.classid='pg_attrdef'::regclass AND d.objid IN (SELECT oid FROM pg_attrdef WHERE adrelid=$2::text::oid) AND $1::text='pg_class') OR (d.classid='pg_rewrite'::regclass AND d.objid IN (SELECT oid FROM pg_rewrite WHERE ev_class=$2::text::oid) AND $1::text='pg_class') OR (d.classid='pg_class'::regclass AND d.objid=(SELECT typrelid FROM pg_type WHERE oid=$2::text::oid) AND $1::text='pg_type') OR (d.classid='pg_constraint'::regclass AND d.objid IN (SELECT oid FROM pg_constraint WHERE contypid=$2::text::oid) AND $1::text='pg_type'))",&[&def.class,&def.oid]).await.map_err(|e|self.error(e))? {let key=(r.get::<_,String>(0),r.get::<_,String>(1));def.deps.insert(aliases.get(&key).unwrap_or(&key).clone());}
            }
            let keys = definitions
                .iter()
                .map(|d| (d.class.clone(), d.oid.clone()))
                .collect::<BTreeSet<_>>();
            let mut done = BTreeSet::new();
            while !definitions.is_empty() {
                cancelled_backup(&cancel)?;
                let index = definitions
                    .iter()
                    .position(|d| {
                        d.deps.iter().all(|dep| {
                            dep == &(d.class.clone(), d.oid.clone())
                                || !keys.contains(dep)
                                || done.contains(dep)
                        })
                    })
                    .ok_or_else(|| {
                        DbError::unsupported(
                            "Cyclic schema dependencies require a separate backup strategy",
                        )
                    })?;
                let d = definitions.remove(index);
                if let Some(sql) = refreshes.remove(&(d.class.clone(), d.oid.clone())) {
                    refresh_order.push(sql);
                }
                sql_record(sink, &d.sql).await?;
                done.insert((d.class, d.oid));
            }
            for (target, columns) in copies {
                cancelled_backup(&cancel)?;
                backup_record(
                    sink,
                    b'C',
                    &serde_json::to_vec(&(target.clone(), columns.clone()))
                        .map_err(|_| DbError::query("Invalid COPY metadata"))?,
                )
                .await?;
                let select_columns = columns
                    .iter()
                    .map(|c| quote_ident(c))
                    .collect::<Vec<_>>()
                    .join(",");
                let stream = self
                    .client
                    .copy_out(&format!(
                        "COPY (SELECT {select_columns} FROM ONLY {target}) TO STDOUT"
                    ))
                    .await
                    .map_err(|e| self.error(e))?;
                pin_mut!(stream);
                let mut count = 0;
                while let Some(chunk) = tokio::select! {_ = cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"Backup cancelled")),chunk=stream.next()=>chunk}
                {
                    let chunk = chunk.map_err(|e| self.error(e))?;
                    count += chunk.iter().filter(|b| **b == b'\n').count() as u64;
                    for bytes in chunk.chunks(BACKUP_CHUNK_BYTES) {
                        backup_record(sink, b'D', bytes).await?;
                    }
                }
                backup_record(sink, b'E', &[]).await?;
                summary.counts.push((target, count));
            }
            for r in self.client.query("SELECT ns.nspname,s.relname,nt.nspname,t.relname,a.attname FROM pg_depend d JOIN pg_class s ON s.oid=d.objid JOIN pg_namespace ns ON ns.oid=s.relnamespace JOIN pg_class t ON t.oid=d.refobjid JOIN pg_namespace nt ON nt.oid=t.relnamespace JOIN pg_attribute a ON a.attrelid=t.oid AND a.attnum=d.refobjsubid WHERE d.classid='pg_class'::regclass AND s.relkind='S' AND d.deptype='a' AND ns.nspname !~ '^pg_' ORDER BY 1,2",&[]).await.map_err(|e|self.error(e))? {after.push(format!("ALTER SEQUENCE {} OWNED BY {}.{}",qname(r.get(0),r.get(1)),qname(r.get(2),r.get(3)),quote_ident(r.get(4))));}
            after.extend(refresh_order);
            for sql in after {
                sql_record(sink, &sql).await?;
            }
            for r in self.client.query("SELECT 'ALTER TABLE '||quote_ident(n.nspname)||'.'||quote_ident(c.relname)||' ADD CONSTRAINT '||quote_ident(x.conname)||' '||pg_get_constraintdef(x.oid,false) FROM pg_constraint x JOIN pg_class c ON c.oid=x.conrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname !~ '^pg_' AND n.nspname<>'information_schema' AND x.conparentid=0 AND x.conislocal AND x.coninhcount=0 AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_constraint'::regclass AND d.objid=x.oid AND d.deptype='e') ORDER BY (x.contype='f'),n.nspname,c.relname,x.conname",&[]).await.map_err(|e|self.error(e))? {sql_record(sink,r.get(0)).await?;}
            for r in self.client.query("SELECT pg_get_indexdef(i.indexrelid) FROM pg_index i JOIN pg_class c ON c.oid=i.indrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname !~ '^pg_' AND n.nspname<>'information_schema' AND NOT EXISTS(SELECT 1 FROM pg_constraint x WHERE x.conindid=i.indexrelid) AND NOT EXISTS(SELECT 1 FROM pg_inherits x WHERE x.inhrelid=i.indexrelid) AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_class'::regclass AND d.objid=i.indexrelid AND d.deptype='e') ORDER BY n.nspname,c.relname,i.indexrelid",&[]).await.map_err(|e|self.error(e))? {sql_record(sink,r.get(0)).await?;}
            for r in self.client.query("SELECT pg_get_triggerdef(t.oid,false),t.tgenabled::text,n.nspname,c.relname,t.tgname FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE NOT t.tgisinternal AND t.tgparentid=0 AND n.nspname !~ '^pg_' AND n.nspname<>'information_schema' AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_trigger'::regclass AND d.objid=t.oid AND d.deptype='e') ORDER BY n.nspname,c.relname,t.tgname",&[]).await.map_err(|e|self.error(e))? {
                sql_record(sink,r.get(0)).await?;let mode:String=r.get(1);
                if mode!="O"{sql_record(sink,&format!("ALTER TABLE {} {} TRIGGER {}",qname(r.get(2),r.get(3)),match mode.as_str(){"D"=>"DISABLE","A"=>"ENABLE ALWAYS",_=>"ENABLE REPLICA"},quote_ident(r.get(4)))).await?;}
            }
            backup_record(sink, b'Z', &[]).await?;
            Ok(summary)
        };
        let result = tokio::select! {r=work=>r,_=cancel.cancelled()=>{let _=self.client.cancel_token().cancel_query(tokio_postgres::NoTls).await;Err(DbError::new(ErrorKind::Cancelled,"Backup cancelled"))}};
        if !matches!(
            tokio::time::timeout(
                Duration::from_secs(3),
                self.client.batch_execute("ROLLBACK")
            )
            .await,
            Ok(Ok(()))
        ) {
            self.connection.abort();
            self.closed.store(true, Ordering::Release);
            return Err(DbError::new(
                ErrorKind::Connection,
                "Backup cleanup failed; session closed",
            ));
        }
        result
    }
    async fn columns(&self, oid: &str, table: bool) -> Result<(Vec<String>, Vec<String>), DbError> {
        let mut definitions = vec![];
        let mut copy = vec![];
        for r in self.client.query("SELECT a.attname,format_type(a.atttypid,a.atttypmod),a.attnotnull,pg_get_expr(d.adbin,d.adrelid),a.attidentity::text,a.attgenerated::text,pg_get_serial_sequence(a.attrelid::regclass::text,a.attname),CASE WHEN a.attcollation<>t.typcollation THEN a.attcollation::regcollation::text END FROM pg_attribute a JOIN pg_type t ON t.oid=a.atttypid LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum WHERE a.attrelid=$1::text::oid AND a.attnum>0 AND NOT a.attisdropped ORDER BY a.attnum",&[&oid]).await.map_err(|e|self.error(e))? {
            let name:String=r.get(0);let identity:String=r.get(4);let generated:String=r.get(5);
            let mut def=format!("{} {}",quote_ident(&name),r.get::<_,String>(1));
            if let Some(collation)=r.get::<_,Option<String>>(7){def+=&format!(" COLLATE {collation}");}
            if !identity.is_empty() {
                let seq:String=r.get(6);let opts=self.client.query_one("SELECT s.seqstart,s.seqmin,s.seqmax,s.seqincrement,s.seqcycle,s.seqcache FROM pg_sequence s WHERE s.seqrelid=$1::text::regclass",&[&seq]).await.map_err(|e|self.error(e))?;
                def+=&format!(" GENERATED {} AS IDENTITY (SEQUENCE NAME {seq} START WITH {} MINVALUE {} MAXVALUE {} INCREMENT BY {} {} CACHE {})",if identity=="a"{"ALWAYS"}else{"BY DEFAULT"},opts.get::<_,i64>(0),opts.get::<_,i64>(1),opts.get::<_,i64>(2),opts.get::<_,i64>(3),if opts.get::<_,bool>(4){"CYCLE"}else{"NO CYCLE"},opts.get::<_,i64>(5));
            }else if !generated.is_empty() {def+=&format!(" GENERATED ALWAYS AS ({}) {}",r.get::<_,String>(3),if generated=="s"{"STORED"}else{"VIRTUAL"});}
            else if let Some(default)=r.get::<_,Option<String>>(3){def+=&format!(" DEFAULT {default}");}
            if table && r.get::<_,bool>(2){def+=" NOT NULL";}
            if generated.is_empty(){copy.push(name);}
            definitions.push(def);
        }
        Ok((definitions, copy))
    }
    pub(super) async fn stream_restore(
        &self,
        summary: &BackupSummary,
        source: &mut (dyn std::io::Read + Send),
        overwrite: bool,
        cancel: CancelToken,
    ) -> Result<(), DbError> {
        if summary.format != "postgres-copy-v2" {
            return Err(DbError::unsupported(
                "Unsupported streaming PostgreSQL backup",
            ));
        }
        let state = self.state.lock().await;
        if state.explicit {
            return Err(DbError::unsupported(
                "Finish the existing transaction before restore",
            ));
        }
        self.client
            .batch_execute(&format!("BEGIN; {BACKUP_SETTINGS}"))
            .await
            .map_err(|e| self.error(e))?;
        let work = async {
            let nonempty:bool=self.client.query_one("SELECT EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname !~ '^pg_' AND n.nspname<>'information_schema' AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_class'::regclass AND d.objid=c.oid AND d.deptype='e')) OR EXISTS(SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname !~ '^pg_' AND n.nspname<>'information_schema' AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_proc'::regclass AND d.objid=p.oid AND d.deptype='e')) OR EXISTS(SELECT 1 FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname !~ '^pg_' AND n.nspname<>'information_schema' AND t.typtype IN ('e','d') AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_type'::regclass AND d.objid=t.oid AND d.deptype='e'))",&[]).await.map_err(|e|self.error(e))?.get(0);
            if nonempty && !overwrite {
                return Err(DbError::new(
                    ErrorKind::Conflict,
                    "Restore requires an empty database",
                ));
            }
            if overwrite {
                for r in self.client.query("SELECT nspname FROM pg_namespace WHERE nspname !~ '^pg_' AND nspname<>'information_schema'",&[]).await.map_err(|e|self.error(e))?{self.client.batch_execute(&format!("DROP SCHEMA {} CASCADE",quote_ident(r.get(0)))).await.map_err(|e|self.error(e))?;}
            }
            loop {
                cancelled_backup(&cancel)?;
                let (tag, data) = read_backup_record(source)?
                    .ok_or_else(|| DbError::query("Incomplete PostgreSQL backup"))?;
                match tag {
                    b'S' => self
                        .client
                        .batch_execute(
                            std::str::from_utf8(&data)
                                .map_err(|_| DbError::query("Invalid schema UTF-8"))?,
                        )
                        .await
                        .map_err(|e| self.error(e))?,
                    b'C' => {
                        let (target, columns): (String, Vec<String>) =
                            serde_json::from_slice(&data)
                                .map_err(|_| DbError::query("Invalid COPY metadata"))?;
                        let columns = if columns.is_empty() {
                            String::new()
                        } else {
                            format!(
                                " ({})",
                                columns
                                    .iter()
                                    .map(|c| quote_ident(c))
                                    .collect::<Vec<_>>()
                                    .join(",")
                            )
                        };
                        let copy = self
                            .client
                            .copy_in(&format!("COPY {target}{columns} FROM STDIN"))
                            .await
                            .map_err(|e| self.error(e))?;
                        pin_mut!(copy);
                        loop {
                            cancelled_backup(&cancel)?;
                            match read_backup_record(source)? {
                                Some((b'D', data)) => copy
                                    .as_mut()
                                    .send(bytes::Bytes::from(data))
                                    .await
                                    .map_err(|e| self.error(e))?,
                                Some((b'E', data)) if data.is_empty() => break,
                                _ => return Err(DbError::query("Invalid COPY record sequence")),
                            }
                        }
                        copy.as_mut().finish().await.map_err(|e| self.error(e))?;
                    }
                    b'Z' if data.is_empty() => {
                        if read_backup_record(source)?.is_some() {
                            return Err(DbError::query("Trailing backup records"));
                        }
                        break;
                    }
                    _ => return Err(DbError::query("Invalid PostgreSQL backup record")),
                }
            }
            cancelled_backup(&cancel)
        };
        let result = tokio::select! {r=work=>r,_=cancel.cancelled()=>{let _=self.client.cancel_token().cancel_query(tokio_postgres::NoTls).await;Err(DbError::new(ErrorKind::Cancelled,"Restore cancelled"))}};
        if !matches!(
            tokio::time::timeout(
                Duration::from_secs(3),
                self.client
                    .batch_execute(if result.is_ok() { "COMMIT" } else { "ROLLBACK" })
            )
            .await,
            Ok(Ok(()))
        ) {
            self.connection.abort();
            self.closed.store(true, Ordering::Release);
            return Err(DbError::new(
                ErrorKind::Connection,
                "Restore outcome unknown; session closed",
            ));
        }
        result
    }
}
fn cancelled_backup(cancel: &CancelToken) -> Result<(), DbError> {
    if cancel.is_cancelled() {
        Err(DbError::new(
            ErrorKind::Cancelled,
            "Backup/restore cancelled",
        ))
    } else {
        Ok(())
    }
}
