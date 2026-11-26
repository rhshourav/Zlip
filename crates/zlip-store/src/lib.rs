#![forbid(unsafe_code)]
//! Encrypted clipboard history.
//!
//! * Metadata (category, timestamps, size, pin state) lives in SQLite in the clear
//!   so listing and retention never need to decrypt anything.
//! * Content and source-app are sealed per row with XChaCha20-Poly1305. The
//!   associated data binds each ciphertext to its row id and field, so rows cannot
//!   be swapped or replayed.
//! * Images are encrypted content-addressed files, named by a *keyed* BLAKE3 hash
//!   so duplicates collapse but the file name reveals nothing about the picture.
//! * The 32-byte master key is supplied by the caller (Windows Credential Manager
//!   in `zlip-platform`); this crate never touches the OS.
//!
//! Sensitive items are not handled here at all: they live only in the in-RAM vault.

mod crypto;
mod store;

pub use crypto::{MasterKey, fill_random};
pub use store::{
    IndexedItem, Inserted, NewItem, RetentionPolicy, Store, StoreError, StoredItem, StoredKind,
};
