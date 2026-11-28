use crate::hotkey::{Chord, Mods, ParseError, QuickPaste};
use crate::password_pattern::parse_password_pattern;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zlip_core::{Category, Transform, parse_color};

pub const SCHEMA_VERSION: u32 = 1;

/// A problem found in the config, with the TOML path it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIssue {
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for ConfigIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

/// The whole configuration file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Config format version. Currently 1.
    pub schema: u32,
    pub hotkeys: Hotkeys,
    pub theme: Theme,
    pub ui: Ui,
    pub retention: Retention,
    pub security: SecurityConfig,
    pub password_generator: PasswordGenerator,
    pub snippet_folders: Vec<SnippetFolder>,
    pub actions: Vec<Action>,
    pub automations: Vec<Automation>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            hotkeys: Hotkeys::default(),
            theme: Theme::default(),
            ui: Ui::default(),
            retention: Retention::default(),
            security: SecurityConfig::default(),
            password_generator: PasswordGenerator::default(),
            snippet_folders: Vec::new(),
            actions: Vec::new(),
            automations: Vec::new(),
        }
    }
}

/// Shortcuts, written like `Ctrl+Shift+Q`. Global ones must include a modifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Hotkeys {
    /// Global: show or hide the overlay.
    pub open_overlay: String,
    /// Global: a second shortcut that opens the overlay even if the main one stops responding. Leave empty to turn it off.
    pub open_overlay_backup: String,
    /// Global: start collecting copies, press again to start pasting them in order.
    pub queue_collect: String,
    /// Global, only while a queue is being pasted: paste the next queued item.
    pub queue_paste: String,
    /// In the overlay: paste the selected item as plain text.
    pub paste_plain: String,
    /// In the overlay: paste the 1st to 9th visible item, written like `Alt+{1..9}`.
    pub quick_paste: String,
    /// In the overlay: expand or collapse the selected item.
    pub expand: String,
    /// In the overlay: pin the selected item.
    pub pin: String,
    /// In the overlay: add the selected item to the paste queue.
    pub queue_add: String,
    /// In the overlay: delete the selected item.
    pub delete: String,
    /// In the overlay: close.
    pub close: String,
    /// In the overlay: previous category.
    pub prev_category: String,
    /// In the overlay: next category.
    pub next_category: String,
    /// In the overlay: open the password generator (and make a new password while it is open).
    pub generate_password: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        let s = |v: &str| v.to_owned();
        Self {
            open_overlay: s("Win+V"),
            open_overlay_backup: s("Ctrl+Alt+V"),
            queue_collect: s("Ctrl+Shift+Q"),
            queue_paste: s("Ctrl+V"),
            paste_plain: s("Shift+Enter"),
            quick_paste: s("Alt+{1..9}"),
            expand: s("Tab"),
            pin: s("Ctrl+P"),
            queue_add: s("Ctrl+Q"),
            delete: s("Delete"),
            close: s("Escape"),
            prev_category: s("Ctrl+Left"),
            next_category: s("Ctrl+Right"),
            generate_password: s("Ctrl+G"),
        }
    }
}

/// The three chords the OS-level hook needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalChords {
    pub open_overlay: Chord,
    /// `None` when the backup shortcut is turned off (empty in the config).
    pub open_overlay_backup: Option<Chord>,
    pub queue_collect: Chord,
    pub queue_paste: Chord,
}

impl Hotkeys {
    pub fn global_chords(&self) -> Result<GlobalChords, ParseError> {
        let backup = self.open_overlay_backup.trim();
        Ok(GlobalChords {
            open_overlay: self.open_overlay.parse()?,
            open_overlay_backup: if backup.is_empty() {
                None
            } else {
                Some(backup.parse::<Chord>()?)
            },
            queue_collect: self.queue_collect.parse()?,
            queue_paste: self.queue_paste.parse()?,
        })
    }

    pub fn quick_paste_mods(&self) -> Result<Mods, ParseError> {
        Ok(self.quick_paste.parse::<QuickPaste>()?.mods)
    }

