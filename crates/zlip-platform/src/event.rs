//! Plain data exchanged between the OS threads and the rest of Zlip.

use zlip_core::{ImageData, Payload};

/// The global hotkeys the low-level hook can fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyId {
    OpenOverlay = 0,
    QueueCollect = 1,
    /// Only active while a paste queue is being drained.
    QueuePaste = 2,
}

impl HotkeyId {
    pub const COUNT: usize = 3;
    pub const ALL: [Self; Self::COUNT] = [Self::OpenOverlay, Self::QueueCollect, Self::QueuePaste];

    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }
}

/// An `HWND` as a plain integer, so it can cross threads and crates safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WindowHandle(pub isize);

impl WindowHandle {
    #[must_use]
    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

/// What the listener thread knows the instant the clipboard changes.
#[derive(Debug, Clone)]
pub struct ClipboardChange {
    /// `GetClipboardSequenceNumber` at that instant.
    pub sequence: u32,
    /// Full path of the process that owns the clipboard, if it could be resolved.
    pub owner_exe: Option<String>,
    pub foreground_title: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PlatformEvent {
    Hotkey {
        id: HotkeyId,
        foreground: WindowHandle,
    },
    ClipboardChanged(ClipboardChange),
}

/// Microsoft's clipboard privacy contract, read from the clipboard formats.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClipboardFlags {
    /// `ExcludeClipboardContentFromMonitorProcessing` is present.
    pub exclude_from_monitor: bool,
    /// Value of `CanIncludeInClipboardHistory`, if present.
    pub can_include_in_history: Option<u32>,
    /// Value of `CanUploadToCloudClipboard`, if present.
    pub can_upload_to_cloud: Option<u32>,
}

/// Upper bounds applied *before* copying data out of the clipboard.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_text_bytes: usize,
    pub max_image_bytes: usize,
    pub max_files: usize,
}

/// What to put on the clipboard. Text is borrowed so a secret never has to be
/// copied into a long-lived `Payload`.
#[derive(Debug, Clone, Copy)]
pub enum ClipboardContent<'a> {
    Text(&'a str),
    Files(&'a [String]),
    Image(&'a ImageData),
}

#[derive(Debug)]
pub enum ReadResult {
    Payload(Payload),
    /// A format was present but exceeded [`Limits`]; nothing was copied.
    TooLarge,
    /// Nothing Zlip understands (for example a private application format).
    Unsupported,
}

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("{call} failed (Windows error {code})")]
    Win32 { call: &'static str, code: u32 },
    #[error("the clipboard is held by another program")]
    ClipboardBusy,
    #[error("{0}")]
    Other(String),
}
