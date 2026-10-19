use super::*;
use tokio_postgres::types::Type;

impl PgSession {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn stream_table(
        &self,
        sql: &str,
        types: &[Type],
        limit: usize,
        cursorable: bool,
        explicit: bool,
        index: usize,
        sink: &dyn AsyncResultSink,
    ) -> Result<(u64, bool), DbError> {
        let cursor = format!("axdb_{}", uuid_free_id());
        let owned_transaction = cursorable && !explicit;
        if owned_transaction {
            self.client
                .batch_execute("BEGIN")
                .await
                .map_err(|e| self.error(e))?;
        }
        let result = async {
            if cursorable {
                self.client
                    .batch_execute(&format!("DECLARE {cursor} NO SCROLL CURSOR FOR {sql}"))
                    .await
                    .map_err(|e| self.error(e))?;
            }
            let mut count = 0usize;
            let mut truncated = false;
            let mut first = true;
            loop {
                // FETCH bounds the PostgreSQL protocol's own response queue as well as our row batch.
                let wanted = (limit.saturating_add(1) - count).min(if first { 200 } else { 2000 });
                let query = if cursorable {
                    format!("FETCH {wanted} FROM {cursor}")
                } else {
                    sql.to_owned()
                };
                let stream = self
                    .client
                    .simple_query_raw(&query)
                    .await
                    .map_err(|e| self.error(e))?;
                pin_mut!(stream);
                let mut fetched = 0usize;
                let mut batch = Vec::new();
                let mut batch_bytes = 0usize;
                while let Some(message) = stream.next().await {
                    if let SimpleQueryMessage::Row(row) = message.map_err(|e| self.error(e))? {
                        fetched += 1;
                        if count >= limit {
                            truncated = true;
                            continue;
                        }
                        let values = (0..types.len())
                            .map(|i| map_value(&types[i], row.get(i)))
                            .collect::<Result<Vec<_>, _>>()?;
                        let bytes = row.len().saturating_mul(32)
                            + (0..row.len())
                                .filter_map(|i| row.get(i))
                                .map(str::len)
                                .sum::<usize>();
                        if !batch.is_empty() && batch_bytes.saturating_add(bytes) > 256 * 1024 {
                            sink.rows(index, std::mem::take(&mut batch)).await?;
                            batch_bytes = 0;
                        }
                        batch_bytes = batch_bytes.saturating_add(bytes);
                        batch.push(values);
                        count += 1;
                        if batch.len() >= if first { 200 } else { 2000 } {
                            sink.rows(index, std::mem::take(&mut batch)).await?;
                            batch_bytes = 0;
                            first = false;
                        }
                    }
                }
                if !batch.is_empty() {
                    sink.rows(index, batch).await?;
                }
                first = false;
                if !cursorable || truncated || fetched < wanted {
                    break;
                }
            }
            if cursorable {
                self.client
                    .batch_execute(&format!("CLOSE {cursor}"))
                    .await
                    .map_err(|e| self.error(e))?;
            }
            Ok((count as u64, truncated))
        }
        .await;
        if owned_transaction {
            self.client
                .batch_execute(if result.is_ok() { "COMMIT" } else { "ROLLBACK" })
                .await
                .map_err(|e| self.error(e))?;
        } else if cursorable && result.is_err() {
            let _ = self.client.batch_execute(&format!("CLOSE {cursor}")).await;
        }
        result
    }
}

struct SyncOutput<'a>(&'a dyn ResultSink);
#[async_trait::async_trait]
impl AsyncResultSink for SyncOutput<'_> {
    async fn start(&self, index: usize, result: &ResultSet) -> Result<(), DbError> {
        self.0.start_pending(index, result)
    }
    async fn rows(&self, index: usize, rows: Vec<Vec<Value>>) -> Result<(), DbError> {
        self.0.rows(index, rows)
    }
    async fn end(&self, index: usize, result: &ResultSet) -> Result<(), DbError> {
        self.0.end(index, result)
    }
}
impl PgSession {
    pub(super) async fn live_query(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
        sink: &dyn ResultSink,
    ) -> Result<Vec<ResultSet>, DbError> {
        self.execute_stream_raw(req, cancel, &SyncOutput(sink))
            .await
    }
}
