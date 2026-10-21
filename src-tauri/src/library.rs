//! Bounded, on-demand SQL library. No worker thread, timer, or async-runtime I/O:
//! every operation leases the same connection inside `spawn_blocking`.
use crate::storage::{HistoryEntry, HistorySource};
use dbcore::DbError;
use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const PAGE_MAX: usize = 200;
// At most 4 KiB of UTF-8; SQLite slices before copying values into Rust.
const SHORT_SCAN_ROWS: usize = 5_000;
const KEEP_ROWS: u64 = 50_000;
const KEEP_DAYS: i64 = 180;
const PRUNE_EVERY: u64 = 256;
const PRUNE_BATCH: u64 = 512;
const HISTORY_INSERT_TRIGGER: &str = "CREATE TRIGGER IF NOT EXISTS history_ai AFTER INSERT ON history BEGIN INSERT INTO history_fts(rowid,text) VALUES(new.id,new.text); END;";
const INSERT_HISTORY: &str = "INSERT INTO history(ts,kind,source,profile_id,profile_name,text,elapsed_ms,row_count,error) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)";
const HISTORY_COLUMNS: &str = "h.id,h.ts,h.kind,h.source,h.profile_id,h.profile_name,substr(h.text,1,1024),h.elapsed_ms,h.row_count,substr(h.error,1,1024)";
const SNIPPET_COLUMNS: &str = "s.id,s.title,substr(s.body,1,1024),s.tags,s.profile_id,s.language,s.pinned,s.created_at,s.updated_at";

fn database_error(error: rusqlite::Error) -> DbError {
    // SQLite errors may include SQL text. Do not surface or log persisted content.
    let _ = error;
    DbError::internal("SQL library operation failed")
}
fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

#[derive(Clone, Serialize)]
pub struct HistoryRecord {
    pub id: i64,
    #[serde(flatten)]
    pub entry: HistoryEntry,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight: Option<String>,
}
impl std::ops::Deref for HistoryRecord {
    type Target = HistoryEntry;
    fn deref(&self) -> &Self::Target {
        &self.entry
    }
}
#[derive(Serialize)]
pub struct HistorySearchResult {
    pub items: Vec<HistoryRecord>,
    pub next_cursor: Option<i64>,
}
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryStatus {
    #[default]
    All,
    Ok,
    Error,
}
#[derive(Default)]
pub struct HistoryFilter {
    pub query: Option<String>,
    pub profile_id: Option<String>,
    pub status: HistoryStatus,
    pub kind: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub before_id: Option<i64>,
    pub limit: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snippet {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub profile_id: Option<String>,
    pub language: String,
    pub pinned: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<String>,
}
#[derive(Serialize)]
pub struct SnippetListResult {
    pub items: Vec<Snippet>,
    pub next_cursor: Option<String>,
}
impl std::ops::Deref for SnippetListResult {
    type Target = [Snippet];
    fn deref(&self) -> &Self::Target {
        &self.items
    }
}
#[derive(Default)]
pub struct SnippetFilter {
    pub query: Option<String>,
    pub tag: Option<String>,
    pub profile_id: Option<String>,
    pub before_id: Option<SnippetCursorInput>,
    pub limit: usize,
}
#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub enum SnippetCursorInput {
    Opaque(String),
    Legacy(i64),
}
impl From<String> for SnippetCursorInput {
    fn from(value: String) -> Self {
        Self::Opaque(value)
    }
}
#[derive(Serialize, Deserialize)]
struct SnippetCursor {
    snapshot: i64,
    pinned: bool,
    updated_at: String,
    id: i64,
}
impl SnippetCursor {
    fn encode(&self) -> Result<String, DbError> {
        serde_json::to_vec(self)
            .map(|bytes| bytes.iter().map(|byte| format!("{byte:02x}")).collect())
            .map_err(|_| DbError::query("Invalid snippet cursor"))
    }
    fn decode(input: SnippetCursorInput) -> Result<Self, DbError> {
        let SnippetCursorInput::Opaque(value) = input else {
            return Err(DbError::query(
                "Legacy snippet cursor is unsupported; restart pagination",
            ));
        };
        let invalid = || DbError::query("Invalid snippet cursor");
        if value.len() > 1024 || value.len() % 2 != 0 {
            return Err(invalid());
        }
        let bytes = value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let pair = std::str::from_utf8(pair).map_err(|_| invalid())?;
                u8::from_str_radix(pair, 16).map_err(|_| invalid())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let cursor: Self = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if cursor.snapshot < 0
            || cursor.id <= 0
            || time::OffsetDateTime::parse(
                &cursor.updated_at,
                &time::format_description::well_known::Rfc3339,
            )
            .is_err()
        {
            return Err(invalid());
        }
        Ok(cursor)
    }
}
#[derive(Deserialize)]
pub struct SnippetSave {
    pub id: Option<i64>,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub profile_id: Option<String>,
    #[serde(default = "sql_language")]
    pub language: String,
    #[serde(default)]
    pub pinned: bool,
}
fn sql_language() -> String {
    "sql".into()
}

