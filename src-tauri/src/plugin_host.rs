//! Isolated, lazily started driver processes. A session never migrates across a crash.
use async_trait::async_trait;
use axdb_driver_protocol::*;
use dbcore::*;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::AsyncReadExt,
    process::{Child, ChildStdin, Command},
    sync::{mpsc, Mutex as AsyncMutex},
};

const DRIVER_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);
// Driver rollback may consume three seconds before its final response can cross the pipe.
const CLEANUP_GRACE: Duration = Duration::from_secs(3);
const RESPONSE_GRACE: Duration = CLEANUP_GRACE.saturating_add(HANDSHAKE_TIMEOUT);
const MAX_RESPONSE: usize = 256 * 1024 * 1024;

fn disconnected() -> DbError {
    DbError::new(
        ErrorKind::Connection,
        "드라이버 연결이 끊겼습니다. 다시 연결하세요",
    )
}
fn invalid_reply() -> DbError {
    DbError::internal("Invalid driver response")
}

pub struct PluginDriver {
    executable: PathBuf,
    manifest: DriverManifest,
    process: AsyncMutex<Option<Arc<Process>>>,
}

impl PluginDriver {
    /// Probe the executable, then release it until the first connection.
    pub async fn load(executable: PathBuf) -> Result<Self, DbError> {
        Self::load_cancelable(executable, CancelToken::new()).await
    }

    /// Cancellation finishes killing and reaping the probe before a package is removed.
    pub async fn load_cancelable(
        executable: PathBuf,
        cancel: CancelToken,
    ) -> Result<Self, DbError> {
        if cancel.is_cancelled() {
            return Err(DbError::new(
                ErrorKind::Cancelled,
                "드라이버 설치가 취소되었습니다",
            ));
        }
        let process = Process::spawn(&executable)?;
        let _probe = Probe(process.clone());
        let result = tokio::select! {
            result = process.handshake() => result,
            _ = cancel.cancelled() => Err(DbError::new(ErrorKind::Cancelled, "드라이버 설치가 취소되었습니다")),
        };
        process.stop().await;
        let manifest = result?;
        Ok(Self {
            executable,
            manifest,
            process: AsyncMutex::new(None),
        })
    }

    async fn process(&self) -> Result<SessionLease, DbError> {
        let mut slot = self.process.lock().await;
        if let Some(lease) = slot.as_ref().and_then(Process::reserve) {
            return Ok(lease);
        }
        let process = Process::spawn(&self.executable)?;
        let manifest = match process.handshake().await {
            Ok(manifest) => manifest,
            Err(error) => { process.stop().await; return Err(error); }
        };
        if manifest != self.manifest {
            process.stop().await;
            return Err(DbError::unsupported(
                "드라이버가 변경되었습니다. 다시 설치하세요",
            ));
        }
        *slot = Some(process.clone());
        process.reserve().ok_or_else(disconnected)
    }

    pub async fn connect_plugin(&self, cfg: &ConnectConfig) -> Result<PluginSession, DbError> {
        let lease = self.process().await?;
        let process = lease.0.clone();
        if let Some(secret) = cfg.secret.as_ref().filter(|s| !s.is_empty()) {
            let mut secrets = process.secrets.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if !secrets.contains(secret) {
                secrets.push(secret.clone());
            }
        }
        let response = process
            .call(
                Request::Connect(cfg.clone()),
                CancelToken::new(),
                Duration::from_millis(cfg.connect_timeout_ms.saturating_add(3000)),
            )
            .await?;
        let Reply::Connected { session } = response.reply else {
            return Err(invalid_reply());
        };
        Ok(PluginSession {
            lease: Mutex::new(Some(lease)),
            process,
            session,
            closed: CancelToken::new(),
            incremental: self.manifest.capabilities.execute_stream,
        })
    }
}

#[async_trait]
impl Driver for PluginDriver {
    fn manifest(&self) -> &DriverManifest {
        &self.manifest
    }
    async fn connect(&self, cfg: &ConnectConfig) -> Result<Box<dyn Session>, DbError> {
        Ok(Box::new(self.connect_plugin(cfg).await?))
    }
}

#[derive(Default)]
struct SessionCount { active: usize, generation: u64, idle: Option<tokio::task::AbortHandle> }
struct SessionLease(Arc<Process>);
impl Drop for SessionLease {
    fn drop(&mut self) {
        let mut sessions = self.0.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        sessions.active -= 1;
        if sessions.active != 0 || self.0.dead.is_cancelled() { return; }
        let generation = sessions.generation;
        let weak = Arc::downgrade(&self.0);
        let deadline = tokio::time::Instant::now() + DRIVER_IDLE_TIMEOUT;
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            sessions.idle = Some(runtime.spawn(async move {
                tokio::time::sleep_until(deadline).await;
                if let Some(process) = weak.upgrade().filter(|process| process.expire_idle(generation)) {
                    process.stop().await;
                }
            }).abort_handle());
        }
    }
}
struct Process {
    sessions: Mutex<SessionCount>,
    reaped: CancelToken,
    child: Mutex<Option<Child>>,
    input: AsyncMutex<ChildStdin>,
    pending: Mutex<HashMap<u64, PendingResponse>>,
    next_id: AtomicU64,
    dead: CancelToken,
    secrets: Mutex<Vec<String>>,
}

impl Drop for Process {
    fn drop(&mut self) { self.fail(); }
}

struct Probe(Arc<Process>);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.fail();
    }
}

struct Response {
    reply: Reply,
    results: Vec<ResultSet>,
    first_chunk: Duration,
}

