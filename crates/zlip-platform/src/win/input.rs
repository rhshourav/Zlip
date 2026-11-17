//! Synthetic keyboard input: paste, and caret movement for `{cursor}`.

use super::util::{ZLIP_MARKER, win32_error};
use crate::{
    PlatformError,
    keymap::{VK_CONTROL, VK_LEFT, VK_V},
};
use std::{
    mem::size_of,
    thread::sleep,
    time::{Duration, Instant},
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, SendInput,
};

/// One key event, stamped with Zlip's marker.
pub(super) fn key_input(vk: u32, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk as u16,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: ZLIP_MARKER,
            },
        },
    }
}

pub(super) fn send(inputs: &[INPUT]) -> Result<(), PlatformError> {
    // SAFETY: `inputs` is a valid slice of fully initialised INPUT structs and
    // the size argument is the size of one element, as SendInput requires.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err(win32_error("SendInput"))
    }
}

pub(super) fn is_down(vk: u32) -> bool {
    // SAFETY: GetAsyncKeyState has no preconditions. The high bit means "down".
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

/// Poll until Win, Alt and Shift are physically released, or `timeout` passes.
/// Returns whether they were released. Pasting while Win is held would turn
/// `Ctrl+V` into `Win+Ctrl+V`; with Shift held it becomes `Ctrl+Shift+V`.
pub fn wait_modifiers_released(timeout: Duration) -> bool {
    const KEYS: [u32; 6] = [0x5B, 0x5C, 0xA4, 0xA5, 0xA0, 0xA1]; // LWIN RWIN LALT RALT LSHIFT RSHIFT
    let start = Instant::now();
    loop {
        if !KEYS.iter().any(|&k| is_down(k)) {
            return true;
        }
        if start.elapsed() >= timeout {
            return false;
        }
        sleep(Duration::from_millis(10));
    }
}

/// Send `Ctrl+V`. If the user is already physically holding Ctrl, only `V` is
/// sent, so their key state is not corrupted by a stray Ctrl-up.
pub fn send_paste() -> Result<(), PlatformError> {
    let ctrl_held = is_down(VK_CONTROL);
    let mut seq = Vec::with_capacity(4);
    if !ctrl_held {
        seq.push(key_input(VK_CONTROL, 0));
    }
    seq.push(key_input(VK_V, 0));
    seq.push(key_input(VK_V, KEYEVENTF_KEYUP));
    if !ctrl_held {
        seq.push(key_input(VK_CONTROL, KEYEVENTF_KEYUP));
    }
    send(&seq)
}

/// Press Left-arrow `count` times (capped), to land on `{cursor}` after a paste.
pub fn send_left_arrows(count: usize) -> Result<(), PlatformError> {
    const MAX: usize = 4096;
    const CHUNK: usize = 128;
    let mut remaining = count.min(MAX);
    while remaining > 0 {
        let n = remaining.min(CHUNK);
        let mut seq = Vec::with_capacity(n * 2);
        for _ in 0..n {
            seq.push(key_input(VK_LEFT, KEYEVENTF_EXTENDEDKEY));
            seq.push(key_input(VK_LEFT, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP));
        }
        send(&seq)?;
        remaining -= n;
        if remaining > 0 {
            sleep(Duration::from_millis(2)); // let the target's input queue breathe
        }
    }
    Ok(())
}
