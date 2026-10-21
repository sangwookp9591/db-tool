//! Run an existing dbcore driver as a protocol-1 stdio process.
// Same allocator policy as the app (src-tauri/src/main.rs): freed result pages go back to the OS
// immediately, so a driver process does not keep 100k-row peaks resident after the query.
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;
unsafe extern "C" {
    fn mi_option_set(option: std::ffi::c_int, value: std::ffi::c_long);
}
// mi_option_purge_delay in mimalloc v2 (libmimalloc-sys 0.1.49, pinned by mimalloc =0.1.52).
const MI_OPTION_PURGE_DELAY: std::ffi::c_int = 15;
use axdb_driver_protocol::*;
use dbcore::*;
use std::{
    collections::HashMap,
    io,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
    task::JoinSet,
};

pub fn serve(driver: impl Driver + 'static) {
    // SAFETY: mimalloc options may be changed at any time; 0 returns freed pages immediately.
    unsafe { mi_option_set(MI_OPTION_PURGE_DELAY, 0) };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build();
    let result = runtime.and_then(|runtime| {
        runtime.block_on(serve_io(
            Arc::new(driver),
            tokio::io::stdin(),
            tokio::io::stdout(),
        ))
    });
    if let Err(error) = result {
        // Never echo a protocol payload or a database error on stderr.
        eprintln!("AxDB driver transport stopped: {:?}", error.kind());
        std::process::exit(1);
    }
}

struct Upload { session: u64, file: std::fs::File, summary: BackupSummary, overwrite: bool, new_database: Option<String>, touched: std::time::Instant }
type Uploads = Arc<Mutex<HashMap<u64, Upload>>>;
type Sessions = Arc<Mutex<HashMap<u64, Arc<dyn Session>>>>;

pub async fn serve_io<R, W>(driver: Arc<dyn Driver>, mut input: R, mut output: W) -> io::Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    // A frame read must survive other requests completing between its bytes.
    let (incoming, mut frames) = mpsc::channel(8);
    let reader = tokio::spawn(async move {
        loop {
            let frame = read_frame(&mut input).await;
            let done = !matches!(frame, Ok(Some(_)));
            if incoming.send(frame).await.is_err() || done {
                break;
            }
        }
    });
    let (tx, mut rx) = mpsc::channel::<Message>(8);
    let mut writer = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            write_message(&mut output, message).await?;
        }
        Ok::<_, io::Error>(())
    });
    let sessions: Sessions = Arc::default();
    let uploads: Uploads = Arc::default();
    let mut pending = HashMap::<u64, CancelToken>::new();
    let mut tasks = JoinSet::new();
    let mut next_session = 1u64;
    let result = loop {
        let expiry = uploads.lock().unwrap().values().map(|upload| tokio::time::Instant::from_std(upload.touched + std::time::Duration::from_secs(120))).min();
        tokio::select! {
            _ = async { if let Some(deadline) = expiry { tokio::time::sleep_until(deadline).await; } else { std::future::pending::<()>().await; } } => { uploads.lock().unwrap().retain(|_,u|u.touched.elapsed() < std::time::Duration::from_secs(120)); }
            written = &mut writer => break written.map_err(io::Error::other)?,
            finished = tasks.join_next(), if !tasks.is_empty() => {
                match finished {
                    Some(Ok(id)) => { pending.remove(&id); }
                    Some(Err(_)) => break Err(io::Error::other("Driver request panicked")),
                    None => {}
                }
            }
            message = frames.recv() => {
                match message {
                    Some(Ok(None)) | None => break Ok(()),
                    Some(Err(error)) => break Err(error),
                    Some(Ok(Some(Message::Cancel { cancel }))) => {
                        if let Some(token) = pending.get(&cancel) { token.cancel(); }
                    }
                    Some(Ok(Some(Message::Request { id, request }))) => {
                        if pending.contains_key(&id) {
                            break Err(io::Error::new(io::ErrorKind::InvalidData, "Duplicate request id"));
                        }
                        let token = CancelToken::new();
                        pending.insert(id, token.clone());
                        let driver = driver.clone(); let sessions = sessions.clone(); let tx = tx.clone(); let uploads = uploads.clone();
                        let session_id = next_session;
                        next_session = next_session.checked_add(1).ok_or_else(|| io::Error::other("Session id exhausted"))?;
                        tasks.spawn(async move {
                            let result = dispatch(driver, &sessions, &uploads, session_id, id, request, token, &tx).await;
                            let message = match result { Ok(result) => Message::Result { id, result }, Err(error) => Message::Error { id, error } };
                            let _ = tx.send(message).await;
                            id
                        });
                    }
                    Some(Ok(Some(_))) => break Err(io::Error::new(io::ErrorKind::InvalidData, "Expected driver request")),
                }
            }
        }
    };
    for token in pending.values() {
        token.cancel();
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    let sessions = std::mem::take(&mut *sessions.lock().unwrap());
    for session in sessions.into_values() {
        session.close().await;
    }
    writer.abort();
    reader.abort();
    result
}

