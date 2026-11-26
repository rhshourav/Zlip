//! The paste sequence. It touches the clipboard, focus and synthetic input, so
//! it always runs on its own thread and never while the engine lock is held.

use crate::{capture::now_ms, state::Shared, windows};
use std::{
    sync::{Arc, PoisonError},
    thread::{self, sleep},
    time::{Duration, Instant},
};
use tauri::Emitter;
use zlip_core::{ImageData, ItemId, Payload};
use zlip_engine::{PasteContent, PasteContext, PasteError, PasteOptions, PreparedPaste};
use zlip_platform::{
    ClipboardContent, ClipboardSession, Limits, ReadResult, clear_clipboard, clipboard_sequence,
    foreground_window, restore_focus, send_left_arrows, send_paste, wait_modifiers_released,
    write_clipboard,
};

/// How long the target program gets to read the clipboard before we restore it.
const RESTORE_DELAY: Duration = Duration::from_millis(300);

/// What the clipboard held before a paste, so it can be put back afterwards.
struct Previous {
    payload: Payload,
}

fn spawn(shared: Arc<Shared>, name: &str, work: impl FnOnce(&Arc<Shared>) + Send + 'static) {
    let _ = thread::Builder::new()
        .name(name.into())
        .spawn(move || work(&shared));
}

/// Paste one history item chosen in the overlay.
pub fn spawn_item(shared: Arc<Shared>, id: ItemId, opts: PasteOptions) {
    spawn(shared, "zlip-paste", move |s| {
        run(s, true, |engine, ctx| engine.prepare_paste(id, &opts, ctx));
    });
}

/// Paste text edited in the quick editor.
pub fn spawn_text(shared: Arc<Shared>, text: String, opts: PasteOptions) {
    spawn(shared, "zlip-paste", move |s| {
        run(s, true, |engine, _| engine.prepare_text_paste(text, &opts));
    });
}

/// Paste the next queued item. The overlay is not involved, so there is no focus
/// change and no clipboard restore: the user's own Ctrl+V keystroke is the trigger.
pub fn spawn_queue_next(shared: Arc<Shared>) {
    spawn(shared, "zlip-queue-paste", move |s| {
        let ctx_now = chrono::Local::now().naive_local();
        let prepared = {
            let ctx = PasteContext {
                now_local: ctx_now,
                clipboard: None,
                now: Instant::now(),
            };
            s.engine().queue_pop(&ctx)
        };
        windows::update_hud(s);
        match prepared {
            Some(Ok(p)) => deliver(s, p, false, None),
            Some(Err(e)) => s.notify("error", e.to_string()),
            None => {}
        }
    });
}

fn run(
    shared: &Arc<Shared>,
    from_overlay: bool,
    prepare: impl FnOnce(&mut zlip_engine::Engine, &PasteContext) -> Result<PreparedPaste, PasteError>,
) {
    let _serial = shared
        .paste_lock
        .lock()
        .unwrap_or_else(PoisonError::into_inner);

    // Remember what is on the clipboard now: `{clipboard}` needs it, and we put it back afterwards.
    let previous = read_previous(shared);
    let clip_text = match previous.as_ref().map(|p| &p.payload) {
        Some(Payload::Text(t)) => Some(t.clone()),
        _ => None,
    };
    let ctx = PasteContext {
        now_local: chrono::Local::now().naive_local(),
        clipboard: clip_text,
        now: Instant::now(),
    };
    let prepared = prepare(&mut shared.engine(), &ctx);
    match prepared {
        Ok(p) => deliver(shared, p, from_overlay, previous),
        Err(e) => {
            tracing::warn!("paste: could not prepare the item: {e}");
            shared.notify("error", e.to_string());
        }
    }
}

