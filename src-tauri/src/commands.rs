//! Commands the overlay and HUD can call. They take item ids, never raw content,
//! except where the user is explicitly sending their own edited text.
//!
//! Every command is `async` so none of them can block the UI thread, and every
//! command returns `Result` because Tauri requires that of async commands that
//! borrow state.

use crate::{
    capture::now_ms,
    paste,
    state::{Control, Notice, Shared},
    windows,
};
use serde::Serialize;
use std::{sync::Arc, time::Instant};
use tauri::State;
use zlip_config::Config;
use zlip_core::{Category, ItemId, Transform};
use zlip_engine::{
    CaptureOutcome, PasteOptions, SearchQuery,
    view::{ActionReport, FolderView, ItemDetail, ItemView, QueueView, StatusView, UiConfig},
};

type Shr<'a> = State<'a, Arc<Shared>>;
type CmdResult<T> = Result<T, String>;

#[tauri::command]
pub async fn search(
    shared: Shr<'_>,
    query: String,
    category: Option<Category>,
    folder: Option<String>,
) -> CmdResult<Vec<ItemView>> {
    let limit = shared.cfg.snapshot().ui.search_limit as usize;
    let q = SearchQuery {
        text: query,
        category,
        folder,
        limit,
    };
    Ok(shared.engine().search(&q, Instant::now()))
}

#[tauri::command]
pub async fn folders(shared: Shr<'_>) -> CmdResult<Vec<FolderView>> {
    Ok(shared.engine().folders())
}

#[tauri::command]
pub async fn item_detail(shared: Shr<'_>, id: u64) -> CmdResult<Option<ItemDetail>> {
    Ok(shared.engine().detail(ItemId(id), Instant::now()))
}

#[tauri::command]
pub async fn thumbnail(shared: Shr<'_>, id: u64) -> CmdResult<Option<String>> {
    Ok(shared.engine().thumbnail(ItemId(id)))
}

/// Explicit "show me the secret". The UI hides it again after a few seconds.
#[tauri::command]
pub async fn reveal(shared: Shr<'_>, id: u64) -> CmdResult<Option<String>> {
    Ok(shared
        .engine()
        .reveal(ItemId(id), Instant::now())
        .map(|s| s.to_string()))
}

#[tauri::command]
pub async fn paste(
    shared: Shr<'_>,
    id: u64,
    transforms: Vec<Transform>,
    plain: bool,
) -> CmdResult<()> {
    paste::spawn_item(
        Arc::clone(&shared),
        ItemId(id),
        PasteOptions { transforms, plain },
    );
    Ok(())
}

/// Paste text the user edited in the quick editor.
#[tauri::command]
pub async fn paste_text(
    shared: Shr<'_>,
    text: String,
    transforms: Vec<Transform>,
) -> CmdResult<()> {
    paste::spawn_text(
        Arc::clone(&shared),
        text,
        PasteOptions {
            transforms,
            plain: false,
        },
    );
    Ok(())
}

#[tauri::command]
pub async fn save_text(shared: Shr<'_>, text: String) -> CmdResult<()> {
    match shared.engine().save_text(text, now_ms(), Instant::now()) {
        CaptureOutcome::Failed(e) => Err(e),
        CaptureOutcome::Ignored(why) => Err(format!("Nothing was saved ({why}).")),
        CaptureOutcome::Stored { .. } | CaptureOutcome::Sensitive { .. } => Ok(()),
    }
}

#[tauri::command]
pub async fn toggle_pin(shared: Shr<'_>, id: u64) -> CmdResult<Option<bool>> {
    Ok(shared.engine().toggle_pin(ItemId(id)))
}

#[tauri::command]
pub async fn delete_item(shared: Shr<'_>, id: u64) -> CmdResult<bool> {
    let removed = shared.engine().delete(ItemId(id));
    // A deleted secret changes when the next expiry is due.
    let _ = shared.control.send(Control::Wake);
    windows::update_hud(&shared);
    Ok(removed)
}

#[tauri::command]
pub async fn clear_history(shared: Shr<'_>) -> CmdResult<usize> {
    Ok(shared.engine().clear_history())
}

#[tauri::command]
pub async fn queue_add(shared: Shr<'_>, id: u64) -> CmdResult<QueueView> {
    let view = shared.engine().queue_add(ItemId(id), Instant::now());
    windows::update_hud(&shared);
    Ok(view)
}

#[tauri::command]
pub async fn queue_clear(shared: Shr<'_>) -> CmdResult<()> {
    shared.engine().queue_clear();
    windows::update_hud(&shared);
    Ok(())
}

