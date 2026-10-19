//! Driver catalog refresh is explicitly triggered by the driver screen, never startup.
use crate::{plugin_host::PluginDriver, storage, CommandResult, Host};
use axdb_driver_package::{
    self as package, Catalog, CatalogEntry, Manifest, SignedCatalog, TrustedKey,
};
use dbcore::{CancelToken, DbError, Driver, Registry};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{ipc::Channel, Emitter, Manager};

const MAX_INDEX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;
fn error(error: impl std::fmt::Display) -> DbError {
    DbError::internal(error.to_string())
}
fn require(condition: bool, message: &str) -> CommandResult<()> {
    if condition {
        Ok(())
    } else {
        Err(error(message))
    }
}
pub fn host_target() -> &'static str {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        _ => "unsupported",
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledMarker {
    pub version: String,
    pub executable: String,
    #[serde(default)]
    pub allow_unverified: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Installed {
    pub id: String,
    pub version: String,
}
#[derive(Clone, Serialize)]
pub struct CatalogDriver {
    #[serde(flatten)]
    pub entry: CatalogEntry,
    pub installed_version: Option<String>,
}
#[derive(Clone, Serialize)]
pub struct CatalogSnapshot {
    pub drivers: Vec<CatalogDriver>,
    pub installed: Vec<Installed>,
    pub refreshing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}
#[derive(Clone, Serialize)]
pub struct Progress {
    pub stage: &'static str,
    pub downloaded: u64,
    pub bytes: u64,
    pub total: Option<u64>,
    #[serde(rename = "bytesPerSecond")]
    pub bytes_per_second: f64,
}
impl Progress {
    fn new(stage: &'static str, bytes: u64, total: Option<u64>) -> Self {
        Self {
            stage,
            downloaded: bytes,
            bytes,
            total,
            bytes_per_second: 0.0,
        }
    }
}
#[derive(Serialize, Deserialize)]
struct CachedCatalog {
    index: SignedCatalog,
    etag: Option<String>,
}

struct ValidatedCatalog {
    digest: String,
    catalog: Catalog,
    etag: Option<String>,
}
pub struct Installer {
    client: reqwest::Client,
    keys: Vec<TrustedKey>,
    index_url: String,
    #[cfg(test)]
    allow_loopback: bool,
    refreshing: AtomicBool,
    warning: Mutex<Option<String>>,
    operation: tokio::sync::Mutex<()>,
    active: Mutex<Option<CancelToken>>,
    validated: Mutex<Option<ValidatedCatalog>>,
}
impl Default for Installer {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
                .timeout(Duration::from_secs(90))
                .build()
                .expect("TLS HTTP client"),
            #[cfg(test)]
            allow_loopback: false,
            keys: package::official_keys(),
            index_url: package::INDEX_URL.into(),
            refreshing: AtomicBool::new(false),
            warning: Mutex::new(None),
            operation: tokio::sync::Mutex::new(()),
            active: Mutex::new(None),
            validated: Mutex::new(None),
        }
    }
}
struct InstallGuard<'a> {
    installer: &'a Installer,
    token: CancelToken,
}
impl Drop for InstallGuard<'_> {
    fn drop(&mut self) {
        self.token.cancel();
        if let Ok(mut active) = self.installer.active.lock() {
            *active = None;
        }
    }
}
async fn cancellable<T>(
    token: &CancelToken,
    future: impl std::future::Future<Output = CommandResult<T>>,
) -> CommandResult<T> {
    tokio::select! { biased; _ = token.cancelled() => Err(error("Driver installation cancelled")), result = future => result }
}
fn progress_channel(
    channel: Channel<Progress>,
) -> (
    tokio::sync::watch::Sender<Progress>,
    tauri::async_runtime::JoinHandle<()>,
) {
    let (sender, mut receiver) = tokio::sync::watch::channel(Progress::new("download", 0, None));
    let task = tauri::async_runtime::spawn(async move {
        let mut previous = None;
        while receiver.changed().await.is_ok() {
            if let Some(previous) = previous {
                tokio::time::sleep_until(previous + Duration::from_millis(100)).await;
            }
            let event = receiver.borrow_and_update().clone();
            if channel.send(event).is_err() {
                break;
            }
            previous = Some(tokio::time::Instant::now());
        }
    });
    (sender, task)
}
fn read_limited(path: &Path, maximum: u64) -> CommandResult<Vec<u8>> {
    require(
        std::fs::metadata(path).map_err(error)?.is_file(),
        "Driver artifacts must be regular files",
    )?;
    let file = std::fs::File::open(path).map_err(error)?;
    require(
        file.metadata().map_err(error)?.len() <= maximum,
        "File exceeds size limit",
    )?;
    let mut data = Vec::new();
    file.take(maximum + 1)
        .read_to_end(&mut data)
        .map_err(error)?;
    require(data.len() as u64 <= maximum, "File exceeds size limit")?;
    Ok(data)
}
fn marker(root: &Path, id: &str) -> CommandResult<Option<InstalledMarker>> {
    package::validate_id(id).map_err(error)?;
    let path = root.join(id).join("current.json");
    match read_limited(&path, 16 * 1024) {
        Ok(data) => {
            let marker: InstalledMarker = serde_json::from_slice(&data).map_err(error)?;
            package::safe_relative(&marker.executable).map_err(error)?;
            require(
                semver::Version::parse(&marker.version).is_ok(),
                "Invalid installed version",
            )?;
            require(
                marker.executable.starts_with(&format!(
                    "{}/bin/{}/",
                    marker.version,
                    host_target()
                )),
                "Invalid installed executable path",
            )?;
            Ok(Some(marker))
        }
        Err(_) if !path.try_exists().map_err(error)? => Ok(None),
        Err(error) => Err(error),
    }
}
fn installed(root: &Path) -> CommandResult<Vec<Installed>> {
    let mut result = Vec::new();
    if !root.exists() {
        return Ok(result);
    }
    for entry in std::fs::read_dir(root).map_err(error)? {
        let entry = entry.map_err(error)?;
        let id = entry.file_name().to_string_lossy().to_string();
        if package::validate_id(&id).is_err() || !entry.file_type().map_err(error)?.is_dir() {
            continue;
        }
        match marker(root, &id) {
            Ok(Some(marker)) => result.push(Installed {
                id,
                version: marker.version,
            }),
            Ok(None) => {}
            Err(failure) => {
                tracing::warn!(driver=%id, error=%failure, "Skipping invalid driver marker")
            }
        }
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}
/// Local-only startup scan. A malformed candidate cannot disable unrelated drivers.
pub fn installed_executables(root: &Path) -> CommandResult<Vec<PathBuf>> {
    let mut executables = Vec::new();
    if !root.exists() {
        return Ok(executables);
    }
    for entry in std::fs::read_dir(root).map_err(error)? {
        let entry = entry.map_err(error)?;
        let id = entry.file_name().to_string_lossy().to_string();
        if package::validate_id(&id).is_err() || !entry.file_type().map_err(error)?.is_dir() {
            continue;
        }
        match checked_executable(root, &id) {
            Ok(Some(path)) => executables.push(path),
            Ok(None) => {}
            Err(failure) => {
                tracing::warn!(driver=%id, error=%failure, "Skipping invalid installed driver")
            }
        }
    }
    Ok(executables)
}
fn checked_executable(root: &Path, id: &str) -> CommandResult<Option<PathBuf>> {
    let Some(marker) = marker(root, id)? else {
        return Ok(None);
    };
    let directory = root.join(id).join(&marker.version);
    let data = read_limited(&directory.join("manifest.json"), 64 * 1024)?;
    let manifest: Manifest = serde_json::from_slice(&data).map_err(error)?;
    manifest.validate().map_err(error)?;
    require(
        manifest.id == id && manifest.version == marker.version,
        "Installed manifest mismatch",
    )?;
    let trusted = if directory.join("manifest.sig").try_exists().map_err(error)? {
        let signature =
            serde_json::from_slice(&read_limited(&directory.join("manifest.sig"), 64 * 1024)?)
                .map_err(error)?;
        package::verify(&data, &signature, &package::official_keys()).map_err(error)?
    } else {
        false
    };
    require(
        trusted || marker.allow_unverified,
        "Installed driver is unverified",
    )?;
    let platform = manifest
        .platforms
        .get(host_target())
        .ok_or_else(|| error("Unsupported driver platform"))?;
    require(
        marker.executable == format!("{}/{}", marker.version, platform.path),
        "Installed executable mismatch",
    )?;
    let path = root.join(id).join(&marker.executable);
    let canonical = path.canonicalize().map_err(error)?;
    require(
        canonical.starts_with(root.join(id).canonicalize().map_err(error)?),
        "Installed executable escapes driver directory",
    )?;
    package::verify_sha256(
        &read_limited(&canonical, package::MAX_UNPACKED_BYTES)?,
        &platform.sha256,
    )
    .map_err(error)?;
    Ok(Some(canonical))
}
impl Installer {
    fn bundled_catalog(&self, bytes: &[u8]) -> CommandResult<Catalog> {
        if bytes.is_empty() {
            return Ok(Catalog::default());
        }
        require(
            bytes.len() as u64 <= MAX_INDEX_BYTES,
            "Driver catalog snapshot exceeds limit",
        )?;
        let index: SignedCatalog = serde_json::from_slice(bytes).map_err(error)?;
        index.verified(&self.keys).map_err(error)
    }
    fn cached(&self, root: &Path) -> CommandResult<(Catalog, Option<String>)> {
        let path = root.join("catalog-cache.json");
        if !path.try_exists().map_err(error)? {
            return Ok((
                self.bundled_catalog(include_bytes!(concat!(
                    env!("OUT_DIR"),
                    "/driver-index.json"
                )))?,
                None,
            ));
        }
        let bytes = read_limited(&path, MAX_INDEX_BYTES + 4096)?;
        let digest = package::sha256(&bytes);
        let mut validated = self.validated.lock().map_err(error)?;
        if let Some(cached) = validated.as_ref().filter(|cached| cached.digest == digest) {
            return Ok((cached.catalog.clone(), cached.etag.clone()));
        }
        let cached: CachedCatalog = serde_json::from_slice(&bytes).map_err(error)?;
        let catalog = cached.index.verified(&self.keys).map_err(error)?;
        *validated = Some(ValidatedCatalog {
            digest,
            catalog: catalog.clone(),
            etag: cached.etag.clone(),
        });
        Ok((catalog, cached.etag))
    }
    fn snapshot(&self, root: &Path) -> CommandResult<CatalogSnapshot> {
        let installed = installed(root)?;
        let (catalog, cache_warning) = match self.cached(root) {
            Ok((catalog, _)) => (catalog, None),
            Err(failure) => (Catalog::default(), Some(failure.message)),
        };
        let drivers = catalog
            .drivers
            .into_iter()
            .map(|entry| CatalogDriver {
                installed_version: installed
                    .iter()
                    .find(|driver| driver.id == entry.id)
                    .map(|driver| driver.version.clone()),
                entry,
            })
            .collect();
        Ok(CatalogSnapshot {
            drivers,
            installed,
            refreshing: self.refreshing.load(Ordering::Acquire),
            warning: cache_warning
                .or_else(|| self.warning.lock().ok().and_then(|warning| warning.clone())),
        })
    }
    fn validate_url(&self, url: &str) -> CommandResult<reqwest::Url> {
        let url = reqwest::Url::parse(url).map_err(error)?;
        #[cfg(test)]
        if self.allow_loopback
            && url.scheme() == "http"
            && url.host_str() == Some("127.0.0.1")
            && url.origin()
                == reqwest::Url::parse(&self.index_url)
                    .map_err(error)?
                    .origin()
        {
            return Ok(url);
        }
        require(
            url.scheme() == "https" && url.username().is_empty() && url.password().is_none(),
            "Driver downloads require HTTPS",
        )?;
        Ok(url)
    }
    async fn response_bytes(
        &self,
        mut response: reqwest::Response,
        max: u64,
        progress: &impl Fn(Progress),
    ) -> CommandResult<Vec<u8>> {
        let total = response.content_length();
        require(
            total.is_none_or(|size| size <= max),
            "Download exceeds size limit",
        )?;
        let mut bytes = Vec::new();
        let started = Instant::now();
        while let Some(chunk) = response.chunk().await.map_err(error)? {
            require(
                (bytes.len() as u64).saturating_add(chunk.len() as u64) <= max,
                "Download exceeds size limit",
            )?;
            bytes.extend_from_slice(&chunk);
            let mut event = Progress::new("download", bytes.len() as u64, total);
            event.bytes_per_second =
                bytes.len() as f64 / started.elapsed().as_secs_f64().max(0.001);
            progress(event);
        }
        Ok(bytes)
    }
    async fn refresh(&self, root: &Path) -> CommandResult<()> {
        let (_, etag) = self.cached(root).unwrap_or_default();
        let mut request = self.client.get(self.validate_url(&self.index_url)?);
        if let Some(etag) = etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let response = request.send().await.map_err(error)?;
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(());
        }
        let response = response.error_for_status().map_err(error)?;
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = self
            .response_bytes(response, MAX_INDEX_BYTES, &|_| {})
            .await?;
        let index: SignedCatalog = serde_json::from_slice(&bytes).map_err(error)?;
        let catalog = index.clone().verified(&self.keys).map_err(error)?;
        for driver in &catalog.drivers {
            self.validate_url(&driver.url)?;
        }
        std::fs::create_dir_all(root).map_err(error)?;
        let bytes = serde_json::to_vec(&CachedCatalog {
            index,
            etag: etag.clone(),
        })
        .map_err(error)?;
        storage::write_private(&root.join("catalog-cache.json"), &bytes)?;
        *self.validated.lock().map_err(error)? = Some(ValidatedCatalog {
            digest: package::sha256(&bytes),
            catalog,
            etag,
        });
        Ok(())
    }
    fn cancel(&self) -> CommandResult<bool> {
        let active = self.active.lock().map_err(error)?;
        if let Some(token) = active.as_ref() {
            token.cancel();
            Ok(true)
        } else {
            Ok(false)
        }
    }
    async fn install(
        &self,
        root: &Path,
        registry: &Registry,
        source: &str,
        allow_unverified: bool,
        progress: impl Fn(Progress),
    ) -> CommandResult<Installed> {
        let _operation = self.operation.lock().await;
        let token = CancelToken::new();
        *self.active.lock().map_err(error)? = Some(token.clone());
        let _active = InstallGuard {
            installer: self,
            token: token.clone(),
        };
        let local = Path::new(source).is_absolute();
        let (bytes, expected) = if local {
            require(
                Path::new(source)
                    .extension()
                    .is_some_and(|ext| ext == "axdriver"),
                "Choose a .axdriver file",
            )?;
            (
                read_limited(Path::new(source), package::MAX_PACKAGE_BYTES)?,
                None,
            )
        } else {
            package::validate_id(source).map_err(error)?;
            let (mut catalog, _) = self.cached(root)?;
            if marker(root, source)?.is_some()
                || !catalog.drivers.iter().any(|driver| driver.id == source)
            {
                cancellable(&token, self.refresh(root)).await?;
                catalog = self.cached(root)?.0;
            }
            let entry = catalog
                .drivers
                .into_iter()
                .find(|driver| driver.id == source)
                .ok_or_else(|| error("Unknown catalog driver"))?;
            let response = cancellable(&token, async {
                self.client
                    .get(self.validate_url(&entry.url)?)
                    .send()
                    .await
                    .map_err(error)?
                    .error_for_status()
                    .map_err(error)
            })
            .await?;
            let bytes = cancellable(
                &token,
                self.response_bytes(
                    response,
                    entry.size.min(package::MAX_PACKAGE_BYTES),
                    &progress,
                ),
            )
            .await?;
            require(bytes.len() as u64 == entry.size, "Package size mismatch")?;
            (bytes, Some(entry))
        };
        progress(Progress::new(
            "verify",
            bytes.len() as u64,
            Some(bytes.len() as u64),
        ));
        require(!token.is_cancelled(), "Driver installation cancelled")?;
        if let Some(entry) = &expected {
            package::verify_sha256(&bytes, &entry.sha256).map_err(error)?;
        }
        progress(Progress::new("signature", 0, None));
        let keys = self.keys.clone();
        let verify_token = token.clone();
        let verified = tokio::task::spawn_blocking(move || {
            package::read_package_cancellable(&bytes, &keys, local && allow_unverified, || {
                verify_token.is_cancelled()
            })
        })
        .await
        .map_err(error)?
        .map_err(error)?;
        require(!token.is_cancelled(), "Driver installation cancelled")?;
        if let Some(entry) = expected {
            require(
                verified.manifest.id == entry.id && verified.manifest.version == entry.version,
                "Catalog and package manifest mismatch",
            )?;
        }
        let id = verified.manifest.id.clone();
        let version = verified.manifest.version.clone();
        let previous = marker(root, &id)?;
        if let Some(previous) = &previous {
            require(
                semver::Version::parse(&version).map_err(error)?
                    > semver::Version::parse(&previous.version).map_err(error)?,
                "Driver is already installed or downgrade was requested",
            )?;
        }
        progress(Progress::new("extract", 0, None));
        let root_owned = root.to_path_buf();
        let trusted = verified.trusted;
        let package_name = verified.manifest.name.clone();
        let package_paradigm = verified.manifest.paradigm.clone();
        let package_executable = verified
            .manifest
            .platforms
            .get(host_target())
            .ok_or_else(|| error("No binary for this platform"))?
            .path
            .clone();
        let extract_token = token.clone();
        let staged = tokio::task::spawn_blocking(move || {
            verified.stage_cancellable(&root_owned, host_target(), || extract_token.is_cancelled())
        })
        .await
        .map_err(error)?
        .map_err(error)?;
        progress(Progress::new("handshake", 0, None));
        let driver = PluginDriver::load_cancelable(staged.executable.clone(), token.clone())
            .await
            .map_err(|failure| {
                if token.is_cancelled() {
                    DbError::new(
                        dbcore::ErrorKind::Cancelled,
                        "Driver installation cancelled",
                    )
                } else {
                    failure
                }
            })?;
        require(
            driver.manifest().id == id,
            "Driver handshake id does not match package manifest",
        )?;
        require(
            driver.manifest().display_name == package_name
                && serde_json::to_value(&driver.manifest().paradigm).map_err(error)?
                    == package_paradigm,
            "Driver handshake metadata does not match package manifest",
        )?;
        let data = InstalledMarker {
            version: version.clone(),
            executable: format!("{version}/{package_executable}"),
            allow_unverified: !trusted,
        };
        progress(Progress::new("register", 0, None));
        {
            let mut active = self.active.lock().map_err(error)?;
            require(!token.is_cancelled(), "Driver installation cancelled")?;
            // Cancel and commit share this lock: after this point cancel returns false.
            *active = None;
        }
        storage::write_private(
            &root.join(&id).join("current.json"),
            &serde_json::to_vec(&data).map_err(error)?,
        )?;
        registry.register(Arc::new(driver));
        staged.commit();
        // Keep exactly the former active version; cleanup failure cannot undo a valid activation.
        let keep: BTreeSet<&str> = std::iter::once(version.as_str())
            .chain(previous.as_ref().map(|p| p.version.as_str()))
            .collect();
        let prune = || -> CommandResult<()> {
            for entry in std::fs::read_dir(root.join(&id)).map_err(error)? {
                let entry = entry.map_err(error)?;
                if entry.file_type().map_err(error)?.is_dir()
                    && !keep.contains(entry.file_name().to_string_lossy().as_ref())
                {
                    std::fs::remove_dir_all(entry.path()).map_err(error)?;
                }
            }
            Ok(())
        };
        if let Err(failure) = prune() {
            tracing::warn!(error=%failure, "Cannot prune previous driver version");
        }
        progress(Progress::new("complete", 0, None));
        Ok(Installed { id, version })
    }
    async fn remove(&self, root: &Path, registry: &Registry, id: &str) -> CommandResult<()> {
        let _guard = self.operation.lock().await;
        package::validate_id(id).map_err(error)?;
        require(marker(root, id)?.is_some(), "Driver is not installed")?;
        let tombstone = root.join(format!(".remove-{id}-{}", uuid::Uuid::new_v4()));
        std::fs::rename(root.join(id), &tombstone).map_err(error)?;
        registry.unregister(id);
        std::fs::remove_dir_all(&tombstone).map_err(error)
    }
}