struct PendingResponse {
    sender: mpsc::Sender<Message>,
    bytes: usize,
    backup_stream: bool,
}
impl PendingResponse {
    fn admit(&mut self,message:&Message,bytes:usize)->bool {
        if self.backup_stream
            && let Message::Chunk {
                chunk: ExecuteChunk::Backup { data },
                ..
            } = message
        {
            return data.data.len() <= BACKUP_CHUNK_BYTES
                && data.format.is_empty()
                && data.counts.is_empty()
                && bytes <= BACKUP_CHUNK_BYTES.div_ceil(3) * 4 + 1024;
        }
        self.add(bytes)
    }
    fn add(&mut self, bytes: usize) -> bool {
        self.bytes = self.bytes.saturating_add(bytes);
        self.bytes <= MAX_RESPONSE
    }
}

impl Process {
    fn spawn(executable: &Path) -> Result<Arc<Self>, DbError> {
        let mut command = Command::new(executable);
        // Signing keys, AI credentials and debug secret stores belong to the host.
        command.env_clear();
        for name in [
            "PATH",
            "HOME",
            "TMPDIR",
            "TMP",
            "TEMP",
            "SystemRoot",
            "LANG",
            "LC_ALL",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| DbError::new(ErrorKind::Connection, "Cannot start driver executable"))?;
        let input = child.stdin.take().ok_or_else(disconnected)?;
        let mut output = child.stdout.take().ok_or_else(disconnected)?;
        let mut stderr = child.stderr.take().ok_or_else(disconnected)?;
        let process = Arc::new(Self {
            sessions: Mutex::default(),
            reaped: CancelToken::new(),
            child: Mutex::new(Some(child)),
            input: AsyncMutex::new(input),
            pending: Mutex::default(),
            next_id: AtomicU64::new(1),
            dead: CancelToken::new(),
            secrets: Mutex::default(),
        });
        let weak = Arc::downgrade(&process);
        tokio::spawn(async move {
            loop {
                let message = read_frame_with_size(&mut output).await;
                let Some(process) = weak.upgrade() else {
                    break;
                };
                match message {
                    Ok(Some((
                        message @ (Message::Result { .. }
                        | Message::Error { .. }
                        | Message::Chunk { .. }),
                        size,
                    ))) => {
                        let id = match &message {
                            Message::Result { id, .. }
                            | Message::Error { id, .. }
                            | Message::Chunk { id, .. } => *id,
                            _ => unreachable!(),
                        };
                        let sender = {
                            let mut pending = process.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                            pending.get_mut(&id).map(|pending| {
                                if pending.admit(&message,size) {
                                    Ok(pending.sender.clone())
                                } else {
                                    Err(())
                                }
                            })
                        };
                        if let Some(Ok(sender)) = sender {
                            tokio::select! { _ = process.dead.cancelled() => break, _ = sender.send(message) => {} }
                        } else if matches!(sender, Some(Err(()))) {
                            process.fail();
                            break;
                        }
                    }
                    _ => {
                        process.fail();
                        break;
                    }
                }
            }
        });
        let weak = Arc::downgrade(&process);
        tokio::spawn(async move {
            // Drop overlong lines rather than emitting a secret split across log chunks.
            let mut line = Vec::new();
            let mut overflow = false;
            let mut bytes = [0; 1024];
            while let Ok(n) = stderr.read(&mut bytes).await {
                if n == 0 {
                    break;
                }
                for &byte in &bytes[..n] {
                    if byte == b'\n' {
                        if !overflow {
                            log_stderr(&weak, &line);
                        }
                        line.clear();
                        overflow = false;
                    } else if line.len() < 8192 {
                        line.push(byte);
                    } else {
                        overflow = true;
                    }
                }
            }
            if !overflow && !line.is_empty() {
                log_stderr(&weak, &line);
            }
        });
        Ok(process)
    }

    fn reserve(self: &Arc<Self>) -> Option<SessionLease> {
        let mut sessions = self.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.dead.is_cancelled() { return None; }
        if let Some(idle) = sessions.idle.take() { idle.abort(); }
        sessions.active += 1;
        sessions.generation = sessions.generation.wrapping_add(1);
        Some(SessionLease(self.clone()))
    }

    fn expire_idle(&self, generation: u64) -> bool {
        let mut sessions = self.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if sessions.active != 0 || sessions.generation != generation || self.dead.is_cancelled() { return false; }
        sessions.idle.take();
        // Mark dead while holding the same lock as reserve; a concurrent connect must spawn a fresh process.
        self.dead.cancel();
        drop(sessions);
        self.fail();
        true
    }

