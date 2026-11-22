//! Start-up and shutdown.

use crate::{
    commands, controller, links, logging,
    state::{Control, Shared},
    tray, windows,
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, RunEvent};
use zlip_config::{
    Config, ConfigHandle, ConfigWatcher, ReloadEvent, config_dir, config_path, data_dir,
    load_or_create, write_atomic,
};
use zlip_engine::Engine;
use zlip_platform::{
    HookHandle, ListenerHandle, PlatformEvent, SystemClipboard, load_or_create_master_key,
    start_hook, start_listener,
};
use zlip_store::{MasterKey, Store, StoreError};

/// Name under which the history key is kept in Windows Credential Manager.
const KEY_TARGET: &str = "dev.rhshourav.zlip/history-key";

/// Things that must stay alive for the whole run.
struct Keepalive {
    _hook: Mutex<Option<HookHandle>>,
    _listener: Mutex<Option<ListenerHandle>>,
    _watcher: Mutex<Option<ConfigWatcher>>,
    _log: Mutex<Option<tracing_appender::non_blocking::WorkerGuard>>,
}

/// Keep `zlip.schema.json` next to `config.toml` so editors can autocomplete it.
fn write_schema(dir: &Path) {
    let path = dir.join("zlip.schema.json");
    let want = Config::json_schema();
    if std::fs::read_to_string(&path).ok().as_deref() != Some(want.as_str()) {
        let _ = write_atomic(&path, want.as_bytes());
    }
}

/// Open the history. If it was written with a key we no longer have, move it
/// aside (never delete it) and start fresh.
fn open_store(data: &Path, key: &MasterKey) -> Result<(Store, Option<String>), StoreError> {
    let (db, images) = (data.join("history.db"), data.join("images"));
    match Store::open(&db, &images, key) {
        Err(StoreError::WrongKey) => {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            for ext in ["", "-wal", "-shm"] {
                let from = PathBuf::from(format!("{}{ext}", db.display()));
                let _ = std::fs::rename(
                    &from,
                    PathBuf::from(format!("{}.unreadable-{stamp}{ext}", db.display())),
                );
            }
            let _ = std::fs::rename(&images, data.join(format!("images.unreadable-{stamp}")));
            let note = "Your previous clipboard history could not be unlocked (its encryption key is missing). It was moved aside and Zlip started with an empty history.".to_owned();
            Store::open(&db, &images, key).map(|s| (s, Some(note)))
        }
        other => other.map(|s| (s, None)),
    }
}