/// A single private connection, lazily opened on a blocking thread. Keeping the
/// mutex here (rather than Host's settings mutex) isolates unrelated persistence.
#[derive(Clone)]
pub struct Library {
    directory: PathBuf,
    store: Arc<Mutex<Option<Store>>>,
    gate: Arc<tokio::sync::Semaphore>,
}
impl Library {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            store: Arc::default(),
            gate: Arc::new(tokio::sync::Semaphore::new(1)),
        }
    }
    pub async fn run<T, F>(&self, operation: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Store) -> Result<T, DbError> + Send + 'static,
    {
        // Queue asynchronously before occupying a blocking thread. Concurrent
        // callers cannot grow a pool of OS threads waiting on one connection.
        let permit = self
            .gate
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| DbError::internal("SQL library queue unavailable"))?;
        let library = self.clone();
        tokio::task::spawn_blocking(move || {
            // Cancellation of the caller must not release the lease while the
            // blocking job is still using the connection.
            let _permit = permit;
            let mut guard = library
                .store
                .lock()
                .map_err(|_| DbError::internal("SQL library lock unavailable"))?;
            if guard.is_none() {
                *guard = Some(Store::open(&library.directory)?);
            }
            let Some(store) = guard.as_mut() else {
                return Err(DbError::internal("SQL library unavailable"));
            };
            operation(store)
        })
        .await
        .map_err(|_| DbError::internal("SQL library task failed"))?
    }
}