/// The clipboard as it is right now, unless it is private, huge or a secret.
fn read_previous(shared: &Shared) -> Option<Previous> {
    let session = ClipboardSession::open().ok()?;
    let flags = session.flags();
    if flags.exclude_from_monitor || flags.can_include_in_history == Some(0) {
        return None;
    }
    let limits = Limits {
        max_text_bytes: 1024 * 1024,
        max_image_bytes: 16 * 1024 * 1024,
        max_files: 1000,
    };
    let result = session.read_payload(&limits);
    drop(session);
    match result {
        ReadResult::Payload(Payload::Text(t)) if shared.engine().looks_sensitive(&t) => None,
        ReadResult::Payload(p) => Some(Previous { payload: p }),
        _ => None,
    }
}

fn write_prepared(content: &PasteContent) -> Result<u32, zlip_platform::PlatformError> {
    match content {
        PasteContent::Text(t) => write_clipboard(ClipboardContent::Text(t)),
        PasteContent::Files(f) => write_clipboard(ClipboardContent::Files(f)),
        PasteContent::Image(i) => write_clipboard(ClipboardContent::Image(i)),
    }
}

fn write_previous(prev: &Previous) -> Result<u32, zlip_platform::PlatformError> {
    match &prev.payload {
        Payload::Text(t) => write_clipboard(ClipboardContent::Text(t)),
        Payload::Files(f) => write_clipboard(ClipboardContent::Files(f)),
        Payload::Image(ImageData { png, width, height }) => {
            let img = ImageData {
                png: png.clone(),
                width: *width,
                height: *height,
            };
            write_clipboard(ClipboardContent::Image(&img))
        }
    }
}

fn deliver(
    shared: &Arc<Shared>,
    prepared: PreparedPaste,
    from_overlay: bool,
    previous: Option<Previous>,
) {
    // The log gets ids and window handles only, never what is being pasted.
    tracing::info!(
        id = prepared.id.0,
        from_overlay,
        window = shared.last_foreground().0,
        "paste: start"
    );
    if from_overlay {
        windows::hide_overlay(shared);
        let target = shared.last_foreground();
        restore_focus(target);
        // Give the target a moment to become active again (bounded: never hang a paste).
        for _ in 0..27 {
            if target.is_null() || foreground_window() == target {
                break;
            }
            sleep(Duration::from_millis(15));
        }
        tracing::info!(
            wanted = target.0,
            in_front = foreground_window().0,
            "paste: focus handed back"
        );
    }

    let written = match write_prepared(&prepared.content) {
        Ok(seq) => seq,
        Err(e) => {
            tracing::warn!("paste: could not write the clipboard: {e}");
            shared.notify("error", format!("Could not use the clipboard: {e}"));
            return;
        }
    };
    shared.set_own_sequence(written);

    // Pasting while Win/Alt/Shift is still held would change the shortcut.
    if !wait_modifiers_released(Duration::from_millis(500)) {
        tracing::debug!("modifiers still held, pasting anyway");
    }
    if let Err(e) = send_paste() {
        tracing::warn!("paste: could not send the keystroke: {e}");
        shared.notify("error", format!("Could not send the paste keystroke: {e}"));
        return;
    }
    tracing::info!(in_front = foreground_window().0, "paste: keystroke sent");
    if prepared.cursor_back > 0 {
        sleep(Duration::from_millis(70));
        let _ = send_left_arrows(prepared.cursor_back);
    }
    {
        let used_at = now_ms();
        let mut engine = shared.engine();
        engine.mark_used(prepared.id, used_at);
        // Using an item from the list makes it the newest one, like copying it again would.
        // A queued paste keeps the list as it is.
        if from_overlay {
            engine.bump_to_top(prepared.id, used_at);
        }
    }
    let sensitive = prepared.sensitive;
    drop(prepared); // wipes pasted secret text from our memory

    // Put the previous clipboard back, but only if nobody copied something since.
    if from_overlay || sensitive {
        sleep(RESTORE_DELAY);
        if clipboard_sequence() == written {
            let seq = match previous.as_ref() {
                Some(prev) => write_previous(prev).ok(),
                None if sensitive => clear_clipboard().ok().map(|()| clipboard_sequence()),
                None => None,
            };
            if let Some(seq) = seq {
                shared.set_own_sequence(seq);
            }
        }
    }
    let _ = shared.app.emit_to("overlay", "zlip:items-changed", ());
}
