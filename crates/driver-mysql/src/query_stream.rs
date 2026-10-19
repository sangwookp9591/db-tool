use super::*;

pub(super) struct SyncOutput<'a>(pub &'a dyn ResultSink);
#[async_trait]
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

pub(super) struct InterruptibleOutput<'a> {
    pub sink: &'a dyn AsyncResultSink,
    pub cancel: CancelToken,
    pub deadline: tokio::time::Instant,
}
impl InterruptibleOutput<'_> {
    async fn send(&self, work: impl Future<Output = Result<(), DbError>>) -> Result<(), DbError> {
        // A stalled consumer must release the query future so guarded can confirm KILL and cleanup.
        tokio::select! { biased;
            _ = self.cancel.cancelled() => Err(DbError::new(ErrorKind::Cancelled, "MySQL stream cancelled")),
            _ = tokio::time::sleep_until(self.deadline) => Err(DbError::new(ErrorKind::Timeout, "MySQL stream timed out")),
            result = work => result,
        }
    }
}
#[async_trait]
impl AsyncResultSink for InterruptibleOutput<'_> {
    async fn start(&self, index: usize, result: &ResultSet) -> Result<(), DbError> {
        self.send(self.sink.start(index, result)).await
    }
    async fn rows(&self, index: usize, rows: Vec<Vec<Value>>) -> Result<(), DbError> {
        self.send(self.sink.rows(index, rows)).await
    }
    async fn end(&self, index: usize, result: &ResultSet) -> Result<(), DbError> {
        self.send(self.sink.end(index, result)).await
    }
}
