use std::collections::HashSet;
use zlip_config::SecurityConfig;

/// Why a copy was not captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    PrivateMode,
    /// `ExcludeClipboardContentFromMonitorProcessing` was present.
    ExcludedFormat,
    /// `CanIncludeInClipboardHistory` was 0.
    HistoryDisabled,
    /// `CanUploadToCloudClipboard` was 0.
    CloudDisabled,
    BlacklistedExe,
    BlacklistedTitle,
}

/// Facts the platform layer gathers about one clipboard change.
#[derive(Debug, Clone, Copy, Default)]
pub struct CaptureFacts<'a> {
    pub private_mode: bool,
    pub exclude_from_monitor: bool,
    /// Value of `CanIncludeInClipboardHistory`, `None` if the format is absent.
    pub can_include_in_history: Option<u32>,
    /// Value of `CanUploadToCloudClipboard`, `None` if the format is absent.
    pub can_upload_to_cloud: Option<u32>,
    /// Full path of the process that owns the clipboard, when it could be resolved.
    pub owner_exe: Option<&'a str>,
    pub foreground_title: Option<&'a str>,
}

/// The three layers of the password-manager bypass (blueprint section 3.2).
#[derive(Debug, Clone)]
pub struct Blacklist {
    exes: HashSet<String>,
    titles: Vec<String>,
}

impl Blacklist {
    #[must_use]
    pub fn from_config(cfg: &SecurityConfig) -> Self {
        Self {
            exes: cfg
                .blacklist_exes
                .iter()
                .map(|e| e.trim().to_lowercase())
                .collect(),
            titles: cfg
                .blacklist_titles
                .iter()
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty())
                .collect(),
        }
    }

    /// Matches on the lower-cased file name, for both `/` and `\` paths. A
    /// spoofed name can only make Zlip skip a copy, so this fails safe.
    #[must_use]
    pub fn is_blocked_exe(&self, path: &str) -> bool {
        let name = path
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(path)
            .to_lowercase();
        self.exes.contains(&name)
    }

    #[must_use]
    pub fn is_blocked_title(&self, title: &str) -> bool {
        let t = title.to_lowercase();
        self.titles.iter().any(|needle| t.contains(needle.as_str()))
    }

    /// `None` means "capture it". Order: cheapest and strongest signals first.
    ///
    /// An unknown owner is *not* blocked (fail open): refusing to capture from
    /// every elevated app would break the product. The title layer still applies.
    #[must_use]
    pub fn decide(&self, f: &CaptureFacts<'_>) -> Option<SkipReason> {
        if f.private_mode {
            Some(SkipReason::PrivateMode)
        } else if f.exclude_from_monitor {
            Some(SkipReason::ExcludedFormat)
        } else if f.can_include_in_history == Some(0) {
            Some(SkipReason::HistoryDisabled)
        } else if f.can_upload_to_cloud == Some(0) {
            Some(SkipReason::CloudDisabled)
        } else if f.owner_exe.is_some_and(|p| self.is_blocked_exe(p)) {
            Some(SkipReason::BlacklistedExe)
        } else if f.foreground_title.is_some_and(|t| self.is_blocked_title(t)) {
            Some(SkipReason::BlacklistedTitle)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bl() -> Blacklist {
        Blacklist::from_config(&SecurityConfig::default())
    }

    #[test]
    fn password_manager_executables_are_blocked_regardless_of_path_style() {
        let b = bl();
        assert!(b.is_blocked_exe(r"C:\Program Files\Bitwarden\Bitwarden.exe"));
        assert!(b.is_blocked_exe("C:/Program Files/KeePassXC/KEEPASSXC.EXE"));
        assert!(b.is_blocked_exe("1password.exe"));
        assert!(!b.is_blocked_exe(r"C:\Windows\notepad.exe"));
        assert!(!b.is_blocked_exe(r"C:\tools\bitwarden.exe.bak"));
    }

    #[test]
    fn browser_extension_titles_are_matched_case_insensitively() {
        let b = bl();
        assert!(b.is_blocked_title("Bitwarden - Vault - Google Chrome"));
        assert!(b.is_blocked_title("my 1PASSWORD popup"));
        assert!(!b.is_blocked_title("Quarterly report - Word"));
    }

    #[test]
    fn microsofts_own_format_contract_wins() {
        let b = bl();
        let f = |mut x: CaptureFacts<'static>| {
            x.owner_exe = Some(r"C:\Windows\notepad.exe");
            b.decide(&x)
        };
        assert_eq!(
            f(CaptureFacts {
                exclude_from_monitor: true,
                ..Default::default()
            }),
            Some(SkipReason::ExcludedFormat)
        );
        assert_eq!(
            f(CaptureFacts {
                can_include_in_history: Some(0),
                ..Default::default()
            }),
            Some(SkipReason::HistoryDisabled)
        );
        assert_eq!(
            f(CaptureFacts {
                can_upload_to_cloud: Some(0),
                ..Default::default()
            }),
            Some(SkipReason::CloudDisabled)
        );
        assert_eq!(
            f(CaptureFacts {
                can_include_in_history: Some(1),
                can_upload_to_cloud: Some(1),
                ..Default::default()
            }),
            None
        );
        assert_eq!(f(CaptureFacts::default()), None);
    }

    #[test]
    fn private_mode_beats_everything_and_unknown_owner_fails_open() {
        let b = bl();
        assert_eq!(
            b.decide(&CaptureFacts {
                private_mode: true,
                ..Default::default()
            }),
            Some(SkipReason::PrivateMode)
        );
        assert_eq!(
            b.decide(&CaptureFacts {
                owner_exe: None,
                foreground_title: Some("Notes"),
                ..Default::default()
            }),
            None
        );
        assert_eq!(
            b.decide(&CaptureFacts {
                owner_exe: None,
                foreground_title: Some("Bitwarden"),
                ..Default::default()
            }),
            Some(SkipReason::BlacklistedTitle),
            "the title layer still catches an unresolved owner"
        );
    }

    #[test]
    fn user_edits_to_the_list_take_effect() {
        let cfg = SecurityConfig {
            blacklist_exes: vec!["MyVault.EXE".into()],
            blacklist_titles: vec![],
            ..SecurityConfig::default()
        };
        let b = Blacklist::from_config(&cfg);
        assert!(b.is_blocked_exe(r"D:\apps\myvault.exe"));
        assert!(!b.is_blocked_exe("bitwarden.exe"));
    }
}