    fn fail(&self) {
        self.dead.cancel();
        if let Some(idle) = self.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner).idle.take() { idle.abort(); }
        self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
        if let Some(mut child) = self.child.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
            let _ = child.start_kill();
            let reaped = self.reaped.clone();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move { let _ = child.wait().await; reaped.cancel(); });
            }
        }
    }

    async fn stop(&self) {
        self.fail();
        self.reaped.cancelled().await;
    }

    async fn handshake(self: &Arc<Self>) -> Result<DriverManifest, DbError> {
        let response = self
            .call(
                Request::Handshake { protocol: PROTOCOL },
                CancelToken::new(),
                HANDSHAKE_TIMEOUT,
            )
            .await?;
        match response.reply {
            Reply::Handshake {
                protocol: PROTOCOL,
                manifest,
            } if !manifest.id.is_empty() => Ok(manifest),
            _ => Err(DbError::unsupported(
                "드라이버 업데이트 필요: incompatible protocol",
            )),
        }
    }

    async fn send(&self, message: &Message) -> Result<(), DbError> {
        if self.dead.is_cancelled() {
            return Err(disconnected());
        }
        let written = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
            write_frame(&mut *self.input.lock().await, message).await
        })
        .await;
        if !matches!(written, Ok(Ok(()))) {
            self.fail();
            return Err(disconnected());
        }
        Ok(())
    }

    async fn call(
        self: &Arc<Self>,
        request: Request,
        cancel: CancelToken,
        timeout: Duration,
    ) -> Result<Response, DbError> {
        self.call_with_sink(request, cancel, timeout, None).await
    }

    async fn call_with_sink(
        self: &Arc<Self>,
        request: Request,
        cancel: CancelToken,
        timeout: Duration,
        sink: Option<&dyn ResultSink>,
    ) -> Result<Response, DbError> {
        self.call_with_sinks(request,cancel,timeout,sink,None).await
    }

    // Probe the driver's dispatcher, independently of any blocked database
    // session. A request timing out does not prove the shared process is dead.
    async fn responsive(self: &Arc<Self>) -> bool {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, mut incoming) = mpsc::channel(1);
        self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(id, PendingResponse { sender, bytes: 0, backup_stream: false });
        let result = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
            self.send(&Message::Request { id, request: Request::Handshake { protocol: PROTOCOL } }).await.ok()?;
            match incoming.recv().await? {
                Message::Result { result: Reply::Handshake { protocol: PROTOCOL, .. }, .. } => Some(()),
                _ => None,
            }
        }).await;
        self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&id);
        matches!(result, Ok(Some(())))
    }

    async fn call_with_sinks(
        self: &Arc<Self>, request: Request, cancel: CancelToken, timeout: Duration,
        sink: Option<&dyn ResultSink>, backup_sink: Option<&dyn BackupSink>,
    ) -> Result<Response, DbError> {
        if self.dead.is_cancelled() {
            return Err(disconnected());
        }
        if cancel.is_cancelled() {
            return Err(DbError::new(ErrorKind::Cancelled, "실행이 취소되었습니다"));
        }
        let incremental = matches!(request, Request::ExecuteStream { .. });
        let handshake = matches!(request, Request::Handshake { .. });
        let mut active: Option<(usize, usize, u64)> = None;
        let started = Instant::now();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, mut incoming) = mpsc::channel(8);
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id, PendingResponse { sender, bytes: 0, backup_stream:matches!(request,Request::BackupStream{..}) });
        let mut guard = Pending {
            process: Arc::downgrade(self),
            id,
            armed: true,
        };
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);
        let message = Message::Request { id, request };
        tokio::select! {
            result = self.send(&message) => result?,
            _ = &mut deadline => { self.fail(); return Err(disconnected()); }
        }
        let mut cancelled = false;
        let mut timed_out = false;
        let mut results = Vec::new();
        let mut first_chunk = None;
        let mut streaming = None;
        loop {
            tokio::select! {
                biased;
                _ = self.dead.cancelled() => return Err(disconnected()),
                _ = cancel.cancelled(), if !cancelled => {
                    cancelled = true;
                    self.send(&Message::Cancel { cancel: id }).await?;
                    deadline.as_mut().reset(tokio::time::Instant::now() + RESPONSE_GRACE);
                }
                _ = &mut deadline => {
                    if handshake {
                        self.fail();
                        return Err(DbError::new(ErrorKind::Connection, "드라이버 응답 시간이 초과되었습니다. 다시 연결하세요"));
                    }
                    if !cancelled {
                        cancelled = true;
                        timed_out = true;
                        self.send(&Message::Cancel { cancel: id }).await?;
                        deadline.as_mut().reset(tokio::time::Instant::now() + RESPONSE_GRACE);
                        continue;
                    }
                    // Retire this request before probing so late chunks cannot
                    // fill its channel and block dispatch of the health reply.
                    guard.armed = false;
                    self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&id);
                    incoming.close();
                    if !self.responsive().await { self.fail(); }
                    return Err(DbError::new(
                        if timed_out { ErrorKind::Timeout } else { ErrorKind::Cancelled },
                        if timed_out { "드라이버 요청 시간이 초과되었습니다" } else { "실행이 취소되었습니다" },
                    ));
                }
                message = incoming.recv() => match message {
                    Some(Message::Result { result, .. }) => {
                        if active.is_some() { self.fail(); return Err(invalid_reply()); }
                        guard.armed = false;
                        self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&id);
                        return Ok(Response { reply: result, results, first_chunk: first_chunk.unwrap_or_else(|| started.elapsed()) });
                    }
                    Some(Message::Error { error, .. }) => {
                        guard.armed = false;
                        self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&id);
                        return Err(error);
                    }
                    Some(Message::Chunk { chunk, .. }) => {
                        match &chunk {
                            ExecuteChunk::StreamStart { index, result } => {
                                if !incremental || active.is_some() || *index != results.len() || result.graph.is_some() || result.documents.is_some() || result.table.as_ref().is_some_and(|t| !t.rows.is_empty()) { self.fail(); return Err(invalid_reply()); }
                                active = Some((*index, result.table.as_ref().map_or(0, |t| t.columns.len()), 0));
                                if let Some(sink) = sink { sink.start_pending(*index, result)?; }
                            }
                            ExecuteChunk::Rows { index, rows } if incremental => {
                                let Some((current, width, count)) = active.as_mut() else { self.fail(); return Err(invalid_reply()); };
                                if index != current || rows.iter().any(|r| r.len() != *width) { self.fail(); return Err(invalid_reply()); }
                                *count = count.checked_add(rows.len() as u64).ok_or_else(invalid_reply)?;
                            }
                            ExecuteChunk::StreamEnd { index, result } => {
                                let Some((current, _, count)) = active.take() else { self.fail(); return Err(invalid_reply()); };
                                if !incremental || current != *index || result.row_count != count || result.graph.is_some() || result.documents.is_some() || result.table.as_ref().map(|t| &t.columns) != results.get(*index).and_then(|r| r.table.as_ref()).map(|t| &t.columns) || result.table.as_ref().is_some_and(|t| !t.rows.is_empty()) { self.fail(); return Err(invalid_reply()); }
                                if let Some(sink) = sink { sink.end(*index, result)?; }
                            }
                            ExecuteChunk::Start { .. } if incremental => { self.fail(); return Err(invalid_reply()); }
                            _ => {}
                        }
                        if let ExecuteChunk::Backup{data}=&chunk {
                            let target=backup_sink.ok_or_else(invalid_reply)?;
                            if data.data.len()>BACKUP_CHUNK_BYTES {self.fail();return Err(invalid_reply());}
                            if !cancelled {target.write(&data.data).await?;}
                            continue;
                        }
                        if let ExecuteChunk::Start { streaming: enabled, .. } = &chunk {
                            if streaming.is_some_and(|old| old != *enabled) { self.fail(); return Err(invalid_reply()); }
                            streaming = Some(*enabled);
                        }
                        if first_chunk.is_none() && !matches!(chunk, ExecuteChunk::Start { .. } | ExecuteChunk::StreamStart { .. } | ExecuteChunk::StreamEnd { .. }) { first_chunk = Some(started.elapsed()); }
                        let chunk = if streaming == Some(true) || incremental {
                            if let Some(sink) = sink {
                                match chunk {
                                    ExecuteChunk::Rows { index, rows } => { sink.rows(index, rows)?; continue; }
                                    chunk => { if let ExecuteChunk::Start { index, result, .. } = &chunk { sink.start(*index, result)?; } chunk }
                                }
                            } else { chunk }
                        } else { chunk };
                        if let Err(error) = chunk.append(&mut results) { self.fail(); return Err(error); }
                    }
                    _ => return Err(disconnected()),
                }
            }
        }
    }
}