#[tauri::command]
pub async fn drivers_catalog(
    app: tauri::AppHandle,
    host: tauri::State<'_, Host>,
    installer: tauri::State<'_, Installer>,
) -> CommandResult<CatalogSnapshot> {
    let root = host.paths.data.join("drivers");
    let mut snapshot = installer.snapshot(&root)?;
    let start = !installer.refreshing.swap(true, Ordering::AcqRel);
    snapshot.refreshing = true;
    if start {
        tauri::async_runtime::spawn(async move {
            let installer = app.state::<Installer>();
            let warning = installer
                .refresh(&root)
                .await
                .err()
                .map(|error| error.message);
            if let Ok(mut state) = installer.warning.lock() {
                *state = warning;
            }
            installer.refreshing.store(false, Ordering::Release);
            if let Ok(snapshot) = installer.snapshot(&root) {
                let _ = app.emit("drivers-catalog-updated", snapshot);
            }
        });
    }
    Ok(snapshot)
}
#[tauri::command]
pub async fn driver_install(
    host: tauri::State<'_, Host>,
    installer: tauri::State<'_, Installer>,
    source: String,
    allow_unverified: Option<bool>,
    on_progress: Channel<Progress>,
) -> CommandResult<Installed> {
    let (progress, task) = progress_channel(on_progress);
    let result = installer
        .install(
            &host.paths.data.join("drivers"),
            &host.registry,
            &source,
            allow_unverified.unwrap_or(false),
            |event| {
                progress.send_replace(event);
            },
        )
        .await;
    drop(progress);
    let _ = task.await;
    result
}
#[tauri::command]
pub async fn driver_update(
    host: tauri::State<'_, Host>,
    installer: tauri::State<'_, Installer>,
    id: String,
    on_progress: Channel<Progress>,
) -> CommandResult<Installed> {
    let root = host.paths.data.join("drivers");
    require(marker(&root, &id)?.is_some(), "Driver is not installed")?;
    let (progress, task) = progress_channel(on_progress);
    let result = installer
        .install(&root, &host.registry, &id, false, |event| {
            progress.send_replace(event);
        })
        .await;
    drop(progress);
    let _ = task.await;
    result
}
#[tauri::command]
pub async fn driver_cancel(installer: tauri::State<'_, Installer>) -> CommandResult<bool> {
    installer.cancel()
}
#[tauri::command]
pub async fn driver_remove(
    host: tauri::State<'_, Host>,
    installer: tauri::State<'_, Installer>,
    id: String,
) -> CommandResult<()> {
    require(
        !host
            .sessions
            .lock()
            .map_err(error)?
            .values()
            .any(|record| record.profile.driver_id == id),
        "Close this driver's connections before removing it",
    )?;
    installer
        .remove(&host.paths.data.join("drivers"), &host.registry, &id)
        .await
}

#[tauri::command]
pub async fn driver_pick_file(window: tauri::WebviewWindow) -> CommandResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    require(
        window.label() == "main",
        "Driver installation is available only in the main window",
    )?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    window
        .dialog()
        .file()
        .set_parent(&window)
        .add_filter("AxDB driver", &["axdriver"])
        .pick_file(move |path| {
            let _ = sender.send(path);
        });
    let path = receiver.await.map_err(error)?;
    path.map(|path| {
        path.into_path().map_err(error).and_then(|path| {
            path.into_os_string()
                .into_string()
                .map_err(|_| error("Invalid file path"))
        })
    })
    .transpose()
}

#[cfg(test)]
mod tests;