#[tauri::command]
pub async fn status(shared: Shr<'_>) -> CmdResult<StatusView> {
    Ok(shared.engine().status(Instant::now()))
}

#[tauri::command]
pub async fn set_private_mode(shared: Shr<'_>, on: bool) -> CmdResult<()> {
    shared.engine().set_private_mode(on);
    crate::tray::sync_private_mode(&shared.app, on);
    Ok(())
}

#[tauri::command]
pub async fn ui_config(shared: Shr<'_>) -> CmdResult<UiConfig> {
    let os = shared.os_info();
    Ok(shared.engine().ui_config(os))
}

#[tauri::command]
pub async fn take_notice(shared: Shr<'_>) -> CmdResult<Option<Notice>> {
    Ok(shared.take_notice())
}

#[tauri::command]
pub async fn hide_overlay(shared: Shr<'_>) -> CmdResult<()> {
    windows::hide_overlay(&shared);
    Ok(())
}

/// Everything the settings screen shows: the saved settings plus facts about this install.
#[derive(Debug, Serialize)]
pub struct SettingsView {
    pub config: Config,
    pub version: &'static str,
    /// The project page on GitHub: Settings links to it and looks there for updates.
    pub repo_url: &'static str,
    pub config_path: String,
    pub log_dir: String,
    /// `off`, `active` or `unavailable` (Windows refused the backup shortcut).
    pub backup_shortcut: &'static str,
}

#[tauri::command]
pub async fn get_settings(shared: Shr<'_>) -> CmdResult<SettingsView> {
    Ok(SettingsView {
        config: (*shared.cfg.snapshot()).clone(),
        version: env!("CARGO_PKG_VERSION"),
        repo_url: crate::links::REPO_URL,
        config_path: zlip_config::config_path().display().to_string(),
        log_dir: zlip_config::data_dir().join("logs").display().to_string(),
        backup_shortcut: zlip_platform::backup_state().as_str(),
    })
}

/// Validate, save and apply new settings. Nothing is written unless every value is valid,
/// and the error lists every problem (not just the first).
#[tauri::command]
pub async fn save_settings(shared: Shr<'_>, settings: Config) -> CmdResult<()> {
    let issues = settings.validate();
    if !issues.is_empty() {
        return Err(issues
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"));
    }
    shared.mark_self_write();
    zlip_config::save_config(&zlip_config::config_path(), &settings).map_err(|e| e.to_string())?;
    shared.cfg.set(settings.clone());
    windows::apply_live(&shared, &settings);
    Ok(())
}

/// "My shortcut stopped working": re-read the shortcuts and replace the keyboard hook.
#[tauri::command]
pub async fn repair_shortcuts(shared: Shr<'_>) -> CmdResult<()> {
    let cfg = shared.cfg.snapshot();
    let chords = cfg.hotkeys.global_chords().map_err(|e| e.to_string())?;
    zlip_platform::set_chords(&chords);
    zlip_platform::request_refresh();
    tracing::info!("keyboard hook refresh requested from the settings screen");
    Ok(())
}

/// Pause the global shortcuts while the settings screen records a new one.
#[tauri::command]
pub async fn pause_shortcuts(paused: bool) -> CmdResult<()> {
    zlip_platform::set_paused(paused);
    Ok(())
}

/// Whether Zlip starts with Windows. It is a per-user entry, so it needs no administrator rights.
#[tauri::command]
pub async fn get_autostart() -> CmdResult<bool> {
    tauri::async_runtime::spawn_blocking(crate::startup::is_enabled)
        .await
        .map_err(|e| e.to_string())?
}

/// Turn "start with Windows" on or off.
#[tauri::command]
pub async fn set_autostart(on: bool) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || crate::startup::set_enabled(on))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn open_logs() -> CmdResult<()> {
    zlip_platform::open_in_shell(&zlip_config::data_dir().join("logs").display().to_string());
    Ok(())
}

#[tauri::command]
pub async fn open_config() -> CmdResult<()> {
    zlip_platform::open_in_shell(&zlip_config::config_path().display().to_string());
    Ok(())
}

/// Runs the script *outside* the engine lock: a slow script must not freeze search or capture.
#[tauri::command]
pub async fn run_action(shared: Shr<'_>, id: u64, name: String) -> CmdResult<ActionReport> {
    let job = shared.engine().begin_action(ItemId(id), &name);
    Ok(match job {
        Ok(job) => tauri::async_runtime::spawn_blocking(move || job.run())
            .await
            .map_err(|e| e.to_string())?,
        Err(report) => report,
    })
}

#[tauri::command]
pub async fn trust_action(shared: Shr<'_>, name: String, sha256: String) -> CmdResult<()> {
    shared.engine().trust_action(&name, &sha256)
}