fn log_stderr(process: &Weak<Process>, line: &[u8]) {
    if let Some(process) = process.upgrade() {
        let text = redact_stderr(line, &process.secrets.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        tracing::warn!(target: "axdb::plugin_host", driver_stderr = %text);
    }
}

fn redact_stderr(line: &[u8], secrets: &[String]) -> String {
    let mut text = String::from_utf8_lossy(line).into_owned();
    let mut secrets: Vec<&str> = secrets
        .iter()
        .flat_map(|s| s.lines())
        .filter(|s| !s.is_empty())
        .collect();
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for secret in secrets {
        text = crate::storage::redact(&text, Some(secret));
    }
    text
}

struct Pending {
    process: Weak<Process>,
    id: u64,
    armed: bool,
}
impl Drop for Pending {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Some(process) = self.process.upgrade() {
            process.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&self.id);
            let id = self.id;
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = process.send(&Message::Cancel { cancel: id }).await;
                });
            }
        }
    }
}

pub struct PluginSession {
    incremental: bool,
    lease: Mutex<Option<SessionLease>>,
    process: Arc<Process>,
    session: u64,
    closed: CancelToken,
}
impl Drop for PluginSession {
    fn drop(&mut self) {
        if !self.closed.is_cancelled() {
            self.closed.cancel();
            let process = self.process.clone();
            let session = self.session;
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = process
                        .call(
                            Request::Close { session },
                            CancelToken::new(),
                            Duration::from_secs(3),
                        )
                        .await;
                });
            }
        }
    }
}
impl PluginSession {
    async fn call(
        &self,
        request: Request,
        cancel: CancelToken,
        timeout_ms: u64,
    ) -> Result<Response, DbError> {
        if self.closed.is_cancelled() {
            return Err(disconnected());
        }
        self.process
            .call(
                request,
                cancel,
                Duration::from_millis(timeout_ms).saturating_add(RESPONSE_GRACE),
            )
            .await
    }
    async fn upload_restore(&self,summary:&BackupSummary,source:&mut (dyn std::io::Read+Send),overwrite:bool,new_database:Option<String>,cancel:CancelToken)->Result<(),DbError> {
        let response=self.call(Request::RestoreStart{session:self.session,summary:summary.clone(),overwrite,new_database},cancel.clone(),30000).await?;
        let Reply::Transfer(transfer)=response.reply else{return Err(invalid_reply());};
        let work=async {
            let mut data=vec![0;BACKUP_CHUNK_BYTES];
            loop {
                if cancel.is_cancelled(){return Err(DbError::new(ErrorKind::Cancelled,"Restore cancelled"));}
                let len=source.read(&mut data).map_err(|_|DbError::query("Cannot read restore file"))?;
                if len==0 {break;}
                let response=self.call(Request::RestoreChunk{session:self.session,transfer,data:BackupData{format:String::new(),data:data[..len].to_vec(),counts:vec![]}},cancel.clone(),30000).await?;
                if !matches!(response.reply,Reply::Unit){return Err(invalid_reply());}
            }
            match self.call(Request::RestoreFinish{session:self.session,transfer},cancel.clone(),3600000).await?.reply {Reply::Unit=>Ok(()),_=>Err(invalid_reply())}
        }.await;
        if work.is_err(){let _=self.call(Request::RestoreAbort{session:self.session,transfer},CancelToken::new(),3000).await;}
        work
    }
    /// Timing starts at IPC dispatch; used by the common in-process/plugin harness.
    pub async fn execute_measured(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<(Vec<ResultSet>, Duration), DbError> {
        let response = self
            .call(
                Request::Execute {
                    session: self.session,
                    req: req.clone(),
                },
                cancel,
                req.timeout_ms,
            )
            .await?;
        if !matches!(response.reply, Reply::Unit) {
            return Err(invalid_reply());
        }
        Ok((response.results, response.first_chunk))
    }
}

#[async_trait]
impl Session for PluginSession {
    async fn preview_grants(&self,changes:&[GrantChange],cancel:CancelToken)->Result<GrantPlan,DbError>{match self.call(Request::PreviewGrants{session:self.session,changes:changes.to_vec()},cancel,30000).await?.reply{Reply::Grants(p)=>Ok(p),_=>Err(invalid_reply())}}

