use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

#[derive(Default)]
pub struct Exports(Mutex<HashMap<String, Grant>>);
struct Grant {
    path: PathBuf,
    created: Instant,
    written: bool,
}
#[derive(Serialize)]
pub struct Selection {
    path: PathBuf,
    token: String,
}
const TTL: Duration = Duration::from_secs(300);

fn selected_path(path: PathBuf) -> Result<PathBuf, String> {
    let name = path.file_name().ok_or("파일 이름이 없습니다")?;
    let parent = path
        .parent()
        .ok_or("저장 폴더가 없습니다")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let path = parent.join(name);
    if let Ok(meta) = path.symlink_metadata() {
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("일반 파일 경로만 저장할 수 있습니다".into());
        }
    }
    Ok(path)
}
impl Exports {
    fn remember(&self, path: PathBuf) -> Result<Selection, String> {
        let path = selected_path(path)?;
        let token = uuid::Uuid::new_v4().to_string();
        let mut grants = self.0.lock().map_err(|_| "내보내기 잠금 오류")?;
        grants.retain(|_, g| g.created.elapsed() < TTL && g.written);
        if grants.len() >= 16 {
            grants.clear();
        }
        grants.insert(
            token.clone(),
            Grant {
                path: path.clone(),
                created: Instant::now(),
                written: false,
            },
        );
        Ok(Selection { path, token })
    }
    fn consume(&self, token: &str, path: &Path) -> Result<Grant, String> {
        let grant = self
            .0
            .lock()
            .map_err(|_| "내보내기 잠금 오류")?
            .remove(token)
            .ok_or("저장 대화상자에서 경로를 먼저 선택하세요")?;
        if grant.written || grant.created.elapsed() >= TTL || grant.path != path {
            return Err("저장 경로 권한이 없거나 만료되었습니다".into());
        }
        Ok(grant)
    }
    fn write(&self, token: String, path: PathBuf, bytes: Vec<u8>) -> Result<(), String> {
        let mut grant = self.consume(&token, &path)?;
        if bytes.len() > 256 * 1024 * 1024 {
            return Err("내보내기 파일은 256 MiB 이하여야 합니다".into());
        }
        if selected_path(path.clone())? != path {
            return Err("저장 폴더가 변경되었습니다".into());
        }
        // Write beside the destination and rename only after flush, preserving the old file on failure.
        let temp = path.with_file_name(format!(".axdb-export-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> std::io::Result<()> {
            use std::io::Write;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temp, &path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result.map_err(|e| e.to_string())?;
        grant.written = true;
        grant.created = Instant::now();
        self.0
            .lock()
            .map_err(|_| "내보내기 잠금 오류")?
            .insert(token, grant);
        Ok(())
    }
}
fn main_window(window: &WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("메인 창에서만 내보낼 수 있습니다".into());
    }
    Ok(())
}
#[tauri::command]
pub async fn export_save_dialog(
    window: WebviewWindow,
    filename: String,
    extension: String,
) -> Result<Option<Selection>, String> {
    main_window(&window)?;
    if !["csv", "json", "md", "ndjson", "svg", "png", "sql", "axbackup"].contains(&extension.as_str())
        || Path::new(&filename).file_name().and_then(|n| n.to_str()) != Some(filename.as_str())
    {
        return Err("지원하지 않는 파일 이름 또는 형식입니다".into());
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    window
        .dialog()
        .file()
        .set_parent(&window)
        .set_file_name(filename)
        .add_filter("내보내기", &[&extension])
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let Some(path) = rx.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    Ok(Some(window.state::<Exports>().remember(path)?))
}
#[tauri::command]
pub async fn export_write(
    window: WebviewWindow,
    path: PathBuf,
    token: String,
    bytes: Vec<u8>,
) -> Result<(), String> {
    main_window(&window)?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || app.state::<Exports>().write(token, path, bytes))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn export_reveal(window: WebviewWindow, token: String) -> Result<(), String> {
    main_window(&window)?;
    let path = {
        let state = window.state::<Exports>();
        let grants = state.0.lock().map_err(|_| "내보내기 잠금 오류")?;
        let grant = grants
            .get(&token)
            .filter(|g| g.written && g.created.elapsed() < TTL)
            .ok_or("저장 기록이 만료되었습니다")?;
        grant.path.clone()
    };
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .status();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let status = std::process::Command::new("xdg-open")
        .arg(path.parent().ok_or("저장 폴더가 없습니다")?)
        .status();
    if status.map_err(|e| e.to_string())?.success() {
        Ok(())
    } else {
        Err("파일 위치를 열지 못했습니다".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_dialog_grants_allow_one_write_to_the_selected_path() {
        let dir = std::env::temp_dir().join(format!("axdb-export-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let path = dir.join("선택.csv");
        let exports = Exports::default();
        assert!(exports
            .write("forged".into(), path.clone(), vec![1])
            .is_err());
        assert!(!path.exists());
        let selected = exports.remember(path.clone()).unwrap();
        assert!(exports
            .write(selected.token, dir.join("other.csv"), vec![1])
            .is_err());
        let selected = exports.remember(path.clone()).unwrap();
        exports
            .write(selected.token.clone(), path.clone(), vec![0xef, 0xbb, 0xbf])
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), vec![0xef, 0xbb, 0xbf]);
        assert!(exports.write(selected.token, path.clone(), vec![]).is_err());
        let selected = exports.remember(path.clone()).unwrap();
        exports
            .write(selected.token, path.clone(), b"replacement".to_vec())
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
        #[cfg(unix)]
        {
            let link = dir.join("link.csv");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(exports.remember(link.clone()).is_err());
            std::fs::remove_file(&link).unwrap();
            let selected = exports.remember(link.clone()).unwrap();
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(exports.write(selected.token, link, vec![]).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
        }
        let selected = exports.remember(path.clone()).unwrap();
        exports
            .0
            .lock()
            .unwrap()
            .get_mut(&selected.token)
            .unwrap()
            .created = Instant::now() - TTL;
        assert!(exports.write(selected.token, path, vec![]).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