pub fn run() {
    let result = tauri::Builder::default()
        // Must be first: a second launch just opens the overlay of the running one.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(shared) = app.try_state::<Arc<Shared>>() {
                windows::show_overlay(&shared, zlip_platform::foreground_window());
            }
        }))
        .on_window_event(windows::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::folders,
            commands::item_detail,
            commands::thumbnail,
            commands::reveal,
            commands::paste,
            commands::paste_text,
            commands::save_text,
            commands::toggle_pin,
            commands::delete_item,
            commands::clear_history,
            commands::queue_add,
            commands::queue_clear,
            commands::status,
            commands::set_private_mode,
            commands::ui_config,
            commands::take_notice,
            commands::hide_overlay,
            commands::open_config,
            commands::get_settings,
            commands::save_settings,
            commands::repair_shortcuts,
            commands::pause_shortcuts,
            commands::open_logs,
            links::open_url,
            commands::run_action,
            commands::trust_action,
            commands::get_autostart,
            commands::set_autostart,
        ])
        .setup(setup)
        .build(tauri::generate_context!());

    match result {
        Ok(app) => app.run(|handle, event| {
            if let RunEvent::Exit = event
                && let Some(shared) = handle.try_state::<Arc<Shared>>()
            {
                // Secrets must not outlive the process in memory, and a copied
                // secret should not stay on the clipboard after Zlip is gone.
                let wiped = shared.engine().wipe_vault(&SystemClipboard);
                tracing::info!(wiped, "exiting");
                let _ = shared.control.send(Control::Shutdown);
            }
        }),
        Err(e) => tracing::error!("Zlip could not start: {e}"),
    }
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let (cfg_dir, data) = (config_dir(), data_dir());
    std::fs::create_dir_all(&cfg_dir)?;
    std::fs::create_dir_all(&data)?;
    let log = logging::init(&data.join("logs"));
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting");

    // ---- config: a broken file never blocks start-up; defaults + a notice instead
    let mut notice: Option<(&'static str, String)> = None;
    let config = match load_or_create(&config_path()) {
        Ok(c) => c,
        Err(e) => {
            notice = Some((
                "error",
                format!(
                    "Your settings file has a problem, so defaults are in use until it is fixed.\n{e}"
                ),
            ));
            Config::default()
        }
    };
    write_schema(&cfg_dir);
    let handle = ConfigHandle::new(config);

    // ---- history
    let key = load_or_create_master_key(KEY_TARGET)?;
    let (store, store_note) = open_store(&data, &MasterKey::from_bytes(*key))?;
    drop(key);
    if let Some(n) = store_note {
        notice = Some(("warn", n));
    }
    let (engine, skipped) = Engine::new(handle.clone(), store, data.join("actions-trust.json"))?;
    if skipped > 0 {
        tracing::warn!(
            skipped,
            "history rows failed authentication and were skipped"
        );
    }

    // ---- shared state and windows
    let (control_tx, control_rx) = crossbeam_channel::unbounded();
    let shared = Arc::new(Shared::new(
        app.handle().clone(),
        handle.clone(),
        engine,
        control_tx,
    ));
    app.manage(Arc::clone(&shared));
    let native = windows::create_all(app.handle(), &handle.snapshot())?;
    shared.set_native_backdrop(native);
    tray::build(app.handle())?;
    if let Some((level, msg)) = notice {
        shared.notify(level, msg);
    }

    // ---- OS threads -> controller
    let (event_tx, event_rx) = crossbeam_channel::unbounded::<PlatformEvent>();
    let chords = handle
        .snapshot()
        .hotkeys
        .global_chords()
        .map_err(|e| e.to_string())?;
    let hook = match start_hook(event_tx.clone(), &chords) {
        Ok(h) => Some(h),
        Err(e) => {
            tracing::error!("keyboard hook failed: {e}");
            shared.notify(
                "error",
                format!(
                    "Zlip could not take over {}: {e}. The tray menu still works.",
                    handle.snapshot().hotkeys.open_overlay
                ),
            );
            None
        }
    };
    let listener = match start_listener(event_tx) {
        Ok(l) => Some(l),
        Err(e) => {
            tracing::error!("clipboard listener failed: {e}");
            shared.notify("error", format!("Zlip cannot watch the clipboard: {e}"));
            None
        }
    };
    controller::spawn(Arc::clone(&shared), event_rx, control_rx);

    // ---- hot reload
    let reload_shared = Arc::clone(&shared);
    let watcher = zlip_config::watch(config_path(), handle, move |event| match event {
        ReloadEvent::Reloaded(cfg) => {
            windows::apply_live(&reload_shared, &cfg);
            // The settings screen saves the file itself and already shows the result.
            if !reload_shared.was_self_write() {
                reload_shared.notify("info", "Settings reloaded.");
            }
        }
        ReloadEvent::Rejected(why) => {
            reload_shared.notify(
                "error",
                format!("Settings were not applied; the previous ones stay active.\n{why}"),
            );
        }
    })
    .map_err(|e| tracing::warn!("settings hot reload is unavailable: {e}"))
    .ok();

    app.manage(Keepalive {
        _hook: Mutex::new(hook),
        _listener: Mutex::new(listener),
        _watcher: Mutex::new(watcher),
        _log: Mutex::new(Some(log)),
    });
    Ok(())
}
