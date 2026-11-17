use crate::PlatformError;
use std::iter::once;
use windows_sys::Win32::Foundation::GetLastError;

/// Stamped on every key event Zlip injects, so the hook can ignore its own input.
pub(super) const ZLIP_MARKER: usize = 0x5A4C_4950; // "ZLIP"

/// UTF-16, NUL-terminated.
pub(super) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(once(0)).collect()
}

/// Text up to the first NUL.
pub(super) fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub(super) fn win32_error(call: &'static str) -> PlatformError {
    // SAFETY: GetLastError has no preconditions.
    PlatformError::Win32 {
        call,
        code: unsafe { GetLastError() },
    }
}