    async fn import_rows_progress(&self,req:&EditRequest,preview:bool,cancel:CancelToken,sink:&dyn ProgressSink)->Result<ImportOutput,DbError>{
        let (tx,mut rx)=tokio::sync::mpsc::unbounded_channel();
        let adapter=ProgressAdapter{tx};
        let work=self.process.call_with_sink(Request::ImportRowsProgress{session:self.session,req:req.clone(),preview},cancel.clone(),Duration::from_millis(req.timeout_ms.saturating_add(6000)),Some(&adapter));
        tokio::pin!(work);
        loop {tokio::select!{result=&mut work=>{while let Ok((done,total))=rx.try_recv(){sink.progress(done,total).await?;}return match result{Ok(response)=>match response.reply{Reply::Import(output)=>Ok(output),_=>Err(invalid_reply())},Err(e) if e.kind==ErrorKind::Unsupported=>self.import_rows(req,preview,cancel.clone()).await,Err(e)=>Err(e)};},Some((done,total))=rx.recv()=>sink.progress(done,total).await?}}

    }

    async fn apply_grants(&self,changes:&[GrantChange],cancel:CancelToken)->Result<(),DbError>{match self.call(Request::ApplyGrants{session:self.session,changes:changes.to_vec()},cancel,30000).await?.reply{Reply::Unit=>Ok(()),_=>Err(invalid_reply())}}

    async fn backup_stream(&self,cancel:CancelToken,sink:&dyn BackupSink)->Result<BackupSummary,DbError> {
        match self.process.call_with_sinks(Request::BackupStream{session:self.session},cancel,Duration::from_secs(3600),None,Some(sink)).await?.reply {
            Reply::BackupSummary(summary)=>Ok(summary),_=>Err(invalid_reply())
        }
    }
    async fn restore_options(&self)->Result<RestoreOptions,DbError>{match self.call(Request::RestoreOptions{session:self.session},CancelToken::new(),30000).await?.reply{Reply::RestoreOptions(o)=>Ok(o),_=>Err(invalid_reply())}}
    async fn restore_database(&self,summary:&BackupSummary,source:&mut (dyn std::io::Read+Send),name:&str,cancel:CancelToken)->Result<(),DbError>{validate_restore_database(name)?;self.upload_restore(summary,source,false,Some(name.into()),cancel).await}
    async fn restore_stream(&self,summary:&BackupSummary,source:&mut (dyn std::io::Read+Send),overwrite:bool,cancel:CancelToken)->Result<(),DbError>{self.upload_restore(summary,source,overwrite,None,cancel).await}
    async fn backup(&self,cancel:CancelToken)->Result<BackupData,DbError>{ match self.call(Request::Backup{session:self.session},cancel,3600000).await?.reply{Reply::Backup(b)=>Ok(b),_=>Err(invalid_reply())} }
    async fn restore(&self,backup:&BackupData,overwrite:bool,cancel:CancelToken)->Result<(),DbError>{ match self.call(Request::Restore{session:self.session,backup:backup.clone(),overwrite},cancel,3600000).await?.reply{Reply::Unit=>Ok(()),_=>Err(invalid_reply())} }
    async fn roles(&self)->Result<RolesSnapshot,DbError>{ match self.call(Request::Roles{session:self.session},CancelToken::new(),30000).await?.reply{Reply::Roles(r)=>Ok(r),_=>Err(invalid_reply())} }
    async fn roles_page(&self, query: &RolesPageQuery)->Result<RolesSnapshot,DbError>{
        match self.call(Request::RolesPage{session:self.session,query:query.clone()},CancelToken::new(),30000).await {
            Ok(response) => match response.reply { Reply::Roles(r)=>Ok(r),_=>Err(invalid_reply()) },
            Err(error) if error.kind == ErrorKind::Unsupported => page_roles_snapshot(self.roles().await?, query),
            Err(error) => Err(error),
        }
    }
    async fn plan_grants(&self,changes:&[GrantChange])->Result<GrantPlan,DbError>{ match self.call(Request::PlanGrants{session:self.session,changes:changes.to_vec()},CancelToken::new(),30000).await?.reply{Reply::Grants(p)=>Ok(p),_=>Err(invalid_reply())} }

    async fn apply_migration(&self,req:&ExecRequest,cancel:CancelToken)->Result<Vec<ResultSet>,DbError>{
        match self.call(Request::ApplyMigration{session:self.session,req:req.clone()},cancel,req.timeout_ms).await?.reply{Reply::Migration(r)=>Ok(r),_=>Err(invalid_reply())}
    }

    async fn schema_snapshot(&self,scope:Option<&str>)->Result<SchemaSnapshot,DbError>{
        match self.call(Request::SchemaSnapshot{session:self.session,scope:scope.map(str::to_owned)},CancelToken::new(),30000).await?.reply {Reply::Snapshot(s)=>Ok(s),_=>Err(invalid_reply())}
    }
    async fn ddl_render(&self,diff:&SnapshotDiff)->Result<Vec<String>,DbError>{
        match self.call(Request::DdlRender{session:self.session,diff:diff.clone()},CancelToken::new(),30000).await?.reply {Reply::Ddl(s)=>Ok(s),_=>Err(invalid_reply())}
    }
    async fn import_rows(&self,req:&EditRequest,preview:bool,cancel:CancelToken)->Result<ImportOutput,DbError>{
        match self.call(Request::ImportRows{session:self.session,req:req.clone(),preview},cancel,req.timeout_ms).await?.reply {Reply::Import(s)=>Ok(s),_=>Err(invalid_reply())}
    }

