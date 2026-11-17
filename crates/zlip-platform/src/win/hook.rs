//! The low-level keyboard hook that owns `Win+V`.
//!
//! `RegisterHotKey` cannot claim `Win+V` (the shell does), so a `WH_KEYBOARD_LL`
//! hook sees the key first and swallows it. The callback must return in
//! microseconds, so it only compares integers and pushes into a channel.

use super::{
    input::{is_down, key_input, send},
    util::{ZLIP_MARKER, win32_error},
};
use crate::{
    HotkeyId, PlatformError, PlatformEvent, WindowHandle,
    keymap::{self, VK_CONTROL, VK_LWIN, VK_MASK, VK_MENU, VK_RWIN, VK_SHIFT},
    policy::ReinstallPolicy,
};
use crossbeam_channel::Sender;
use std::{
    ptr::null_mut,
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicU32, Ordering::Relaxed},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::KEYEVENTF_KEYUP,
        WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, HC_ACTION, HHOOK,
            KBDLLHOOKSTRUCT, KillTimer, LLKHF_INJECTED, MSG, PostThreadMessageW, SetTimer,
            SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN,
            WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER,
        },
    },
};
use zlip_config::{GlobalChords, hotkey::Mods};

const WATCHDOG_TICK_MS: u32 = 5_000;
const REINSTALL_EVERY: Duration = Duration::from_secs(30);

/// Encoded chord per [`HotkeyId`]; 0 disables a slot. Atomics, so the callback never locks.
static SLOTS: [AtomicU32; HotkeyId::COUNT] =
    [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];
static DRAIN: AtomicBool = AtomicBool::new(false);
/// Virtual key of a swallowed hotkey whose key-up (and auto-repeat) must also be swallowed.
static SWALLOWED_VK: AtomicU32 = AtomicU32::new(0);
static SENDER: OnceLock<Sender<PlatformEvent>> = OnceLock::new();

/// Replace the global chords (hot reload of the config).
pub fn set_chords(chords: &GlobalChords) {
    SLOTS[HotkeyId::OpenOverlay.index()].store(keymap::encode(chords.open_overlay), Relaxed);
    SLOTS[HotkeyId::QueueCollect.index()].store(keymap::encode(chords.queue_collect), Relaxed);
    SLOTS[HotkeyId::QueuePaste.index()].store(keymap::encode(chords.queue_paste), Relaxed);
}

/// While `true`, the queue-paste chord (default `Ctrl+V`) is intercepted.
pub fn set_drain_mode(on: bool) {
    DRAIN.store(on, Relaxed);
}

/// Physical modifier state right now. Safe to query for keys *other than* the one
/// whose event is being processed (that key's state is not yet updated).
fn physical_mods() -> Mods {
    let mut m = Mods::NONE;
    if is_down(VK_LWIN) || is_down(VK_RWIN) {
        m = m | Mods::WIN;
    }
    if is_down(VK_CONTROL) {
        m = m | Mods::CTRL;
    }
    if is_down(VK_MENU) {
        m = m | Mods::ALT;
    }
    if is_down(VK_SHIFT) {
        m = m | Mods::SHIFT;
    }
    m
}

fn matching_hotkey(vk: u32, mods: Mods) -> Option<HotkeyId> {
    let draining = DRAIN.load(Relaxed);
    HotkeyId::ALL.into_iter().find(|id| {
        (*id != HotkeyId::QueuePaste || draining)
            && keymap::matches(SLOTS[id.index()].load(Relaxed), mods, vk)
    })
}

