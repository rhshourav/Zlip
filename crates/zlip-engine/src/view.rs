//! Plain data sent to the UI. All field names are `snake_case`, like the config.

use serde::Serialize;
use zlip_config::{Config, Hotkeys, PasswordGenerator, Theme, Ui};
use zlip_core::Category;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ItemView {
    pub id: u64,
    pub category: Category,
    /// Short secondary text: language, domain, dimensions, hex, folder...
    pub label: Option<String>,
    /// First lines as plain text (masked for secrets).
    pub preview: String,
    /// Escaped, highlighted HTML of the preview (code only).
    pub preview_html: Option<String>,
    /// `#RRGGBB(AA)` for colour swatches.
    pub color: Option<String>,
    pub created_at_ms: i64,
    pub pinned: bool,
    pub folder: Option<String>,
    pub size_bytes: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub source_app: Option<String>,
    /// Secrets only: time left before the wipe.
    pub expires_in_ms: Option<u64>,
    /// 1-based position in the paste queue, if queued.
    pub queue_position: Option<usize>,
    /// Config snippets cannot be pinned, deleted or edited from the overlay.
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct ItemDetail {
    pub id: u64,
    /// Escaped HTML: highlighted and pretty-printed for code.
    pub html: String,
    pub language: Option<String>,
    pub reformatted: bool,
    pub truncated: bool,
    /// Text the quick editor starts from (never for secrets or very large items).
    pub editable_text: Option<String>,
    pub files: Vec<String>,
    /// `data:image/png;base64,...` for images.
    pub image: Option<String>,
    pub line_count: usize,
    pub char_count: usize,
    pub size_bytes: u64,
    pub source_app: Option<String>,
    /// Names of configured script actions that apply to this item.
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QueueView {
    /// `idle`, `collecting` or `draining`.
    pub mode: &'static str,
    pub len: usize,
    /// Preview of the item that the next paste will insert.
    pub next_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StatusView {
    pub private_mode: bool,
    pub queue: QueueView,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FolderView {
    pub name: String,
    pub count: usize,
    /// `pinned` (history items you pinned) or `snippets` (from the config file).
    pub kind: &'static str,
    /// The `folder` value to pass to `search`.
    pub filter: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ActionReport {
    Done {
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        timed_out: bool,
        truncated: bool,
    },
    /// Nothing ran. Show `sha256` and ask the user to approve this exact script.
    NeedsTrust {
        action: String,
        path: String,
        sha256: String,
    },
    Failed {
        message: String,
    },
}

/// What the OS layer knows that affects drawing.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
pub struct OsInfo {
    pub build: u32,
    pub native_backdrop: bool,
    pub transparency: bool,
    pub dark: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OsView {
    /// Windows 11 rounds window corners; on Windows 10 the panel stays square.
    pub rounded: bool,
    pub native_backdrop: bool,
    pub transparency: bool,
    pub dark: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UiConfig {
    pub hotkeys: Hotkeys,
    pub theme: Theme,
    pub ui: Ui,
    /// The generator's settings: the overlay needs to know whether to offer it, and what it makes.
    pub password_generator: PasswordGenerator,
    pub os: OsView,
    pub action_names: Vec<String>,
}

impl UiConfig {
    #[must_use]
    pub fn new(cfg: &Config, os: OsInfo) -> Self {
        Self {
            hotkeys: cfg.hotkeys.clone(),
            theme: cfg.theme.clone(),
            ui: cfg.ui.clone(),
            password_generator: cfg.password_generator.clone(),
            os: OsView {
                rounded: os.build >= 22000,
                native_backdrop: os.native_backdrop,
                transparency: os.transparency,
                dark: os.dark,
            },
            action_names: cfg.actions.iter().map(|a| a.name.clone()).collect(),
        }
    }
}
