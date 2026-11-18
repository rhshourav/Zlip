//! The controller thread: the one place OS events turn into actions.
//!
//! Hook and listener threads only classify an event and send it here (blueprint
//! section 2.2). This thread owns the timing of everything else: it sleeps until
//! the next secret is due to expire, and runs housekeeping on a slow cadence.
//!
//! Two rules keep the shortcut dependable:
//! * **Hotkeys are never queued behind slow work.** Reading the clipboard can take
//!   most of a second when another program holds it, so clipboard captures run on
//!   their own `zlip-capture` thread.
//! * **A bug must not take the shortcut down with it.** Each handler runs inside
//!   `catch_unwind`, so one failure is logged and the next event is still served.

use crate::{
    capture, paste,
    state::{Control, Shared},
    windows,
};
use crossbeam_channel::{Receiver, Sender, select};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use zlip_platform::{ClipboardChange, HotkeyId, PlatformEvent, SystemClipboard};

/// Wake at least this often even when no secret is pending.
const IDLE_WAKE: Duration = Duration::from_secs(30);
const RETENTION_EVERY: Duration = Duration::from_secs(600);

/// Run `work`; if it panics, log it and carry on.
fn guarded(what: &str, work: impl FnOnce()) {
    if catch_unwind(AssertUnwindSafe(work)).is_err() {
        tracing::error!("{what} panicked; the app keeps running");
    }
}

pub fn spawn(shared: Arc<Shared>, events: Receiver<PlatformEvent>, control: Receiver<Control>) {
    let (capture_tx, capture_rx) = crossbeam_channel::unbounded::<ClipboardChange>();

    let capture_shared = Arc::clone(&shared);
    let worker = thread::Builder::new()
        .name("zlip-capture".into())
        .spawn(move || {
            while let Ok(change) = capture_rx.recv() {
                guarded("clipboard capture", || {
                    capture::on_clipboard_change(&capture_shared, &change);
                });
            }
        });
    if let Err(e) = worker {
        tracing::error!("could not start the capture thread: {e}");
    }

    let result = thread::Builder::new()
        .name("zlip-controller".into())
        .spawn(move || run(&shared, &events, &control, &capture_tx));
    if let Err(e) = result {
        tracing::error!("could not start the controller thread: {e}");
    }
}

fn run(
    shared: &Arc<Shared>,
    events: &Receiver<PlatformEvent>,
    control: &Receiver<Control>,
    capture_tx: &Sender<ClipboardChange>,
) {
    let mut last_retention = Instant::now() - RETENTION_EVERY;
    loop {
        let wait = shared
            .engine()
            .vault_deadline()
            .map_or(IDLE_WAKE, |d| d.saturating_duration_since(Instant::now()))
            .min(IDLE_WAKE);
        select! {
            recv(events) -> ev => match ev {
                Ok(ev) => guarded("event handler", || handle(shared, ev, capture_tx)),
                Err(_) => break,
            },
            recv(control) -> msg => match msg {
                Ok(Control::Wake) => {}
                Ok(Control::Shutdown) | Err(_) => break,
            },
            default(wait) => {}
        }

        guarded("housekeeping", || {
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
        });
    }
}

fn notify_lists_changed(shared: &Shared) {
    use tauri::Emitter;
    let _ = shared.app.emit_to("overlay", "zlip:items-changed", ());
    windows::update_hud(shared);
}

fn handle(shared: &Arc<Shared>, event: PlatformEvent, capture_tx: &Sender<ClipboardChange>) {
    match event {
        PlatformEvent::Hotkey {
            id: HotkeyId::OpenOverlay,
            foreground,
        } => {
            tracing::info!("open/close shortcut pressed");
            windows::toggle_overlay(shared, foreground);
        }
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
        PlatformEvent::ClipboardChanged(change) => {
            let _ = capture_tx.send(change);
        }
    }
}
