use crate::model::{Config, ConfigIssue};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Why a config could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read or write the config: {0}")]
    Io(#[from] io::Error),
    #[error("{message}")]
    Parse {
        message: String,
        line: Option<usize>,
        column: Option<usize>,
    },
    #[error("{}", format_issues(.0))]
    Invalid(Vec<ConfigIssue>),
}

fn format_issues(issues: &[ConfigIssue]) -> String {
    let mut s = format!("{} problem(s) in the config:", issues.len());
    for i in issues {
        s.push_str("\n  - ");
        s.push_str(&i.to_string());
    }
    s
}

impl Config {
    /// The commented template written on first run.
    #[must_use]
    pub fn default_toml() -> &'static str {
        include_str!("../default-config.toml")
    }

    /// Parse and validate. Nothing is applied unless the whole file is good.
    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        let cfg: Self = toml::from_str(text).map_err(|e| {
            let (line, column) = e.span().map_or((None, None), |s| position(text, s.start));
            ConfigError::Parse {
                message: e.message().to_owned(),
                line,
                column,
            }
        })?;
        let issues = cfg.validate();
        if issues.is_empty() {
            Ok(cfg)
        } else {
            Err(ConfigError::Invalid(issues))
        }
    }

    /// The settings screen saves with this. It writes every setting (including
    /// snippets, actions and automations) but not the comments of a hand-edited file.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        let body = toml::to_string_pretty(self).map_err(|e| ConfigError::Io(io::Error::other(e)))?;
        // An f32 widened to f64 prints as 0.14000000357...; write what the user sees instead.
        let tint = {
            let mut t = self.theme.tint_opacity.to_string();
            if !t.contains('.') {
                t.push_str(".0");
            }
            format!("tint_opacity = {t}")
        };
        let body = body
            .lines()
            .map(|l| {
                if l.trim_start().starts_with("tint_opacity") {
                    tint.as_str()
                } else {
                    l
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(format!(
            "#:schema ./zlip.schema.json\n# Saved by Zlip's settings screen. You can still edit this file by hand;\n# Zlip reloads it when you save.\n{body}\n"
        ))
    }

    /// JSON Schema for editor autocomplete (`#:schema ./zlip.schema.json`).
    #[must_use]
    pub fn json_schema() -> String {
        let schema = schemars::schema_for!(Config);
        let mut s = serde_json::to_string_pretty(&schema).expect("a schema always serialises");
        s.push('\n');
        s
    }
}

fn position(text: &str, byte: usize) -> (Option<usize>, Option<usize>) {
    let before = &text[..byte.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (Some(line), Some(column))
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

/// `%APPDATA%\zlip` on Windows (roaming: settings follow the user).
/// Elsewhere (developer machines, CI) `$XDG_CONFIG_HOME/zlip` or `~/.config/zlip`.
#[must_use]
pub fn config_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("APPDATA") {
        return PathBuf::from(p).join("zlip");
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map_or_else(|| home().join(".config"), PathBuf::from)
        .join("zlip")
}

/// `%LOCALAPPDATA%\zlip` on Windows (history, images, logs: never roamed).
#[must_use]
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(p).join("zlip");
    }
    std::env::var_os("XDG_DATA_HOME")
        .map_or_else(|| home().join(".local").join("share"), PathBuf::from)
        .join("zlip")
}

#[must_use]
pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Write via a temp file in the same directory, then rename over the target, so
/// a crash or a concurrent reader never sees a half-written config.
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        std::process::id()
    ));
    {
        use io::Write;
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// Write the settings screen's result to `path`, atomically.
pub fn save_config(path: &Path, config: &Config) -> Result<(), ConfigError> {
    write_atomic(path, config.to_toml()?.as_bytes())?;
    Ok(())
}

/// `key = value  # comment`: the value part, if `line` sets `key`.
fn value_of<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(key)?.trim_start().strip_prefix('=')?;
    Some(rest.split('#').next()?.trim())
}

/// Replace the value of a `key = value  # comment` line, keeping spacing and comment.
fn with_value(line: &str, value: &str) -> String {
    let eq = line.find('=').unwrap_or(0);
    let (head, tail) = line.split_at(eq + 1);
    let (old, comment) = match tail.find('#') {
        Some(at) => tail.split_at(at),
        None => (tail, ""),
    };
    let lead = &old[..old.len() - old.trim_start().len()];
    let gap = if comment.is_empty() { "" } else { "  " };
    format!("{head}{lead}{value}{gap}{comment}")
}

