//! Batch runner: extract → transform → load with bounded memory and cooperative cancellation.
//!
//! The engine owns no runtime, timers or I/O. Hosts adapt their sessions to [`Extractor`] and
//! [`Loader`] and apply timeouts there, so the engine is testable without a database.
use crate::spec::{CastType, OnError, Pipeline};
use crate::transform::{RowError, Transform};
use async_trait::async_trait;
use dbcore::{CancelToken, ChangePreview, Column, DbError, ErrorKind, FloatValue, Value};
use serde::{Deserialize, Serialize};
use std::time::Instant;

pub const MAX_REJECT_SAMPLES: usize = 20;
pub const MAX_PREVIEW_ROWS: u32 = 200;
/// Preview reads at most this many source rows looking for sample output.
pub const PREVIEW_READ_LIMIT: u64 = 10_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
}

#[async_trait]
pub trait Extractor: Send {
    /// Next rows, at most `limit`. `None` or an empty batch ends the source.
    async fn next(&mut self, limit: u32, cancel: &CancelToken) -> Result<Option<Batch>, DbError>;
    /// The source stopped at its own ceiling rather than running out of rows.
    fn truncated(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadRequest {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    /// Present only on the first load when the pipeline creates its target.
    pub create_table: Option<Vec<Column>>,
    /// Run and roll back.
    pub preview: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub enum LoadOutput {
    Preview(ChangePreview),
    Applied(u64),
}
#[async_trait]
pub trait Loader: Send {
    /// One atomic write of `req.rows` (≤ `spec::MAX_BATCH_ROWS`).
    async fn load(&mut self, req: LoadRequest, cancel: &CancelToken)
    -> Result<LoadOutput, DbError>;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Extract,
    Load,
    Done,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Progress {
    pub phase: Phase,
    pub rows_read: u64,
    pub rows_written: u64,
    pub rows_filtered: u64,
    pub rows_rejected: u64,
    pub batches_committed: u64,
}
pub trait Observer: Send + Sync {
    fn progress(&self, progress: &Progress);
}
impl Observer for () {
    fn progress(&self, _: &Progress) {}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RejectedRow {
    /// 1-based position in the source stream.
    pub row: u64,
    pub step: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreviewReport {
    pub input_columns: Vec<Column>,
    pub output_columns: Vec<Column>,
    pub sample: Vec<Vec<Value>>,
    pub rows_read: u64,
    pub rows_filtered: u64,
    pub rows_rejected: u64,
    pub rejected: Vec<RejectedRow>,
    /// Target-side rollback preview of the sample rows.
    pub target: Option<ChangePreview>,
    pub source_truncated: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Completed,
    Failed,
    Cancelled,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunReport {
    pub status: RunStatus,
    #[serde(flatten)]
    pub progress: Progress,
    pub rejected: Vec<RejectedRow>,
    pub error: Option<String>,
    pub source_truncated: bool,
    pub elapsed_ms: u64,
}

/// Portable column type for `create_table`: explicit casts win, otherwise the sample decides.
pub fn infer_columns(
    names: &[String],
    casts: &[Option<CastType>],
    rows: &[Vec<Value>],
) -> Vec<Column> {
    names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let type_name = casts
                .get(i)
                .copied()
                .flatten()
                .map(CastType::column_type)
                .unwrap_or_else(|| {
                    let mut kind: Option<&'static str> = None;
                    for row in rows {
                        let next = match row.get(i) {
                            None | Some(Value::Null) => continue,
                            Some(Value::Int(_)) => "integer",
                            Some(Value::Float(FloatValue::Number(_)) | Value::Decimal(_)) => {
                                "float"
                            }
                            Some(Value::Bool(_)) => "boolean",
                            Some(Value::Json(_) | Value::List(_) | Value::Map(_)) => "json",
                            Some(_) => "text",
                        };
                        kind = Some(match (kind, next) {
                            (None, n) => n,
                            (Some(a), b) if a == b => a,
                            (Some("integer"), "float") | (Some("float"), "integer") => "float",
                            _ => "text",
                        });
                        if kind == Some("text") {
                            break;
                        }
                    }
                    kind.unwrap_or("text")
                });
            Column {
                name: name.clone(),
                type_name: type_name.into(),
            }
        })
        .collect()
}

struct State {
    progress: Progress,
    rejected: Vec<RejectedRow>,
    transform: Option<Transform>,
    input: Vec<Column>,
}
impl State {
    fn new() -> Self {
        Self {
            progress: Progress::default(),
            rejected: vec![],
            transform: None,
            input: vec![],
        }
    }
    fn reject(&mut self, row: u64, error: RowError) {
        self.progress.rows_rejected += 1;
        if self.rejected.len() < MAX_REJECT_SAMPLES {
            self.rejected.push(RejectedRow {
                row,
                step: error.step,
                message: error.message,
            });
        }
    }
    /// Transform one source batch. With `strict`, the first row error is returned.
    fn transform(
        &mut self,
        pipeline: &Pipeline,
        batch: Batch,
        strict: bool,
    ) -> Result<Vec<Vec<Value>>, DbError> {
        if self.transform.is_none() {
            let names: Vec<String> = batch.columns.iter().map(|c| c.name.clone()).collect();
            self.transform = Some(Transform::compile(&names, &pipeline.steps)?);
            self.input = batch.columns.clone();
        } else if batch.columns.len() != self.input.len()
            || batch
                .columns
                .iter()
                .zip(&self.input)
                .any(|(a, b)| a.name != b.name)
        {
            return Err(DbError::query("소스 컬럼 구성이 실행 중에 바뀌었습니다"));
        }
        let width = self.input.len();
        let first = self.progress.rows_read;
        self.progress.rows_read += batch.rows.len() as u64;
        let mut out = Vec::with_capacity(batch.rows.len());
        for (i, row) in batch.rows.into_iter().enumerate() {
            let number = first + i as u64 + 1;
            if row.len() != width {
                return Err(DbError::query(format!(
                    "소스 행 {number}의 컬럼 수가 맞지 않습니다"
                )));
            }
            let Some(transform) = self.transform.as_mut() else {
                return Err(DbError::internal("변환이 준비되지 않았습니다"));
            };
            match transform.apply(row) {
                Ok(Some(row)) => out.push(row),
                Ok(None) => self.progress.rows_filtered += 1,
                Err(e) if strict => {
                    return Err(DbError::query(format!(
                        "소스 행 {number}, 단계 {}: {}",
                        e.step, e.message
                    )));
                }
                Err(e) => self.reject(number, e),
            }
        }
        Ok(out)
    }
    fn output(&self) -> (Vec<String>, Vec<Option<CastType>>) {
        self.transform
            .as_ref()
            .map(|t| (t.output().to_vec(), t.output_casts().to_vec()))
            .unwrap_or_default()
    }
}

fn next_limit(pipeline: &Pipeline, read: u64, ceiling: Option<u64>) -> u32 {
    let batch = u64::from(pipeline.options.batch_size);
    ceiling.map_or(batch, |c| batch.min(c.saturating_sub(read))) as u32
}

/// Transform a sample and run the target write with rollback. Nothing is committed.
pub async fn preview(
    pipeline: &Pipeline,
    extractor: &mut dyn Extractor,
    loader: &mut dyn Loader,
    sample_rows: u32,
    cancel: &CancelToken,
) -> Result<PreviewReport, DbError> {
    pipeline.validate()?;
    let sample_rows = sample_rows.clamp(1, MAX_PREVIEW_ROWS) as usize;
    let ceiling = Some(
        pipeline
            .read_limit()
            .map_or(PREVIEW_READ_LIMIT, |c| c.min(PREVIEW_READ_LIMIT)),
    );
    let mut state = State::new();
    let mut sample = Vec::new();
    let mut warnings = Vec::new();
    while sample.len() < sample_rows {
        if cancel.is_cancelled() {
            return Err(DbError::new(
                ErrorKind::Cancelled,
                "미리보기가 취소되었습니다",
            ));
        }
        let limit = next_limit(pipeline, state.progress.rows_read, ceiling);
        if limit == 0 {
            break;
        }
        let Some(batch) = extractor.next(limit, cancel).await? else {
            break;
        };
        if batch.rows.is_empty() && state.transform.is_some() {
            break;
        }
        let empty = batch.rows.is_empty();
        let rows = state.transform(pipeline, batch, false)?;
        sample.extend(rows.into_iter().take(sample_rows - sample.len()));
        if empty {
            break;
        }
    }
    let (names, casts) = state.output();
    if state.transform.is_none() {
        warnings.push("소스에서 행을 받지 못해 컬럼을 확인할 수 없습니다".into());
    } else if sample.is_empty() {
        warnings.push("미리보기 범위에서 대상에 쓸 행이 없습니다".into());
    }
    if state.progress.rows_rejected > 0 && pipeline.options.on_error == OnError::Abort {
        warnings.push("변환 오류가 있어 실행하면 첫 오류에서 중단됩니다".into());
    }
    let output_columns = infer_columns(&names, &casts, &sample);
    let target = if sample.is_empty() {
        None
    } else {
        let req = LoadRequest {
            columns: names,
            rows: sample.clone(),
            create_table: pipeline.target.create_table.then(|| output_columns.clone()),
            preview: true,
        };
        match loader.load(req, cancel).await? {
            LoadOutput::Preview(p) if !p.committed => Some(p),
            _ => {
                return Err(DbError::internal(
                    "대상 드라이버가 롤백 미리보기를 만들지 않았습니다",
                ));
            }
        }
    };
    Ok(PreviewReport {
        input_columns: state.input,
        output_columns,
        sample,
        rows_read: state.progress.rows_read,
        rows_filtered: state.progress.rows_filtered,
        rows_rejected: state.progress.rows_rejected,
        rejected: state.rejected,
        target,
        source_truncated: extractor.truncated(),
        warnings,
    })
}

/// Run to completion, failure or cancellation. Each loaded batch is committed on its own.
pub async fn run(
    pipeline: &Pipeline,
    extractor: &mut dyn Extractor,
    loader: &mut dyn Loader,
    observer: &dyn Observer,
    cancel: &CancelToken,
) -> RunReport {
    let started = Instant::now();
    let mut state = State::new();
    let result = run_inner(pipeline, extractor, loader, observer, cancel, &mut state).await;
    state.progress.phase = Phase::Done;
    observer.progress(&state.progress);
    let (status, error) = match result {
        Ok(()) => (RunStatus::Completed, None),
        Err(e) if e.kind == ErrorKind::Cancelled || cancel.is_cancelled() => {
            (RunStatus::Cancelled, Some(e.message))
        }
        Err(e) => (RunStatus::Failed, Some(e.message)),
    };
    RunReport {
        status,
        progress: state.progress,
        rejected: state.rejected,
        error,
        source_truncated: extractor.truncated(),
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}
async fn run_inner(
    pipeline: &Pipeline,
    extractor: &mut dyn Extractor,
    loader: &mut dyn Loader,
    observer: &dyn Observer,
    cancel: &CancelToken,
    state: &mut State,
) -> Result<(), DbError> {
    pipeline.validate()?;
    let ceiling = pipeline.read_limit();
    let strict = pipeline.options.on_error == OnError::Abort;
    let mut created = !pipeline.target.create_table;
    loop {
        if cancel.is_cancelled() {
            return Err(DbError::new(ErrorKind::Cancelled, "실행이 취소되었습니다"));
        }
        let limit = next_limit(pipeline, state.progress.rows_read, ceiling);
        if limit == 0 {
            return Ok(());
        }
        state.progress.phase = Phase::Extract;
        observer.progress(&state.progress);
        let Some(batch) = extractor.next(limit, cancel).await? else {
            return Ok(());
        };
        if batch.rows.is_empty() {
            return Ok(());
        }
        let rows = state.transform(pipeline, batch, strict)?;
        if rows.is_empty() {
            continue;
        }
        if cancel.is_cancelled() {
            return Err(DbError::new(ErrorKind::Cancelled, "실행이 취소되었습니다"));
        }
        let (names, casts) = state.output();
        let create_table = (!created).then(|| infer_columns(&names, &casts, &rows));
        let count = rows.len() as u64;
        state.progress.phase = Phase::Load;
        observer.progress(&state.progress);
        match loader
            .load(
                LoadRequest {
                    columns: names,
                    rows,
                    create_table,
                    preview: false,
                },
                cancel,
            )
            .await?
        {
            LoadOutput::Applied(_) => {}
            LoadOutput::Preview(_) => {
                return Err(DbError::internal(
                    "대상 드라이버가 쓰기 대신 미리보기를 반환했습니다",
                ));
            }
        }
        created = true;
        state.progress.rows_written += count;
        state.progress.batches_committed += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{Cmp, Options, Source, Step, Target};
    use std::sync::Mutex;

    fn int(v: i64) -> Value {
        Value::Int(v.to_string())
    }
    struct Rows {
        columns: Vec<Column>,
        rows: std::vec::IntoIter<Vec<Value>>,
        calls: u32,
    }
    #[async_trait]
    impl Extractor for Rows {
        async fn next(&mut self, limit: u32, _: &CancelToken) -> Result<Option<Batch>, DbError> {
            self.calls += 1;
            let rows: Vec<_> = self.rows.by_ref().take(limit as usize).collect();
            Ok((!rows.is_empty()).then(|| Batch {
                columns: self.columns.clone(),
                rows,
            }))
        }
    }
    fn source(n: i64) -> Rows {
        Rows {
            columns: vec![
                Column {
                    name: "id".into(),
                    type_name: "int8".into(),
                },
                Column {
                    name: "v".into(),
                    type_name: "text".into(),
                },
            ],
            rows: (1..=n)
                .map(|i| {
                    vec![
                        int(i),
                        Value::Text(if i % 10 == 0 {
                            "bad".into()
                        } else {
                            i.to_string()
                        }),
                    ]
                })
                .collect::<Vec<_>>()
                .into_iter(),
            calls: 0,
        }
    }
    #[derive(Default)]
    struct Sink {
        loads: Vec<LoadRequest>,
        fail_at: Option<usize>,
        cancel_after: Option<(usize, CancelToken)>,
    }
    #[async_trait]
    impl Loader for Sink {
        async fn load(&mut self, req: LoadRequest, _: &CancelToken) -> Result<LoadOutput, DbError> {
            if self.fail_at == Some(self.loads.len()) {
                return Err(DbError::query("constraint"));
            }
            let preview = req.preview;
            let n = req.rows.len() as u64;
            self.loads.push(req);
            if let Some((after, token)) = &self.cancel_after
                && self.loads.len() == *after
            {
                token.cancel();
            }
            Ok(if preview {
                LoadOutput::Preview(ChangePreview::default())
            } else {
                LoadOutput::Applied(n)
            })
        }
    }
    fn pipeline(steps: Vec<Step>, on_error: OnError) -> Pipeline {
        Pipeline {
            id: "p".into(),
            name: "test".into(),
            description: String::new(),
            version: crate::spec::SPEC_VERSION,
            source: Source::Table {
                profile_id: "a".into(),
                node_id: "t".into(),
            },
            steps,
            target: Target {
                profile_id: "b".into(),
                node_id: "u".into(),
                create_table: true,
            },
            options: Options {
                batch_size: 25,
                max_rows: None,
                on_error,
                batch_timeout_ms: 1000,
            },
        }
    }
    fn cast_v() -> Vec<Step> {
        vec![Step::Cast {
            column: "v".into(),
            to: CastType::Int,
        }]
    }
    struct Seen(Mutex<Vec<Phase>>);
    impl Observer for Seen {
        fn progress(&self, p: &Progress) {
            self.0.lock().unwrap().push(p.phase);
        }
    }

    #[tokio::test]
    async fn run_skips_bad_rows_and_commits_batches() {
        let p = pipeline(cast_v(), OnError::SkipRow);
        let mut sink = Sink::default();
        let seen = Seen(Mutex::new(vec![]));
        let report = run(&p, &mut source(60), &mut sink, &seen, &CancelToken::new()).await;
        assert_eq!(report.status, RunStatus::Completed, "{report:?}");
        assert_eq!(report.progress.rows_read, 60);
        assert_eq!(report.progress.rows_rejected, 6);
        assert_eq!(report.progress.rows_written, 54);
        assert_eq!(report.progress.batches_committed, 3);
        assert_eq!(
            report.rejected[0],
            RejectedRow {
                row: 10,
                step: 1,
                message: report.rejected[0].message.clone()
            }
        );
        let create = sink.loads[0].create_table.as_ref().unwrap();
        assert_eq!(
            create
                .iter()
                .map(|c| c.type_name.as_str())
                .collect::<Vec<_>>(),
            ["integer", "integer"]
        );
        assert!(sink.loads[1].create_table.is_none());
        assert_eq!(seen.0.lock().unwrap().last(), Some(&Phase::Done));
    }
    #[tokio::test]
    async fn abort_stops_before_loading_the_failing_batch() {
        let p = pipeline(cast_v(), OnError::Abort);
        let mut sink = Sink::default();
        let report = run(&p, &mut source(60), &mut sink, &(), &CancelToken::new()).await;
        assert_eq!(report.status, RunStatus::Failed);
        assert!(report.error.unwrap().contains("소스 행 10"));
        assert!(sink.loads.is_empty());
    }
    #[tokio::test]
    async fn load_failure_keeps_committed_counts() {
        let p = pipeline(vec![], OnError::Abort);
        let mut sink = Sink {
            fail_at: Some(1),
            ..Default::default()
        };
        let report = run(&p, &mut source(60), &mut sink, &(), &CancelToken::new()).await;
        assert_eq!(report.status, RunStatus::Failed);
        assert_eq!(report.progress.batches_committed, 1);
        assert_eq!(report.progress.rows_written, 25);
    }
    #[tokio::test]
    async fn cancellation_stops_between_batches() {
        let token = CancelToken::new();
        let p = pipeline(vec![], OnError::Abort);
        let mut sink = Sink {
            cancel_after: Some((1, token.clone())),
            ..Default::default()
        };
        let report = run(&p, &mut source(100), &mut sink, &(), &token).await;
        assert_eq!(report.status, RunStatus::Cancelled);
        assert_eq!(report.progress.batches_committed, 1);
    }
    #[tokio::test]
    async fn max_rows_and_filters_bound_the_run() {
        let mut p = pipeline(
            vec![Step::Filter {
                column: "id".into(),
                cmp: Cmp::Gt,
                value: Some(int(5)),
            }],
            OnError::Abort,
        );
        p.options.max_rows = Some(30);
        let mut src = source(100);
        let mut sink = Sink::default();
        let report = run(&p, &mut src, &mut sink, &(), &CancelToken::new()).await;
        assert_eq!(report.progress.rows_read, 30);
        assert_eq!(report.progress.rows_filtered, 5);
        assert_eq!(report.progress.rows_written, 25);
        assert_eq!(src.calls, 2);
    }
    #[tokio::test]
    async fn preview_samples_and_rolls_back_once() {
        let p = pipeline(cast_v(), OnError::Abort);
        let mut sink = Sink::default();
        let report = preview(&p, &mut source(60), &mut sink, 30, &CancelToken::new())
            .await
            .unwrap();
        assert_eq!(report.sample.len(), 30);
        // Two whole batches of 25 are read to fill 30 rows; rows 10..=50 step 10 fail.
        assert_eq!(report.rows_read, 50);
        assert_eq!(report.rows_rejected, 5);
        assert_eq!(report.rejected.len(), 5);
        assert_eq!(report.output_columns[1].type_name, "integer");
        assert_eq!(sink.loads.len(), 1);
        assert!(sink.loads[0].preview && sink.loads[0].create_table.is_some());
        assert!(report.warnings.iter().any(|w| w.contains("첫 오류")));
        assert!(report.target.is_some());
    }
    #[tokio::test]
    async fn preview_of_empty_source_warns() {
        let p = pipeline(vec![], OnError::Abort);
        let report = preview(
            &p,
            &mut source(0),
            &mut Sink::default(),
            10,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert!(report.target.is_none());
        assert_eq!(report.warnings.len(), 1);
    }
    /// Replays scripted batches so tests can change columns or widths mid-run.
    struct Script(std::vec::IntoIter<Batch>, u32);
    #[async_trait]
    impl Extractor for Script {
        async fn next(&mut self, _: u32, _: &CancelToken) -> Result<Option<Batch>, DbError> {
            self.1 += 1;
            Ok(self.0.next())
        }
    }
    fn col(name: &str) -> Column {
        Column {
            name: name.into(),
            type_name: "text".into(),
        }
    }
    struct Wrong(bool);
    #[async_trait]
    impl Loader for Wrong {
        async fn load(&mut self, _: LoadRequest, _: &CancelToken) -> Result<LoadOutput, DbError> {
            Ok(if self.0 {
                LoadOutput::Preview(ChangePreview {
                    committed: true,
                    ..Default::default()
                })
            } else {
                LoadOutput::Applied(1)
            })
        }
    }
    #[tokio::test]
    async fn failure_paths_report_without_panicking() {
        let p = pipeline(vec![], OnError::SkipRow);
        let batch = |cols: &[&str], rows: Vec<Vec<Value>>| Batch {
            columns: cols.iter().map(|c| col(c)).collect(),
            rows,
        };
        // Column set changes between batches.
        let mut src = Script(
            vec![
                batch(&["a"], vec![vec![int(1)]]),
                batch(&["b"], vec![vec![int(2)]]),
            ]
            .into_iter(),
            0,
        );
        let report = run(&p, &mut src, &mut Sink::default(), &(), &CancelToken::new()).await;
        assert_eq!(
            (report.status, report.progress.batches_committed),
            (RunStatus::Failed, 1)
        );
        assert!(report.error.unwrap().contains("컬럼 구성"));
        // A row narrower than the header is a source error, even with skip_row.
        let mut src = Script(vec![batch(&["a", "b"], vec![vec![int(1)]])].into_iter(), 0);
        let report = run(&p, &mut src, &mut Sink::default(), &(), &CancelToken::new()).await;
        assert!(report.error.unwrap().contains("소스 행 1"));
        // The target answered a real write with a preview.
        let report = run(
            &p,
            &mut source(3),
            &mut Wrong(true),
            &(),
            &CancelToken::new(),
        )
        .await;
        assert_eq!(
            (report.status, report.progress.rows_written),
            (RunStatus::Failed, 0)
        );
        // A preview that claims to have committed is rejected.
        assert!(
            preview(&p, &mut source(3), &mut Wrong(true), 5, &CancelToken::new())
                .await
                .is_err()
        );
        // A preview receiving an applied write is rejected too.
        assert!(
            preview(
                &p,
                &mut source(3),
                &mut Wrong(false),
                5,
                &CancelToken::new()
            )
            .await
            .is_err()
        );
    }
    #[tokio::test]
    async fn nothing_is_read_when_invalid_or_cancelled_first() {
        let mut bad = pipeline(vec![], OnError::Abort);
        bad.options.batch_size = 0;
        let mut src = source(10);
        let report = run(
            &bad,
            &mut src,
            &mut Sink::default(),
            &(),
            &CancelToken::new(),
        )
        .await;
        assert_eq!((report.status, src.calls), (RunStatus::Failed, 0));
        let token = CancelToken::new();
        token.cancel();
        let mut src = source(10);
        let mut sink = Sink::default();
        let report = run(
            &pipeline(vec![], OnError::Abort),
            &mut src,
            &mut sink,
            &(),
            &token,
        )
        .await;
        assert_eq!(
            (report.status, src.calls, sink.loads.len()),
            (RunStatus::Cancelled, 0, 0)
        );
    }
    #[tokio::test]
    async fn fully_filtered_batches_skip_the_load_and_continue() {
        let p = pipeline(
            vec![Step::Filter {
                column: "id".into(),
                cmp: Cmp::Gt,
                value: Some(int(25)),
            }],
            OnError::Abort,
        );
        let mut sink = Sink::default();
        let report = run(&p, &mut source(50), &mut sink, &(), &CancelToken::new()).await;
        assert_eq!(report.status, RunStatus::Completed);
        assert_eq!(
            (
                report.progress.rows_filtered,
                report.progress.rows_written,
                sink.loads.len()
            ),
            (25, 25, 1)
        );
        // The create_table request moves to the first batch that is actually loaded.
        assert!(sink.loads[0].create_table.is_some());
    }
    #[test]
    fn inference_widens_conflicting_types() {
        let names = vec!["a".into(), "b".into(), "c".into()];
        let rows = vec![
            vec![int(1), int(1), Value::Null],
            vec![
                Value::Float(FloatValue::Number(1.5)),
                Value::Text("x".into()),
                Value::Null,
            ],
        ];
        let cols = infer_columns(&names, &[None, None, Some(CastType::Bool)], &rows);
        assert_eq!(
            cols.iter()
                .map(|c| c.type_name.as_str())
                .collect::<Vec<_>>(),
            ["float", "text", "boolean"]
        );
    }
}
