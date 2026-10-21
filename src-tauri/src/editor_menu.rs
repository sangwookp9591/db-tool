use tauri::Emitter;
#[cfg(target_os = "macos")]
use tauri::Manager;

#[cfg(target_os = "macos")]
struct HistoryMenu {
    edit: tauri::menu::Submenu<tauri::Wry>,
    native: [tauri::menu::MenuItemKind<tauri::Wry>; 2],
    editor: [tauri::menu::MenuItem<tauri::Wry>; 2],
    owner: std::sync::Mutex<Option<String>>,
}
pub fn install(app: &tauri::App) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let menu = tauri::menu::Menu::default(app.handle())?;
        let edit = menu.items()?.into_iter().find_map(|item| match item {
            tauri::menu::MenuItemKind::Submenu(edit) if edit.text().is_ok_and(|text| text == "Edit") => Some(edit),
            _ => None,
        }).ok_or_else(|| tauri::Error::AssetNotFound("Tauri default Edit menu".into()))?;
        let native = edit.items()?;
        app.manage(HistoryMenu {
            edit,
            native: [native[0].clone(), native[1].clone()],
            editor: [
                tauri::menu::MenuItem::with_id(app, "editor-undo", "Undo", true, Some("CmdOrCtrl+Z"))?,
                tauri::menu::MenuItem::with_id(app, "editor-redo", "Redo", true, Some("CmdOrCtrl+Shift+Z"))?,
            ],
            owner: Default::default(),
        });
        app.set_menu(menu)?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
    Ok(())
}
pub fn event(app: &tauri::AppHandle, id: &str) {
    if let Some(action) = id.strip_prefix("editor-").filter(|action| matches!(*action, "undo" | "redo")) {
        let _ = app.emit_to("main", "editor-history", action);
    }
}
#[tauri::command]
pub async fn editor_history_mode(window: tauri::WebviewWindow, owner: String, active: bool) -> Result<(), String> {
    if window.label() != "main" || owner.is_empty() || owner.len() > 128 { return Err("Invalid editor history target".into()); }
    #[cfg(target_os = "macos")]
    {
        let history = window.state::<HistoryMenu>();
        let mut current = history.owner.lock().map_err(|_| "Editor history lock unavailable")?;
        if !active && current.as_ref() != Some(&owner) { return Ok(()); }
        let previous = current.is_some();
        *current = active.then_some(owner);
        if previous == active { return Ok(()); }
        // Cocoa's predefined undo: consumes Cmd+Z before CodeMirror receives keydown.
        history.edit.remove_at(0).map_err(|e|e.to_string())?;
        history.edit.remove_at(0).map_err(|e|e.to_string())?;
        if active {
            history.edit.insert(&history.editor[0], 0).map_err(|e|e.to_string())?;
            history.edit.insert(&history.editor[1], 1).map_err(|e|e.to_string())?;
        } else {
            history.edit.insert(&history.native[0], 0).map_err(|e|e.to_string())?;
            history.edit.insert(&history.native[1], 1).map_err(|e|e.to_string())?;
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = active;
    Ok(())
}
