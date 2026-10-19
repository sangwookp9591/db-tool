use super::*;
use std::io::{Read, Seek, SeekFrom, Write};
impl SqliteSession {
    pub(super) async fn stream_backup(
        &self,
        cancel: CancelToken,
        sink: &dyn BackupSink,
    ) -> Result<BackupSummary, DbError> {
        let connection = self.connection.clone();
        let token = cancel.clone();
        let (mut file,counts)=tokio::task::spawn_blocking(move||{
            let guard=connection.blocking_lock();let conn=guard.as_ref().ok_or_else(||DbError::new(ErrorKind::Connection,"Session closed"))?;
            if !conn.is_autocommit(){return Err(DbError::unsupported("Finish the existing transaction before backup"));}
            let file=tempfile::NamedTempFile::new().map_err(|_|DbError::query("Cannot create private backup file"))?;
            let mut target=Connection::open(file.path()).map_err(sql_error)?;
            super::administration::copy(conn,&mut target,&token)?;
            let mut statement=target.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").map_err(sql_error)?;
            let names=statement.query_map([],|r|r.get::<_,String>(0)).map_err(sql_error)?.collect::<Result<Vec<_>,_>>().map_err(sql_error)?;
            let mut counts=vec![];for name in names{let count=target.query_row(&format!("SELECT count(*) FROM {}",quote(&name)),[],|r|r.get::<_,i64>(0)).map_err(sql_error)?;counts.push((name,count as u64));}
            Ok::<_,DbError>((file,counts))
        }).await.map_err(|_|DbError::internal("Backup worker failed"))??;
        let mut bytes = vec![0; BACKUP_CHUNK_BYTES];
        loop {
            if cancel.is_cancelled() {
                return Err(DbError::new(ErrorKind::Cancelled, "Backup cancelled"));
            }
            let len = file
                .read(&mut bytes)
                .map_err(|_| DbError::query("Cannot read backup file"))?;
            if len == 0 {
                break;
            }
            sink.write(&bytes[..len]).await?;
        }
        Ok(BackupSummary {
            format: "sqlite-online-v1".into(),
            counts,
            warnings: vec![],
        })
    }
    pub(super) async fn stream_restore(
        &self,
        summary: &BackupSummary,
        source: &mut (dyn Read + Send),
        overwrite: bool,
        cancel: CancelToken,
    ) -> Result<(), DbError> {
        if summary.format != "sqlite-online-v1" {
            return Err(DbError::unsupported("Unsupported SQLite backup"));
        }
        let mut file = tempfile::NamedTempFile::new()
            .map_err(|_| DbError::query("Cannot create private restore file"))?;
        let mut bytes = vec![0; BACKUP_CHUNK_BYTES];
        loop {
            if cancel.is_cancelled() {
                return Err(DbError::new(ErrorKind::Cancelled, "Restore cancelled"));
            }
            let len = source
                .read(&mut bytes)
                .map_err(|_| DbError::query("Cannot read restore"))?;
            if len == 0 {
                break;
            }
            file.write_all(&bytes[..len])
                .map_err(|_| DbError::query("Cannot stage restore"))?;
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|_| DbError::query("Cannot rewind restore"))?;
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
            let source = Connection::open_with_flags(file.path(), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(sql_error)?;
            let integrity: String = source
                .query_row("PRAGMA integrity_check", [], |r| r.get(0))
                .map_err(sql_error)?;
            if integrity != "ok" {
                return Err(DbError::query("SQLite backup integrity check failed"));
            }
            super::administration::copy(&source, conn, &cancel)
        })
        .await
        .map_err(|_| DbError::internal("Restore worker failed"))?
    }
}
