//! Declarative pipeline specification. Stored as JSON; contains profile ids, never secrets.
use dbcore::{DbError, Value};
use serde::{Deserialize, Serialize};

/// Rows a target driver accepts in one atomic `import_rows` call.
pub const MAX_BATCH_ROWS: u32 = dbcore::IMPORT_MAX_ROWS as u32;
/// Query sources are collected in one call, so they share the host row ceiling.
pub const MAX_QUERY_ROWS: u64 = 100_000;
pub const MAX_STEPS: usize = 64;
pub const MAX_NAME_BYTES: usize = 200;
pub const MAX_QUERY_BYTES: usize = 256 * 1024;
pub const DEFAULT_BATCH_ROWS: u32 = 1_000;
pub const DEFAULT_BATCH_TIMEOUT_MS: u64 = 300_000;
pub const SPEC_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pipeline {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "spec_version")]
    pub version: u32,
    pub source: Source,
    #[serde(default)]
    pub steps: Vec<Step>,
    pub target: Target,
    #[serde(default)]
    pub options: Options,
}
fn spec_version() -> u32 {
    SPEC_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// Executed once with `read_only: true`; bounded by `MAX_QUERY_ROWS`.
    Query { profile_id: String, text: String },
    /// Read page by page through `browse`, so size is bounded only by `max_rows`.
    Table { profile_id: String, node_id: String },
}
impl Source {
    pub fn profile_id(&self) -> &str {
        match self {
            Source::Query { profile_id, .. } | Source::Table { profile_id, .. } => profile_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Target {
    pub profile_id: String,
    /// Explorer node id of the table/collection; for a new table use the host's `import_target`.
    pub node_id: String,
    /// Create the target with the output columns on the first batch.
    #[serde(default)]
    pub create_table: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnError {
    /// Any transform error stops the run before the failing batch is loaded.
    #[default]
    Abort,
    /// Rows that fail a transform are counted and sampled, the rest continue.
    SkipRow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Options {
    #[serde(default = "default_batch")]
    pub batch_size: u32,
    #[serde(default)]
    pub max_rows: Option<u64>,
    #[serde(default)]
    pub on_error: OnError,
    #[serde(default = "default_timeout")]
    pub batch_timeout_ms: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_ROWS,
            max_rows: None,
            on_error: OnError::Abort,
            batch_timeout_ms: DEFAULT_BATCH_TIMEOUT_MS,
        }
    }
}
fn default_batch() -> u32 {
    DEFAULT_BATCH_ROWS
}
fn default_timeout() -> u64 {
    DEFAULT_BATCH_TIMEOUT_MS
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColumnMap {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CastType {
    Text,
    Int,
    Float,
    Decimal,
    Bool,
    Date,
    DateTime,
    Json,
}
impl CastType {
    /// Type names understood by the drivers' `create_table` (same set as file import).
    pub fn column_type(self) -> &'static str {
        match self {
            CastType::Int => "integer",
            CastType::Float | CastType::Decimal => "float",
            CastType::Bool => "boolean",
            CastType::Json => "json",
            CastType::Text | CastType::Date | CastType::DateTime => "text",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    IsNull,
    NotNull,
    Contains,
    StartsWith,
}
impl Cmp {
    pub fn needs_value(self) -> bool {
        !matches!(self, Cmp::IsNull | Cmp::NotNull)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextFunc {
    Trim,
    Upper,
    Lower,
}

/// One transform step. Steps run in order on every row (Hop/PDI style linear flow).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Step {
    /// Keep only these columns, in this order, optionally renamed.
    Select {
        columns: Vec<ColumnMap>,
    },
    Rename {
        from: String,
        to: String,
    },
    Drop {
        columns: Vec<String>,
    },
    Cast {
        column: String,
        to: CastType,
    },
    /// Keep rows matching the comparison; several filters combine with AND.
    Filter {
        column: String,
        cmp: Cmp,
        #[serde(default)]
        value: Option<Value>,
    },
    /// Add (or overwrite) a column with a constant.
    Constant {
        column: String,
        value: Value,
    },
    Text {
        column: String,
        func: TextFunc,
    },
    /// Replace NULL with a value.
    FillNull {
        column: String,
        value: Value,
    },
    /// Write the text of `inputs` joined by `separator` into `column` (NULL inputs are skipped).
    Concat {
        column: String,
        inputs: Vec<String>,
        #[serde(default)]
        separator: String,
    },
    /// Drop rows whose key columns were already seen in this run (bounded key set).
    Dedupe {
        columns: Vec<String>,
    },
}

impl Pipeline {
    /// Structural checks that do not need a connection. Column names are checked by `transform::compile`.
    pub fn validate(&self) -> Result<(), DbError> {
        let name = self.name.trim();
        if name.is_empty() || name.len() > MAX_NAME_BYTES {
            return Err(DbError::query("파이프라인 이름은 1..200바이트여야 합니다"));
        }
        if self.version != SPEC_VERSION {
            return Err(DbError::unsupported("지원하지 않는 파이프라인 버전입니다"));
        }
        match &self.source {
            Source::Query { profile_id, text } => {
                if profile_id.is_empty() {
                    return Err(DbError::query("소스 연결을 선택하세요"));
                }
                if text.trim().is_empty() || text.len() > MAX_QUERY_BYTES {
                    return Err(DbError::query(
                        "소스 쿼리는 1바이트 이상 256 KiB 이하여야 합니다",
                    ));
                }
            }
            Source::Table {
                profile_id,
                node_id,
            } => {
                if profile_id.is_empty() || node_id.is_empty() {
                    return Err(DbError::query("소스 연결과 테이블을 선택하세요"));
                }
            }
        }
        if self.target.profile_id.is_empty() || self.target.node_id.is_empty() {
            return Err(DbError::query("대상 연결과 테이블을 선택하세요"));
        }
        if self.steps.len() > MAX_STEPS {
            return Err(DbError::query("변환 단계는 64개 이하여야 합니다"));
        }
        let o = &self.options;
        if o.batch_size == 0 || o.batch_size > MAX_BATCH_ROWS {
            return Err(DbError::query("배치 크기는 1..10000행이어야 합니다"));
        }
        if o.max_rows == Some(0) {
            return Err(DbError::query("최대 행 수는 1 이상이어야 합니다"));
        }
        if o.batch_timeout_ms == 0 || o.batch_timeout_ms > 3_600_000 {
            return Err(DbError::query("배치 제한 시간은 1ms..1시간이어야 합니다"));
        }
        for (index, step) in self.steps.iter().enumerate() {
            if let Step::Filter { cmp, value, .. } = step
                && cmp.needs_value()
                && value.is_none()
            {
                return Err(DbError::query(format!(
                    "단계 {}: 비교할 값이 필요합니다",
                    index + 1
                )));
            }
        }
        Ok(())
    }
    /// Rows the source may produce; query sources never exceed `MAX_QUERY_ROWS`.
    pub fn read_limit(&self) -> Option<u64> {
        match self.source {
            Source::Query { .. } => Some(
                self.options
                    .max_rows
                    .map_or(MAX_QUERY_ROWS, |m| m.min(MAX_QUERY_ROWS)),
            ),
            Source::Table { .. } => self.options.max_rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Pipeline {
        serde_json::from_value(serde_json::json!({
            "name": "copy users",
            "source": {"kind": "query", "profile_id": "a", "text": "select * from users"},
            "steps": [{"op": "filter", "column": "age", "cmp": "ge", "value": {"t": "Int", "v": "18"}}],
            "target": {"profile_id": "b", "node_id": "table:users"}
        }))
        .unwrap()
    }
    #[test]
    fn defaults_and_validation() {
        let p = sample();
        assert_eq!(p.options.batch_size, DEFAULT_BATCH_ROWS);
        assert_eq!(p.version, SPEC_VERSION);
        p.validate().unwrap();
        assert_eq!(p.read_limit(), Some(MAX_QUERY_ROWS));
        let mut bad = p.clone();
        bad.options.batch_size = 10_001;
        assert!(bad.validate().is_err());
        let mut bad = p.clone();
        bad.steps = vec![Step::Filter {
            column: "age".into(),
            cmp: Cmp::Eq,
            value: None,
        }];
        assert!(bad.validate().is_err());
        let mut bad = p;
        bad.name = " ".into();
        assert!(bad.validate().is_err());
    }
    #[test]
    fn rejects_boundary_values() {
        let reject = |edit: &dyn Fn(&mut Pipeline)| {
            let mut p = sample();
            edit(&mut p);
            assert!(p.validate().is_err(), "{p:?}");
        };
        reject(&|p| p.options.batch_size = 0);
        reject(&|p| p.options.max_rows = Some(0));
        reject(&|p| p.options.batch_timeout_ms = 0);
        reject(&|p| p.options.batch_timeout_ms = 3_600_001);
        reject(&|p| p.version = SPEC_VERSION + 1);
        reject(&|p| p.name = "x".repeat(MAX_NAME_BYTES + 1));
        reject(&|p| p.target.node_id.clear());
        reject(&|p| {
            p.source = Source::Query {
                profile_id: "a".into(),
                text: "  ".into(),
            }
        });
        reject(&|p| {
            p.source = Source::Table {
                profile_id: "a".into(),
                node_id: String::new(),
            }
        });
        reject(&|p| p.steps = vec![Step::Drop { columns: vec![] }; MAX_STEPS + 1]);
        let mut ok = sample();
        ok.options.batch_size = MAX_BATCH_ROWS;
        ok.steps = vec![Step::Filter {
            column: "x".into(),
            cmp: Cmp::IsNull,
            value: None,
        }];
        ok.validate().unwrap();
    }
    #[test]
    fn read_limit_caps_only_query_sources() {
        let mut p = sample();
        p.options.max_rows = Some(500);
        assert_eq!(p.read_limit(), Some(500));
        p.options.max_rows = Some(MAX_QUERY_ROWS * 2);
        assert_eq!(p.read_limit(), Some(MAX_QUERY_ROWS));
        p.source = Source::Table {
            profile_id: "a".into(),
            node_id: "t".into(),
        };
        assert_eq!(p.read_limit(), Some(MAX_QUERY_ROWS * 2));
        p.options.max_rows = None;
        assert_eq!(p.read_limit(), None);
        assert_eq!(p.source.profile_id(), "a");
    }
    #[test]
    fn round_trips_json() {
        let p = sample();
        let text = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Pipeline>(&text).unwrap(), p);
        assert!(text.contains("\"op\":\"filter\""));
    }
}
