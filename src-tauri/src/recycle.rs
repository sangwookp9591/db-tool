//! Idle WebView replacement; snapshots stay in host RAM and never contain result rows.
use crate::{agent_bridge, driver_install, storage, CommandResult, DbError, Host};
use serde_json::{json, Value};
use std::{
    sync::{mpsc, Mutex},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

const IDLE: Duration = Duration::from_secs(120);
// M5 adoption gate: keep opt-in until all paired measurements and native regressions pass.
const DEFAULT_ENABLED: bool = false;
#[derive(Default, Debug, PartialEq)]
enum Phase {
    #[default]
    Idle,
    Waiting,
    Preparing,
    Committing,
    Replacing,
    Failed,
}
#[derive(Default)]
struct State {
    phase: Phase,
    generation: u64,
    inactive: Option<Instant>,
    large: bool,
    enabled: bool,
    snapshot: Option<Value>,
    activity: Option<mpsc::Sender<()>>,
}
impl State {
    fn reserve(&mut self, generation: u64) -> bool {
        if self.generation != generation || self.phase != Phase::Preparing {
            return false;
        }
        self.phase = Phase::Committing;
        true
    }
    fn activate(&mut self) -> bool {
        self.inactive = None;
        let replacing = matches!(self.phase, Phase::Replacing | Phase::Failed);
        if !replacing {
            self.generation += 1;
            self.phase = Phase::Idle;
            self.activity.take();
        }
        replacing
    }
}
#[derive(Default)]
pub struct Recycler(Mutex<State>);

fn enabled(settings: &Value) -> bool {
    cfg!(target_os = "macos")
        && settings["idle_memory_cleanup"]
            .as_bool()
            .unwrap_or(DEFAULT_ENABLED)
}
fn eligible(state: &State, now: Instant) -> bool {
    state.enabled
        && state.large
        && state.phase == Phase::Waiting
        && state
            .inactive
            .is_some_and(|at| now.saturating_duration_since(at) >= IDLE)
}
fn busy(host: &Host, installer: &driver_install::Installer) -> bool {
    !host.runs.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_empty()
        || !host.approvals.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_empty()
        || !host.previews.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_empty()
        || !installer.idle()
}
pub fn focus(app: &tauri::AppHandle, focused: bool) {
    let Some(recycler) = app.try_state::<Recycler>() else {
        return;
    };
    let mut state = recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if focused {
        let recover = state.phase == Phase::Failed;
        let replacing = state.activate();
        drop(state);
        if let Some(webview) = app.get_webview("main") {
            let _ = webview.set_focus();
        } else if replacing && recover {
            {
                let mut state = recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                state.phase = Phase::Replacing;
                state.activity = Some(activity());
            }
            let mut restored = false;
            if let Some(window) = app.get_window("main")
                && let Ok(size) = window.inner_size()
            {
                let config = app.config().app.windows[0].clone();
                let result = window.add_child(
                    tauri::webview::WebviewBuilder::from_config(&config).auto_resize(),
                    tauri::PhysicalPosition::new(0, 0),
                    size,
                );
                restored = result.is_ok();
            }
            if !restored {
                recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).phase = Phase::Failed;
            }
        }
        return;
    }
    if matches!(state.phase, Phase::Replacing | Phase::Failed) {
        return;
    }
    state.inactive = Some(Instant::now());
    state.phase = Phase::Waiting;
    state.generation += 1;
    let generation = state.generation;
    drop(state);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(IDLE).await;
        let Some(window) = app.get_window("main") else {
            return;
        };
        // Never hold state while querying Wry: a native focus callback also takes this lock.
        if window.is_focused().unwrap_or(true) {
            return;
        }
        let recycler = app.state::<Recycler>();
        let mut state = recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.generation != generation || !eligible(&state, Instant::now()) {
            return;
        }
        if busy(
            &app.state::<Host>(),
            &app.state::<driver_install::Installer>(),
        ) {
            return;
        }
        state.phase = Phase::Preparing;
        state.activity = Some(activity());
        drop(state);
        if app.emit_to("main", "recycle-prepare", generation).is_err() {
            let mut state = recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.generation == generation {
                state.phase = Phase::Idle;
                state.activity.take();
            }
        }
    });
}
#[tauri::command]
pub async fn recycle_settings(
    host: tauri::State<'_, Host>,
    recycler: tauri::State<'_, Recycler>,
    value: Option<bool>,
) -> CommandResult<bool> {
    let _guard = host.storage.lock().await;
    let mut settings = agent_bridge::read_settings(&host.paths.config)?;
    if let Some(value) = value {
        settings["idle_memory_cleanup"] = json!(value);
        storage::write_private(
            &host.paths.config.join("settings.json"),
            &serde_json::to_vec(&settings)
                .map_err(|_| DbError::internal("Cannot encode settings"))?,
        )?;
    }
    let value = enabled(&settings);
    let mut state = recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    state.enabled = value;
    if !value && state.phase != Phase::Replacing {
        state.generation += 1;
        state.phase = Phase::Idle;
        state.activity.take();
    }
    Ok(value)
}
#[tauri::command]
pub fn recycle_large(recycler: tauri::State<'_, Recycler>) {
    recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).large = true;
}
#[tauri::command]
pub fn recycle_restore(recycler: tauri::State<'_, Recycler>) -> Option<Value> {
    recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).snapshot.clone()
}
#[tauri::command]
pub fn recycle_ready(recycler: tauri::State<'_, Recycler>) {
    let mut state = recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.phase == Phase::Replacing {
        state.phase = Phase::Idle;
        state.large = false;
        state.snapshot = None;
        state.activity.take();
    }
}
#[tauri::command]
pub async fn recycle_commit(
    app: tauri::AppHandle,
    generation: u64,
    snapshot: Option<Value>,
) -> CommandResult<bool> {
    let recycler = app.state::<Recycler>();
    {
        let mut state = recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.reserve(generation) {
            return Ok(false);
        }
        if snapshot
            .as_ref()
            .is_none_or(|value| !value.is_object() || value.to_string().len() > 2 * 1024 * 1024)
        {
            state.phase = Phase::Idle;
            state.activity.take();
            return Ok(false);
        }
    }
    let bridge = app.state::<agent_bridge::Bridge>();
    if !bridge.pause_for_recycle().await {
        let mut state = recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.phase = Phase::Idle;
        state.activity.take();
        return Ok(false);
    }
    let (send, receive) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    let scheduled = app.run_on_main_thread(move || {
        let result = snapshot.map(|snapshot| replace(&handle, generation, snapshot)).unwrap_or_else(|| Err(DbError::internal("Idle cleanup snapshot unavailable")));
        let _ = send.send(result);
    });
    let result = if scheduled.is_err() {
        Err(DbError::internal("Cannot schedule idle cleanup"))
    } else {
        receive
            .await
            .unwrap_or_else(|_| Err(DbError::internal("Idle cleanup interrupted")))
    };
    if !matches!(result, Ok(true)) {
        let has_view = app.get_webview("main").is_some();
        {
            let mut state = recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            state.activity.take();
            if has_view {
                state.phase = Phase::Idle;
                state.snapshot = None;
            } else {
                state.phase = Phase::Failed;
            }
        }
        if has_view {
            bridge
                .resume_after_recycle(&app.state::<Host>().paths.config)
                .await?;
        } else if app
            .get_window("main")
            .is_some_and(|window| window.is_focused().unwrap_or(false))
        {
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || focus(&handle, true));
        }
    }
    result
}
fn replace(app: &tauri::AppHandle, generation: u64, snapshot: Value) -> CommandResult<bool> {
    let window = app
        .get_window("main")
        .ok_or_else(|| DbError::internal("No workbench window"))?;
    let host = app.state::<Host>();
    let native_busy = window.is_focused().unwrap_or(true)
        || busy(&host, &app.state::<driver_install::Installer>());
    let recycler = app.state::<Recycler>();
    let mut state = recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.generation != generation || state.phase != Phase::Committing {
        return Ok(false);
    }
    if !state.enabled || native_busy {
        state.phase = Phase::Idle;
        state.activity.take();
        return Ok(false);
    }
    drop(state);
    let size = window
        .inner_size()
        .map_err(|_| DbError::internal("Cannot read window size"))?;
    let config = app.config().app.windows[0].clone();
    {
        let mut state = recycler.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.generation != generation || state.phase != Phase::Committing {
            return Ok(false);
        }
        state.snapshot = Some(snapshot);
        state.phase = Phase::Replacing;
    }
    app.get_webview("main")
        .ok_or_else(|| DbError::internal("No workbench webview"))?
        .close()
        .map_err(|_| DbError::internal("Cannot close idle webview"))?;
    window
        .add_child(
            tauri::webview::WebviewBuilder::from_config(&config).auto_resize(),
            tauri::PhysicalPosition::new(0, 0),
            size,
        )
        .map_err(|_| DbError::internal("Cannot restore workbench webview"))?;
    Ok(true)
}
fn activity() -> mpsc::Sender<()> {
    let (end, receive) = mpsc::channel();
    #[cfg(target_os = "macos")]
    {
        let (started, ready) = mpsc::sync_channel(0);
        std::thread::spawn(move || {
            use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
            let info = NSProcessInfo::processInfo();
            let token = info.beginActivityWithOptions_reason(
                NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
                &NSString::from_str("Restore idle workbench"),
            );
            let _ = started.send(());
            let _ = receive.recv_timeout(Duration::from_secs(12));
            // The opaque token is created and ended on this thread exactly once.
            unsafe {
                info.endActivity(&token);
            }
        });
        let _ = ready.recv();
    }
    #[cfg(not(target_os = "macos"))]
    drop(receive);
    end
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_prepare_ticket_can_be_committed_only_once() {
        let mut state = State {
            phase: Phase::Preparing,
            generation: 7,
            ..State::default()
        };
        assert!(!state.reserve(6));
        assert_eq!(state.phase, Phase::Preparing);
        assert!(state.reserve(7));
        assert_eq!(state.phase, Phase::Committing);
        assert!(!state.reserve(7));
        state.activate();
        assert!(!state.reserve(7));
    }
    #[test]
    fn activation_cancels_before_close_and_preserves_an_in_progress_restore() {
        for phase in [Phase::Waiting, Phase::Preparing, Phase::Committing] {
            let mut state = State {
                phase,
                generation: 7,
                inactive: Some(Instant::now()),
                ..State::default()
            };
            assert!(!state.activate());
            assert_eq!(state.phase, Phase::Idle);
            assert_eq!(state.generation, 8);
            assert!(state.inactive.is_none());
        }
        let mut state = State {
            phase: Phase::Replacing,
            generation: 7,
            snapshot: Some(json!({"draft":"retain"})),
            ..State::default()
        };
        assert!(state.activate());
        assert_eq!(state.phase, Phase::Replacing);
        assert_eq!(state.generation, 7);
        assert_eq!(state.snapshot.as_ref().unwrap()["draft"], "retain");
        state.phase = Phase::Failed;
        assert!(state.activate());
        assert_eq!(state.phase, Phase::Failed);
        assert_eq!(state.snapshot.as_ref().unwrap()["draft"], "retain");
    }
    #[test]
    fn running_queries_and_unused_approval_receipts_block_recycling() {
        let dir = tempfile::tempdir().unwrap();
        let host = Host::new(storage::Paths {
            config: dir.path().into(),
            data: dir.path().into(),
            log: dir.path().into(),
        });
        let installer = driver_install::Installer::default();
        assert!(!busy(&host, &installer));
        let run = host.run("session", "query").unwrap();
        assert!(busy(&host, &installer));
        drop(run);
        assert!(!busy(&host, &installer));
        for approvals in [&host.approvals, &host.previews] {
            approvals.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(
                "receipt".into(),
                crate::permissions::Approval {
                    fingerprint: "fixture".into(),
                    created: Instant::now(),
                },
            );
            assert!(busy(&host, &installer));
            approvals.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
        }
    }
    #[test]
    fn idle_policy_requires_large_result_setting_and_full_two_minutes() {
        let at = Instant::now();
        let mut state = State {
            enabled: true,
            large: true,
            phase: Phase::Waiting,
            inactive: Some(at),
            ..State::default()
        };
        assert!(!eligible(&state, at + IDLE - Duration::from_millis(1)));
        assert!(eligible(&state, at + IDLE));
        for phase in [
            Phase::Idle,
            Phase::Preparing,
            Phase::Committing,
            Phase::Replacing,
            Phase::Failed,
        ] {
            state.phase = phase;
            assert!(!eligible(&state, at + IDLE));
        }
        state.phase = Phase::Waiting;
        state.large = false;
        assert!(!eligible(&state, at + IDLE));
        state.large = true;
        state.enabled = false;
        assert!(!eligible(&state, at + IDLE));
        assert!(!enabled(&json!({})));
    }
}
