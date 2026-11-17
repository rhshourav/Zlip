//! Clipboard change listener, reader and writer.
//!
//! Reading is a two-step *session* so the privacy decision happens before any
//! content is copied: open, read [`ClipboardFlags`], let the caller decide, and
//! only then read the payload.

use super::{
    util::{wide, win32_error},
    window::{foreground_window, process_image_name, window_pid, window_title},
};
use crate::{
    ClipboardChange, ClipboardContent, ClipboardFlags, Limits, PlatformError, PlatformEvent,
    ReadResult,
};
use crossbeam_channel::Sender;
use std::{
    marker::PhantomData,
    ptr::{copy_nonoverlapping, null, null_mut},
    sync::{OnceLock, mpsc},
    thread::{self, JoinHandle, sleep},
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{GlobalFree, HANDLE, HWND, LPARAM, LRESULT, WPARAM},
    System::{
        DataExchange::{
            AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData,
            GetClipboardOwner, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
            OpenClipboard, RegisterClipboardFormatW, RemoveClipboardFormatListener,
            SetClipboardData,
        },
        LibraryLoader::GetModuleHandleW,
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
        Ole::{CF_DIB, CF_DIBV5, CF_HDROP, CF_UNICODETEXT},
        Threading::GetCurrentThreadId,
    },
    UI::{
        Shell::{DROPFILES, DragQueryFileW, HDROP},
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
            HWND_MESSAGE, MSG, PostQuitMessage, PostThreadMessageW, RegisterClassW,
            TranslateMessage, WM_CLIPBOARDUPDATE, WM_DESTROY, WM_QUIT, WNDCLASSW,
        },
    },
};
use zeroize::Zeroizing;
use zlip_core::{
    ImageData, Payload,
    image::{decode_png, dib_to_rgba, encode_png, png_dimensions, rgba_to_dib},
};
use zlip_security::ClipboardGuard;

const FMT_EXCLUDE: &str = "ExcludeClipboardContentFromMonitorProcessing";
const FMT_HISTORY: &str = "CanIncludeInClipboardHistory";
const FMT_CLOUD: &str = "CanUploadToCloudClipboard";
const FMT_PNG: &str = "PNG";

fn register(name: &str) -> u32 {
    let w = wide(name);
    // SAFETY: `w` is NUL-terminated and outlives the call.
    unsafe { RegisterClipboardFormatW(w.as_ptr()) }
}

#[must_use]
pub fn clipboard_sequence() -> u32 {
    // SAFETY: no preconditions.
    unsafe { GetClipboardSequenceNumber() }
}

/// Another program may hold the clipboard open for a few milliseconds.
fn open_with_retry() -> Result<(), PlatformError> {
    for attempt in 0..12u64 {
        // SAFETY: a NULL owner window is allowed for read access and for clearing.
        if unsafe { OpenClipboard(null_mut()) } != 0 {
            return Ok(());
        }
        sleep(Duration::from_millis(10 + attempt * 10));
    }
    Err(PlatformError::ClipboardBusy)
}

// ------------------------------------------------------------ the listener

static LISTENER_TX: OnceLock<Sender<PlatformEvent>> = OnceLock::new();

unsafe extern "system" fn listener_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CLIPBOARDUPDATE => {
            if let Some(tx) = LISTENER_TX.get() {
                // Gather only what is cheap and time-sensitive; the controller does the rest.
                // SAFETY: both calls have no preconditions.
                let (sequence, owner) =
                    unsafe { (GetClipboardSequenceNumber(), GetClipboardOwner()) };
                let owner_exe = window_pid(owner).and_then(process_image_name);
                let foreground_title = window_title(foreground_window().0 as HWND);
                let _ = tx.try_send(PlatformEvent::ClipboardChanged(ClipboardChange {
                    sequence,
                    owner_exe,
                    foreground_title,
                }));
            }
            0
        }
        WM_DESTROY => {
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            0
        }
        // SAFETY: default handling with the unmodified arguments.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[must_use = "dropping the handle stops clipboard monitoring"]
pub struct ListenerHandle {
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ListenerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ListenerHandle")
    }
}

impl ListenerHandle {
    pub fn stop(mut self) {
        self.shutdown();
    }
    fn shutdown(&mut self) {
        if let Some(j) = self.join.take() {
            // SAFETY: posting WM_QUIT to a thread id we own is sound.
            unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
            let _ = j.join();
        }
    }
}