    async fn execute_stream(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
        sink: &dyn ResultSink,
    ) -> Result<Vec<ResultSet>, DbError> {
        if self.closed.is_cancelled() {
            return Err(disconnected());
        }
        let response = self
            .process
            .call_with_sink(
                if self.incremental && req.row_limit > 2000 { Request::ExecuteStream { session: self.session, req: req.clone() } } else { Request::Execute { session: self.session, req: req.clone() } },
                cancel,
                Duration::from_millis(req.timeout_ms).saturating_add(RESPONSE_GRACE),
                Some(sink),
            )
            .await?;
        if !matches!(response.reply, Reply::Unit) {
            return Err(invalid_reply());
        }
        Ok(response.results)
    }
    async fn ping(&self) -> Result<(), DbError> {
        match self
            .call(
                Request::Ping {
                    session: self.session,
                },
                CancelToken::new(),
                5000,
            )
            .await?
            .reply
        {
            Reply::Unit => Ok(()),
            _ => Err(invalid_reply()),
        }
    }
    async fn server_info(&self) -> Result<String, DbError> {
        match self
            .call(
                Request::ServerInfo {
                    session: self.session,
                },
                CancelToken::new(),
                5000,
            )
            .await?
            .reply
        {
            Reply::Info(info) => Ok(info),
            _ => Err(invalid_reply()),
        }
    }
    async fn list_children(&self, parent: Option<&str>) -> Result<Vec<ExplorerNode>, DbError> {
        match self
            .call(
                Request::ListChildren {
                    session: self.session,
                    parent: parent.map(str::to_owned),
                },
                CancelToken::new(),
                30000,
            )
            .await?
            .reply
        {
            Reply::Children(nodes) => Ok(nodes),
            _ => Err(invalid_reply()),
        }
    }
    async fn browse(&self, node_id: &str, page: PageRequest) -> Result<ResultSet, DbError> {
        match self
            .call(
                Request::Browse {
                    session: self.session,
                    node_id: node_id.into(),
                    page,
                },
                CancelToken::new(),
                30000,
            )
            .await?
            .reply
        {
            Reply::ResultSet(result) => Ok(result),
            _ => Err(invalid_reply()),
        }
    }
    async fn execute(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<Vec<ResultSet>, DbError> {
        Ok(self.execute_measured(req, cancel).await?.0)
    }
    async fn expand(&self, element_id: &str, limit: u32) -> Result<ResultSet, DbError> {
        match self
            .call(
                Request::Expand {
                    session: self.session,
                    element_id: element_id.into(),
                    limit,
                },
                CancelToken::new(),
                30000,
            )
            .await?
            .reply
        {
            Reply::ResultSet(result) => Ok(result),
            _ => Err(invalid_reply()),
        }
    }
    async fn schema_graph(&self, scope: Option<&str>) -> Result<SchemaGraph, DbError> {
        match self
            .call(
                Request::SchemaGraph {
                    session: self.session,
                    scope: scope.map(str::to_owned),
                },
                CancelToken::new(),
                30000,
            )
            .await?
            .reply
        {
            Reply::SchemaGraph(graph) => Ok(graph),
            _ => Err(invalid_reply()),
        }
    }
    async fn explain(&self, query: &str, analyze: bool, cancel: CancelToken) -> Result<PlanTree, DbError> {
        match self.call(Request::Explain { session: self.session, query: query.into(), analyze }, cancel, 30000).await?.reply {
            Reply::Plan(plan) => Ok(plan),
            _ => Err(invalid_reply()),
        }
    }
    async fn preview(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<ChangePreview, DbError> {
        match self
            .call(
                Request::Preview {
                    session: self.session,
                    req: req.clone(),
                },
                cancel,
                req.timeout_ms,
            )
            .await?
            .reply
        {
            Reply::Preview(preview) => Ok(preview),
            _ => Err(invalid_reply()),
        }
    }
    async fn preview_edits(
        &self,
        req: &EditRequest,
        cancel: CancelToken,
    ) -> Result<ChangePreview, DbError> {
        match self
            .call(
                Request::PreviewEdits {
                    session: self.session,
                    req: req.clone(),
                },
                cancel,
                req.timeout_ms,
            )
            .await?
            .reply
        {
            Reply::Preview(preview) => Ok(preview),
            _ => Err(invalid_reply()),
        }
    }
    async fn apply_edits(
        &self,
        req: &EditRequest,
        cancel: CancelToken,
    ) -> Result<EditResult, DbError> {
        match self
            .call(
                Request::ApplyEdits {
                    session: self.session,
                    req: req.clone(),
                },
                cancel,
                req.timeout_ms,
            )
            .await?
            .reply
        {
            Reply::Edits(result) => Ok(result),
            _ => Err(invalid_reply()),
        }
    }
    async fn close(&self) {
        if !self.closed.is_cancelled() {
            let _ = self
                .call(
                    Request::Close {
                        session: self.session,
                    },
                    CancelToken::new(),
                    5000,
                )
                .await;
            self.closed.cancel();
            self.lease.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        }
    }
}


struct ProgressAdapter{tx:tokio::sync::mpsc::UnboundedSender<(u64,u64)>}
impl ResultSink for ProgressAdapter{
 fn start(&self,_:usize,_:&ResultSet)->Result<(),DbError>{Ok(())}
 fn rows(&self,_:usize,rows:Vec<Vec<Value>>)->Result<(),DbError>{for row in rows {if let [Value::Int(done),Value::Int(total)]=row.as_slice(){let done=done.parse().map_err(|_|DbError::query("Invalid progress"))?;let total=total.parse().map_err(|_|DbError::query("Invalid progress"))?;self.tx.send((done,total)).map_err(|_|DbError::internal("Progress channel closed"))?;}else{return Err(DbError::query("Invalid progress"));}}Ok(())}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sqlite() -> Option<PathBuf> {
        std::env::var_os("AXDB_DRIVER_DIR").map(|p| {
            PathBuf::from(p).join(format!(
                "axdb-driver-sqlite{}",
                std::env::consts::EXE_SUFFIX
            ))
        })
    }
    fn config() -> ConnectConfig {
        ConnectConfig {
            fields: std::collections::BTreeMap::from([("path".into(), ":memory:".into())]),
            secret: None,
            connect_timeout_ms: 2000,
        }
    }
    fn request(text: &str) -> ExecRequest {
        serde_json::from_value(serde_json::json!({"text":text})).unwrap()
    }

    #[tokio::test]
    async fn multiplexes_sessions_and_restarts_after_crash_without_replaying_old_sessions() {
        let Some(path) = sqlite() else {
            return;
        };
        let driver = PluginDriver::load(path).await.unwrap();
        assert!(
            driver.process.lock().await.is_none(),
            "probe does not keep a process alive"
        );
        let a = driver.connect_plugin(&config()).await.unwrap();
        let b = driver.connect_plugin(&config()).await.unwrap();
        assert!(Arc::ptr_eq(&a.process, &b.process));
        assert_ne!(a.session, b.session);
        a.execute(
            &request("CREATE TABLE private(n); INSERT INTO private VALUES(1)"),
            CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            b.execute(&request("SELECT * FROM private"), CancelToken::new())
                .await
                .unwrap_err()
                .kind,
            ErrorKind::Query
        );
        let pid = a.process.child.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref().unwrap().id();
        a.process
            .child
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .start_kill()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), a.process.dead.cancelled())
            .await
            .unwrap();
        assert_eq!(a.ping().await.unwrap_err().kind, ErrorKind::Connection);
        assert_eq!(b.ping().await.unwrap_err().kind, ErrorKind::Connection);
        let c = driver.connect_plugin(&config()).await.unwrap();
        assert_ne!(pid, c.process.child.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref().unwrap().id());
        c.ping().await.unwrap();
        assert_eq!(a.ping().await.unwrap_err().kind, ErrorKind::Connection);
        c.close().await;
    }

