//! Local discovery only: installed signed packages and debug workspace executables.
use crate::plugin_host::PluginDriver;
use dbcore::{DbError, Registry};
use std::{io::Read, path::Path, sync::Arc};

pub use crate::driver_install::installed_executables as scan_install_dir;

pub fn read_manifest(path: &Path) -> Result<axdb_driver_package::Manifest, DbError> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(1024 * 1024 + 1).read_to_end(&mut bytes))
        .map_err(|_| DbError::internal("Cannot read driver manifest"))?;
    if bytes.len() > 1024 * 1024 {
        return Err(DbError::internal("Driver manifest exceeds 1 MiB"));
    }
    let manifest: axdb_driver_package::Manifest =
        serde_json::from_slice(&bytes).map_err(|_| DbError::internal("Invalid driver manifest"))?;
    manifest
        .validate()
        .map_err(|_| DbError::internal("Invalid driver manifest"))?;
    Ok(manifest)
}

pub fn registry(root: &Path) -> Registry {
    let registry = Registry::new();
    let mut executables = scan_install_dir(root).unwrap_or_else(|error| {
        tracing::warn!(target: "axdb::drivers", %error, "Cannot scan installed drivers");
        Vec::new()
    });
    #[cfg(debug_assertions)]
    if let Some(dir) = std::env::var_os("AXDB_DRIVER_DIR")
        && let Ok(entries) = std::fs::read_dir(dir)
    {
        let mut debug: Vec<std::path::PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path.file_name().and_then(|s| s.to_str()).is_some_and(|s| {
                            s.starts_with("axdb-driver-")
                                && (!s.contains('.') || s.ends_with(".exe"))
                    })
            })
            .collect();
        debug.sort();
        executables.extend(debug);
    }
    if executables.is_empty() {
        return registry;
    }
    // Host::new is synchronous, including when called from an existing Tokio runtime.
    let drivers = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| DbError::internal("Cannot start driver discovery"))?;
        runtime.block_on(async move {
            let mut drivers = Vec::new();
            for executable in executables {
                match PluginDriver::load(executable).await {
                    Ok(driver) => drivers.push(driver),
                    Err(error) => {
                        tracing::warn!(target: "axdb::drivers", %error, "Driver handshake rejected")
                    }
                }
            }
            Ok::<_, DbError>(drivers)
        })
    })
    .join();
    if let Ok(Ok(drivers)) = drivers {
        for driver in drivers {
            registry.register(Arc::new(driver));
        }
    }
    registry
}
