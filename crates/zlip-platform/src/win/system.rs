//! Small OS queries and shell integration.

use super::util::{from_wide, wide};
use crate::WindowHandle;
use std::{
    ffi::c_void,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Dwm::{
        DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
        DwmSetWindowAttribute,
    },
    System::Registry::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
    },
    UI::{Controls::MARGINS, Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
};

/// Whether Windows "Transparency effects" are on. The UI falls back to a solid
/// panel when they are off, since Acrylic would not render anyway.
/// Defaults to `true` if the setting cannot be read.
#[must_use]
pub fn transparency_enabled() -> bool {
    let sub = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let val = wide("EnableTransparency");
    let mut data = 1u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: both names are NUL-terminated; `data`/`size` describe a valid DWORD buffer.
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            sub.as_ptr(),
            val.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&raw mut data).cast::<c_void>(),
            &mut size,
        )
    };
    rc != 0 || data != 0
}

/// Open a file with its default program; falls back to Notepad for types with no
/// association (a `.toml` file often has none).
pub fn open_in_shell(path: &str) {
    let file = wide(path);
    let verb = wide("open");
    // SAFETY: all strings are NUL-terminated and outlive the call. Values above 32 mean success.
    let r = unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    } as usize;
    if r <= 32 {
        let _ = std::process::Command::new("notepad.exe").arg(path).spawn();
    }
}

fn read_dword(
    key: windows_sys::Win32::System::Registry::HKEY,
    sub: &str,
    value: &str,
) -> Option<u32> {
    let (sub, value) = (wide(sub), wide(value));
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: names are NUL-terminated; `data`/`size` describe a valid DWORD buffer.
    let rc = unsafe {
        RegGetValueW(
            key,
            sub.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&raw mut data).cast::<c_void>(),
            &mut size,
        )
    };
    (rc == 0).then_some(data)
}

/// Windows build number (for example 22631 for Windows 11 23H2); 0 if unknown.
#[must_use]
pub fn windows_build() -> u32 {
    let sub = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let val = wide("CurrentBuildNumber");
    let mut buf = [0u16; 32];
    let mut size = std::mem::size_of_val(&buf) as u32;
    // SAFETY: `buf`/`size` describe a valid byte buffer for a REG_SZ value.
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            sub.as_ptr(),
            val.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr().cast::<c_void>(),
            &mut size,
        )
    };
    if rc != 0 {
        return 0;
    }
    from_wide(&buf).trim().parse().unwrap_or(0)
}

/// Whether apps are set to the light theme (defaults to light if unreadable).
#[must_use]
pub fn apps_use_light_theme() -> bool {
    read_dword(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "AppsUseLightTheme",
    )
    .is_none_or(|v| v != 0)
}

/// How a window got its translucent backdrop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    /// Windows 11 22H2+: the system acrylic backdrop, clipped to rounded corners.
    NativeAcrylic,
    /// Not available (Windows 10, early Windows 11). The caller falls back to
    /// `window-vibrancy`'s acrylic, whose blur is rectangular.
    Unavailable,
}

fn set_attr(h: HWND, attr: i32, value: u32) -> bool {
    // SAFETY: `value` is a live u32 and its size is passed as the attribute size.
    unsafe { DwmSetWindowAttribute(h, attr as u32, (&raw const value).cast::<c_void>(), 4) >= 0 }
}

/// Round the window corners (Windows 11) and, on 22H2+, attach the native
/// acrylic backdrop. The webview must have a transparent background.
pub fn apply_native_backdrop(window: WindowHandle, dark: bool) -> Backdrop {
    let h = window.0 as HWND;
    let build = windows_build();
    if build >= 22000 {
        set_attr(h, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND as u32);
    }
    if build < 22621 {
        return Backdrop::Unavailable;
    }
    set_attr(h, DWMWA_USE_IMMERSIVE_DARK_MODE, u32::from(dark));
    let ok = set_attr(h, DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_TRANSIENTWINDOW as u32);
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    // SAFETY: `margins` is a live MARGINS; -1 extends the frame over the whole client area.
    let extended = unsafe { DwmExtendFrameIntoClientArea(h, &margins) } >= 0;
    if ok && extended {
        Backdrop::NativeAcrylic
    } else {
        Backdrop::Unavailable
    }
}
