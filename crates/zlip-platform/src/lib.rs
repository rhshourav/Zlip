//! The only crate in Zlip that talks to the operating system.
//!
//! * The *portable* modules ([`event`], [`keymap`], [`policy`]) contain every
//!   decision that can be made without Win32, so they are unit-tested anywhere.
//! * The `win` module holds the `unsafe` Win32 calls: keyboard hook, clipboard
//!   listener/reader/writer, synthetic input, foreground-window handling and the
//!   Credential Manager. It is compiled on Windows, and on other hosts with
//!   `--features typecheck` (used to type-check the Win32 code on Linux; such a
//!   build cannot be *run*).
//!
//! # Threading rules
//! Two dedicated threads run Win32 message loops (`hook`, `listener`). They only
//! classify an event and push it down a channel; all real work happens elsewhere.
//! A low-level hook callback that takes too long is silently skipped by Windows.

pub mod event;
pub mod keymap;
pub mod policy;

pub use event::{
    ClipboardChange, ClipboardContent, ClipboardFlags, HotkeyId, Limits, PlatformError,
    PlatformEvent, ReadResult, WindowHandle,
};

#[cfg(any(windows, feature = "typecheck"))]
mod win;
#[cfg(any(windows, feature = "typecheck"))]
pub use win::*;