pub struct Store {
    connection: Connection,
    rows: u64,
    writes: u64,
    keep_rows: u64,
    keep_days: i64,
}
impl Store {
    pub fn open(directory: &Path) -> Result<Self, DbError> {
        std::fs::create_dir_all(directory)
            .map_err(|_| DbError::internal("Cannot create SQL library directory"))?;
        let path = directory.join("library.sqlite3");
        // Create before SQLite opens it so neither DB nor WAL is ever world-readable.
        let file = crate::storage::open_private_log(&path)
            .map_err(|_| DbError::internal("Cannot create SQL library"))?;
        drop(file);
        let connection = Connection::open(&path).map_err(database_error)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(database_error)?;
        connection.set_prepared_statement_cache_capacity(32);
        // A 1.5 MiB page cache bounds RSS. Checkpoint at 4 MiB of WAL and merge
        // FTS segments eight at a time, reducing work on small write transactions
        // while retaining incremental merges (and fast searches).
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;
            PRAGMA cache_size=-1536; PRAGMA mmap_size=0; PRAGMA temp_store=FILE;
            PRAGMA wal_autocheckpoint=1024;
            CREATE TABLE IF NOT EXISTS library_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS history(
                id INTEGER PRIMARY KEY AUTOINCREMENT, ts TEXT NOT NULL, kind TEXT NOT NULL,
                source TEXT NOT NULL, profile_id TEXT, profile_name TEXT NOT NULL,
                text TEXT NOT NULL, elapsed_ms INTEGER NOT NULL, row_count INTEGER NOT NULL, error TEXT);
            CREATE INDEX IF NOT EXISTS history_ts ON history(ts DESC);
            CREATE INDEX IF NOT EXISTS history_profile_ts ON history(profile_id,ts DESC);
            CREATE INDEX IF NOT EXISTS history_profile_id ON history(profile_id,id DESC);
            CREATE TABLE IF NOT EXISTS snippets(
                id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, body TEXT NOT NULL,
                tags TEXT NOT NULL, profile_id TEXT, language TEXT NOT NULL, pinned INTEGER NOT NULL,
                created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS snippets_profile_id ON snippets(profile_id,id DESC);
            CREATE INDEX IF NOT EXISTS snippets_pinned_updated ON snippets(pinned DESC,updated_at DESC,id DESC);
            CREATE TABLE IF NOT EXISTS snippet_order(
                revision INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL,
                pinned INTEGER NOT NULL, updated_at TEXT NOT NULL, valid_until INTEGER);
            CREATE INDEX IF NOT EXISTS snippet_order_sort ON snippet_order(pinned DESC,updated_at DESC,id DESC);
            CREATE INDEX IF NOT EXISTS snippet_order_current ON snippet_order(id) WHERE valid_until IS NULL;
            INSERT INTO snippet_order(id,pinned,updated_at)
                SELECT id,pinned,updated_at FROM snippets s
                WHERE NOT EXISTS(SELECT 1 FROM snippet_order o WHERE o.id=s.id AND o.valid_until IS NULL);
            CREATE TRIGGER IF NOT EXISTS snippet_order_ai AFTER INSERT ON snippets BEGIN
                INSERT INTO snippet_order(id,pinned,updated_at) VALUES(new.id,new.pinned,new.updated_at); END;
            CREATE TRIGGER IF NOT EXISTS snippet_order_au AFTER UPDATE ON snippets BEGIN
                INSERT INTO snippet_order(id,pinned,updated_at) VALUES(new.id,new.pinned,new.updated_at);
                UPDATE snippet_order SET valid_until=(SELECT max(revision) FROM snippet_order)
                    WHERE id=new.id AND valid_until IS NULL AND revision<(SELECT max(revision) FROM snippet_order); END;
            CREATE VIRTUAL TABLE IF NOT EXISTS history_fts USING fts5(
                text, content=history, content_rowid=id, tokenize='trigram');
            CREATE VIRTUAL TABLE IF NOT EXISTS snippets_fts USING fts5(
                title,body,tags, content=snippets, content_rowid=id, tokenize='trigram');
            INSERT INTO history_fts(history_fts,rank) VALUES('automerge',8),('crisismerge',32);
            INSERT INTO snippets_fts(snippets_fts,rank) VALUES('automerge',8),('crisismerge',32);
            CREATE TRIGGER IF NOT EXISTS history_ad AFTER DELETE ON history BEGIN
                INSERT INTO history_fts(history_fts,rowid,text) VALUES('delete',old.id,old.text); END;
            CREATE TRIGGER IF NOT EXISTS history_au AFTER UPDATE ON history BEGIN
                INSERT INTO history_fts(history_fts,rowid,text) VALUES('delete',old.id,old.text);
                INSERT INTO history_fts(rowid,text) VALUES(new.id,new.text); END;
            CREATE TRIGGER IF NOT EXISTS snippets_ai AFTER INSERT ON snippets BEGIN
                INSERT INTO snippets_fts(rowid,title,body,tags) VALUES(new.id,new.title,new.body,new.tags); END;
            CREATE TRIGGER IF NOT EXISTS snippets_ad AFTER DELETE ON snippets BEGIN
                INSERT INTO snippets_fts(snippets_fts,rowid,title,body,tags) VALUES('delete',old.id,old.title,old.body,old.tags); END;
            CREATE TRIGGER IF NOT EXISTS snippets_au AFTER UPDATE ON snippets BEGIN
                INSERT INTO snippets_fts(snippets_fts,rowid,title,body,tags) VALUES('delete',old.id,old.title,old.body,old.tags);
                INSERT INTO snippets_fts(rowid,title,body,tags) VALUES(new.id,new.title,new.body,new.tags); END;")
            .map_err(database_error)?;
        connection
            .execute_batch(HISTORY_INSERT_TRIGGER)
            .map_err(database_error)?;
        let mut store = Self {
            connection,
            rows: 0,
            writes: 0,
            keep_rows: KEEP_ROWS,
            keep_days: KEEP_DAYS,
        };
        store.migrate(&directory.join("history.jsonl"))?;
        store.rows = store
            .connection
            .query_row("SELECT count(*) FROM history", [], |r| r.get::<_, i64>(0))
            .map_err(database_error)? as u64;
        // Age retention also advances at first use without an idle timer.
        store.prune()?;
        // Startup may import an older installation with more than the retention limit.
        while store.rows > store.keep_rows {
            store.prune()?;
        }
        Ok(store)
    }

    fn migrate(&mut self, path: &Path) -> Result<(), DbError> {
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(DbError::internal("Cannot read legacy query history")),
        };
        let migrated = self
            .connection
            .query_row(
                "SELECT value FROM library_meta WHERE key='jsonl_migrated'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(database_error)?
            .is_some();
        if !migrated {
            let transaction = self.connection.transaction().map_err(database_error)?;
            // External-content rebuild is much cheaper than firing an FTS VM
            // program for every imported row. DDL, import, index, and marker all
            // commit together, so a crash restores the original trigger/index.
            transaction
                .execute_batch("DROP TRIGGER history_ai")
                .map_err(database_error)?;
            let mut corrupt = 0u64;
            {
                let mut insert = transaction
                    .prepare_cached(INSERT_HISTORY)
                    .map_err(database_error)?;
                let mut reader = BufReader::new(file);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => match serde_json::from_str::<HistoryEntry>(&line) {
                            Ok(entry) => {
                                insert_entry(&mut insert, &entry).map_err(database_error)?;
                            }
                            Err(_) => corrupt += 1,
                        },
                        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                            corrupt += 1;
                        }
                        Err(_) => {
                            return Err(DbError::internal("Cannot read legacy query history"));
                        }
                    }
                }
            }
            transaction
                .execute("INSERT INTO history_fts(history_fts) VALUES('rebuild')", [])
                .map_err(database_error)?;
            transaction
                .execute_batch(HISTORY_INSERT_TRIGGER)
                .map_err(database_error)?;
            transaction
                .execute(
                    "INSERT INTO library_meta(key,value) VALUES('jsonl_migrated','1')",
                    [],
                )
                .map_err(database_error)?;
            transaction.commit().map_err(database_error)?;
            if corrupt > 0 {
                tracing::warn!(
                    corrupt_lines = corrupt,
                    "Skipped corrupt legacy history lines"
                );
            }
        }
        // A committed marker makes retry after a crash/failed rename idempotent.
        // Never overwrite a pre-existing backup.
        let backup = path.with_file_name("history.jsonl.migrated");
        if !backup.exists() {
            std::fs::rename(path, backup)
                .map_err(|_| DbError::internal("Cannot archive migrated query history"))?;
        }
        Ok(())
    }

    pub fn insert(&mut self, entry: &HistoryEntry) -> Result<i64, DbError> {
        self.connection
            .prepare_cached(INSERT_HISTORY)
            .and_then(|mut statement| insert_entry(&mut statement, entry))
            .map_err(database_error)?;
        let id = self.connection.last_insert_rowid();
        self.rows += 1;
        self.writes += 1;
        if self.writes % PRUNE_EVERY == 0 {
            self.prune()?;
        }
        Ok(id)
    }
    fn prune(&mut self) -> Result<(), DbError> {
        let transaction = self.connection.transaction().map_err(database_error)?;
        let excess = self.rows.saturating_sub(self.keep_rows).min(PRUNE_BATCH);
        let mut deleted = 0;
        if excess > 0 {
            deleted += transaction
                .execute(
                    "DELETE FROM history WHERE id IN (SELECT id FROM history ORDER BY id LIMIT ?1)",
                    [excess as i64],
                )
                .map_err(database_error)?;
        }
        if self.keep_days > 0 {
            let cutoff = (time::OffsetDateTime::now_utc() - time::Duration::days(self.keep_days))
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default();
            deleted += transaction.execute("DELETE FROM history WHERE id IN (SELECT id FROM history WHERE ts < ?1 ORDER BY ts LIMIT ?2)", params![cutoff, PRUNE_BATCH as i64]).map_err(database_error)?;
        }
        transaction.commit().map_err(database_error)?;
        self.rows = self.rows.saturating_sub(deleted as u64);
        Ok(())
    }
    pub fn history_list(&self, limit: usize) -> Result<Vec<HistoryRecord>, DbError> {
        Ok(self
            .history_search(HistoryFilter {
                limit,
                ..Default::default()
            })?
            .items)
    }
    pub fn history_search(&self, filter: HistoryFilter) -> Result<HistorySearchResult, DbError> {
        let (since, until) =
            validate_history_range(filter.since.as_deref(), filter.until.as_deref())?;
        let ranged = filter.since.is_some() || filter.until.is_some();
        let history_source = if ranged {
            if filter.profile_id.is_some() {
                "history h INDEXED BY history_profile_ts"
            } else {
                "history h INDEXED BY history_ts"
            }
        } else {
            "history h"
        };
        let query = search_query(filter.query.as_deref())?;
        let short = query.as_ref().is_some_and(|q| q.fts.is_none());
        let limit = filter.limit.min(PAGE_MAX);
        if limit == 0 {
            return Ok(HistorySearchResult {
                items: Vec::new(),
                next_cursor: None,
            });
        }
        let mut clause = String::from(" WHERE 1=1");
        let mut values = Vec::new();
        if let Some(id) = filter.before_id {
            condition(&mut clause, &mut values, "h.id < ?", id.into());
        }
        if let Some(profile) = filter.profile_id {
            condition(&mut clause, &mut values, "h.profile_id = ?", profile.into());
        }
        if let Some(since) = since {
            history_date_condition(&mut clause, &mut values, since, true)?;
        }
        if let Some(until) = until {
            history_date_condition(&mut clause, &mut values, until, false)?;
        }
        if let Some(kind) = filter.kind {
            condition(&mut clause, &mut values, "h.kind = ?", kind.into());
        }
        match filter.status {
            HistoryStatus::All => (),
            HistoryStatus::Ok => clause.push_str(" AND h.error IS NULL"),
            HistoryStatus::Error => clause.push_str(" AND h.error IS NOT NULL"),
        }
        let mut scan_cursor = None;
        let sql = if short || (ranged && query.is_some()) {
            // Trigrams cannot index one/two Unicode characters. Walk a bounded
            // candidate window and return its boundary even when there are no hits,
            // so callers can continue searching older windows without gaps.
            let candidates = format!(
                "SELECT h.id FROM {history_source}{clause} ORDER BY h.id DESC LIMIT {SHORT_SCAN_ROWS}"
            );
            let (boundary, count): (Option<i64>, i64) = self
                .connection
                .prepare_cached(&format!("SELECT min(id),count(*) FROM ({candidates})"))
                .and_then(|mut s| {
                    s.query_row(params_from_iter(values.iter()), |r| {
                        Ok((r.get(0)?, r.get(1)?))
                    })
                })
                .map_err(database_error)?;
            if count == SHORT_SCAN_ROWS as i64 {
                scan_cursor = boundary;
            }
            let n = values.len() + 1;
            let search = query
                .as_ref()
                .ok_or_else(|| DbError::query("Invalid search query"))?;
            if short {
                let columns = HISTORY_COLUMNS.replace(
                    "substr(h.text,1,1024)",
                    &format!("substr(h.text,max(1,instr(lower(h.text),lower(?{n}))-64),1024)"),
                );
                values.push(search.literal.clone().into());
                format!(
                    "WITH candidates AS MATERIALIZED ({candidates}) SELECT {columns} FROM history h JOIN candidates c ON c.id=h.id WHERE instr(lower(h.text),lower(?{n}))>0 ORDER BY h.id DESC LIMIT ?{}",
                    n + 1
                )
            } else {
                values.push(
                    search
                        .fts
                        .clone()
                        .ok_or_else(|| DbError::query("Invalid search query"))?
                        .into(),
                );
                // Unary + prevents SQLite from turning membership into 5000
                // separate FTS rowid/MATCH evaluations. Traverse the bounded
                // FTS rowid range once, probing the materialized candidate set.
                format!(
                    "WITH candidates AS MATERIALIZED ({candidates}) SELECT {HISTORY_COLUMNS},substr(snippet(history_fts,0,'<mark>','</mark>',' … ',32),1,1024) FROM history_fts CROSS JOIN history h ON h.id=history_fts.rowid WHERE history_fts MATCH ?{n} AND +history_fts.rowid IN (SELECT id FROM candidates) AND history_fts.rowid >= (SELECT min(id) FROM candidates) AND history_fts.rowid <= (SELECT max(id) FROM candidates) ORDER BY history_fts.rowid DESC LIMIT ?{}",
                    n + 1
                )
            }
        } else if ranged {
            // Sort only indexed IDs before fetching response bodies. A broad
            // date range must not copy every matching SQL body into the sorter.
            format!(
                "WITH candidates AS MATERIALIZED (SELECT h.id FROM {history_source}{clause} ORDER BY h.id DESC LIMIT ?) SELECT {HISTORY_COLUMNS} FROM candidates c JOIN history h ON h.id=c.id ORDER BY h.id DESC"
            )
        } else {
            let mut sql = format!("SELECT {HISTORY_COLUMNS}");
            if let Some(query) = &query {
                sql.push_str(",substr(snippet(history_fts,0,'<mark>','</mark>',' … ',32),1,1024) FROM history h JOIN history_fts ON history_fts.rowid=h.id");
                sql.push_str(&clause);
                let Some(fts) = query.fts.clone() else {
                    return Err(DbError::internal("History search index query unavailable"));
                };
                condition(&mut sql, &mut values, "history_fts MATCH ?", fts.into());
                sql.push_str(" ORDER BY history_fts.rowid DESC LIMIT ?");
            } else {
                sql.push_str(&format!(" FROM {history_source}"));
                sql.push_str(&clause);
                sql.push_str(" ORDER BY h.id DESC LIMIT ?");
            }
            sql
        };
        values.push(((limit + 1) as i64).into());
        let mut statement = self
            .connection
            .prepare_cached(&sql)
            .map_err(database_error)?;
        let mut rows = statement
            .query(params_from_iter(values))
            .map_err(database_error)?;
        let mut items = Vec::with_capacity(limit + 1);
        while let Some(row) = rows.next().map_err(database_error)? {
            let mut item = history_row(row).map_err(database_error)?;
            if let Some(query) = query.as_ref() {
                item.highlight = Some(if short {
                    short_highlight(&item.text, &query.literal)
                } else {
                    row.get(10).map_err(database_error)?
                });
            }
            items.push(item);
        }
        let more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = if more {
            items.last().map(|i| i.id)
        } else {
            scan_cursor
        };
        Ok(HistorySearchResult { items, next_cursor })
    }
    pub fn history_get(&self, id: i64) -> Result<HistoryRecord, DbError> {
        self.connection.prepare_cached("SELECT id,ts,kind,source,profile_id,profile_name,text,elapsed_ms,row_count,error FROM history WHERE id=?1")
            .and_then(|mut s| s.query_row([id], history_row)).optional().map_err(database_error)?
            .ok_or_else(|| DbError::query("History entry not found"))
    }
    pub fn history_delete(
        &mut self,
        id: Option<i64>,
        timestamp: Option<String>,
        profile_name: Option<String>,
        text: Option<String>,
    ) -> Result<(), DbError> {
        let deleted = if let Some(id) = id {
            self.connection
                .prepare_cached("DELETE FROM history WHERE id=?1")
                .and_then(|mut s| s.execute([id]))
                .map_err(database_error)?
        } else if let (Some(timestamp), Some(name), Some(text)) = (timestamp, profile_name, text) {
            // Old clients send the visible SQL rather than an ID, including truncated list rows.
            self.connection.prepare_cached("DELETE FROM history WHERE ts=?1 AND profile_name=?2 AND (text=?3 OR substr(text,1,1024)=?3)")
                .and_then(|mut s| s.execute(params![timestamp,name,text])).map_err(database_error)?
        } else {
            return Err(DbError::query(
                "Provide history id or timestamp, profile_name and text",
            ));
        };
        self.rows = self.rows.saturating_sub(deleted as u64);
        Ok(())
    }
    pub fn history_clear(&mut self, profile_id: Option<String>) -> Result<(), DbError> {
        // Explicit clear may delete many rows, but never materializes them in Rust.
        let deleted = if let Some(profile) = profile_id {
            self.connection
                .prepare_cached("DELETE FROM history WHERE profile_id=?1")
                .and_then(|mut s| s.execute([profile]))
                .map_err(database_error)?
        } else {
            self.connection
                .execute("DELETE FROM history", [])
                .map_err(database_error)?
        };
        self.rows = self.rows.saturating_sub(deleted as u64);
        Ok(())
    }
    pub fn snippet_list(&self, filter: SnippetFilter) -> Result<SnippetListResult, DbError> {
        let cursor = filter.before_id.map(SnippetCursor::decode).transpose()?;
        let snapshot = match &cursor {
            Some(cursor) => cursor.snapshot,
            None => self
                .connection
                .query_row(
                    "SELECT coalesce(max(revision),0) FROM snippet_order",
                    [],
                    |r| r.get(0),
                )
                .map_err(database_error)?,
        };
        // A cursor freezes ordering, not content: every still-existing matching row
        // at the first page's revision is visited exactly once despite pin/unpin or
        // updated_at changes. New rows are excluded; deletions and changes to query,
        // tag/profile/content membership are not snapshot-isolated. Historical sort
        // keys are retained so cursors survive reopening without a live transaction.
        let limit = filter.limit.min(PAGE_MAX);
        let query = search_query(filter.query.as_deref())?;
        let short = query.as_ref().is_some_and(|q| q.fts.is_none());
        if limit == 0 {
            return Ok(SnippetListResult {
                items: Vec::new(),
                next_cursor: None,
            });
        }
        let mut clause = String::from(
            " WHERE o.revision <= ?1 AND (o.valid_until IS NULL OR o.valid_until > ?1)",
        );
        let mut values = vec![Value::Integer(snapshot)];
        if let Some(cursor) = cursor {
            clause.push_str(" AND (o.pinned,o.updated_at,o.id) < (?,?,?)");
            values.extend([
                Value::Integer(i64::from(cursor.pinned)),
                cursor.updated_at.into(),
                cursor.id.into(),
            ]);
        }
        if let Some(profile) = filter.profile_id {
            condition(&mut clause, &mut values, "s.profile_id = ?", profile.into());
        }
        if let Some(tag) = filter.tag {
            condition(
                &mut clause,
                &mut values,
                "EXISTS(SELECT 1 FROM json_each(s.tags) WHERE value=?)",
                tag.into(),
            );
        }
        let source = "snippet_order o INDEXED BY snippet_order_sort JOIN snippets s ON s.id=o.id";
        let order = "o.pinned DESC,o.updated_at DESC,o.id DESC";
        let columns = SNIPPET_COLUMNS.replace(
            "s.pinned,s.created_at,s.updated_at",
            "o.pinned,s.created_at,o.updated_at",
        );
        let mut scan_cursor = None;
        let sql = if short {
            let candidates = format!(
                "SELECT o.id,o.pinned,o.updated_at FROM {source}{clause} ORDER BY {order} LIMIT {SHORT_SCAN_ROWS}"
            );
            let mut statement = self
                .connection
                .prepare_cached(&candidates)
                .map_err(database_error)?;
            let mut rows = statement
                .query(params_from_iter(values.iter()))
                .map_err(database_error)?;
            let mut count = 0;
            let mut boundary = None;
            while let Some(row) = rows.next().map_err(database_error)? {
                count += 1;
                boundary = Some(SnippetCursor {
                    snapshot,
                    id: row.get(0).map_err(database_error)?,
                    pinned: row.get(1).map_err(database_error)?,
                    updated_at: row.get(2).map_err(database_error)?,
                });
            }
            if count == SHORT_SCAN_ROWS {
                scan_cursor = boundary.map(|cursor| cursor.encode()).transpose()?;
            }
            let n = values.len() + 1;
            let literal = query
                .as_ref()
                .ok_or_else(|| DbError::query("Invalid search query"))?
                .literal
                .clone();
            values.push(literal.into());
            let columns = columns.replace(
                "substr(s.body,1,1024)",
                &format!("substr(s.body,max(1,instr(lower(s.body),lower(?{n}))-64),1024)"),
            );
            format!(
                "WITH candidates AS MATERIALIZED ({candidates}) SELECT {columns} FROM candidates o JOIN snippets s ON s.id=o.id WHERE instr(lower(s.title),lower(?{n}))>0 OR instr(lower(s.body),lower(?{n}))>0 OR instr(lower(s.tags),lower(?{n}))>0 ORDER BY {order} LIMIT ?{}",
                n + 1
            )
        } else if let Some(query) = &query {
            let fts = query
                .fts
                .clone()
                .ok_or_else(|| DbError::query("Invalid search query"))?;
            condition(&mut clause, &mut values, "snippets_fts MATCH ?", fts.into());
            format!(
                "SELECT {columns},substr(snippet(snippets_fts,-1,'<mark>','</mark>',' … ',32),1,1024) FROM {source} JOIN snippets_fts ON snippets_fts.rowid=s.id{clause} ORDER BY {order} LIMIT ?"
            )
        } else {
            format!("SELECT {columns} FROM {source}{clause} ORDER BY {order} LIMIT ?")
        };
        values.push(((limit + 1) as i64).into());
        let mut statement = self
            .connection
            .prepare_cached(&sql)
            .map_err(database_error)?;
        let mut rows = statement
            .query(params_from_iter(values))
            .map_err(database_error)?;
        let mut items = Vec::with_capacity(limit + 1);
        while let Some(row) = rows.next().map_err(database_error)? {
            let mut item = snippet_row(row).map_err(database_error)?;
            if let Some(query) = &query {
                item.highlight = Some(if short {
                    short_highlight(&item.body, &query.literal)
                } else {
                    row.get(9).map_err(database_error)?
                });
            }
            items.push(item);
        }
        let more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = if more {
            items
                .last()
                .map(|item| {
                    SnippetCursor {
                        snapshot,
                        pinned: item.pinned,
                        updated_at: item.updated_at.clone(),
                        id: item.id,
                    }
                    .encode()
                })
                .transpose()?
        } else {
            scan_cursor
        };
        Ok(SnippetListResult { items, next_cursor })
    }
    pub fn snippet_get(&self, id: i64) -> Result<Snippet, DbError> {
        self.connection.prepare_cached("SELECT id,title,body,tags,profile_id,language,pinned,created_at,updated_at FROM snippets WHERE id=?1")
            .and_then(|mut s| s.query_row([id], snippet_row)).optional().map_err(database_error)?
            .ok_or_else(|| DbError::query("Snippet not found"))
    }
    pub fn snippet_save(&mut self, input: SnippetSave) -> Result<Snippet, DbError> {
        if input.title.trim().is_empty()
            || input.title.len() > 4096
            || input.language.len() > 128
            || input.tags.len() > 100
            || input.tags.iter().any(|t| t.len() > 256)
        {
            return Err(DbError::query("Invalid snippet title, language or tags"));
        }
        let tags = serde_json::to_string(&input.tags)
            .map_err(|_| DbError::query("Invalid snippet tags"))?;
        let timestamp = now();
        let id = if let Some(id) = input.id {
            let changed = self.connection.prepare_cached("UPDATE snippets SET title=?1,body=?2,tags=?3,profile_id=?4,language=?5,pinned=?6,updated_at=?7 WHERE id=?8")
                .and_then(|mut s| s.execute(params![input.title,input.body,tags,input.profile_id,input.language,input.pinned,timestamp,id])).map_err(database_error)?;
            if changed == 0 {
                return Err(DbError::query("Snippet not found"));
            }
            id
        } else {
            self.connection.prepare_cached("INSERT INTO snippets(title,body,tags,profile_id,language,pinned,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)")
                .and_then(|mut s| s.execute(params![input.title,input.body,tags,input.profile_id,input.language,input.pinned,timestamp])).map_err(database_error)?;
            self.connection.last_insert_rowid()
        };
        self.snippet_get(id)
    }
    pub fn snippet_delete(&mut self, id: i64) -> Result<(), DbError> {
        self.connection
            .prepare_cached("DELETE FROM snippets WHERE id=?1")
            .and_then(|mut s| s.execute([id]))
            .map_err(database_error)?;
        Ok(())
    }
}
fn insert_entry(
    statement: &mut rusqlite::Statement<'_>,
    entry: &HistoryEntry,
) -> rusqlite::Result<usize> {
    statement.execute(params![
        entry.timestamp,
        entry.kind,
        match entry.source {
            HistorySource::Ui => "ui",
            HistorySource::Agent => "agent",
        },
        entry.profile_id,
        entry.profile_name,
        entry.text,
        entry.elapsed_ms.min(i64::MAX as u64) as i64,
        entry.row_count.min(i64::MAX as u64) as i64,
        entry.error
    ])
}
fn history_row(row: &Row<'_>) -> rusqlite::Result<HistoryRecord> {
    Ok(HistoryRecord {
        id: row.get(0)?,
        entry: HistoryEntry {
            timestamp: row.get(1)?,
            kind: row.get(2)?,
            source: if row.get::<_, String>(3)? == "agent" {
                HistorySource::Agent
            } else {
                HistorySource::Ui
            },
            profile_id: row.get(4)?,
            profile_name: row.get(5)?,
            text: row.get(6)?,
            elapsed_ms: row.get::<_, i64>(7)?.max(0) as u64,
            row_count: row.get::<_, i64>(8)?.max(0) as u64,
            error: row.get(9)?,
        },
        highlight: None,
    })
}
fn snippet_row(row: &Row<'_>) -> rusqlite::Result<Snippet> {
    let tags: String = row.get(3)?;
    Ok(Snippet {
        id: row.get(0)?,
        title: row.get(1)?,
        body: row.get(2)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        profile_id: row.get(4)?,
        language: row.get(5)?,
        pinned: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        highlight: None,
    })
}
type HistoryRange = (Option<time::OffsetDateTime>, Option<time::OffsetDateTime>);
fn validate_history_range(
    since: Option<&str>,
    until: Option<&str>,
) -> Result<HistoryRange, DbError> {
    let parse = |value: &str| {
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
            .map(|value| value.to_offset(time::UtcOffset::UTC))
            .map_err(|_| DbError::query("Invalid history timestamp; expected RFC3339"))
    };
    let since = since.map(parse).transpose()?;
    let until = until.map(parse).transpose()?;
    if matches!((since, until), (Some(since), Some(until)) if since > until) {
        return Err(DbError::query(
            "Invalid history date range: since must not exceed until",
        ));
    }
    Ok((since, until))
}
fn history_date_condition(
    sql: &mut String,
    values: &mut Vec<Value>,
    value: time::OffsetDateTime,
    inclusive: bool,
) -> Result<(), DbError> {
    let invalid = || DbError::query("Invalid history timestamp; expected RFC3339");
    let formatted = value
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|_| invalid())?;
    let second = formatted.get(..19).ok_or_else(invalid)?.to_owned();
    let next = value
        .checked_add(time::Duration::seconds(1))
        .ok_or_else(invalid)?
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|_| invalid())?;
    let next_second = next.get(..19).ok_or_else(invalid)?.to_owned();
    let precise = format!("{second}.{:09}", value.nanosecond());
    // History writes use UTC RFC3339. Keep the existing ts index sargable with
    // whole-second bounds; pad fractions only at the boundary, because RFC3339
    // permits both `...00Z` and variable-width fractional seconds. Compare all
    // nine digits so sub-millisecond boundaries retain their exact semantics.
    let normalized = "(substr(h.ts,1,19)||'.'||substr(CASE WHEN substr(h.ts,20,1)='.' THEN substr(h.ts,21,length(h.ts)-21) ELSE '' END||'000000000',1,9))";
    let n = values.len() + 1;
    if inclusive {
        sql.push_str(&format!(
            " AND h.ts >= ?{n} AND (h.ts >= ?{} OR {normalized} >= ?{})",
            n + 1,
            n + 2
        ));
    } else {
        sql.push_str(&format!(
            " AND h.ts < ?{} AND (h.ts < ?{n} OR {normalized} < ?{})",
            n + 1,
            n + 2
        ));
    }
    values.extend([second.into(), next_second.into(), precise.into()]);
    Ok(())
}
fn condition(sql: &mut String, values: &mut Vec<Value>, predicate: &str, value: Value) {
    sql.push_str(" AND ");
    sql.push_str(predicate);
    values.push(value);
}
struct SearchQuery {
    literal: String,
    fts: Option<String>,
}
/// Quote user input as a literal trigram phrase, never as an FTS expression.
fn search_query(query: Option<&str>) -> Result<Option<SearchQuery>, DbError> {
    let Some(query) = query.map(str::trim).filter(|q| !q.is_empty()) else {
        return Ok(None);
    };
    if query.len() > 1024 {
        return Err(DbError::query("Search query is too long"));
    }
    Ok(Some(SearchQuery {
        literal: query.into(),
        fts: (query.chars().count() >= 3).then(|| format!("\"{}\"", query.replace('"', "\"\""))),
    }))
}
fn short_highlight(text: &str, query: &str) -> String {
    // Bound expansion too (a one-character term can match every character).
    text.replace(query, &format!("<mark>{query}</mark>"))
        .chars()
        .take(1024)
        .collect()
}