async fn write_message<W: AsyncWrite + Unpin>(writer: &mut W, message: Message) -> io::Result<()> {
    let mut pending = vec![message];
    while let Some(message) = pending.pop() {
        match encode(&message) {
            Ok(bytes) => write_encoded_frame(writer, &bytes).await?,
            Err(error) => {
                if let Message::Chunk { id, chunk } = message
                    && let Some((first, second)) = chunk.split()
                {
                    pending.push(Message::Chunk { id, chunk: second });
                    pending.push(Message::Chunk { id, chunk: first });
                    continue;
                }
                return Err(error);
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn dispatch(
    driver: Arc<dyn Driver>,
    sessions: &Sessions,
    uploads: &Uploads,
    next_session: u64,
    id: u64,
    request: Request,
    cancel: CancelToken,
    tx: &mpsc::Sender<Message>,
) -> Result<Reply, DbError> {
    match request {
        Request::Unsupported => Err(DbError::unsupported("Unknown optional driver method")),
        Request::Handshake { protocol } => {
            if protocol != PROTOCOL {
                return Err(DbError::unsupported(
                    "드라이버 업데이트 필요: incompatible protocol",
                ));
            }
            Ok(Reply::Handshake {
                protocol: PROTOCOL,
                manifest: driver.manifest().clone(),
            })
        }
        Request::Connect(cfg) => {
            let session: Arc<dyn Session> = Arc::from(driver.connect(&cfg).await?);
            sessions.lock().unwrap().insert(next_session, session);
            Ok(Reply::Connected {
                session: next_session,
            })
        }
        request => {
            let session_id = request
                .session()
                .ok_or_else(|| DbError::internal("Missing session"))?;
            let session = sessions
                .lock()
                .unwrap()
                .get(&session_id)
                .cloned()
                .ok_or_else(|| DbError::new(ErrorKind::Connection, "Driver session is closed"))?;
            match request {
                Request::Ping { .. } => {
                    session.ping().await?;
                    Ok(Reply::Unit)
                }
                Request::ServerInfo { .. } => Ok(Reply::Info(session.server_info().await?)),
                Request::ListChildren { parent, .. } => Ok(Reply::Children(
                    session.list_children(parent.as_deref()).await?,
                )),
                Request::Browse { node_id, page, .. } => {
                    Ok(Reply::ResultSet(session.browse(&node_id, page).await?))
                }
                Request::ExecuteStream { req, .. } => {
                    if !driver.manifest().capabilities.execute_stream { return Err(DbError::unsupported("Incremental execution unavailable")); }
                    session.execute_stream_raw(&req, cancel, &QueryOutput { tx, id }).await?;
                    Ok(Reply::Unit)
                }
                Request::Execute { req, .. } => {
                    let results = session.execute(&req, cancel.clone()).await?;
                    let streaming = streamable_tables(&results);
                    for (index, mut result) in results.into_iter().enumerate() {
                        if cancel.is_cancelled() {
                            return Err(DbError::new(
                                ErrorKind::Cancelled,
                                "실행이 취소되었습니다",
                            ));
                        }
                        let rows = result
                            .table
                            .as_mut()
                            .map(|t| std::mem::take(&mut t.rows))
                            .unwrap_or_default();
                        let documents = result
                            .documents
                            .as_mut()
                            .map(std::mem::take)
                            .unwrap_or_default();
                        let (nodes, rels) = result
                            .graph
                            .as_mut()
                            .map(|g| (std::mem::take(&mut g.nodes), std::mem::take(&mut g.rels)))
                            .unwrap_or_default();
                        send(
                            tx,
                            id,
                            ExecuteChunk::Start {
                                index,
                                streaming,
                                result: Box::new(result),
                            },
                        )
                        .await?;
                        batches(tx, id, rows, |rows| ExecuteChunk::Rows { index, rows }).await?;
                        batches(tx, id, documents, |documents| ExecuteChunk::Documents {
                            index,
                            documents,
                        })
                        .await?;
                        batches(tx, id, nodes, |nodes| ExecuteChunk::Nodes { index, nodes })
                            .await?;
                        batches(tx, id, rels, |rels| ExecuteChunk::Relationships {
                            index,
                            rels,
                        })
                        .await?;
                    }
                    Ok(Reply::Unit)
                }
                Request::Expand {
                    element_id, limit, ..
                } => Ok(Reply::ResultSet(session.expand(&element_id, limit).await?)),
                Request::SchemaGraph { scope, .. } => Ok(Reply::SchemaGraph(
                    session.schema_graph(scope.as_deref()).await?,
                )),
                Request::Explain { query, analyze, .. } => {
                    Ok(Reply::Plan(session.explain(&query, analyze, cancel).await?))
                }
                Request::Preview { req, .. } => {
                    Ok(Reply::Preview(session.preview(&req, cancel).await?))
                }
                Request::PreviewEdits { req, .. } => {
                    Ok(Reply::Preview(session.preview_edits(&req, cancel).await?))
                }
                Request::ApplyEdits { req, .. } => {
                    Ok(Reply::Edits(session.apply_edits(&req, cancel).await?))
                }
                Request::ApplyMigration {req,..} => Ok(Reply::Migration(session.apply_migration(&req,cancel).await?)),
                Request::SchemaSnapshot {scope,..} => Ok(Reply::Snapshot(session.schema_snapshot(scope.as_deref()).await?)),
                Request::DdlRender {diff,..} => Ok(Reply::Ddl(session.ddl_render(&diff).await?)),
                Request::ImportRowsProgress {req,preview,..} => {
                    send(tx,id,ExecuteChunk::Start{index:0,result:Box::default(),streaming:true}).await?;
                    Ok(Reply::Import(session.import_rows_progress(&req,preview,cancel,&Progress{tx,id}).await?))
                },
                Request::ImportRows {req,preview,..} => Ok(Reply::Import(session.import_rows(&req,preview,cancel).await?)),
                Request::RestoreOptions {..}=>Ok(Reply::RestoreOptions(session.restore_options().await?)),
                Request::BackupStream {..} => {
                    let output=BackupOutput{tx,id,buffer:tokio::sync::Mutex::new(Vec::with_capacity(BACKUP_CHUNK_BYTES))};
                    let summary=session.backup_stream(cancel,&output).await?;output.flush().await?;Ok(Reply::BackupSummary(summary))
                },
                Request::RestoreStart {summary,overwrite,new_database,..} => {
                    let mut uploads=uploads.lock().unwrap();
                    if uploads.values().any(|u|u.session==session_id) { return Err(DbError::new(ErrorKind::Conflict,"A restore upload is already active")); }
                    uploads.insert(id,Upload{session:session_id,file:tempfile::tempfile().map_err(|_|DbError::query("Cannot create private restore file"))?,summary,overwrite,new_database,touched:std::time::Instant::now()});
                    Ok(Reply::Transfer(id))
                },
                Request::RestoreChunk {transfer,data,..} => {
                    use std::io::Write;
                    if data.data.len()>BACKUP_CHUNK_BYTES { return Err(DbError::query("Restore chunk exceeds 64 KiB")); }
                    let mut uploads=uploads.lock().unwrap();
                    let upload=uploads.get_mut(&transfer).filter(|u|u.session==session_id).ok_or_else(||DbError::query("Restore upload expired"))?;
                    upload.file.write_all(&data.data).map_err(|_|DbError::query("Cannot stage restore chunk"))?;
                    upload.touched=std::time::Instant::now();
                    Ok(Reply::Unit)
                },
                Request::RestoreAbort {transfer,..} => {
                    let mut uploads=uploads.lock().unwrap();
                    if uploads.get(&transfer).is_some_and(|u|u.session==session_id) { uploads.remove(&transfer); }
                    Ok(Reply::Unit)
                },
                Request::RestoreFinish {transfer,..} => {
                    use std::io::{Seek,SeekFrom};
                    let mut upload={let mut uploads=uploads.lock().unwrap();
                        if uploads.get(&transfer).is_none_or(|u|u.session!=session_id) { return Err(DbError::query("Restore upload expired")); }
                        uploads.remove(&transfer).unwrap()};
                    upload.file.seek(SeekFrom::Start(0)).map_err(|_|DbError::query("Cannot rewind restore"))?;
                    if let Some(name)=upload.new_database {
                        if upload.overwrite{return Err(DbError::query("New database cannot overwrite"));}
                        session.restore_database(&upload.summary,&mut upload.file,&name,cancel).await?;
                    }else{session.restore_stream(&upload.summary,&mut upload.file,upload.overwrite,cancel).await?;}
                    Ok(Reply::Unit)
                },
                Request::Backup {..} => Ok(Reply::Backup(session.backup(cancel).await?)),
                Request::Restore {backup,overwrite,..} => {session.restore(&backup,overwrite,cancel).await?; Ok(Reply::Unit)},
                Request::PreviewGrants {changes,..} => Ok(Reply::Grants(session.preview_grants(&changes,cancel).await?)),
                Request::ApplyGrants {changes,..} => {session.apply_grants(&changes,cancel).await?;Ok(Reply::Unit)},
                Request::Roles {..} => Ok(Reply::Roles(session.roles().await?)),
                Request::RolesPage {query,..} => Ok(Reply::Roles(session.roles_page(&query).await?)),
                Request::PlanGrants {changes,..} => Ok(Reply::Grants(session.plan_grants(&changes).await?)),
                Request::Close { .. } => {
                    uploads.lock().unwrap().retain(|_,u|u.session!=session_id);
                    sessions.lock().unwrap().remove(&session_id);
                    session.close().await;
                    Ok(Reply::Unit)
                }
                _ => Err(DbError::internal("Invalid session method")),
            }
        }
    }
}

async fn send(tx: &mpsc::Sender<Message>, id: u64, chunk: ExecuteChunk) -> Result<(), DbError> {
    tx.send(Message::Chunk { id, chunk })
        .await
        .map_err(|_| DbError::new(ErrorKind::Connection, "Driver output closed"))
}

async fn batches<T>(
    tx: &mpsc::Sender<Message>,
    id: u64,
    values: Vec<T>,
    chunk: impl Fn(Vec<T>) -> ExecuteChunk,
) -> Result<(), DbError> {
    let mut values = values.into_iter().peekable();
    let mut first = true;
    while values.peek().is_some() {
        send(
            tx,
            id,
            chunk(
                values
                    .by_ref()
                    .take(if first { 200 } else { CHUNK_ITEMS })
                    .collect(),
            ),
        )
        .await?;
        first = false;
    }
    Ok(())
}


struct Progress<'a>{tx:&'a mpsc::Sender<Message>,id:u64}
#[async_trait::async_trait]
impl ProgressSink for Progress<'_>{
    async fn progress(&self,completed:u64,total:u64)->Result<(),DbError>{send(self.tx,self.id,ExecuteChunk::Rows{index:0,rows:vec![vec![Value::Int(completed.to_string()),Value::Int(total.to_string())]]}).await}
}

struct BackupOutput<'a> {tx:&'a mpsc::Sender<Message>,id:u64,buffer:tokio::sync::Mutex<Vec<u8>>}
impl BackupOutput<'_>{
    async fn send(&self,data:Vec<u8>)->Result<(),DbError>{send(self.tx,self.id,ExecuteChunk::Backup{data:BackupData{format:String::new(),data,counts:vec![]}}).await}
    async fn flush(&self)->Result<(),DbError>{let mut buffer=self.buffer.lock().await;if !buffer.is_empty(){self.send(std::mem::take(&mut *buffer)).await?;}Ok(())}
}
#[async_trait::async_trait]
impl BackupSink for BackupOutput<'_> {
    async fn write(&self, mut bytes:&[u8])->Result<(),DbError> {
        let mut buffer=self.buffer.lock().await;
        while !bytes.is_empty(){let len=(BACKUP_CHUNK_BYTES-buffer.len()).min(bytes.len());buffer.extend_from_slice(&bytes[..len]);bytes=&bytes[len..];
            if buffer.len()==BACKUP_CHUNK_BYTES{self.send(std::mem::replace(&mut *buffer,Vec::with_capacity(BACKUP_CHUNK_BYTES))).await?;}
        }
        Ok(())
    }
}