    fn local(&self) -> [(&'static str, &str); 9] {
        [
            ("paste_plain", &self.paste_plain),
            ("expand", &self.expand),
            ("pin", &self.pin),
            ("queue_add", &self.queue_add),
            ("delete", &self.delete),
            ("close", &self.close),
            ("prev_category", &self.prev_category),
            ("next_category", &self.next_category),
            ("generate_password", &self.generate_password),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Auto,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Theme {
    pub mode: ThemeMode,
    /// How much the glass is tinted, 0.0 (clear) to 1.0 (solid).
    #[schemars(range(min = 0.0, max = 1.0))]
    pub tint_opacity: f32,
    /// Window corner radius in pixels, 0 to 48.
    #[schemars(range(min = 0, max = 48))]
    pub corner_radius: u32,
    /// Turn motion off entirely when false.
    pub animations: bool,
    pub categories: CategoryColors,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Auto,
            tint_opacity: 0.14,
            corner_radius: 24,
            animations: true,
            categories: CategoryColors::default(),
        }
    }
}

/// Pastel colour per category, written as `#RRGGBB`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CategoryColors {
    pub text: String,
    pub link: String,
    pub code: String,
    pub image: String,
    pub color: String,
    pub file: String,
    pub snippet: String,
    pub sensitive: String,
}

impl Default for CategoryColors {
    fn default() -> Self {
        let d = |c: Category| c.default_hex().to_owned();
        Self {
            text: d(Category::Text),
            link: d(Category::Link),
            code: d(Category::Code),
            image: d(Category::Image),
            color: d(Category::Color),
            file: d(Category::File),
            snippet: d(Category::Snippet),
            sensitive: d(Category::Sensitive),
        }
    }
}

impl CategoryColors {
    #[must_use]
    pub fn get(&self, c: Category) -> &str {
        match c {
            Category::Text => &self.text,
            Category::Link => &self.link,
            Category::Code => &self.code,
            Category::Image => &self.image,
            Category::Color => &self.color,
            Category::File => &self.file,
            Category::Snippet => &self.snippet,
            Category::Sensitive => &self.sensitive,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Position {
    /// Next to the text cursor when Windows reports one, else bottom-centre.
    Caret,
    /// Always bottom-centre of the active monitor.
    Center,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Ui {
    /// Overlay width in pixels.
    #[schemars(range(min = 320, max = 900))]
    pub width: u32,
    /// Overlay height in pixels.
    #[schemars(range(min = 360, max = 1200))]
    pub height: u32,
    pub position: Position,
    /// Most results shown per search.
    #[schemars(range(min = 10, max = 200))]
    pub search_limit: u32,
    /// Close the overlay when it loses focus.
    pub close_on_blur: bool,
    /// Hide a footer hint after its shortcut was used this many times (0 = never hide).
    pub hint_hide_after: u32,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            width: 360,
            height: 500,
            position: Position::Caret,
            search_limit: 50,
            close_on_blur: true,
            hint_hide_after: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Retention {
    /// Most items kept (pinned items are exempt).
    #[schemars(range(min = 10, max = 100000))]
    pub max_items: u32,
    /// Oldest age kept, like `12h`, `30d`, `8w`, or `never`.
    pub max_age: String,
    /// Images larger than this many megabytes are ignored.
    #[schemars(range(min = 1, max = 512))]
    pub max_image_mb: u32,
    /// Text larger than this many kilobytes is ignored.
    #[schemars(range(min = 1, max = 65536))]
    pub max_text_kb: u32,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            max_items: 500,
            max_age: "30d".into(),
            max_image_mb: 25,
            max_text_kb: 1024,
        }
    }
}

impl Retention {
    /// `None` means keep forever. Falls back to 30 days if the text is invalid
    /// (validation reports it; this keeps callers infallible).
    #[must_use]
    pub fn max_age_secs(&self) -> Option<u64> {
        parse_duration_secs(&self.max_age).unwrap_or(Some(30 * 86_400))
    }
}

/// Parse `45s`, `10m`, `12h`, `30d`, `8w` or `never`.
pub fn parse_duration_secs(s: &str) -> Result<Option<u64>, String> {
    let s = s.trim().to_ascii_lowercase();
    if s == "never" {
        return Ok(None);
    }
    let split = s
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(|| format!("`{s}` needs a unit (s, m, h, d, w)"))?;
    let (n, unit) = s.split_at(split);
    let n: u64 = n
        .parse()
        .map_err(|_| format!("`{s}` does not start with a number"))?;
    let mult = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        "w" => 604_800,
        _ => {
            return Err(format!(
                "unknown unit `{unit}` in `{s}` (use s, m, h, d, w)"
            ));
        }
    };
    n.checked_mul(mult)
        .map(Some)
        .ok_or_else(|| format!("`{s}` is too large"))
}

/// Fewest and most characters a generated password can have.
pub const PASSWORD_MIN_LEN: u32 = 8;
pub const PASSWORD_MAX_LEN: u32 = 128;

/// How the generator decides what a password looks like.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PasswordMode {
    /// A mix of the kinds of character that are turned on, in any order.
    #[default]
    Random,
    /// A fixed shape such as `LL######` (two capitals, then six numbers).
    Pattern,
}

/// The built-in password generator: a key button and a shortcut in the overlay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PasswordGenerator {
    /// Show the generator in the overlay. Off hides its button and ignores its shortcut.
    pub enabled: bool,
    /// `random` mixes the kinds below; `pattern` follows the shape written in `pattern`.
    pub mode: PasswordMode,
    /// How many characters a random password has.
    #[schemars(range(min = 8, max = 128))]
    pub length: u32,
    /// Random mode: use lower-case letters (a-z).
    pub lowercase: bool,
    /// Random mode: use upper-case letters (A-Z).
    pub uppercase: bool,
    /// Random mode: use digits (0-9).
    pub digits: bool,
    /// Random mode: use symbols such as ! @ # $ % & *.
    pub symbols: bool,
    /// Pattern mode: the shape of the password. `L` capital, `l` small letter, `#` number,
    /// `*` symbol; a backslash keeps the next character as typed; anything else is kept
    /// as it is. `LL######` makes `QK482915`.
    #[schemars(length(min = 1, max = 256))]
    pub pattern: String,
    /// Leave out characters that are easy to mix up when read: O 0 o, I l 1.
    pub avoid_lookalikes: bool,
}

impl Default for PasswordGenerator {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: PasswordMode::Random,
            length: 20,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            pattern: "LL######".to_owned(),
            avoid_lookalikes: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SecurityConfig {
    /// Seconds a sensitive item lives before it is wiped (and the clipboard cleared if untouched).
    #[schemars(range(min = 5, max = 3600))]
    pub sensitive_ttl_secs: u32,
    /// Shortest random-looking token treated as a secret.
    #[schemars(range(min = 8, max = 256))]
    pub min_token_len: usize,
    /// Shannon entropy (bits per character) above which a token looks random.
    #[schemars(range(min = 1.0, max = 6.0))]
    pub min_entropy_bits: f64,
    /// Copies made by these programs are never captured.
    pub blacklist_exes: Vec<String>,
    /// Copies made while the active window title contains one of these words are skipped.
    pub blacklist_titles: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        let v = |xs: &[&str]| xs.iter().map(|s| (*s).to_owned()).collect();
        Self {
            sensitive_ttl_secs: 60,
            min_token_len: 16,
            min_entropy_bits: 3.5,
            blacklist_exes: v(&[
                "bitwarden.exe",
                "1password.exe",
                "keepass.exe",
                "keepassxc.exe",
                "dashlane.exe",
                "enpass.exe",
                "nordpass.exe",
            ]),
            blacklist_titles: v(&[
                "Bitwarden",
                "1Password",
                "KeePass",
                "Dashlane",
                "NordPass",
                "Enpass",
                "Proton Pass",
                "RoboForm",
                "LastPass",
            ]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SnippetFolder {
    pub name: String,
    #[serde(default)]
    pub items: Vec<Snippet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Snippet {
    pub title: String,
    /// Text pasted. Placeholders: `{date}`, `{date:%Y-%m-%d}`, `{time}`, `{clipboard}`, `{cursor}`.
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Run a program with the item on stdin.
    Script,
}

/// A user-defined action. Declared here only, never built from clipboard text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub name: String,
    pub kind: ActionKind,
    /// Absolute path to the program or script. Started directly, never through a shell.
    pub path: String,
    /// Arguments passed as-is (no shell parsing).
    #[serde(default)]
    pub args: Vec<String>,
    /// Kill the process after this many milliseconds.
    #[serde(default = "default_timeout_ms")]
    #[schemars(range(min = 100, max = 60000))]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    5000
}

/// Rule: when a copy matches `when`, apply the `do` transforms before it is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Automation {
    pub when: When,
    #[serde(rename = "do")]
    pub then: Vec<Transform>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct When {
    /// Only for this kind of item.
    pub category: Option<Category>,
    /// Only for copies made by this program, like `chrome.exe`.
    pub source_exe: Option<String>,
}

impl Automation {
    #[must_use]
    pub fn matches(&self, category: Category, source_exe: Option<&str>) -> bool {
        self.when.category.is_none_or(|c| c == category)
            && self
                .when
                .source_exe
                .as_deref()
                .is_none_or(|want| source_exe.is_some_and(|got| got.eq_ignore_ascii_case(want)))
    }
}

// ---------------------------------------------------------------- validation

/// Windows drive path, UNC path or POSIX path. Host-independent on purpose, so
/// the same config validates identically on the Linux CI runner.
fn is_absolute_path(p: &str) -> bool {
    let b = p.as_bytes();
    p.starts_with('/')
        || p.starts_with("\\\\")
        || (b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && (b[2] == b'\\' || b[2] == b'/'))
}

fn has_dot_dot(p: &str) -> bool {
    p.split(['/', '\\']).any(|seg| seg == "..")
}

impl Config {
    /// Every problem in the file, not just the first.
    #[must_use]
    pub fn validate(&self) -> Vec<ConfigIssue> {
        let mut v = Vec::new();
        let mut bad = |path: &str, message: String| {
            v.push(ConfigIssue {
                path: path.to_owned(),
                message,
            })
        };

        if self.schema != SCHEMA_VERSION {
            bad(
                "schema",
                format!(
                    "this Zlip understands schema {SCHEMA_VERSION}, the file says {}",
                    self.schema
                ),
            );
        }

        // hotkeys
        let h = &self.hotkeys;
        let mut global: Vec<(&str, Chord)> = Vec::new();
        for (name, text) in [
            ("open_overlay", &h.open_overlay),
            ("queue_collect", &h.queue_collect),
            ("queue_paste", &h.queue_paste),
        ] {
            match text.parse::<Chord>() {
                Ok(c) if c.mods.is_empty() => bad(
                    &format!("hotkeys.{name}"),
                    format!("`{text}` needs a modifier: a bare key would be swallowed system-wide"),
                ),
                Ok(c) => {
                    if let Some((other, _)) = global.iter().find(|(_, o)| *o == c) {
                        bad(
                            &format!("hotkeys.{name}"),
                            format!("`{text}` is already used by hotkeys.{other}"),
                        );
                    }
                    global.push((name, c));
                }
                Err(e) => bad(&format!("hotkeys.{name}"), e.to_string()),
            }
        }
        let backup = h.open_overlay_backup.trim();
        if !backup.is_empty() {
            match backup.parse::<Chord>() {
                Ok(c) if c.mods.is_empty() => bad(
                    "hotkeys.open_overlay_backup",
                    format!("`{backup}` needs a modifier: a bare key would be swallowed system-wide"),
                ),
                Ok(c) => {
                    if let Some((other, _)) = global.iter().find(|(_, o)| *o == c) {
                        bad(
                            "hotkeys.open_overlay_backup",
                            format!("`{backup}` is already used by hotkeys.{other}"),
                        );
                    }
                }
                Err(e) => bad("hotkeys.open_overlay_backup", e.to_string()),
            }
        }
        if let Err(e) = h.quick_paste.parse::<QuickPaste>() {
            bad("hotkeys.quick_paste", e.to_string());
        }
        let mut seen: Vec<(&str, Chord)> = Vec::new();
        for (name, text) in h.local() {
            match text.parse::<Chord>() {
                Ok(c) => {
                    if let Some((other, _)) = seen.iter().find(|(_, o)| *o == c) {
                        bad(
                            &format!("hotkeys.{name}"),
                            format!("`{text}` is already used by hotkeys.{other}"),
                        );
                    }
                    seen.push((name, c));
                }
                Err(e) => bad(&format!("hotkeys.{name}"), e.to_string()),
            }
        }

        // theme
        if !(0.0..=1.0).contains(&self.theme.tint_opacity) {
            bad(
                "theme.tint_opacity",
                format!("{} is outside 0.0 to 1.0", self.theme.tint_opacity),
            );
        }
        if self.theme.corner_radius > 48 {
            bad(
                "theme.corner_radius",
                format!("{} is above 48", self.theme.corner_radius),
            );
        }
        for c in Category::ALL {
            let hex = self.theme.categories.get(c);
            if !(hex.starts_with('#') && parse_color(hex).is_some()) {
                bad(
                    &format!("theme.categories.{}", c.as_str()),
                    format!("`{hex}` is not a #RRGGBB colour"),
                );
            }
        }

        // ui
        range(&mut bad, "ui.width", self.ui.width, 320, 900);
        range(&mut bad, "ui.height", self.ui.height, 360, 1200);
        range(&mut bad, "ui.search_limit", self.ui.search_limit, 10, 200);

        // retention
        range(
            &mut bad,
            "retention.max_items",
            self.retention.max_items,
            10,
            100_000,
        );
        range(
            &mut bad,
            "retention.max_image_mb",
            self.retention.max_image_mb,
            1,
            512,
        );
        range(
            &mut bad,
            "retention.max_text_kb",
            self.retention.max_text_kb,
            1,
            65_536,
        );
        if let Err(e) = parse_duration_secs(&self.retention.max_age) {
            bad("retention.max_age", e);
        }

        // password generator
        let g = &self.password_generator;
        range(
            &mut bad,
            "password_generator.length",
            g.length,
            PASSWORD_MIN_LEN,
            PASSWORD_MAX_LEN,
        );
        // The kinds only matter in random mode, so a pattern setup need not keep one on.
        if g.mode == PasswordMode::Random && !(g.lowercase || g.uppercase || g.digits || g.symbols)
        {
            bad(
                "password_generator",
                "turn on at least one kind of character (lowercase, uppercase, digits or symbols)"
                    .to_owned(),
            );
        }
        if let Err(e) = parse_password_pattern(&g.pattern) {
            bad("password_generator.pattern", e.to_string());
        }

        // security
        let s = &self.security;
        range(
            &mut bad,
            "security.sensitive_ttl_secs",
            s.sensitive_ttl_secs,
            5,
            3600,
        );
        range(&mut bad, "security.min_token_len", s.min_token_len, 8, 256);
        if !(1.0..=6.0).contains(&s.min_entropy_bits) {
            bad(
                "security.min_entropy_bits",
                format!("{} is outside 1.0 to 6.0", s.min_entropy_bits),
            );
        }
        for (i, exe) in s.blacklist_exes.iter().enumerate() {
            if exe.trim().is_empty()
                || exe.contains(['/', '\\'])
                || !exe.to_ascii_lowercase().ends_with(".exe")
            {
                bad(
                    &format!("security.blacklist_exes[{i}]"),
                    format!("`{exe}` must be a bare program name ending in .exe"),
                );
            }
        }
        for (i, t) in s.blacklist_titles.iter().enumerate() {
            if t.trim().is_empty() {
                bad(
                    &format!("security.blacklist_titles[{i}]"),
                    "must not be empty".into(),
                );
            }
        }

        // snippets
        let mut folders = HashSet::new();
        for (i, f) in self.snippet_folders.iter().enumerate() {
            if f.name.trim().is_empty() {
                bad(
                    &format!("snippet_folders[{i}].name"),
                    "must not be empty".into(),
                );
            } else if !folders.insert(f.name.to_lowercase()) {
                bad(
                    &format!("snippet_folders[{i}].name"),
                    format!("`{}` is used twice", f.name),
                );
            }
            for (j, sn) in f.items.iter().enumerate() {
                if sn.title.trim().is_empty() {
                    bad(
                        &format!("snippet_folders[{i}].items[{j}].title"),
                        "must not be empty".into(),
                    );
                }
                if sn.body.is_empty() {
                    bad(
                        &format!("snippet_folders[{i}].items[{j}].body"),
                        "must not be empty".into(),
                    );
                }
            }
        }

        // actions
        let mut names = HashSet::new();
        for (i, a) in self.actions.iter().enumerate() {
            if a.name.trim().is_empty() {
                bad(&format!("actions[{i}].name"), "must not be empty".into());
            } else if !names.insert(a.name.to_lowercase()) {
                bad(
                    &format!("actions[{i}].name"),
                    format!("`{}` is used twice", a.name),
                );
            }
            if !is_absolute_path(&a.path) {
                bad(
                    &format!("actions[{i}].path"),
                    format!("`{}` must be an absolute path", a.path),
                );
            } else if has_dot_dot(&a.path) {
                bad(
                    &format!("actions[{i}].path"),
                    "must not contain `..`".into(),
                );
            }
            range(
                &mut bad,
                &format!("actions[{i}].timeout_ms"),
                a.timeout_ms,
                100,
                60_000,
            );
        }

        // automations
        for (i, a) in self.automations.iter().enumerate() {
            if a.then.is_empty() {
                bad(
                    &format!("automations[{i}].do"),
                    "needs at least one transform".into(),
                );
            }
            if a.when.category == Some(Category::Sensitive) {
                bad(
                    &format!("automations[{i}].when.category"),
                    "sensitive items are never processed by automations".into(),
                );
            }
        }
        v
    }
}

fn range<T: PartialOrd + std::fmt::Display + Copy>(
    bad: &mut impl FnMut(&str, String),
    path: &str,
    value: T,
    min: T,
    max: T,
) {
    if value < min || value > max {
        bad(path, format!("{value} is outside {min} to {max}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issues(toml_text: &str) -> Vec<String> {
        let cfg: Config = toml::from_str(toml_text).unwrap();
        cfg.validate().into_iter().map(|i| i.to_string()).collect()
    }

    #[test]
    fn defaults_are_valid() {
        assert_eq!(Config::default().validate(), vec![]);
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration_secs("30d"), Ok(Some(30 * 86_400)));
        assert_eq!(parse_duration_secs("12H"), Ok(Some(43_200)));
        assert_eq!(parse_duration_secs("never"), Ok(None));
        assert!(parse_duration_secs("30").is_err());
        assert!(parse_duration_secs("d").is_err());
        assert!(parse_duration_secs("5y").is_err());
        assert!(parse_duration_secs("99999999999999999999d").is_err());
    }

    #[test]
    fn global_hotkeys_need_modifiers_and_must_not_collide() {
        let v = issues(
            "[hotkeys]\nopen_overlay = \"V\"\nqueue_collect = \"Ctrl+Shift+Q\"\nqueue_paste = \"Ctrl+Shift+Q\"",
        );
        assert!(
            v.iter()
                .any(|m| m.starts_with("hotkeys.open_overlay") && m.contains("modifier")),
            "{v:?}"
        );
        assert!(
            v.iter()
                .any(|m| m.starts_with("hotkeys.queue_paste") && m.contains("already used")),
            "{v:?}"
        );
    }

    #[test]
    fn the_backup_shortcut_is_optional_but_must_be_valid_and_distinct() {
        // Empty means "off".
        let cfg: Config = toml::from_str("[hotkeys]\nopen_overlay_backup = \"\"").unwrap();
        assert_eq!(cfg.validate(), vec![]);
        assert_eq!(cfg.hotkeys.global_chords().unwrap().open_overlay_backup, None);
        // The default is on.
        let on = Config::default().hotkeys.global_chords().unwrap();
        assert!(on.open_overlay_backup.is_some());
        // A bare key, a typo and a clash are all reported.
        for (value, why) in [
            ("V", "modifier"),
            ("Ctrl+Nope", "unknown key"),
            ("Win+V", "already used"),
        ] {
            let v = issues(&format!("[hotkeys]\nopen_overlay_backup = \"{value}\""));
            assert!(
                v.iter()
                    .any(|m| m.starts_with("hotkeys.open_overlay_backup") && m.contains(why)),
                "{value}: {v:?}"
            );
        }
    }

    #[test]
    fn local_hotkeys_must_be_distinct_and_parse() {
        let v = issues(
            "[hotkeys]\npin = \"Ctrl+Q\"\nqueue_add = \"Ctrl+Q\"\ndelete = \"Nope\"\nquick_paste = \"Alt+5\"",
        );
        assert!(
            v.iter()
                .any(|m| m.starts_with("hotkeys.queue_add") && m.contains("already used")),
            "{v:?}"
        );
        assert!(
            v.iter()
                .any(|m| m.starts_with("hotkeys.delete") && m.contains("unknown key")),
            "{v:?}"
        );
        assert!(
            v.iter().any(|m| m.starts_with("hotkeys.quick_paste")),
            "{v:?}"
        );
    }

    #[test]
    fn all_problems_are_reported_at_once_with_paths() {
        let v = issues(
            "[theme]\ntint_opacity = 2.0\n[theme.categories]\ntext = \"red\"\n[retention]\nmax_age = \"soon\"\n[security]\nblacklist_exes = [\"C:/x/pm.exe\"]",
        );
        for p in [
            "theme.tint_opacity",
            "theme.categories.text",
            "retention.max_age",
            "security.blacklist_exes[0]",
        ] {
            assert!(v.iter().any(|m| m.starts_with(p)), "missing {p}: {v:?}");
        }
    }

    #[test]
    fn the_password_generator_is_on_by_default_and_old_files_still_load() {
        // A settings file written before the generator existed has no such section.
        let cfg: Config = toml::from_str("[ui]\nwidth = 400").unwrap();
        assert_eq!(cfg.password_generator, PasswordGenerator::default());
        assert!(cfg.password_generator.enabled);
        assert_eq!(cfg.hotkeys.generate_password, "Ctrl+G");
    }

    #[test]
    fn the_password_generator_settings_are_checked() {
        assert_eq!(
            issues("[password_generator]\nlength = 64\nsymbols = false\nenabled = false"),
            Vec::<String>::new(),
            "turning it off or dropping a kind is fine"
        );
        for length in [7, 129] {
            let v = issues(&format!("[password_generator]\nlength = {length}"));
            assert!(
                v.iter().any(|m| m.starts_with("password_generator.length")),
                "{length}: {v:?}"
            );
        }
        let v = issues(
            "[password_generator]\nlowercase = false\nuppercase = false\ndigits = false\nsymbols = false",
        );
        assert!(
            v.iter()
                .any(|m| m.starts_with("password_generator:") && m.contains("at least one")),
            "{v:?}"
        );
    }

    #[test]
    fn the_generator_starts_in_random_mode_with_a_ready_made_pattern() {
        let g = PasswordGenerator::default();
        assert_eq!(g.mode, PasswordMode::Random);
        assert_eq!(g.pattern, "LL######");
        let cfg: Config = toml::from_str("[password_generator]\nmode = \"pattern\"").unwrap();
        assert_eq!(cfg.password_generator.mode, PasswordMode::Pattern);
        assert!(toml::from_str::<Config>("[password_generator]\nmode = \"words\"").is_err());
    }

    #[test]
    fn a_pattern_is_checked_even_before_it_is_used() {
        assert_eq!(
            issues("[password_generator]\nmode = \"pattern\"\npattern = \"ZQ-####\""),
            Vec::<String>::new()
        );
        for (pattern, expect) in [
            ("", "empty"),
            ("AA123456", "at least one"),
            ("##\\", "ends with a backslash"),
        ] {
            // TOML basic strings: a backslash must be written twice.
            let toml = format!(
                "[password_generator]\npattern = \"{}\"",
                pattern.replace('\\', "\\\\")
            );
            let v = issues(&toml);
            assert!(
                v.iter()
                    .any(|m| m.starts_with("password_generator.pattern") && m.contains(expect)),
                "{pattern:?}: {v:?}"
            );
        }
        let long = format!("[password_generator]\npattern = \"{}\"", "#".repeat(129));
        assert!(
            issues(&long)
                .iter()
                .any(|m| m.starts_with("password_generator.pattern") && m.contains("too long"))
        );
    }

    #[test]
    fn pattern_mode_does_not_need_a_kind_turned_on() {
        let none = "lowercase = false\nuppercase = false\ndigits = false\nsymbols = false";
        assert_eq!(
            issues(&format!("[password_generator]\nmode = \"pattern\"\n{none}")),
            Vec::<String>::new()
        );
        assert!(!issues(&format!("[password_generator]\n{none}")).is_empty());
    }

    #[test]
    fn the_generator_shortcut_must_not_clash_with_another() {
        let v = issues("[hotkeys]\ngenerate_password = \"Ctrl+P\"");
        assert!(
            v.iter()
                .any(|m| m.starts_with("hotkeys.generate_password") && m.contains("already used")),
            "{v:?}"
        );
    }

    #[test]
    fn unknown_keys_are_parse_errors() {
        assert!(toml::from_str::<Config>("[theme]\ntint_opacty = 0.2").is_err());
        assert!(toml::from_str::<Config>("surprise = 1").is_err());
    }

    #[test]
    fn actions_are_validated_independently_of_the_host_os() {
        let ok = "[[actions]]\nname=\"a\"\nkind=\"script\"\npath=\"C:/tools/x.ps1\"";
        assert_eq!(issues(ok), Vec::<String>::new());
        for (path, why) in [("tools/x.ps1", "absolute"), ("C:/a/../b.ps1", "..")] {
            let v = issues(&format!(
                "[[actions]]\nname=\"a\"\nkind=\"script\"\npath=\"{path}\""
            ));
            assert!(v.iter().any(|m| m.contains(why)), "{path}: {v:?}");
        }
        let v = issues(
            "[[actions]]\nname=\"a\"\nkind=\"script\"\npath=\"/x\"\ntimeout_ms=5\n[[actions]]\nname=\"A\"\nkind=\"script\"\npath=\"/y\"",
        );
        assert!(v.iter().any(|m| m.contains("timeout_ms")), "{v:?}");
        assert!(v.iter().any(|m| m.contains("used twice")), "{v:?}");
    }

    #[test]
    fn automations_round_trip_the_do_keyword() {
        let cfg: Config = toml::from_str(
            "[[automations]]\nwhen = { category = \"link\" }\ndo = [\"strip_tracking_params\"]",
        )
        .unwrap();
        assert_eq!(
            cfg.automations[0].then,
            vec![Transform::StripTrackingParams]
        );
        assert!(cfg.automations[0].matches(Category::Link, None));
        assert!(!cfg.automations[0].matches(Category::Text, None));
        let v = issues("[[automations]]\nwhen = { category = \"sensitive\" }\ndo = [\"trim\"]");
        assert!(v.iter().any(|m| m.contains("never processed")), "{v:?}");
        assert!(
            toml::from_str::<Config>("[[automations]]\nwhen = {}\ndo = [\"explode\"]").is_err()
        );
    }

    #[test]
    fn automation_source_exe_is_case_insensitive() {
        let a = Automation {
            when: When {
                category: None,
                source_exe: Some("Chrome.exe".into()),
            },
            then: vec![Transform::Trim],
        };
        assert!(a.matches(Category::Text, Some("chrome.EXE")));
        assert!(!a.matches(Category::Text, None));
    }

    #[test]
    fn snippet_folders_are_checked() {
        let v = issues(
            "[[snippet_folders]]\nname=\"T\"\nitems=[{title=\"\", body=\"\"}]\n[[snippet_folders]]\nname=\"t\"",
        );
        assert!(
            v.iter().any(|m| m.contains("title")) && v.iter().any(|m| m.contains("body")),
            "{v:?}"
        );
        assert!(v.iter().any(|m| m.contains("used twice")), "{v:?}");
    }
}
