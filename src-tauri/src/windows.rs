//! The overlay and the queue HUD: creation, placement and show/hide.
//!
//! Both windows are created once at startup and kept hidden ("warm"), so the
//! hotkey only has to move and show an existing window: no process or webview
//! start-up on the hot path.

use crate::state::Shared;
use std::{sync::Arc, time::Duration};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use zlip_config::{Config, Position};
use zlip_core::geometry::place_overlay;
use zlip_platform::{
    Backdrop, WindowHandle, apply_native_backdrop, apps_use_light_theme, caret_rect,
    force_foreground, foreground_window, hide_window, set_noactivate_toolwindow, show_no_activate,
    work_area_for,
};

/// Breathing room between the overlay and the caret / screen edge, in pixels.
const GAP: i32 = 12;
/// Ignore a focus loss this soon after showing (Windows can bounce focus once).
const BLUR_GRACE: Duration = Duration::from_millis(200);
/// How long to wait before checking that a freshly opened overlay really has focus.
const FOCUS_RETRY: Duration = Duration::from_millis(80);

/// The Win32 handle of a Tauri window (0 if unavailable).
pub fn native_handle(w: &WebviewWindow) -> WindowHandle {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match w.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => WindowHandle(h.hwnd.get()),
        _ => WindowHandle(0),
    }
}

/// Create both windows hidden. Returns whether the native Windows 11 backdrop is active.
pub fn create_all(app: &AppHandle, cfg: &Config) -> tauri::Result<bool> {
    let overlay = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("index.html".into()))
        .title("Zlip")
        .inner_size(f64::from(cfg.ui.width), f64::from(cfg.ui.height))
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()?;
    let native = apply_backdrop(&overlay);

    let hud = WebviewWindowBuilder::new(app, "hud", WebviewUrl::App("hud.html".into()))
        .title("Zlip queue")
        .inner_size(236.0, 60.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()?;
    hud.set_ignore_cursor_events(true)?; // click-through: it must never steal a click
    set_noactivate_toolwindow(native_handle(&hud));
    Ok(native)
}

/// Windows 11 22H2+: native acrylic with system-rounded corners. Otherwise the
/// `window-vibrancy` acrylic (its blur is rectangular, so the UI keeps square corners).
fn apply_backdrop(window: &WebviewWindow) -> bool {
    let dark = !apps_use_light_theme();
    if apply_native_backdrop(native_handle(window), dark) == Backdrop::NativeAcrylic {
        return true;
    }
    fallback_acrylic(window, dark);
    false
}

/// `window-vibrancy` exists on every platform (it just reports "unsupported" off
/// Windows), so this call is type-checked by CI on Linux too.
fn fallback_acrylic(window: &WebviewWindow, dark: bool) {
    let tint = if dark {
        (24, 24, 30, 120)
    } else {
        (246, 246, 250, 120)
    };
    if let Err(e) = window_vibrancy::apply_acrylic(window, Some(tint)) {
        tracing::warn!("acrylic is unavailable, the overlay will be solid: {e}");
    }
}

fn overlay(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("overlay")
}