impl Drop for ListenerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Watch for clipboard changes on a dedicated thread (message-only window).
pub fn start_listener(tx: Sender<PlatformEvent>) -> Result<ListenerHandle, PlatformError> {
    LISTENER_TX
        .set(tx)
        .map_err(|_| PlatformError::Other("the clipboard listener is already running".into()))?;
    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, PlatformError>>();
    let join = thread::Builder::new()
        .name("zlip-listener".into())
        .spawn(move || run_listener(&ready_tx))
        .map_err(|e| PlatformError::Other(e.to_string()))?;
    match ready_rx.recv() {
        Ok(Ok(thread_id)) => Ok(ListenerHandle {
            thread_id,
            join: Some(join),
        }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(PlatformError::Other(
            "the listener thread ended unexpectedly".into(),
        )),
    }
}

fn run_listener(ready: &mpsc::Sender<Result<u32, PlatformError>>) {
    let class = wide("ZlipClipboardListener");
    // SAFETY: standard window-class registration and message-only window creation.
    let hwnd = unsafe {
        let hinst = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(listener_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            0,
            class.as_ptr(),
            null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            hinst,
            null(),
        )
    };
    if hwnd.is_null() {
        let _ = ready.send(Err(win32_error("CreateWindowExW")));
        return;
    }
    // SAFETY: `hwnd` is the window we just created on this thread.
    if unsafe { AddClipboardFormatListener(hwnd) } == 0 {
        let _ = ready.send(Err(win32_error("AddClipboardFormatListener")));
        unsafe { DestroyWindow(hwnd) };
        return;
    }
    // SAFETY: no preconditions.
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: standard message loop; `msg` is valid for the whole loop.
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    // SAFETY: `hwnd` is still our live window.
    unsafe {
        RemoveClipboardFormatListener(hwnd);
        DestroyWindow(hwnd);
    }
}

// ------------------------------------------------------------- reading

/// An open clipboard. It is closed when dropped, and it must be dropped on the
/// thread that opened it (hence `!Send`). Keep it open only briefly.
#[derive(Debug)]
pub struct ClipboardSession {
    _not_send: PhantomData<*const ()>,
}

/// Copy the bytes of a global-memory handle without trusting its size.
fn with_locked<R>(h: HANDLE, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
    if h.is_null() {
        return None;
    }
    // SAFETY: `h` is a clipboard-owned HGLOBAL; the slice covers exactly the
    // allocation (GlobalSize) and is only used while the lock is held.
    unsafe {
        let size = GlobalSize(h);
        let p = GlobalLock(h);
        if p.is_null() {
            return None;
        }
        let r = f(std::slice::from_raw_parts(p.cast::<u8>(), size));
        GlobalUnlock(h);
        Some(r)
    }
}

fn global_size(h: HANDLE) -> usize {
    // SAFETY: GlobalSize accepts any HGLOBAL (returns 0 on failure).
    if h.is_null() {
        0
    } else {
        unsafe { GlobalSize(h) }
    }
}

impl ClipboardSession {
    pub fn open() -> Result<Self, PlatformError> {
        open_with_retry()?;
        Ok(Self {
            _not_send: PhantomData,
        })
    }

    /// The sequence number *while the clipboard is held open*: nobody can change
    /// it until this session ends, so it describes exactly the content read here.
    #[must_use]
    pub fn sequence(&self) -> u32 {
        clipboard_sequence()
    }

    /// Microsoft's privacy contract, read first so excluded content is never touched.
    #[must_use]
    pub fn flags(&self) -> ClipboardFlags {
        let dword = |name: &str| -> Option<u32> {
            let fmt = register(name);
            // SAFETY: IsClipboardFormatAvailable/GetClipboardData need the clipboard open, which `self` guarantees.
            if unsafe { IsClipboardFormatAvailable(fmt) } == 0 {
                return None;
            }
            let h = unsafe { GetClipboardData(fmt) };
            with_locked(h, |b| {
                b.get(..4)
                    .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
            })
            .flatten()
        };
        ClipboardFlags {
            // SAFETY: as above.
            exclude_from_monitor: unsafe { IsClipboardFormatAvailable(register(FMT_EXCLUDE)) } != 0,
            can_include_in_history: dword(FMT_HISTORY),
            can_upload_to_cloud: dword(FMT_CLOUD),
        }
    }

    /// Files, then text, then image: a spreadsheet range offers text *and* a
    /// bitmap, and the text is what the user wants.
    #[must_use]
    pub fn read_payload(&self, limits: &Limits) -> ReadResult {
        // SAFETY: format availability queries only need the clipboard open.
        let has = |fmt: u32| unsafe { IsClipboardFormatAvailable(fmt) } != 0;
        if has(u32::from(CF_HDROP))
            && let Some(r) = self.read_files(limits)
        {
            return r;
        }
        if has(u32::from(CF_UNICODETEXT)) {
            return self.read_text(limits);
        }
        let png = register(FMT_PNG);
        if has(png) || has(u32::from(CF_DIBV5)) || has(u32::from(CF_DIB)) {
            return self.read_image(limits, png);
        }
        ReadResult::Unsupported
    }

    fn read_text(&self, limits: &Limits) -> ReadResult {
        // SAFETY: the clipboard is open.
        let h = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT)) };
        if global_size(h) > limits.max_text_bytes.saturating_mul(2) {
            return ReadResult::TooLarge;
        }
        // Decode straight from the locked clipboard memory: no intermediate UTF-16
        // copy of possibly secret text is made.
        let text = with_locked(h, |b| {
            let units = b
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .take_while(|&u| u != 0);
            char::decode_utf16(units)
                .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect::<String>()
        });
        match text {
            Some(t) if !t.is_empty() => ReadResult::Payload(Payload::Text(t)),
            _ => ReadResult::Unsupported,
        }
    }

    fn read_files(&self, limits: &Limits) -> Option<ReadResult> {
        // SAFETY: the clipboard is open; an HDROP is a valid argument to DragQueryFileW.
        let drop = unsafe { GetClipboardData(u32::from(CF_HDROP)) } as HDROP;
        if drop.is_null() {
            return None;
        }
        let count = unsafe { DragQueryFileW(drop, u32::MAX, null_mut(), 0) } as usize;
        if count == 0 {
            return None;
        }
        if count > limits.max_files {
            return Some(ReadResult::TooLarge);
        }
        let mut paths = Vec::with_capacity(count);
        for i in 0..count {
            // SAFETY: first call asks for the length, second fills a buffer of len+1 units.
            let len = unsafe { DragQueryFileW(drop, i as u32, null_mut(), 0) } as usize;
            let mut buf = vec![0u16; len + 1];
            let n = unsafe { DragQueryFileW(drop, i as u32, buf.as_mut_ptr(), buf.len() as u32) }
                as usize;
            paths.push(String::from_utf16_lossy(&buf[..n.min(len)]));
        }
        Some(ReadResult::Payload(Payload::Files(paths)))
    }

    fn read_image(&self, limits: &Limits, png_fmt: u32) -> ReadResult {
        // Prefer the PNG format (keeps alpha); fall back to the bitmap formats.
        // SAFETY: the clipboard is open for all calls below.
        if unsafe { IsClipboardFormatAvailable(png_fmt) } != 0 {
            let h = unsafe { GetClipboardData(png_fmt) };
            if global_size(h) > limits.max_image_bytes {
                return ReadResult::TooLarge;
            }
            if let Some(png) = with_locked(h, <[u8]>::to_vec)
                && let Some((width, height)) = png_dimensions(&png)
            {
                return ReadResult::Payload(Payload::Image(ImageData { png, width, height }));
            }
        }
        for fmt in [u32::from(CF_DIBV5), u32::from(CF_DIB)] {
            if unsafe { IsClipboardFormatAvailable(fmt) } == 0 {
                continue;
            }
            let h = unsafe { GetClipboardData(fmt) };
            if global_size(h) > limits.max_image_bytes.saturating_mul(2) {
                return ReadResult::TooLarge;
            }
            let converted = with_locked(h, |b| {
                dib_to_rgba(b).ok().and_then(|img| {
                    encode_png(&img)
                        .ok()
                        .map(|png| (png, img.width, img.height))
                })
            });
            if let Some(Some((png, width, height))) = converted {
                if png.len() > limits.max_image_bytes {
                    return ReadResult::TooLarge;
                }
                return ReadResult::Payload(Payload::Image(ImageData { png, width, height }));
            }
        }
        ReadResult::Unsupported
    }
}

