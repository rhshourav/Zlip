//! State shared by the UI commands, the controller thread and the paste workers.

use crossbeam_channel::Sender;
use serde::Serialize;
use std::{
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering::SeqCst},
    },
    time::{Duration, Instant},
};
use tauri::AppHandle;
use zlip_config::ConfigHandle;
use zlip_engine::{Engine, view::OsInfo};
use zlip_platform::WindowHandle;

/// Messages for the controller thread besides OS events.
#[derive(Debug)]
pub enum Control {
    /// Something changed that may move the next deadline.
    Wake,
    Shutdown,
}

#[derive(Debug, Clone, Serialize)]
pub struct Notice {
    /// `info`, `warn` or `error`.
    pub level: &'static str,
    pub message: String,
}

pub struct Shared {
    pub app: AppHandle,
    pub cfg: ConfigHandle,
    engine: Mutex<Engine>,
    pub control: Sender<Control>,
    /// The window to paste into: whatever was in front when the overlay opened.
    last_fg: AtomicIsize,
    /// Clipboard sequence number right after Zlip's own last write.
    own_seq: AtomicU32,
    /// Serialises paste sequences so queued pastes never interleave.
    pub paste_lock: Mutex<()>,
    overlay_shown_at: Mutex<Option<Instant>>,
    overlay_focused: AtomicBool,
    /// When the open/close hotkey last took effect (see [`Shared::accept_toggle`]).
    last_toggle: Mutex<Option<Instant>>,
    /// When Zlip itself last wrote the settings file (see [`Shared::was_self_write`]).
    self_write_at: Mutex<Option<Instant>>,
    native_backdrop: AtomicBool,
    notice: Mutex<Option<Notice>>,
}

impl Shared {
    pub fn new(
        app: AppHandle,
        cfg: ConfigHandle,
        engine: Engine,
        control: Sender<Control>,
    ) -> Self {
        Self {
            app,
            cfg,
            engine: Mutex::new(engine),
            control,
            last_fg: AtomicIsize::new(0),
            own_seq: AtomicU32::new(0),
            paste_lock: Mutex::new(()),
            overlay_shown_at: Mutex::new(None),
            overlay_focused: AtomicBool::new(false),
            last_toggle: Mutex::new(None),
            self_write_at: Mutex::new(None),
            native_backdrop: AtomicBool::new(false),
            notice: Mutex::new(None),
        }
    }

    /// Lock the engine. Locks are short: never hold one while touching the
    /// clipboard, sleeping or sending input.
    pub fn engine(&self) -> MutexGuard<'_, Engine> {
        self.engine.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn last_foreground(&self) -> WindowHandle {
        WindowHandle(self.last_fg.load(SeqCst))
    }
    pub fn set_last_foreground(&self, h: WindowHandle) {
        self.last_fg.store(h.0, SeqCst);
    }

    pub fn own_sequence(&self) -> u32 {
        self.own_seq.load(SeqCst)
    }
    pub fn set_own_sequence(&self, seq: u32) {
        self.own_seq.store(seq, SeqCst);
    }

    pub fn mark_overlay_shown(&self) {
        *self
            .overlay_shown_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
        self.overlay_focused.store(false, SeqCst);
    }
    pub fn overlay_shown_for(&self) -> Option<std::time::Duration> {
        self.overlay_shown_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .map(|t| t.elapsed())
    }
    pub fn set_overlay_focused(&self, focused: bool) {
        self.overlay_focused.store(focused, SeqCst);
    }
    pub fn overlay_focused(&self) -> bool {
        self.overlay_focused.load(SeqCst)
    }

    /// `false` when this open/close request follows the previous one too closely to be
    /// a deliberate second press (key auto-repeat, a double tap, the backup shortcut
    /// firing next to the main one). Such repeats must not flip the overlay back.
    pub fn accept_toggle(&self) -> bool {
        const MIN_GAP: Duration = Duration::from_millis(250);
        let mut last = self
            .last_toggle
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let now = Instant::now();
        if last.is_some_and(|t| now.duration_since(t) < MIN_GAP) {
            return false;
        }
        *last = Some(now);
        true
    }

    /// Call right before Zlip writes the settings file itself.
    pub fn mark_self_write(&self) {
        *self
            .self_write_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
    }

    /// `true` if the file change the watcher just reported is Zlip's own write.
    pub fn was_self_write(&self) -> bool {
        self.self_write_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some_and(|t| t.elapsed() < Duration::from_secs(3))
    }

    pub fn set_native_backdrop(&self, on: bool) {
        self.native_backdrop.store(on, SeqCst);
    }

    /// Facts about this Windows install that change how the UI is drawn.
    pub fn os_info(&self) -> OsInfo {
        OsInfo {
            build: zlip_platform::windows_build(),
            native_backdrop: self.native_backdrop.load(SeqCst),
            transparency: zlip_platform::transparency_enabled(),
            dark: !zlip_platform::apps_use_light_theme(),
        }
    }

    /// Remember a message for the user and push it to the overlay if it is open.
    pub fn notify(&self, level: &'static str, message: impl Into<String>) {
        use tauri::Emitter;
        let n = Notice {
            level,
            message: message.into(),
        };
        *self.notice.lock().unwrap_or_else(PoisonError::into_inner) = Some(n.clone());
        let _ = self.app.emit_to("overlay", "zlip:notice", n);
    }

    pub fn take_notice(&self) -> Option<Notice> {
        self.notice
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}
