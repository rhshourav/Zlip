//! Tray icon and menu. Zlip has no taskbar button: the tray is its home.

use crate::{state::Shared, windows};
use std::sync::Arc;
use tauri::{
    AppHandle, Manager,
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use zlip_platform::{WindowHandle, foreground_window};

const PRIVATE_ID: &str = "private";

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Zlip", true, None::<&str>)?;
    let private = CheckMenuItem::with_id(
        app,
        PRIVATE_ID,
        "Private mode (pause capture)",
        true,
        false,
        None::<&str>,
    )?;
    let config = MenuItem::with_id(app, "config", "Open settings file", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Zlip", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &PredefinedMenuItem::separator(app)?,
            &private,
            &config,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    app.manage(private.clone());

    TrayIconBuilder::with_id("zlip")
        .icon(Image::from_bytes(include_bytes!("../icons/32x32.png"))?)
        .tooltip("Zlip")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let Some(shared) = app.try_state::<Arc<Shared>>() else {
                return;
            };
            match event.id().as_ref() {
                "open" => windows::show_overlay(&shared, foreground_window()),
                PRIVATE_ID => {
                    let on = !shared.engine().private_mode();
                    shared.engine().set_private_mode(on);
                    sync_private_mode(app, on);
                }
                "config" => {
                    zlip_platform::open_in_shell(&zlip_config::config_path().display().to_string())
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
                && let Some(shared) = tray.app_handle().try_state::<Arc<Shared>>()
            {
                // The tray click made the taskbar the foreground window; there is nothing to paste into.
                windows::show_overlay(&shared, WindowHandle(0));
            }
        })
        .build(app)?;
    Ok(())
}

/// Keep the menu's check mark in step with the real state (it can change from the overlay too).
pub fn sync_private_mode(app: &AppHandle, on: bool) {
    if let Some(item) = app.try_state::<CheckMenuItem<tauri::Wry>>() {
        let _ = item.set_checked(on);
    }
}