impl Drop for ClipboardSession {
    fn drop(&mut self) {
        // SAFETY: the session opened the clipboard on this thread.
        unsafe { CloseClipboard() };
    }
}

// ------------------------------------------------------------- writing

/// Put `bytes` on the clipboard as `format` (the system takes ownership on success).
fn set_data(format: u32, bytes: &[u8]) -> Result<(), PlatformError> {
    // SAFETY: allocate a movable block, copy into it while locked, hand it to the
    // clipboard; free it ourselves only if the clipboard did not take it.
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
        if h.is_null() {
            return Err(win32_error("GlobalAlloc"));
        }
        let p = GlobalLock(h);
        if p.is_null() {
            GlobalFree(h);
            return Err(win32_error("GlobalLock"));
        }
        copy_nonoverlapping(bytes.as_ptr(), p.cast::<u8>(), bytes.len());
        GlobalUnlock(h);
        if SetClipboardData(format, h).is_null() {
            let e = win32_error("SetClipboardData");
            GlobalFree(h);
            return Err(e);
        }
    }
    Ok(())
}

fn utf16_bytes(text: &str) -> Zeroizing<Vec<u8>> {
    let mut v = Zeroizing::new(Vec::with_capacity(text.len() * 2 + 2));
    for u in text.encode_utf16().chain(std::iter::once(0)) {
        v.extend_from_slice(&u.to_le_bytes());
    }
    v
}

