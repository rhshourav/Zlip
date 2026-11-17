#![forbid(unsafe_code)]
//! The brain of Zlip. Everything that *decides* something lives here, with no
//! operating-system or UI dependency, so it is exercised by ordinary unit tests
//! on any machine:
//!
//! * the capture pipeline: privacy skip rules, secret detection, automations,
//!   storage, de-duplication, paste-queue collection;
//! * the in-RAM search index and the data the UI renders ([`view`]);
//! * preparing a paste: placeholders, transforms, secrets from the vault;
//! * the paste queue, retention, vault expiry and script actions.
//!
//! The Tauri shell is a thin adapter: it reads the OS, calls into [`Engine`]
//! behind one mutex, and performs the resulting OS actions.

mod engine;
mod entry;
pub mod view;

pub use engine::{
    ActionJob, CaptureInput, CaptureOutcome, Engine, EngineError, PINNED_FILTER, PasteContent,
    PasteContext, PasteError, PasteOptions, PreparedPaste, SearchQuery,
};
