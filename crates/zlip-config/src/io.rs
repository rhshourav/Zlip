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

/// Load `path`, creating it from the template on first run. A broken existing
/// file is an error (never overwritten), so the caller can fall back to
/// defaults *and* tell the user.
pub fn load_or_create(path: &Path) -> Result<Config, ConfigError> {
    match fs::read_to_string(path) {
        Ok(text) => Config::from_toml(&text),
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
