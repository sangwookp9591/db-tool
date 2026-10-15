use super::*;
impl NeoSession {
    pub(super) async fn preview_changes(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<ChangePreview, DbError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(DbError::new(ErrorKind::Connection, "Session closed"));
        }
        let ts = dbcore::preview::tokens(&req.text)?;
        if ts.iter().any(|t| {
            ["CALL", "LOAD", "DATABASE", "TRANSACTIONS"]
                .iter()
                .any(|w| t.is(w))
        }) {
            return Ok(ChangePreview { notes: vec!["Preview refused: procedures, external loading, database administration and subtransactions cannot be guaranteed to roll back".into()], ..Default::default() });
        }
        let graph = neo4rs::Graph::connect(self.config.clone()).map_err(|e| self.error(e))?;
        let deadline = tokio::time::sleep(Duration::from_millis(req.timeout_ms.max(1)));
        tokio::pin!(deadline);
        let mut txn = tokio::select! { biased;
            _=cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"Preview cancelled")),
            _=&mut deadline=>return Err(DbError::new(ErrorKind::Timeout,"Preview timed out")),
            tx=graph.start_txn()=>tx.map_err(|e| self.error(e))?,
        };
        let result = {
            let work = async {
                let mut q = query(&req.text);
                for (key, v) in &req.params {
                    q = q.param(key, parameter(v)?);
                }
                let mut stream = txn.execute(q).await.map_err(|e| self.error(e))?;
                let mut columns = Vec::new();
                let mut rows = Vec::new();
                let limit = req.preview_sample_limit.min(1000) as usize;
                // Drain the stream so summary counters describe the full statement, not just its sample.
                while let Some(row) = stream.next(&mut txn).await.map_err(|e| self.error(e))? {
                    if rows.len() >= limit {
                        continue;
                    }
                    if columns.is_empty() {
                        let mut names: Vec<_> =
                            row.keys().into_iter().map(|k| k.value.clone()).collect();
                        names.sort();
                        columns = names
                            .into_iter()
                            .map(|name| Column {
                                name,
                                type_name: "Null".into(),
                            })
                            .collect();
                    }
                    let mut cells = Vec::new();
                    for c in &mut columns {
                        let raw: BoltType = row
                            .get(&c.name)
                            .map_err(|_| DbError::query("Cannot decode preview record"))?;
                        let value = map_value(&raw)?;
                        if c.type_name == "Null" {
                            c.type_name = type_name(&value).into();
                        }
                        cells.push(value);
                    }
                    rows.push(cells);
                }
                let summary = stream.finish(&mut txn).await.map_err(|e| self.error(e))?;
                let s = summary.stats();
                let counters = [
                    ("nodes_created", s.nodes_created),
                    ("nodes_deleted", s.nodes_deleted),
                    ("relationships_created", s.relationships_created),
                    ("relationships_deleted", s.relationships_deleted),
                    ("properties_set", s.properties_set),
                    ("labels_added", s.labels_added),
                    ("labels_removed", s.labels_removed),
                    ("indexes_added", s.indexes_added),
                    ("indexes_removed", s.indexes_removed),
                    ("constraints_added", s.constraints_added),
                    ("constraints_removed", s.constraints_removed),
                    ("system_updates", s.system_updates),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_string(), i64::try_from(v).unwrap_or(i64::MAX)))
                .collect::<Vec<_>>();
                let mut statement =
                    StatementPreview::new(req.text.clone(), StatementKind::GraphWrite);
                statement.affected = Some(counters.iter().map(|(_, n)| *n as u64).sum());
                statement.counters = counters;
                statement.after = if columns.is_empty() {
                    None
                } else {
                    Some(Table { columns, rows })
                };
                statement.warnings.push("After samples contain only explicitly RETURNed values; counters count graph operations, not distinct records".into());
                Ok(ChangePreview { statements: vec![statement], notes: vec!["Explicit transaction rolled back; returned entities are samples from the uncommitted transaction".into()], ..Default::default() })
            };
            tokio::pin!(work);
            tokio::select! { biased;
                _=cancel.cancelled()=>Err(DbError::new(ErrorKind::Cancelled,"Preview cancelled")),
                _=&mut deadline=>Err(DbError::new(ErrorKind::Timeout,"Preview timed out")),
                result=&mut work=>result,
            }
        };
        // The execution's dedicated pool is discarded even if an interrupted protocol cannot acknowledge rollback.
        let cleanup = tokio::time::timeout(Duration::from_secs(3), txn.rollback()).await;
        match (result, cleanup) {
            (Ok(value), Ok(Ok(()))) => Ok(value),
            (Err(e), _) => Err(e),
            _ => Err(DbError::new(
                ErrorKind::Connection,
                "Preview rollback failed; connection discarded",
            )),
        }
    }
}
