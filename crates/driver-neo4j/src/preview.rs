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
            return Ok(ChangePreview { notes: vec!["미리보기를 거부했습니다: 프로시저, 외부 로딩, 데이터베이스 관리, 하위 트랜잭션은 롤백을 보장할 수 없습니다.".into()], ..Default::default() });
        }
        let graph = neo4rs::Graph::connect(self.config.clone()).map_err(|e| self.error(e))?;
        let deadline = tokio::time::sleep(Duration::from_millis(req.timeout_ms.max(1)));
        tokio::pin!(deadline);
        let mut txn = tokio::select! { biased;
            _=cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"미리보기가 취소되었습니다")),
            _=&mut deadline=>return Err(DbError::new(ErrorKind::Timeout,"미리보기 시간이 초과되었습니다")),
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
                statement.warnings.push("변경 후 샘플에는 RETURN으로 명시한 값만 들어 있으며, 카운터는 고유 레코드가 아니라 그래프 작업 수를 셉니다.".into());
                Ok(ChangePreview { statements: vec![statement], notes: vec!["명시적 트랜잭션을 롤백했습니다. 반환된 엔티티는 커밋하지 않은 트랜잭션의 샘플입니다.".into()], ..Default::default() })
            };
            tokio::pin!(work);
            tokio::select! { biased;
                _=cancel.cancelled()=>Err(DbError::new(ErrorKind::Cancelled,"미리보기가 취소되었습니다")),
                _=&mut deadline=>Err(DbError::new(ErrorKind::Timeout,"미리보기 시간이 초과되었습니다")),
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
                "미리보기 롤백에 실패해 연결을 폐기했습니다",
            )),
        }
    }
}