struct QueryOutput<'a> { tx: &'a mpsc::Sender<Message>, id: u64 }
#[async_trait::async_trait]
impl AsyncResultSink for QueryOutput<'_> {
    async fn start(&self, index: usize, result: &ResultSet) -> Result<(), DbError> {
        send(self.tx, self.id, ExecuteChunk::StreamStart { index, result: Box::new(result.clone()) }).await
    }
    async fn rows(&self, index: usize, rows: Vec<Vec<Value>>) -> Result<(), DbError> {
        send(self.tx, self.id, ExecuteChunk::Rows { index, rows }).await
    }
    async fn end(&self, index: usize, result: &ResultSet) -> Result<(), DbError> {
        send(self.tx, self.id, ExecuteChunk::StreamEnd { index, result: Box::new(result.clone()) }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn incremental_sink_backpressures_at_eight_frames_and_propagates_closed_receiver() {
        let (tx, mut rx) = mpsc::channel(8);
        let (progress, mut received) = mpsc::channel(1);
        let producer = tokio::spawn(async move {
            let output = QueryOutput { tx: &tx, id: 1 };
            for i in 0..10 { output.rows(0, vec![vec![Value::Int(i.to_string())]]).await?; progress.send(i).await.unwrap(); }
            Ok::<_, DbError>(())
        });
        for expected in 0..8 { assert_eq!(received.recv().await, Some(expected)); }
        assert!(tokio::time::timeout(std::time::Duration::from_millis(10), received.recv()).await.is_err());
        rx.recv().await.unwrap();
        assert_eq!(received.recv().await, Some(8));
        drop(rx);
        assert_eq!(producer.await.unwrap().unwrap_err().kind, ErrorKind::Connection);
    }
    #[tokio::test]
    async fn oversized_batch_splits_without_losing_order() {
        let rows = vec![
            vec![Value::Text("a".repeat(MAX_FRAME / 2))],
            vec![Value::Text("b".repeat(MAX_FRAME / 2))],
        ];
        let mut bytes = Vec::new();
        write_message(
            &mut bytes,
            Message::Chunk {
                id: 1,
                chunk: ExecuteChunk::Rows { index: 0, rows },
            },
        )
        .await
        .unwrap();
        let mut input = bytes.as_slice();
        for expected in *b"ab" {
            let Some(Message::Chunk {
                chunk: ExecuteChunk::Rows { rows, .. },
                ..
            }) = read_frame(&mut input).await.unwrap()
            else {
                panic!("expected row chunk")
            };
            assert_eq!(rows.len(), 1);
            let Value::Text(value) = &rows[0][0] else {
                panic!("expected text")
            };
            assert_eq!(value.len(), MAX_FRAME / 2);
            assert!(value.bytes().all(|b| b == expected));
        }
        assert!(read_frame(&mut input).await.unwrap().is_none());
    }
}