// New commands use snake_case argument names, matching their public contract.
#[tauri::command(rename_all = "snake_case")]
pub async fn history_search(
    host: tauri::State<'_, crate::Host>,
    query: Option<String>,
    profile_id: Option<String>,
    status: Option<HistoryStatus>,
    kind: Option<String>,
    since: Option<String>,
    until: Option<String>,
    before_id: Option<i64>,
    limit: Option<usize>,
) -> Result<HistorySearchResult, DbError> {
    host.library
        .run(move |s| {
            s.history_search(HistoryFilter {
                query,
                profile_id,
                status: status.unwrap_or_default(),
                kind,
                since,
                until,
                before_id,
                limit: limit.unwrap_or(100),
            })
        })
        .await
}
#[tauri::command]
pub async fn history_get(
    host: tauri::State<'_, crate::Host>,
    id: i64,
) -> Result<HistoryRecord, DbError> {
    host.library.run(move |s| s.history_get(id)).await
}
#[tauri::command(rename_all = "snake_case")]
pub async fn history_clear(
    host: tauri::State<'_, crate::Host>,
    profile_id: Option<String>,
) -> Result<(), DbError> {
    host.library.run(move |s| s.history_clear(profile_id)).await
}
#[tauri::command(rename_all = "snake_case")]
pub async fn snippet_list(
    host: tauri::State<'_, crate::Host>,
    query: Option<String>,
    tag: Option<String>,
    profile_id: Option<String>,
    before_id: Option<SnippetCursorInput>,
    limit: Option<usize>,
) -> Result<SnippetListResult, DbError> {
    host.library
        .run(move |s| {
            s.snippet_list(SnippetFilter {
                query,
                tag,
                profile_id,
                before_id,
                limit: limit.unwrap_or(100),
            })
        })
        .await
}
#[tauri::command(rename_all = "snake_case")]
pub async fn snippet_save(
    host: tauri::State<'_, crate::Host>,
    id: Option<i64>,
    title: String,
    body: String,
    tags: Option<Vec<String>>,
    profile_id: Option<String>,
    language: Option<String>,
    pinned: Option<bool>,
) -> Result<Snippet, DbError> {
    host.library
        .run(move |s| {
            s.snippet_save(SnippetSave {
                id,
                title,
                body,
                tags: tags.unwrap_or_default(),
                profile_id,
                language: language.unwrap_or_else(sql_language),
                pinned: pinned.unwrap_or(false),
            })
        })
        .await
}
#[tauri::command]
pub async fn snippet_delete(host: tauri::State<'_, crate::Host>, id: i64) -> Result<(), DbError> {
    host.library.run(move |s| s.snippet_delete(id)).await
}
#[tauri::command]
pub async fn snippet_get(host: tauri::State<'_, crate::Host>, id: i64) -> Result<Snippet, DbError> {
    host.library.run(move |s| s.snippet_get(id)).await
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;
