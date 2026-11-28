//! The low-level keyboard hook that owns `Win+V`.
//!
//! `RegisterHotKey` cannot claim `Win+V` (the shell does), so a `WH_KEYBOARD_LL`
//! hook sees the key first and swallows it. The callback must return in
//! microseconds, so it only compares integers and pushes into a channel.
//!
//! # Why this is built the way it is
//!
//! * **Swallowed keys never reach the OS key-state table.** Once the hook returns
//!   1 for a `V`, `GetAsyncKeyState(V)` keeps saying "up" for the whole press. The
//!   hook therefore never asks the OS about the key it swallowed; it remembers
//!   the key itself and swallows every auto-repeat until the key-up arrives.
//!   (Asking the OS made a held key re-fire the hotkey and toggle the overlay in
//!   a loop.)
//! * **Windows can drop a hook without telling anyone** (a slow callback, a
//!   session change). Two defences: the hook is re-installed on a schedule, and a
//!   second, independent shortcut is registered with `RegisterHotKey`, which the
//!   OS never silently removes. Using that shortcut also schedules a re-install.

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
        atomic::{
            AtomicBool, AtomicU8, AtomicU32,
            Ordering::{Relaxed, SeqCst},
        },
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{
            KEYEVENTF_KEYUP, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
        },
        WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, HC_ACTION, HHOOK,
            KBDLLHOOKSTRUCT, KillTimer, LLKHF_INJECTED, MSG, PostThreadMessageW, SetTimer,
            SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_HOTKEY,
            WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER,
        },
    },
};
use zlip_config::{GlobalChords, hotkey::Mods};

/// How often the hook thread wakes up to do housekeeping.
const WATCHDOG_TICK_MS: u32 = 1_000;
/// How often the hook is proactively replaced (skipped while a modifier is held).
const REINSTALL_EVERY: Duration = Duration::from_secs(30);
/// A swallowed key that has been silent this long and then goes down again is a
/// new press whose earlier key-up was lost, not an auto-repeat. (Auto-repeat starts
/// after at most ~1 s and then repeats every 30-500 ms.)
const STALE_AFTER_MS: u32 = 1_500;
/// Identifier of the `RegisterHotKey` registration (arbitrary, private to this thread).
const BACKUP_HOTKEY_ID: i32 = 0x5A4C;

/// Encoded chord per [`HotkeyId`]; 0 disables a slot. Atomics, so the callback never locks.
static SLOTS: [AtomicU32; HotkeyId::COUNT] =
    [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];
static DRAIN: AtomicBool = AtomicBool::new(false);
/// While `true` the hook passes every key through (used while a shortcut is being recorded).
static PAUSED: AtomicBool = AtomicBool::new(false);
/// Someone asked for the hook to be replaced as soon as no modifier is held.
static REFRESH: AtomicBool = AtomicBool::new(false);
/// Virtual key of a swallowed hotkey whose key-up (and auto-repeat) must also be swallowed.
static SWALLOWED_VK: AtomicU32 = AtomicU32::new(0);
/// `KBDLLHOOKSTRUCT::time` of the last event seen for [`SWALLOWED_VK`].
static SWALLOWED_AT: AtomicU32 = AtomicU32::new(0);
/// Encoded backup chord for opening the overlay; 0 = none. Registered on the hook thread.
static BACKUP: AtomicU32 = AtomicU32::new(0);
static BACKUP_STATE: AtomicU8 = AtomicU8::new(STATE_OFF);
static SENDER: OnceLock<Sender<PlatformEvent>> = OnceLock::new();

const STATE_OFF: u8 = 0;
const STATE_ACTIVE: u8 = 1;
const STATE_UNAVAILABLE: u8 = 2;

/// Whether the backup shortcut is working.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupState {
    /// No backup shortcut is configured.
    Off,
    /// Registered with Windows and ready.
    Active,
    /// Windows refused it, usually because another program already uses that shortcut.
    Unavailable,
}

impl BackupState {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Active => "active",
            Self::Unavailable => "unavailable",
        }
    }
}

/// The state of the backup shortcut as of the hook thread's last housekeeping tick.
#[must_use]
pub fn backup_state() -> BackupState {
    match BACKUP_STATE.load(Relaxed) {
        STATE_ACTIVE => BackupState::Active,
        STATE_UNAVAILABLE => BackupState::Unavailable,
        _ => BackupState::Off,
    }
}

/// Replace the global chords (hot reload of the config).
pub fn set_chords(chords: &GlobalChords) {
    SLOTS[HotkeyId::OpenOverlay.index()].store(keymap::encode(chords.open_overlay), Relaxed);
    SLOTS[HotkeyId::QueueCollect.index()].store(keymap::encode(chords.queue_collect), Relaxed);
    SLOTS[HotkeyId::QueuePaste.index()].store(keymap::encode(chords.queue_paste), Relaxed);
    BACKUP.store(
        chords.open_overlay_backup.map_or(0, keymap::encode),
        Relaxed,
    );
}

/// While `true`, the queue-paste chord (default `Ctrl+V`) is intercepted.
pub fn set_drain_mode(on: bool) {
    DRAIN.store(on, Relaxed);
}

/// Pause or resume every global shortcut (the settings screen pauses them while the
/// user records a new one, so pressing it does not trigger it).
pub fn set_paused(on: bool) {
    PAUSED.store(on, SeqCst);
}

