#![forbid(unsafe_code)]
//! Script actions: hand the selected item to a program the user configured.
//!
//! Safety rules (blueprint section 3.8):
//! * Actions exist only in the config file; nothing is ever built from clipboard text.
//! * The program is started *directly* with an argument vector: no `cmd /c`, no
//!   shell parsing. Batch files are refused for that reason.
//! * The clipboard text goes to **stdin**, never into arguments.
//! * Every run has a timeout, after which the process is killed.
//! * The first run of a script (and the first run after the file changes)
//!   requires confirmation of its SHA-256; see [`TrustStore`].
//! * Sensitive items must never reach this crate. [`ActionInput`] makes the caller say so.

mod runner;
mod trust;

pub use runner::{ActionError, ActionInput, ActionResult, RunOutcome, run_script};
pub use trust::{TrustStore, sha256_file};