/// Tell Windows the Win key was used as a modifier, so Start does not open when
/// it is released (the `Win+V` keystroke itself never reached the shell).
fn mask_win_key() {
    let _ = send(&[key_input(VK_MASK, 0), key_input(VK_MASK, KEYEVENTF_KEYUP)]);
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to a valid KBDLLHOOKSTRUCT for the
        // duration of this call (documented contract of WH_KEYBOARD_LL).
        let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        // Never react to injected input, including our own.
        let injected = kb.flags & LLKHF_INJECTED != 0 || kb.dwExtraInfo == ZLIP_MARKER;
        if !injected {
            let w = wparam as u32;
            let down = w == WM_KEYDOWN || w == WM_SYSKEYDOWN;
            let up = w == WM_KEYUP || w == WM_SYSKEYUP;
            let swallowed = SWALLOWED_VK.load(Relaxed);
            if swallowed != 0 && kb.vkCode == swallowed {
                // Auto-repeat arrives while the key is still physically down. A *fresh*
                // press (key physically up) means the earlier key-up was lost, so forget
                // it instead of swallowing a real keystroke.
                if down && !is_down(swallowed) {
                    SWALLOWED_VK.store(0, Relaxed);
                } else {
                    if up {
                        SWALLOWED_VK.store(0, Relaxed); // the hotkey's key is released
                    }
                    return 1; // swallow auto-repeat and the key-up
                }
            }
            if down {
                let mods = physical_mods();
                if let Some(id) = matching_hotkey(kb.vkCode, mods) {
                    if let Some(tx) = SENDER.get() {
                        // SAFETY: GetForegroundWindow has no preconditions.
                        let fg = unsafe { GetForegroundWindow() } as isize;
                        let _ = tx.try_send(PlatformEvent::Hotkey {
                            id,
                            foreground: WindowHandle(fg),
                        });
                    }
                    SWALLOWED_VK.store(kb.vkCode, Relaxed);
                    if mods.contains(Mods::WIN) {
                        mask_win_key();
                    }
                    return 1;
                }
            }
        }
    }
    // SAFETY: forwarding the unmodified arguments we were called with.
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

fn install() -> Result<HHOOK, PlatformError> {
    // SAFETY: a NULL module name returns the handle of the current executable;
    // `hook_proc` has the exact HOOKPROC signature; dwThreadId 0 = global hook.
    let hook = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(hook_proc),
            GetModuleHandleW(std::ptr::null()),
            0,
        )
    };
    if hook.is_null() {
        Err(win32_error("SetWindowsHookExW"))
    } else {
        Ok(hook)
    }
}

/// Owns the hook thread; stopping or dropping it removes the hook.
#[must_use = "dropping the handle removes the hook"]
pub struct HookHandle {
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for HookHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HookHandle")
    }
}

impl HookHandle {
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if let Some(join) = self.join.take() {
            // SAFETY: posting WM_QUIT to a thread id we own is always sound.
            unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
            let _ = join.join();
        }
    }
}

impl Drop for HookHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Install the hook on its own thread and start delivering events to `tx`.
/// Only one hook per process: a second call returns an error.
pub fn start_hook(
    tx: Sender<PlatformEvent>,
    chords: &GlobalChords,
) -> Result<HookHandle, PlatformError> {
    SENDER
        .set(tx)
        .map_err(|_| PlatformError::Other("the keyboard hook is already running".into()))?;
    set_chords(chords);

    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, PlatformError>>();
    let join = thread::Builder::new()
        .name("zlip-hook".into())
        .spawn(move || run_hook_thread(&ready_tx))
        .map_err(|e| PlatformError::Other(e.to_string()))?;
    match ready_rx.recv() {
        Ok(Ok(thread_id)) => Ok(HookHandle {
            thread_id,
            join: Some(join),
        }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(PlatformError::Other(
            "the hook thread ended unexpectedly".into(),
        )),
    }
}

fn run_hook_thread(ready: &mpsc::Sender<Result<u32, PlatformError>>) {
    let mut hook = match install() {
        Ok(h) => h,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    // SAFETY: GetCurrentThreadId has no preconditions.
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));

    let mut policy = ReinstallPolicy::new(REINSTALL_EVERY);
    // SAFETY: a NULL window with id 0 creates a thread timer whose WM_TIMER
    // messages arrive in this thread's queue.
    let timer = unsafe { SetTimer(null_mut(), 0, WATCHDOG_TICK_MS, None) };
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: standard message loop; `msg` is a valid MSG for the whole loop.
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
        if msg.message == WM_TIMER {
            if policy.tick(
                Duration::from_millis(u64::from(WATCHDOG_TICK_MS)),
                !physical_mods().is_empty(),
            ) {
                // New hook first, then drop the old one: there is never a gap.
                match install() {
                    Ok(fresh) => {
                        // SAFETY: `hook` is a live handle returned by SetWindowsHookExW.
                        unsafe { UnhookWindowsHookEx(hook) };
                        hook = fresh;
                    }
                    Err(e) => tracing::warn!("could not refresh the keyboard hook: {e}"),
                }
            }
        } else {
            // SAFETY: `msg` was filled in by GetMessageW.
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    // SAFETY: both handles are live and owned by this thread.
    unsafe {
        KillTimer(null_mut(), timer);
        UnhookWindowsHookEx(hook);
    }
}
