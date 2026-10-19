use super::*;
use neo4rs::bolt::Bolt;
use std::collections::HashMap;

fn node(map: &HashMap<String, Bolt>) -> Result<PlanTree, DbError> {
    let args = match map.get("args") {
        Some(Bolt::Dictionary(args)) => Some(args),
        _ => None,
    };
    let number = |v: Option<&Bolt>| match v {
        Some(Bolt::Integer(v)) => Some(*v as f64),
        Some(Bolt::Float(v)) => Some(*v),
        _ => None,
    };
    let text = |v: Option<&Bolt>| match v {
        Some(Bolt::String(v)) => Some(v.clone()),
        _ => None,
    };
    let mut plan = PlanTree {
        op: text(map.get("operatorType")).unwrap_or_else(|| "Unknown".into()),
        target: text(args.and_then(|a| a.get("Details"))),
        est_rows: number(args.and_then(|a| a.get("EstimatedRows"))),
        actual_rows: number(map.get("rows")),
        cost: number(map.get("dbHits")),
        children: match map.get("children") {
            Some(Bolt::List(children)) => children
                .iter()
                .map(|v| match v {
                    Bolt::Dictionary(m) => node(m),
                    _ => Err(DbError::query("Invalid Neo4j plan node")),
                })
                .collect::<Result<_, _>>()?,
            _ => vec![],
        },
        warnings: vec![],
    };
    plan.warn_estimate();
    Ok(plan)
}
impl NeoSession {
    pub(super) async fn explain_plan(
        &self,
        text: &str,
        analyze: bool,
        cancel: CancelToken,
    ) -> Result<PlanTree, DbError> {
        let ts = dbcore::preview::tokens(text)?;
        if ts.is_empty()
            || ts.iter().any(|t| {
                [
                    "CALL",
                    "LOAD",
                    "DATABASE",
                    "TRANSACTIONS",
                    "EXPLAIN",
                    "PROFILE",
                ]
                .iter()
                .any(|w| t.is(w))
                    || t.text == ";"
            })
        {
            return Err(DbError::query(
                "Explain requires one query without procedures or transaction control",
            ));
        }
        if self.closed.load(Ordering::Acquire) {
            return Err(DbError::new(ErrorKind::Connection, "Session closed"));
        }
        let graph = neo4rs::Graph::connect(self.operation_config(30000)).map_err(|e| self.error(e))?;
        let mut tx = tokio::select! { biased;
            _ = cancel.cancelled() => return Err(DbError::new(ErrorKind::Cancelled, "Explain cancelled")),
            result = tokio::time::timeout(Duration::from_secs(30), graph.start_txn()) => result.map_err(|_| DbError::new(ErrorKind::Timeout, "Explain timed out"))?.map_err(|e| self.error(e))?,
        };
        let result = {
            let work = async {
                let mut stream = tx
                    .execute(query(&format!(
                        "{} {text}",
                        if analyze { "PROFILE" } else { "EXPLAIN" }
                    )))
                    .await
                    .map_err(|e| self.error(e))?;
                while stream
                    .next(&mut tx)
                    .await
                    .map_err(|e| self.error(e))?
                    .is_some()
                {}
                let summary = stream.finish(&mut tx).await.map_err(|e| self.error(e))?;
                let raw = if analyze {
                    summary.profile()
                } else {
                    summary.plan()
                }
                .ok_or_else(|| DbError::query("Missing Neo4j plan"))?;
                node(raw)
            };
            tokio::select! { biased;
                _ = cancel.cancelled() => Err(DbError::new(ErrorKind::Cancelled, "Explain cancelled")),
                result = tokio::time::timeout(Duration::from_secs(30), work) => result.map_err(|_| DbError::new(ErrorKind::Timeout, "Explain timed out")).and_then(|r| r),
            }
        };
        let cleanup = tokio::time::timeout(Duration::from_secs(3), tx.rollback()).await;
        match (result, cleanup) {
            (Ok(mut plan), Ok(Ok(()))) => {
                if analyze {
                    plan.warnings
                        .push("PROFILE executed inside a rolled-back transaction".into());
                }
                Ok(plan)
            }
            (Err(e), _) => Err(e),
            _ => Err(DbError::new(
                ErrorKind::Connection,
                "Explain rollback failed; dedicated connection discarded",
            )),
        }
    }
}
