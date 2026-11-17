//! The controller thread: the one place OS events turn into actions.
//!
//! Hook and listener threads only classify an event and send it here (blueprint
//! section 2.2). This thread owns the timing of everything else: it sleeps until
//! the next secret is due to expire, and runs housekeeping on a slow cadence.

use crate::{
    capture, paste,
    state::{Control, Shared},
    windows,
};
use crossbeam_channel::{Receiver, select};
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use zlip_platform::{HotkeyId, PlatformEvent, SystemClipboard};

/// Wake at least this often even when no secret is pending.
const IDLE_WAKE: Duration = Duration::from_secs(30);
const RETENTION_EVERY: Duration = Duration::from_secs(600);

pub fn spawn(shared: Arc<Shared>, events: Receiver<PlatformEvent>, control: Receiver<Control>) {
    let result = thread::Builder::new()
        .name("zlip-controller".into())
        .spawn(move || run(&shared, &events, &control));
    if let Err(e) = result {
        tracing::error!("could not start the controller thread: {e}");
    }
}

fn run(shared: &Arc<Shared>, events: &Receiver<PlatformEvent>, control: &Receiver<Control>) {
    let mut last_retention = Instant::now() - RETENTION_EVERY;
    loop {
        let wait = shared
            .engine()
            .vault_deadline()
            .map_or(IDLE_WAKE, |d| d.saturating_duration_since(Instant::now()))
            .min(IDLE_WAKE);
        select! {
            recv(events) -> ev => match ev {
                Ok(ev) => handle(shared, ev),
                Err(_) => break,
            },
            recv(control) -> msg => match msg {
                Ok(Control::Wake) => {}
                Ok(Control::Shutdown) | Err(_) => break,
            },
            default(wait) => {}
        }

        let gone = shared
            .engine()
            .sweep_vault(Instant::now(), &SystemClipboard);
        if !gone.is_empty() {
            tracing::debug!(count = gone.len(), "secrets expired");
            notify_lists_changed(shared);
        }
        if last_retention.elapsed() >= RETENTION_EVERY {
            last_retention = Instant::now();
            let removed = shared.engine().enforce_retention(capture::now_ms());
            if removed > 0 {
                tracing::debug!(removed, "retention applied");
                notify_lists_changed(shared);
            }
        }
    }
}

fn notify_lists_changed(shared: &Shared) {
    use tauri::Emitter;
    let _ = shared.app.emit_to("overlay", "zlip:items-changed", ());
    windows::update_hud(shared);
}

fn handle(shared: &Arc<Shared>, event: PlatformEvent) {
    match event {
        PlatformEvent::Hotkey {
            id: HotkeyId::OpenOverlay,
            foreground,
        } => windows::toggle_overlay(shared, foreground),
        PlatformEvent::Hotkey {
            id: HotkeyId::QueueCollect,
            ..
        } => {
            shared.engine().queue_toggle();
            windows::update_hud(shared); // also switches Ctrl+V interception on or off
        }
        PlatformEvent::Hotkey {
            id: HotkeyId::QueuePaste,
            ..
        } => {
            paste::spawn_queue_next(Arc::clone(shared));
        }
        PlatformEvent::ClipboardChanged(change) => capture::on_clipboard_change(shared, &change),
    }
}