    #[tokio::test]
    async fn missing_cancel_reply_does_not_kill_a_responsive_shared_driver() {
        let Some(path) = sqlite() else { return; };
        let driver = PluginDriver::load(path).await.unwrap();
        let session = driver.connect_plugin(&config()).await.unwrap();
        let sibling = driver.connect_plugin(&config()).await.unwrap();
        let process = session.process.clone();
        let token = CancelToken::new();
        let stop = token.clone();
        let session_id = session.session;
        let caller = process.clone();
        let run = tokio::spawn(async move {
            caller.call(Request::Execute { session: session_id, req: request("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n") }, token, Duration::from_secs(30)).await
        });
        // Withhold only this request's response to simulate cancellation whose
        // acknowledgement is lost. The driver and its other sessions remain live.
        let (sender, mut held) = mpsc::channel(8);
        let original = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(pending) = process.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner).values_mut().next() {
                    break std::mem::replace(&mut pending.sender, sender.clone());
                }
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
        let drain = tokio::spawn(async move { while held.recv().await.is_some() {} });
        stop.cancel();
        let result = tokio::time::timeout(RESPONSE_GRACE + HANDSHAKE_TIMEOUT + Duration::from_secs(2), run).await.unwrap().unwrap();
        assert!(matches!(result, Err(DbError { kind: ErrorKind::Cancelled, .. })));
        assert!(!process.dead.is_cancelled(), "cancelling one request killed the shared driver");
        sibling.ping().await.unwrap();
        session.ping().await.unwrap();
        drop(original);
        drain.abort();
        session.close().await;
        sibling.close().await;
    }