/// Zlip 0.1 shipped a 440 x 640 window, which is large. A file that still holds
/// exactly those two untouched values gets the compact default instead. Anything
/// else (a size the user chose) is left alone. Returns the new text, or `None`.
fn upgrade_legacy_text(text: &str) -> Option<String> {
    let mut in_ui = false;
    let (mut width_at, mut height_at) = (None, None);
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with('[') {
            in_ui = t.starts_with("[ui]");
        } else if in_ui {
            if let Some(v) = value_of(t, "width") {
                if v != "440" {
                    return None;
                }
                width_at = Some(i);
            } else if let Some(v) = value_of(t, "height") {
                if v != "640" {
                    return None;
                }
                height_at = Some(i);
            }
        }
    }
    let (w, h) = (width_at?, height_at?);
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out: Vec<String> = text.lines().map(str::to_owned).collect();
    out[w] = with_value(&out[w], "360");
    out[h] = with_value(&out[h], "500");
    let mut joined = out.join(eol);
    if text.ends_with('\n') {
        joined.push_str(eol);
    }
    Some(joined)
}

/// Load `path`, creating it from the template on first run. A broken existing
/// file is an error (never overwritten), so the caller can fall back to
/// defaults *and* tell the user.
pub fn load_or_create(path: &Path) -> Result<Config, ConfigError> {
    match fs::read_to_string(path) {
        Ok(text) => {
            if let Some(upgraded) = upgrade_legacy_text(&text)
                && write_atomic(path, upgraded.as_bytes()).is_ok()
            {
                return Config::from_toml(&upgraded);
            }
            Config::from_toml(&text)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            write_atomic(path, Config::default_toml().as_bytes())?;
            Ok(Config::default())
        }
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_template_equals_the_built_in_defaults() {
        let parsed = Config::from_toml(Config::default_toml()).expect("template must be valid");
        assert_eq!(
            parsed,
            Config::default(),
            "default-config.toml drifted from Config::default()"
        );
    }

    #[test]
    fn the_old_default_window_size_is_upgraded_once_and_nothing_else_changes() {
        let old = Config::default_toml()
            .replace("width             = 360", "width             = 440")
            .replace("height            = 500", "height            = 640");
        let up = upgrade_legacy_text(&old).expect("legacy size is upgraded");
        assert_eq!(up, Config::default_toml());
        // Only the two values change: comments and spacing survive.
        assert!(up.contains("position          = \"caret\"  # caret"));
        // A size the user chose is never touched.
        assert_eq!(upgrade_legacy_text("[ui]\nwidth = 440\nheight = 700\n"), None);
        assert_eq!(upgrade_legacy_text("[ui]\nwidth = 500\nheight = 640\n"), None);
        // Neither is a file that does not mention the size.
        assert_eq!(upgrade_legacy_text("[ui]\nclose_on_blur = true\n"), None);
        // Other sections' `width` keys do not count.
        assert_eq!(upgrade_legacy_text("[other]\nwidth = 440\nheight = 640\n"), None);
    }

    #[test]
    fn saved_settings_read_back_identically() {
        let mut cfg = Config::default();
        cfg.theme.tint_opacity = 0.3;
        cfg.ui.width = 420;
        cfg.hotkeys.open_overlay_backup = String::new();
        let text = cfg.to_toml().unwrap();
        assert!(text.contains("tint_opacity = 0.3\n"), "{text}");
        assert_eq!(Config::from_toml(&text).unwrap(), cfg);
    }

    #[test]
    fn empty_file_means_all_defaults() {
        assert_eq!(Config::from_toml("").unwrap(), Config::default());
    }

    #[test]
    fn parse_errors_carry_line_and_column() {
        let err = Config::from_toml("schema = 1\n[theme]\nmode = \"loud\"\n").unwrap_err();
        match err {
            ConfigError::Parse { line, column, .. } => {
                assert_eq!(line, Some(3));
                assert!(column.is_some());
            }
            other => panic!("expected a parse error, got {other:?}"),
        }
    }

    #[test]
    fn validation_errors_are_listed_in_the_message() {
        let err = Config::from_toml("[ui]\nwidth = 5\nheight = 5")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("2 problem(s)") && err.contains("ui.width") && err.contains("ui.height"),
            "{err}"
        );
    }

    #[test]
    fn first_run_creates_the_template_and_second_run_reads_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        assert_eq!(load_or_create(&path).unwrap(), Config::default());
        assert_eq!(fs::read_to_string(&path).unwrap(), Config::default_toml());
        assert_eq!(load_or_create(&path).unwrap(), Config::default());
    }

    #[test]
    fn a_broken_file_is_reported_and_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "this is = = not toml").unwrap();
        assert!(load_or_create(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "this is = = not toml");
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.toml");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "two");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn the_committed_json_schema_is_current() {
        assert_eq!(
            Config::json_schema(),
            include_str!("../../../zlip.schema.json"),
            "zlip.schema.json is stale. Regenerate it with:\n  cargo run -p zlip-config --example export_schema > zlip.schema.json"
        );
    }

    #[test]
    fn schema_is_valid_json_and_forbids_unknown_keys() {
        let v: serde_json::Value = serde_json::from_str(&Config::json_schema()).unwrap();
        assert_eq!(v["additionalProperties"], serde_json::Value::Bool(false));
        assert!(v["properties"]["hotkeys"].is_object());
    }
}