pub fn overlay_visible(shared: &Shared) -> bool {
    overlay(&shared.app)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Open the overlay next to the caret (or bottom-centre) of the window in front.
pub fn show_overlay(shared: &Arc<Shared>, foreground: WindowHandle) {
    let Some(w) = overlay(&shared.app) else {
        return;
    };
    let cfg = shared.cfg.snapshot();

    shared.set_last_foreground(foreground);
    let work = work_area_for(foreground);
    let caret = if cfg.ui.position == Position::Caret {
        caret_rect(foreground)
    } else {
        None
    };

    // A hidden window keeps the pixel size it had on the monitor it was last shown on.
    // Park it on the monitor it is about to open on first, so Windows rescales it there,
    // then ask for the configured size again. The size measured below is then the size
    // the overlay really has when it appears, on any monitor and at any scaling.
    if !overlay_visible(shared) {
        let _ = w.set_position(PhysicalPosition::new(work.left, work.top));
    }
    let _ = w.set_size(tauri::LogicalSize::new(
        f64::from(cfg.ui.width),
        f64::from(cfg.ui.height),
    ));
    if let Ok(size) = w.outer_size() {
        let p = place_overlay(work, caret, (size.width as i32, size.height as i32), GAP);
        let _ = w.set_position(PhysicalPosition::new(p.x, p.y));
        // Set ZLIP_LOG=debug to see these in the log files: they say which size the window
        // really had and whether the app in front reported a text caret.
        tracing::debug!(
            want = ?(cfg.ui.width, cfg.ui.height),
            got = ?(size.width, size.height),
            scale = ?w.scale_factor().ok(),
            caret = caret.is_some(),
            at = ?(p.x, p.y),
            "overlay placed"
        );
    }
    shared.mark_overlay_shown();
    let _ = w.set_always_on_top(true);
    let _ = w.show();
    let me = native_handle(&w);
    force_foreground(me);
    let _ = w.set_focus();
    let _ = shared.app.emit_to("overlay", "zlip:overlay-shown", ());

    // Windows sometimes refuses the first focus request (the "foreground lock").
    // Check shortly after and try again, so the search box always receives the keys.
    let (shared, w) = (Arc::clone(shared), w.clone());
    let _ = std::thread::Builder::new()
        .name("zlip-focus".into())
        .spawn(move || {
            for _ in 0..4 {
                std::thread::sleep(FOCUS_RETRY);
                if !overlay_visible(&shared) || foreground_window() == me {
                    return;
                }
                tracing::debug!("overlay did not get focus, retrying");
                force_foreground(me);
                let _ = w.set_focus();
            }
        });
}

pub fn hide_overlay(shared: &Shared) {
    if let Some(w) = overlay(&shared.app) {
        let _ = w.hide();
    }
}

/// Open the settings screen (from the tray). Opens the overlay first if it is hidden.
pub fn show_settings(shared: &Arc<Shared>) {
    show_overlay(shared, WindowHandle(0));
    let _ = shared.app.emit_to("overlay", "zlip:open-settings", ());
}

/// The open/close hotkey. Pressing it while the overlay is up and focused closes it;
/// pressing it while the overlay is up but *not* focused brings it forward instead,
/// so it can never get stuck on screen out of reach of the keyboard.
pub fn toggle_overlay(shared: &Arc<Shared>, foreground: WindowHandle) {
    if !shared.accept_toggle() {
        tracing::debug!("open/close hotkey ignored: too soon after the last one");
        return;
    }
    if overlay_visible(shared) && shared.overlay_focused() {
        hide_overlay(shared);
    } else {
        show_overlay(shared, foreground);
    }
}

/// Apply new size settings after a config reload.
pub fn apply_config(app: &AppHandle, cfg: &Config) {
    if let Some(w) = overlay(app) {
        let _ = w.set_size(tauri::LogicalSize::new(
            f64::from(cfg.ui.width),
            f64::from(cfg.ui.height),
        ));
    }
}

/// Make a new, valid config take effect everywhere: shortcuts, window size, the UI.
/// Used after the settings file is edited by hand and after the settings screen saves.
pub fn apply_live(shared: &Shared, cfg: &Config) {
    if let Ok(chords) = cfg.hotkeys.global_chords() {
        zlip_platform::set_chords(&chords);
    }
    apply_config(&shared.app, cfg);
    let ui = {
        let os = shared.os_info();
        shared.engine().ui_config(os)
    };
    let _ = shared.app.emit_to("overlay", "zlip:config", ui);
    let _ = shared.app.emit_to("overlay", "zlip:items-changed", ());
}

/// Show or hide the HUD to match the paste queue, and tell it what to display.
pub fn update_hud(shared: &Shared) {
    let view = shared.engine().queue_view(std::time::Instant::now());
    // `Ctrl+V` is intercepted exactly while a queue is being pasted. Doing this here,
    // after every queue change, means it can never be left on with an empty queue.
    zlip_platform::set_drain_mode(view.mode == "draining");
    let Some(hud) = shared.app.get_webview_window("hud") else {
        return;
    };
    // Nothing queued means nothing to show. That also covers "collecting" before the
    // first copy: the pill appears with the first item.
    if view.mode == "idle" || view.len == 0 {
        // Not `hud.hide()`: the pill is shown with a plain `ShowWindow`, so Tauri still believes
        // it is hidden and `hide()` would do nothing, leaving the pill on screen for good.
        hide_window(native_handle(&hud));
    } else {
        if let Ok(size) = hud.outer_size() {
            let work = work_area_for(WindowHandle(0));
            let (w, h) = (size.width as i32, size.height as i32);
            let _ = hud.set_position(PhysicalPosition::new(
                work.right - w - 20,
                work.bottom - h - 20,
            ));
        }
        show_no_activate(native_handle(&hud));
    }
    let _ = shared.app.emit_to("hud", "zlip:queue", &view);
    let _ = shared.app.emit_to("overlay", "zlip:queue", &view);
}

/// Close the overlay when it loses focus, if the user wants that.
pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if window.label() != "overlay" {
        return;
    }
    let Some(shared) = window.app_handle().try_state::<Arc<Shared>>() else {
        return;
    };
    match event {
        WindowEvent::Focused(true) => shared.set_overlay_focused(true),
        WindowEvent::Focused(false) => {
            let settled = shared.overlay_shown_for().is_some_and(|d| d > BLUR_GRACE);
            let was_focused = shared.overlay_focused();
            shared.set_overlay_focused(false);
            if shared.cfg.snapshot().ui.close_on_blur && was_focused && settled {
                hide_overlay(&shared);
            }
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close(); // the app lives in the tray; closing only hides
            hide_overlay(&shared);
        }
        _ => {}
    }
}
