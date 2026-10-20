use super::*;
impl SqliteSession {
    pub(crate) async fn load_snapshot(
        &self,
        _scope: Option<&str>,
    ) -> Result<SchemaSnapshot, DbError> {
        self.with_connection(|c|{
            let mut snapshot=SchemaSnapshot{version:1,dialect:"sqlite".into(),objects:vec![]};
            let mut stmt=c.prepare("SELECT name,type,sql FROM sqlite_master WHERE type IN ('table','view','index','trigger') AND name NOT LIKE 'sqlite_%' AND sql IS NOT NULL ORDER BY type,name").map_err(sql_error)?;
            let objects=stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).map_err(sql_error)?.collect::<Result<Vec<_>,_>>().map_err(sql_error)?;
            for (name,kind,definition) in objects {
                let columns=if kind=="table"||kind=="view" {let mut s=c.prepare("SELECT name,type,\"notnull\",dflt_value FROM pragma_table_xinfo(?1) ORDER BY cid").map_err(sql_error)?;s.query_map([&name],|r|Ok(SnapshotColumn{name:r.get(0)?,type_name:r.get(1)?,nullable:r.get::<_,i64>(2)?==0,default:r.get(3)?})).map_err(sql_error)?.collect::<Result<Vec<_>,_>>().map_err(sql_error)?}else{vec![]};
                snapshot.objects.push(SchemaObject{namespace:String::new(),name,kind,definition,columns});
            }
            snapshot.normalize()?;Ok(snapshot)
        }).await
    }
}
impl SqliteSession {
    pub(crate) async fn migrate(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<Vec<ResultSet>, DbError> {
        let req = req.clone();
        self.with_connection(move |c| {
            if !c.is_autocommit() {
                return Err(DbError::unsupported(
                    "Finish the existing transaction before migrating",
                ));
            }
            let tokens = dbcore::preview::tokens(&req.text)?;
            let mut start = 0;
            for end in tokens
                .iter()
                .enumerate()
                .filter_map(|(i, t)| (t.text == ";" && t.depth == 0).then_some(i))
                .chain(std::iter::once(tokens.len()))
            {
                if let Some(reason) = dbcore::preview::rejection(&tokens[start..end]) {
                    return Err(DbError::query(reason));
                }
                start = end.saturating_add(1);
            }
            if cancel.is_cancelled() {
                return Err(DbError::new(ErrorKind::Cancelled, "Migration cancelled"));
            }
            c.execute_batch("SAVEPOINT axdb_migration")
                .map_err(sql_error)?;
            let deadline = Instant::now() + Duration::from_millis(req.timeout_ms.clamp(1, 3600000));
            let token = cancel.clone();
            c.progress_handler(
                1000,
                Some(move || token.is_cancelled() || Instant::now() >= deadline),
            )
            .map_err(sql_error)?;
            let result = c.execute_batch(&req.text).map_err(sql_error);
            let hooks = c.progress_handler(0, None::<fn() -> bool>);
            let rollback = result.is_err()
                || hooks.is_err()
                || cancel.is_cancelled()
                || Instant::now() >= deadline;
            if !c.is_autocommit()
                && c.execute_batch(if rollback {
                    "ROLLBACK TO axdb_migration; RELEASE axdb_migration"
                } else {
                    "RELEASE axdb_migration"
                })
                .is_err()
            {
                return Err(DbError::new(
                    ErrorKind::Connection,
                    "Migration cleanup failed",
                ));
            }
            hooks.map_err(sql_error)?;
            result?;
            if rollback {
                return Err(DbError::new(
                    if cancel.is_cancelled() {
                        ErrorKind::Cancelled
                    } else {
                        ErrorKind::Timeout
                    },
                    "Migration rolled back",
                ));
            }
            Ok(vec![ResultSet {
                message: Some("Migration committed".into()),
                ..Default::default()
            }])
        })
        .await
    }
}
