use super::*;
impl PgSession {
    pub(crate) async fn load_snapshot(
        &self,
        scope: Option<&str>,
    ) -> Result<SchemaSnapshot, DbError> {
        let _guard = self.state.lock().await;
        self.snapshot_unlocked(scope).await
    }
    pub(super) async fn snapshot_unlocked(&self, scope: Option<&str>) -> Result<SchemaSnapshot,DbError> {
        let scope = scope.map(str::to_owned);
        let mut snapshot = SchemaSnapshot {
            version: 1,
            dialect: "postgres".into(),
            objects: vec![],
        };
        let tables=self.client.query("SELECT c.oid::text,n.nspname,c.relname,c.relkind::text FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE ($1::text IS NULL OR n.nspname=$1) AND n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema' AND c.relkind IN ('r','p','v','m') ORDER BY n.nspname,c.relname LIMIT 50001",&[&scope]).await.map_err(|e|self.error(e))?;
        for r in tables {
            let oid: String = r.get(0);
            if snapshot.objects.len() >= 50_000 {
                return Err(DbError::query("Schema exceeds snapshot object cap"));
            }
            let scope: String = r.get(1);
            let name: String = r.get(2);
            let kind: String = r.get(3);
            let columns=self.client.query("SELECT a.attname,format_type(a.atttypid,a.atttypmod),NOT a.attnotnull,pg_get_expr(d.adbin,d.adrelid) FROM pg_attribute a LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum WHERE a.attrelid=$1::text::oid AND a.attnum>0 AND NOT a.attisdropped ORDER BY a.attnum",&[&oid]).await.map_err(|e|self.error(e))?.into_iter().map(|c|SnapshotColumn{name:c.get(0),type_name:c.get(1),nullable:c.get(2),default:c.get(3)}).collect::<Vec<_>>();
            let target = format!("{}.{}", quote_ident(&scope), quote_ident(&name));
            let (kind, definition) = if kind == "r" || kind == "p" {
                let defs = columns
                    .iter()
                    .map(|c| {
                        format!(
                            "{} {}{}{}",
                            quote_ident(&c.name),
                            c.type_name,
                            if c.nullable { "" } else { " NOT NULL" },
                            c.default
                                .as_ref()
                                .map(|d| format!(" DEFAULT {d}"))
                                .unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>();
                (
                    "table",
                    format!("CREATE TABLE {target} ({})", defs.join(", ")),
                )
            } else {
                let view: String = self
                    .client
                    .query_one("SELECT pg_get_viewdef($1::text::oid,true)", &[&oid])
                    .await
                    .map_err(|e| self.error(e))?
                    .get(0);
                let kind = if kind == "m" {
                    "materialized view"
                } else {
                    "view"
                };
                (
                    kind,
                    format!("CREATE {} {target} AS {view}", kind.to_uppercase()),
                )
            };
            snapshot.objects.push(SchemaObject {
                namespace: scope.clone(),
                name: name.clone(),
                kind: kind.into(),
                definition,
                columns,
            });
            let constraints=self.client.query("SELECT conname,pg_get_constraintdef(oid,true) FROM pg_constraint WHERE conrelid=$1::text::oid ORDER BY conname",&[&oid]).await.map_err(|e|self.error(e))?;
            for c in constraints {
                let cname: String = c.get(0);
                let def: String = c.get(1);
                snapshot.objects.push(SchemaObject {
                    namespace: scope.clone(),
                    name: format!("{name}.{cname}"),
                    kind: "constraint".into(),
                    definition: format!(
                        "ALTER TABLE {target} ADD CONSTRAINT {} {def}",
                        quote_ident(&cname)
                    ),
                    columns: vec![],
                });
            }
            let indexes=self.client.query("SELECT i.relname,pg_get_indexdef(i.oid) FROM pg_index x JOIN pg_class i ON i.oid=x.indexrelid WHERE x.indrelid=$1::text::oid AND NOT EXISTS(SELECT 1 FROM pg_constraint c WHERE c.conindid=i.oid) ORDER BY i.relname",&[&oid]).await.map_err(|e|self.error(e))?;
            for c in indexes {
                snapshot.objects.push(SchemaObject {
                    namespace: scope.clone(),
                    name: c.get(0),
                    kind: "index".into(),
                    definition: c.get(1),
                    columns: vec![],
                });
            }
        }
        snapshot.normalize()?;
        Ok(snapshot)
    }
}
impl PgSession {
    pub(crate) async fn migrate(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<Vec<ResultSet>, DbError> {
        self.migrate_transaction(req,cancel,false).await
    }
    pub(super) async fn migrate_transaction(&self,req:&ExecRequest,cancel:CancelToken,preview:bool)->Result<Vec<ResultSet>,DbError>{
        let state = tokio::select! {_ = cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"Migration cancelled")),s=self.state.lock()=>s};
        if state.explicit {
            return Err(DbError::unsupported(
                "Finish the existing transaction before migrating",
            ));
        }
        let statements = split_statements(&req.text)?;
        for sql in &statements {
            if let Some(reason) = dbcore::preview::rejection(&dbcore::preview::tokens(sql)?) {
                if dbcore::preview::tokens(sql)?.first().is_some_and(|t|t.is("GRANT")||t.is("REVOKE")){continue;}
                return Err(DbError::query(reason));
            }
        }
        self.client
            .batch_execute("BEGIN")
            .await
            .map_err(|e| self.error(e))?;
        let execute = self.client.batch_execute(&req.text);
        tokio::pin!(execute);
        let mut interrupted = None;
        let result = tokio::select! {
            r=&mut execute=>r.map_err(|e|self.error(e)),
            _=cancel.cancelled()=>{interrupted=Some(ErrorKind::Cancelled);Err(DbError::new(ErrorKind::Cancelled,"Migration cancelled"))},
            _=tokio::time::sleep(Duration::from_millis(req.timeout_ms.max(1)))=>{interrupted=Some(ErrorKind::Timeout);Err(DbError::new(ErrorKind::Timeout,"Migration timed out"))}
        };
        if interrupted.is_some() {
            let cleanup = async {
                let mut retry = tokio::time::interval(Duration::from_millis(50));
                loop {
                    tokio::select! {_=&mut execute=>break,_=retry.tick()=>{let _=self.client.cancel_token().cancel_query(tokio_postgres::NoTls).await;}}
                }
            };
            if tokio::time::timeout(Duration::from_secs(3), cleanup)
                .await
                .is_err()
            {
                self.connection.abort();
                self.closed.store(true, Ordering::Release);
                return Err(DbError::new(
                    ErrorKind::Connection,
                    "Migration cancellation could not be confirmed; session closed",
                ));
            }
        }
        let rollback = result.is_err() || cancel.is_cancelled();
        let cleanup = tokio::time::timeout(
            Duration::from_secs(3),
            self.client
                .batch_execute(if rollback || preview { "ROLLBACK" } else { "COMMIT" }),
        )
        .await;
        if !matches!(cleanup, Ok(Ok(()))) {
            self.connection.abort();
            self.closed.store(true, Ordering::Release);
            return Err(DbError::new(
                ErrorKind::Connection,
                "Migration transaction outcome requires verification; session closed",
            ));
        }
        result?;
        if rollback {
            return Err(DbError::new(ErrorKind::Cancelled, "Migration cancelled"));
        }
        Ok(vec![ResultSet {
            message: Some(if preview{"Grant preview rolled back"}else{"Migration committed"}.into()),
            ..Default::default()
        }])
    }
}
