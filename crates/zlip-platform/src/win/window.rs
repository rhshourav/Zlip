//! Foreground window, caret, monitor and focus handling.

use super::{
    input::{key_input, send},
    util::from_wide,
};
use crate::{WindowHandle, keymap::VK_MASK};
use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HWND, POINT},
    Graphics::Gdi::{
        ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
        MonitorFromWindow,
    },
    System::Threading::{
        AttachThreadInput, GetCurrentThreadId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    },
    UI::{
        Input::KeyboardAndMouse::KEYEVENTF_KEYUP,
        WindowsAndMessaging::{
            BringWindowToTop, GUITHREADINFO, GWL_EXSTYLE, GetCursorPos, GetForegroundWindow,
            GetGUIThreadInfo, GetWindowLongPtrW, GetWindowTextW, GetWindowThreadProcessId,
            IsWindow, SW_SHOWNOACTIVATE, SetForegroundWindow, SetWindowLongPtrW, ShowWindow,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        },
    },
};
use zlip_core::geometry::Rect;

fn hwnd(h: WindowHandle) -> HWND {
    h.0 as HWND
}

pub fn foreground_window() -> WindowHandle {
    // SAFETY: no preconditions.
    WindowHandle(unsafe { GetForegroundWindow() } as isize)
}

#[must_use]
pub fn is_window(h: WindowHandle) -> bool {
    !h.is_null() && unsafe { IsWindow(hwnd(h)) != 0 }
}

pub(super) fn window_title(h: HWND) -> Option<String> {
    if h.is_null() {
        return None;
    }
    let mut buf = [0u16; 512];
    // SAFETY: `buf` is valid for `buf.len()` UTF-16 units.
    let n = unsafe { GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32) };
    (n > 0).then(|| from_wide(&buf[..n as usize]))
}

/// Full path of the executable behind a process id.
pub(super) fn process_image_name(pid: u32) -> Option<String> {
    // SAFETY: LIMITED_INFORMATION is the least privilege that allows the query
    // and works for most elevated processes too.
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if h.is_null() {
        return None;
    }
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` holds `len` units; `h` is a valid process handle closed below.
    let ok = unsafe { QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) };
    unsafe { CloseHandle(h) };
    (ok != 0).then(|| from_wide(&buf[..len as usize]))
}

pub(super) fn window_pid(h: HWND) -> Option<u32> {
    if h.is_null() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: `pid` is a valid out pointer.
    unsafe { GetWindowThreadProcessId(h, &mut pid) };
    (pid != 0).then_some(pid)
}

/// The text caret of the focused control in `fg`, in screen pixels.
///
/// Only classic Win32 controls report one. Chromium, Electron and UWP apps do
/// not, in which case `None` sends the overlay to its bottom-centre fallback.
#[must_use]
pub fn caret_rect(fg: WindowHandle) -> Option<Rect> {
    if fg.is_null() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: valid out pointer; a zero thread id is returned for dead windows.
    let tid = unsafe { GetWindowThreadProcessId(hwnd(fg), &mut pid) };
    if tid == 0 {
        return None;
    }
    // SAFETY: zeroed GUITHREADINFO with cbSize set, as the API requires.
    let mut info: GUITHREADINFO = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<GUITHREADINFO>() as u32;
    if unsafe { GetGUIThreadInfo(tid, &mut info) } == 0 || info.hwndCaret.is_null() {
        return None;
    }
    let r = info.rcCaret;
    if r.left == 0 && r.top == 0 && r.right == 0 && r.bottom == 0 {
        return None;
    }
    let mut tl = POINT {
        x: r.left,
        y: r.top,
    };
    let mut br = POINT {
        x: r.right,
        y: r.bottom,
    };
    // SAFETY: valid window handle and POINT out-parameters.
    let ok = unsafe {
        ClientToScreen(info.hwndCaret, &mut tl) != 0 && ClientToScreen(info.hwndCaret, &mut br) != 0
    };
    ok.then_some(Rect {
        left: tl.x,
        top: tl.y,
        right: br.x.max(tl.x + 1),
        bottom: br.y.max(tl.y + 1),
    })
}

/// Work area (screen minus taskbar) of the monitor that holds `fg`, or of the
/// monitor under the mouse when there is no foreground window.
#[must_use]
pub fn work_area_for(fg: WindowHandle) -> Rect {
    // SAFETY: both lookups accept any handle/point and fall back to the nearest monitor.
    let monitor = unsafe {
        if fg.is_null() {
            let mut p = POINT { x: 0, y: 0 };
            GetCursorPos(&mut p);
            MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST)
        } else {
            MonitorFromWindow(hwnd(fg), MONITOR_DEFAULTTONEAREST)
        }
    };
    let mut mi: MONITORINFO = unsafe { std::mem::zeroed() };
    mi.cbSize = size_of::<MONITORINFO>() as u32;
    // SAFETY: `mi.cbSize` is set; `monitor` came from the lookups above.
    if unsafe { GetMonitorInfoW(monitor, &mut mi) } == 0 {
        return Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
    }
    let w = mi.rcWork;
    Rect {
        left: w.left,
        top: w.top,
        right: w.right,
        bottom: w.bottom,
    }
}

/// Bring `target` to the foreground even though this process did not receive
/// the last input event (Windows' "foreground lock" normally refuses that).
///
/// Two things lift the lock: this process being the last to *provide input*, and
/// sharing input state with the current foreground thread. Both are used. The
/// input is a tap of an unassigned key, which no program reacts to.
pub fn force_foreground(target: WindowHandle) {
    if target.is_null() {
        return;
    }
    let target = hwnd(target);
    // SAFETY: every call below accepts arbitrary window handles and fails harmlessly.
    unsafe {
        if GetForegroundWindow() == target {
            return;
        }
        let _ = send(&[key_input(VK_MASK, 0), key_input(VK_MASK, KEYEVENTF_KEYUP)]);
        let fg = GetForegroundWindow();
        let me = GetCurrentThreadId();
        let fg_thread = if fg.is_null() {
            0
        } else {
            GetWindowThreadProcessId(fg, null_mut())
        };
        let attached =
            fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, 1) != 0;
        BringWindowToTop(target);
        SetForegroundWindow(target);
        if attached {
            AttachThreadInput(me, fg_thread, 0);
        }
    }
}

/// Give focus back to the window that was active when the overlay opened.
pub fn restore_focus(target: WindowHandle) {
    if is_window(target) {
        force_foreground(target);
    }
}

/// Make a window that never takes focus and never appears in Alt-Tab (the queue HUD).
pub fn set_noactivate_toolwindow(h: WindowHandle) {
    if h.is_null() {
        return;
    }
    // SAFETY: plain style read-modify-write on a window handle; invalid handles fail harmlessly.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd(h), GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd(h),
            GWL_EXSTYLE,
            ex | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize,
        );
    }
}

/// Show a window without stealing focus from the program the user is typing in.
pub fn show_no_activate(h: WindowHandle) {
    if !h.is_null() {
        // SAFETY: invalid handles fail harmlessly.
        unsafe { ShowWindow(hwnd(h), SW_SHOWNOACTIVATE) };
    }
}
