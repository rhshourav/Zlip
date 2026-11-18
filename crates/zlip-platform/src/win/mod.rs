//! Win32 implementation. Everything `unsafe` in Zlip lives in this directory.

mod clipboard;
mod credential;
mod hook;
mod input;
mod system;
mod util;
mod window;

pub use clipboard::{
    ClipboardSession, ListenerHandle, SystemClipboard, clear_clipboard, clipboard_sequence,
    start_listener, write_clipboard,
};
pub use credential::{delete_master_key, load_or_create_master_key};
pub use hook::{
    BackupState, HookHandle, backup_state, request_refresh, set_chords, set_drain_mode, set_paused,
    start_hook,
};
pub use input::{send_left_arrows, send_paste, wait_modifiers_released};
pub use system::{
    Backdrop, apply_native_backdrop, apps_use_light_theme, open_in_shell, transparency_enabled,
    windows_build,
};
pub use window::{
    caret_rect, force_foreground, foreground_window, is_window, restore_focus,
    set_noactivate_toolwindow, show_no_activate, work_area_for,
};