/// A `CF_HDROP` payload: `DROPFILES` header, then NUL-separated wide paths and a final NUL.
fn hdrop_bytes(paths: &[String]) -> Vec<u8> {
    let header = DROPFILES {
        pFiles: size_of::<DROPFILES>() as u32,
        pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
        fNC: 0,
        fWide: 1,
    };
    let mut v = Vec::new();
    // SAFETY: DROPFILES is a plain-old-data struct; viewing it as bytes is sound.
    v.extend_from_slice(unsafe {
        std::slice::from_raw_parts((&raw const header).cast::<u8>(), size_of::<DROPFILES>())
    });
    for p in paths {
        for u in p.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v.extend_from_slice(&[0, 0]);
    }
    v.extend_from_slice(&[0, 0]);
    v
}

/// Replace the clipboard with `content`, marked so that neither Windows' own
/// history, the cloud clipboard, nor Zlip itself re-captures it. Returns the
/// clipboard sequence number *after* the write, so the caller can recognise and
/// ignore the resulting change notification.
pub fn write_clipboard(content: ClipboardContent<'_>) -> Result<u32, PlatformError> {
    open_with_retry()?;
    let result = (|| {
        // SAFETY: the clipboard is open.
        if unsafe { EmptyClipboard() } == 0 {
            return Err(win32_error("EmptyClipboard"));
        }
        match content {
            ClipboardContent::Text(t) => set_data(u32::from(CF_UNICODETEXT), &utf16_bytes(t))?,
            ClipboardContent::Files(f) => {
                set_data(u32::from(CF_HDROP), &hdrop_bytes(f))?;
                set_data(u32::from(CF_UNICODETEXT), &utf16_bytes(&f.join("\r\n")))?;
            }
            ClipboardContent::Image(img) => {
                if let Ok(rgba) = decode_png(&img.png) {
                    set_data(u32::from(CF_DIB), &rgba_to_dib(&rgba))?;
                }
                set_data(register(FMT_PNG), &img.png)?;
            }
        }
        set_data(register(FMT_EXCLUDE), &[0u8; 4])?;
        set_data(register(FMT_HISTORY), &0u32.to_le_bytes())?;
        set_data(register(FMT_CLOUD), &0u32.to_le_bytes())
    })();
    // SAFETY: closes the clipboard we opened above.
    unsafe { CloseClipboard() };
    result?;
    Ok(clipboard_sequence())
}

pub fn clear_clipboard() -> Result<(), PlatformError> {
    open_with_retry()?;
    // SAFETY: the clipboard is open; it is closed right after.
    let ok = unsafe { EmptyClipboard() } != 0;
    let err = (!ok).then(|| win32_error("EmptyClipboard"));
    unsafe { CloseClipboard() };
    err.map_or(Ok(()), Err)
}

/// The vault's view of the system clipboard.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClipboard;

impl ClipboardGuard for SystemClipboard {
    fn clipboard_sequence(&self) -> u32 {
        clipboard_sequence()
    }
    fn clear_clipboard(&self) {
        if let Err(e) = clear_clipboard() {
            tracing::warn!("could not clear the clipboard: {e}");
        }
    }
}