    #[tokio::test]
    async fn cancellation_propagates_and_large_results_reassemble() {
        let Some(path) = sqlite() else {
            return;
        };
        let driver = PluginDriver::load(path).await.unwrap();
        let session = driver.connect_plugin(&config()).await.unwrap();
        let cancel = CancelToken::new();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            trigger.cancel();
        });
        assert_eq!(session.execute(&request("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n"), cancel).await.unwrap_err().kind, ErrorKind::Cancelled);
        let mut req = request("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000) SELECT x, '한글' FROM n");
        req.row_limit = 100000;
        let (result, first) = session
            .execute_measured(&req, CancelToken::new())
            .await
            .unwrap();
        assert_eq!(result[0].row_count, 100000);
        let rows = &result[0].table.as_ref().unwrap().rows;
        assert_eq!(rows.len(), 100000);
        assert_eq!(rows[99999][0], Value::Int("100000".into()));
        assert!(first > Duration::ZERO);
        session.close().await;
        assert_eq!(
            session.ping().await.unwrap_err().kind,
            ErrorKind::Connection
        );
    }

    #[test]
    fn stderr_redacts_overlapping_and_multiline_secrets() {
        assert_eq!(
            redact_stderr(
                b"secret-long secret second",
                &[
                    "secret".into(),
                    "secret-long".into(),
                    "first\nsecond".into()
                ]
            ),
            "*** *** ***"
        );
    }

    #[test]
    fn result_memory_budget_rejects_overflow() {
        let (sender, _) = mpsc::channel(1);
        let mut pending = PendingResponse { sender, bytes: 0, backup_stream:false };
        assert!(pending.add(MAX_RESPONSE));
        assert!(!pending.add(1));
        assert!(!pending.add(usize::MAX));
    }

    #[test]
    fn only_bounded_backup_chunks_bypass_the_aggregate_response_budget() {
        let (sender,_) = mpsc::channel(1);
        let mut pending=PendingResponse{sender,bytes:MAX_RESPONSE,backup_stream:true};
        let chunk=|len|Message::Chunk{id:1,chunk:ExecuteChunk::Backup{data:BackupData{format:String::new(),data:vec![0;len],counts:vec![]}}};
        assert!(pending.admit(&chunk(BACKUP_CHUNK_BYTES),BACKUP_CHUNK_BYTES.div_ceil(3)*4+128));
        assert_eq!(pending.bytes,MAX_RESPONSE);
        let oversized=chunk(BACKUP_CHUNK_BYTES+1);
        assert!(!pending.admit(&oversized,encode(&oversized).unwrap().len()));
        assert!(!pending.admit(&chunk(1),MAX_FRAME));
        pending.backup_stream=false;
        assert!(!pending.admit(&chunk(1),100));
    }

    #[cfg(unix)]
    fn script(body: &str) -> (tempfile::TempDir, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("driver");
        std::fs::write(&path, format!("#!/usr/bin/env python3\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        (dir, path)
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::print_stderr)] // Intentional notice and benchmark output from the driver lifecycle test.
    async fn idle_shutdown_protects_live_sessions_invalidates_stale_deadlines_and_reaps() {
        let Some(path) = sqlite() else { eprintln!("AXDB_DRIVER_DIR unset: real idle lifecycle check not run"); return; };
        let driver = PluginDriver::load(path).await.unwrap();
        let first = driver.connect_plugin(&config()).await.unwrap();
        let process = first.process.clone();
        let pid = process.child.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref().unwrap().id().unwrap();
        let generation = process.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner).generation;
        assert!(!process.expire_idle(generation), "live session must survive");
        first.close().await;
        assert!(process.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner).idle.is_some());
        let second = driver.connect_plugin(&config()).await.unwrap();
        assert!(Arc::ptr_eq(&process, &second.process));
        assert!(process.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner).idle.is_none(), "reuse cancels idle timer");
        second.close().await;
        assert!(!process.expire_idle(generation), "stale deadline must not stop a reused process");
        let rss = Command::new("/bin/ps").args(["-p", &pid.to_string(), "-o", "rss="]).output().await.unwrap();
        let rss_kib: u64 = std::str::from_utf8(&rss.stdout).unwrap().trim().parse().unwrap();
        let generation = process.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner).generation;
        assert!(process.expire_idle(generation));
        process.stop().await;
        assert!(process.reserve().is_none());
        let status = Command::new("/bin/ps").args(["-p", &pid.to_string(), "-o", "pid="]).output().await.unwrap();
        assert!(status.stdout.is_empty(), "terminated driver must be reaped");
        let fresh = driver.connect_plugin(&config()).await.unwrap();
        assert!(!Arc::ptr_eq(&process, &fresh.process));
        fresh.ping().await.unwrap();
        fresh.close().await;
        fresh.process.stop().await;
        let default_idle_seconds = DRIVER_IDLE_TIMEOUT.as_secs();
        eprintln!("{{\"benchmark\":\"sqlite-idle-expiry-boundary\",\"before_rss_kib\":{rss_kib},\"after_rss_kib\":0,\"default_idle_seconds\":{default_idle_seconds},\"real_five_minute_wait\":false}}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancelling_handshake_reaps_probe_before_returning() {
        let state = tempfile::tempdir().unwrap();
        let precancelled = CancelToken::new();
        precancelled.cancel();
        assert_eq!(
            PluginDriver::load_cancelable(state.path().join("must-not-start"), precancelled)
                .await
                .err()
                .unwrap()
                .kind,
            ErrorKind::Cancelled
        );
        let pid_file = state.path().join("probe.pid");
        let (_dir, path) = script(&format!(
            "import os,time\nopen({:?},'w').write(str(os.getpid()))\ntime.sleep(30)",
            pid_file.to_string_lossy()
        ));
        let cancel = CancelToken::new();
        let token = cancel.clone();
        let task = tokio::spawn(PluginDriver::load_cancelable(path, token));
        let pid = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(pid) = std::fs::read_to_string(&pid_file)
                    .ok()
                    .and_then(|s| s.parse::<u32>().ok())
                {
                    break pid.to_string();
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        cancel.cancel();
        assert_eq!(
            task.await.unwrap().err().unwrap().kind,
            ErrorKind::Cancelled
        );
        let status = std::process::Command::new("ps")
            .args(["-p", &pid])
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(
            !status.success(),
            "probe must be reaped before candidate deletion"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn incompatible_protocol_is_rejected_and_silent_handshake_is_bounded() {
        let manifest = driver_postgres::PostgresDriver::new().manifest().clone();
        let bytes = encode(&Message::Result {
            id: 1,
            result: Reply::Handshake {
                protocol: 2,
                manifest,
            },
        })
        .unwrap();
        let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
        frame.extend(bytes);
        let hex: String = frame.iter().map(|b| format!("{b:02x}")).collect();
        let (_dir, path) = script(&format!("import os,sys,time\nassert 'AXDB_TEST_PG' not in os.environ\nassert 'AXDB_DEV_SECRET_STORE' not in os.environ\nsys.stdout.buffer.write(bytes.fromhex('{hex}'))\nsys.stdout.buffer.flush()\ntime.sleep(30)"));
        assert_eq!(
            PluginDriver::load(path).await.err().unwrap().kind,
            ErrorKind::Unsupported
        );
        let (_dir, path) = script("import time\ntime.sleep(30)");
        let start = Instant::now();
        assert_eq!(
            PluginDriver::load(path).await.err().unwrap().kind,
            ErrorKind::Connection
        );
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}