/// Ask the hook thread to replace the hook as soon as no modifier key is held.
/// The "my shortcut stopped working" button calls this.
pub fn request_refresh() {
    REFRESH.store(true, SeqCst);
}

/// Physical modifier state right now. Safe to query for keys *other than* the one
/// whose event is being processed (that key's state is not yet updated), and for
/// modifiers in general, because Zlip never swallows a modifier.
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

/// Tell the controller a hotkey fired. Never blocks (the channel is unbounded).
fn fire(id: HotkeyId) {
    if let Some(tx) = SENDER.get() {
        // SAFETY: GetForegroundWindow has no preconditions.
        let fg = unsafe { GetForegroundWindow() } as isize;
        let _ = tx.try_send(PlatformEvent::Hotkey {
            id,
            foreground: WindowHandle(fg),
        });
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && !PAUSED.load(Relaxed) {
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
                // Do NOT ask the OS whether this key is down: it never saw it.
                let silent_for = kb.time.wrapping_sub(SWALLOWED_AT.load(Relaxed));
                if down && silent_for > STALE_AFTER_MS {
                    // The earlier key-up was lost; this is a fresh press. Handle it below.
                    SWALLOWED_VK.store(0, Relaxed);
                } else {
                    if up {
                        SWALLOWED_VK.store(0, Relaxed); // the hotkey's key is released
                    } else {
                        SWALLOWED_AT.store(kb.time, Relaxed); // auto-repeat: still held
                    }
                    return 1; // swallow auto-repeat and the key-up
                }
            }

            if down {
                let mods = physical_mods();
                if let Some(id) = matching_hotkey(kb.vkCode, mods) {
                    fire(id);
                    SWALLOWED_VK.store(kb.vkCode, Relaxed);
                    SWALLOWED_AT.store(kb.time, Relaxed);
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

/// Make the registered backup shortcut match [`BACKUP`]. Must run on the hook thread,
/// because a hotkey registered without a window belongs to the registering thread.
fn sync_backup(registered: &mut u32) {
    let want = BACKUP.load(Relaxed);
    if want == *registered {
        return;
    }
    if *registered != 0 {
        // SAFETY: removes the thread hotkey this thread registered earlier; failure is harmless.
        unsafe { UnregisterHotKey(null_mut(), BACKUP_HOTKEY_ID) };
    }
    // Remember the attempt even if it fails, so a refused shortcut is not retried every tick.
    *registered = want;
    if want == 0 {
        BACKUP_STATE.store(STATE_OFF, Relaxed);
        return;
    }
    let mods = keymap::win_hotkey_mods((want >> 16) as u8) | MOD_NOREPEAT;
    // SAFETY: a NULL window registers the hotkey for this thread's message queue;
    // the modifiers and virtual key come from a validated chord.
    let ok = unsafe { RegisterHotKey(null_mut(), BACKUP_HOTKEY_ID, mods, want & 0xFFFF) } != 0;
    if ok {
        BACKUP_STATE.store(STATE_ACTIVE, Relaxed);
    } else {
        BACKUP_STATE.store(STATE_UNAVAILABLE, Relaxed);
        tracing::warn!(
            "the backup shortcut could not be registered: {}",
            win32_error("RegisterHotKey")
        );
    }
}

/// Install a new hook, then drop the old one: there is never a gap.
fn replace_hook(hook: &mut HHOOK) {
    match install() {
        Ok(fresh) => {
            // SAFETY: `hook` is a live handle returned by SetWindowsHookExW.
            unsafe { UnhookWindowsHookEx(*hook) };
            *hook = fresh;
            tracing::debug!("keyboard hook re-installed");
        }
        Err(e) => tracing::warn!("could not refresh the keyboard hook: {e}"),
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

    let mut registered = 0u32;
    sync_backup(&mut registered);

    let mut policy = ReinstallPolicy::new(REINSTALL_EVERY);
    // SAFETY: a NULL window with id 0 creates a thread timer whose WM_TIMER
    // messages arrive in this thread's queue.
    let timer = unsafe { SetTimer(null_mut(), 0, WATCHDOG_TICK_MS, None) };
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: standard message loop; `msg` is a valid MSG for the whole loop.
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
        match msg.message {
            WM_TIMER => {
                sync_backup(&mut registered);
                let idle = physical_mods().is_empty();
                let due = policy.tick(Duration::from_millis(u64::from(WATCHDOG_TICK_MS)), !idle);
                // A requested refresh also waits for the modifiers to be released.
                let requested = idle && REFRESH.swap(false, SeqCst);
                if due || requested {
                    replace_hook(&mut hook);
                }
            }
            WM_HOTKEY => {
                if msg.wParam == BACKUP_HOTKEY_ID as usize && !PAUSED.load(Relaxed) {
                    fire(HotkeyId::OpenOverlay);
                    // The backup was needed: the main hook may be the thing that broke.
                    REFRESH.store(true, SeqCst);
                }
            }
            _ => {
                // SAFETY: `msg` was filled in by GetMessageW.
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
    // SAFETY: both handles are live and owned by this thread.
    unsafe {
        KillTimer(null_mut(), timer);
        UnhookWindowsHookEx(hook);
        if registered != 0 {
            UnregisterHotKey(null_mut(), BACKUP_HOTKEY_ID);
        }
    }
}
