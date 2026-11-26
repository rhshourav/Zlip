#![forbid(unsafe_code)]
//! Zlip's security layers, all OS-independent and unit-tested on Linux.
//!
//! * [`detector`]  what looks like a secret (API keys, cards, random tokens)
//! * [`vault`]     in-RAM only storage with a time limit and zeroisation
//! * [`blacklist`] which copies must never be captured at all
//! * [`password`]  random passwords for the built-in generator
//!
//! Threat model: see blueprint section 3.1. Malware running as the same user
//! can read the clipboard directly and is out of scope; the defence there is to
//! keep secrets alive for as short a time as possible.

pub mod blacklist;
pub mod detector;
pub mod password;
pub mod vault;

pub use blacklist::{Blacklist, CaptureFacts, SkipReason};
pub use detector::{SensitiveKind, detect_sensitive, is_sensitive};
pub use password::{PasswordError, generate_password};
pub use vault::{ClipboardGuard, SensitiveVault, VaultEntry};
