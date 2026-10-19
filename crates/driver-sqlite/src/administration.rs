use super::*;
use rusqlite::backup::{Backup, StepResult};
use std::io::{Read, Write};
impl SqliteSession {
    pub(super) async fn native_backup(&self, cancel: CancelToken) -> Result<BackupData, DbError> {
        let connection = self.connection.clone();
        tokio::task::spawn_blocking(move||{
            let guard=connection.blocking_lock();let conn=guard.as_ref().ok_or_else(||DbError::new(ErrorKind::Connection,"Session closed"))?;
            if !conn.is_autocommit(){return Err(DbError::unsupported("Finish the existing transaction before backup"));}
            let file=tempfile::NamedTempFile::new().map_err(|_|DbError::internal("Cannot create private backup file"))?;
            let mut target=Connection::open(file.path()).map_err(sql_error)?;
            copy(conn,&mut target,&cancel)?;
            let mut data=vec![];file.reopen().map_err(|_|DbError::query("Cannot read backup"))?.take(BACKUP_MAX_BYTES as u64+1).read_to_end(&mut data).map_err(|_|DbError::query("Cannot read backup"))?;
            let mut counts=vec![];
            let mut statement=target.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").map_err(sql_error)?;
            let names=statement.query_map([],|r|r.get::<_,String>(0)).map_err(sql_error)?.collect::<Result<Vec<_>,_>>().map_err(sql_error)?;
            for name in names {let count=target.query_row(&format!("SELECT count(*) FROM {}",quote(&name)),[],|r|r.get::<_,i64>(0)).map_err(sql_error)?;counts.push((name,count as u64));}
            let result=BackupData{format:"sqlite-online-v1".into(),data,counts};result.validate("sqlite-online-v1")?;Ok(result)
        }).await.map_err(|_|DbError::internal("Backup worker failed"))?
    }
    pub(super) async fn native_restore(
        &self,
        backup: &BackupData,
        overwrite: bool,
        cancel: CancelToken,
    ) -> Result<(), DbError> {
        backup.validate("sqlite-online-v1")?;
        let backup = backup.clone();
        let connection = self.connection.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = connection.blocking_lock();
            let conn = guard
                .as_mut()
                .ok_or_else(|| DbError::new(ErrorKind::Connection, "Session closed"))?;
            if !conn.is_autocommit() {
                return Err(DbError::unsupported(
                    "Finish the existing transaction before restore",
                ));
            }
            let nonempty: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name NOT LIKE 'sqlite_%')",
                    [],
                    |r| r.get(0),
                )
                .map_err(sql_error)?;
            if nonempty && !overwrite {
                return Err(DbError::new(
                    ErrorKind::Conflict,
                    "Restore requires an empty database",
                ));
            }
            let mut file = tempfile::NamedTempFile::new()
                .map_err(|_| DbError::internal("Cannot create private restore file"))?;
            file.write_all(&backup.data)
                .map_err(|_| DbError::query("Cannot stage restore"))?;
            let source = Connection::open_with_flags(file.path(), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(sql_error)?;
            let integrity: String = source
                .query_row("PRAGMA integrity_check", [], |r| r.get(0))
                .map_err(sql_error)?;
            if integrity != "ok" {
                return Err(DbError::query("SQLite backup integrity check failed"));
            }
            copy(&source, conn, &cancel)
        })
        .await
        .map_err(|_| DbError::internal("Restore worker failed"))?
    }
}
pub(super) fn copy(source: &Connection, target: &mut Connection, cancel: &CancelToken) -> Result<(), DbError> {
    let backup = Backup::new(source, target).map_err(sql_error)?;
    let started = Instant::now();
    loop {
        if cancel.is_cancelled() {
            return Err(DbError::new(
                ErrorKind::Cancelled,
                "Backup/restore cancelled",
            ));
        }
        if started.elapsed() > Duration::from_secs(60) {
            return Err(DbError::new(ErrorKind::Timeout, "Backup/restore timed out"));
        }
        match backup.step(64).map_err(sql_error)? {
            StepResult::Done => return Ok(()),
            StepResult::More => {}
            StepResult::Busy | StepResult::Locked => std::thread::sleep(Duration::from_millis(10)),
            _ => return Err(DbError::query("Unexpected SQLite backup state")),
        }
    }
}
