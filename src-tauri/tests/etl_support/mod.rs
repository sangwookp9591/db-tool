//! Shared helpers for the ETL integration tests: drivers (in-process or AXDB_DRIVER_DIR plugin
//! processes) and minimal Extractor/Loader adapters over `dyn Session`, mirroring the host.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::print_stdout, clippy::print_stderr)]
use async_trait::async_trait;
use axdb_etl::engine::{Batch, Extractor, LoadOutput, LoadRequest, Loader};
use dbcore::*;
use std::collections::BTreeMap;

pub async fn driver(id: &str) -> Box<dyn Driver> {
    if let Some(path) = std::env::var_os("AXDB_DRIVER_DIR") {
        let exe = std::path::PathBuf::from(path).join(format!("axdb-driver-{id}"));
        return Box::new(axdb::plugin_host::PluginDriver::load(exe).await.unwrap());
    }
    match id {
        "postgres" => Box::new(driver_postgres::PostgresDriver::new()),
        "sqlite" => Box::new(driver_sqlite::SqliteDriver::new()),
        _ => unreachable!(),
    }
}
pub fn req(sql: &str) -> ExecRequest {
    serde_json::from_value(serde_json::json!({"text": sql, "row_limit": 100000, "timeout_ms": 600000})).unwrap()
}
pub async fn exec(s: &dyn Session, sql: &str) -> Vec<ResultSet> {
    s.execute(&req(sql), CancelToken::new()).await.unwrap()
}
pub async fn scalar(s: &dyn Session, sql: &str) -> String {
    let sets = exec(s, sql).await;
    match &sets[0].table.as_ref().unwrap().rows[0][0] {
        Value::Int(v) | Value::Text(v) | Value::Decimal(v) => v.clone(),
        other => format!("{other:?}"),
    }
}
pub async fn sqlite(path: &std::path::Path) -> Box<dyn Session> {
    let cfg = ConnectConfig {
        fields: BTreeMap::from([("path".into(), path.to_string_lossy().into_owned())]),
        secret: None,
        connect_timeout_ms: 5000,
    };
    driver("sqlite").await.connect(&cfg).await.unwrap()
}
pub fn pg_config(database: &str) -> Option<ConnectConfig> {
    let dsn = std::env::var("AXDB_TEST_PG").ok()?;
    let pg: tokio_postgres::Config = dsn.parse().unwrap();
    let host = match &pg.get_hosts()[0] {
        tokio_postgres::config::Host::Tcp(h) => h.clone(),
        #[allow(unreachable_patterns)]
        _ => "127.0.0.1".into(),
    };
    Some(ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), host),
            ("port".into(), pg.get_ports()[0].to_string()),
            ("database".into(), database.into()),
            ("user".into(), pg.get_user().unwrap().into()),
            ("sslmode".into(), "disable".into()),
        ]),
        secret: pg.get_password().map(|p| String::from_utf8_lossy(p).into_owned()),
        connect_timeout_ms: 5000,
    })
}


pub struct Browse<'a> {
    pub session: &'a dyn Session,
    pub node: String,
    pub offset: u64,
    pub done: bool,
}
#[async_trait]
impl Extractor for Browse<'_> {
    async fn next(&mut self, limit: u32, _: &CancelToken) -> Result<Option<Batch>, DbError> {
        if self.done {
            return Ok(None);
        }
        let set = self.session.browse(&self.node, PageRequest { offset: self.offset, limit, filter: vec![] }).await?;
        let Some(table) = set.table else { return Err(DbError::query("browse returned no table")) };
        self.offset += table.rows.len() as u64;
        self.done = !set.page.is_some_and(|p| p.has_more) || table.rows.is_empty();
        Ok(Some(Batch { columns: table.columns, rows: table.rows }))
    }
}
pub struct NoProgress;
#[async_trait]
impl ProgressSink for NoProgress {
    async fn progress(&self, _: u64, _: u64) -> Result<(), DbError> {
        Ok(())
    }
}
pub struct Import<'a> {
    pub session: &'a dyn Session,
    pub node: String,
}
#[async_trait]
impl Loader for Import<'_> {
    async fn load(&mut self, req: LoadRequest, cancel: &CancelToken) -> Result<LoadOutput, DbError> {
        let n = req.rows.len() as u64;
        let edit = EditRequest {
            originals: vec![],
            create_table: req.create_table,
            node_id: self.node.clone(),
            edits: req.rows.into_iter().map(|r| RowEdit::Insert { values: req.columns.iter().cloned().zip(r).collect() }).collect(),
            timeout_ms: 600_000,
            preview_sample_limit: 100,
        };
        match self.session.import_rows_progress(&edit, req.preview, cancel.clone(), &NoProgress).await? {
            ImportOutput::Preview(p) => Ok(LoadOutput::Preview(p)),
            ImportOutput::Applied(_) => Ok(LoadOutput::Applied(n)),
        }
    }
}

