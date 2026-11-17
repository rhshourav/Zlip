#![forbid(unsafe_code)]
//! Zlip configuration: one TOML file, one source of truth.
//!
//! Layers: embedded defaults, then the user's `config.toml`. Unknown keys are
//! errors (a typo should never silently do nothing). A watcher hot-reloads on
//! save, keeps the last good config when the new file is broken, and reports
//! every problem with its path. The settings GUI reads and writes this same file.

pub mod hotkey;
mod io;
mod model;
mod watch;

pub use io::{ConfigError, config_dir, config_path, data_dir, load_or_create, write_atomic};
pub use model::{
    Action, ActionKind, Automation, CategoryColors, Config, ConfigIssue, GlobalChords, Hotkeys,
    Position, Retention, SCHEMA_VERSION, SecurityConfig, Snippet, SnippetFolder, Theme, ThemeMode,
    Ui, When, parse_duration_secs,
};
pub use watch::{ConfigHandle, ConfigWatcher, ReloadEvent, watch};
