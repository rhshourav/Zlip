//! The capture path: from "the clipboard changed" to "it is in the history".
//!
//! Order matters. The privacy decision is made from the clipboard's *formats and
//! the owning program* before any content is read, and secrets go to the in-RAM
//! vault before anything else can see them.

use crate::{state::Shared, windows};
use std::{
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Emitter;
use zlip_engine::{CaptureInput, CaptureOutcome};
use zlip_platform::{ClipboardChange, ClipboardSession, Limits, ReadResult};
use zlip_security::CaptureFacts;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn limits(shared: &Shared) -> Limits {
    let r = &shared.cfg.snapshot().retention;
    Limits {
        max_text_bytes: r.max_text_kb as usize * 1024,
        max_image_bytes: r.max_image_mb as usize * 1024 * 1024,
        max_files: 1000,
    }
}

pub fn on_clipboard_change(shared: &Arc<Shared>, change: &ClipboardChange) {
    // Our own writes (a paste, a restore, a secret wipe) are not new copies.
    if change.sequence == shared.own_sequence() {
        return;
    }
    let session = match ClipboardSession::open() {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!("clipboard busy, copy not captured: {e}");
            return;
        }
    };
    let flags = session.flags();
    let facts = CaptureFacts {
        private_mode: false, // filled in by the engine
        exclude_from_monitor: flags.exclude_from_monitor,
        can_include_in_history: flags.can_include_in_history,
        can_upload_to_cloud: flags.can_upload_to_cloud,
        owner_exe: change.owner_exe.as_deref(),
        foreground_title: change.foreground_title.as_deref(),
    };
    if let Some(reason) = shared.engine().should_capture(facts) {
        tracing::debug!(?reason, "copy skipped");
        return; // nothing was read
    }
    let result = session.read_payload(&limits(shared));
    let clip_seq = session.sequence();
    drop(session); // release the clipboard before doing anything slow

    let payload = match result {
        ReadResult::Payload(p) => p,
        ReadResult::TooLarge => {
            tracing::debug!("copy ignored: larger than the configured limit");
            return;
        }
        ReadResult::Unsupported => return,
    };
    let outcome = shared.engine().ingest(CaptureInput {
        payload,
        owner_exe: change.owner_exe.clone(),
        clip_seq,
        now_ms: now_ms(),
        now: Instant::now(),
        enqueue: true,
    });
    match &outcome {
        CaptureOutcome::Stored {
            category,
            duplicate,
            queued,
            ..
        } => {
            tracing::debug!(category = category.as_str(), duplicate, queued, "captured");
        }
        CaptureOutcome::Sensitive { label, queued, .. } => {
            tracing::debug!(label, queued, "secret held in memory only");
        }
        CaptureOutcome::Ignored(why) => tracing::debug!(why, "copy ignored"),
        CaptureOutcome::Failed(e) => tracing::warn!("could not store the copy: {e}"),
    }
    if matches!(
        outcome,
        CaptureOutcome::Stored { .. } | CaptureOutcome::Sensitive { .. }
    ) {
        let _ = shared.app.emit_to("overlay", "zlip:items-changed", ());
        windows::update_hud(shared);
    }
}
