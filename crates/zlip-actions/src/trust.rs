use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Pin {
    path: String,
    sha256: String,
}

/// Remembers which exact script bytes the user approved, per action name.
/// Editing the script, or re-pointing the action, invalidates the approval.
#[derive(Debug, Clone)]
pub struct TrustStore {
    file: PathBuf,
    pins: BTreeMap<String, Pin>,
}

/// Lower-case hex SHA-256 of a file, streamed.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    io::copy(&mut fs::File::open(path)?, &mut hasher)?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

impl TrustStore {
    /// Load from `file`. A missing or unreadable file means nothing is trusted yet.
    #[must_use]
    pub fn load(file: PathBuf) -> Self {
        let pins = fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self { file, pins }
    }

    #[must_use]
    pub fn is_trusted(&self, action: &str, path: &str, sha256: &str) -> bool {
        self.pins
            .get(action)
            .is_some_and(|p| p.path == path && p.sha256.eq_ignore_ascii_case(sha256))
    }

    /// Record approval and persist it atomically.
    pub fn trust(&mut self, action: &str, path: &str, sha256: &str) -> io::Result<()> {
        self.pins.insert(
            action.to_owned(),
            Pin {
                path: path.to_owned(),
                sha256: sha256.to_ascii_lowercase(),
            },
        );
        let json = serde_json::to_vec_pretty(&self.pins).map_err(io::Error::other)?;
        zlip_config::write_atomic(&self.file, &json)
    }

    pub fn revoke(&mut self, action: &str) -> io::Result<()> {
        if self.pins.remove(action).is_some() {
            let json = serde_json::to_vec_pretty(&self.pins).map_err(io::Error::other)?;
            zlip_config::write_atomic(&self.file, &json)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_known_vector() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("abc");
        fs::write(&f, "abc").unwrap();
        assert_eq!(
            sha256_file(&f).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn trust_is_per_action_path_and_hash_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("trust.json");
        let mut t = TrustStore::load(file.clone());
        assert!(!t.is_trusted("lint", "/x", "aa"));
        t.trust("lint", "/x", "AA").unwrap();
        assert!(t.is_trusted("lint", "/x", "aa"));
        assert!(!t.is_trusted("lint", "/x", "bb"), "different bytes");
        assert!(!t.is_trusted("lint", "/y", "aa"), "different path");
        assert!(!t.is_trusted("other", "/x", "aa"), "different action");
        assert!(
            TrustStore::load(file.clone()).is_trusted("lint", "/x", "aa"),
            "survives a restart"
        );
        let mut t = TrustStore::load(file);
        t.revoke("lint").unwrap();
        assert!(!t.is_trusted("lint", "/x", "aa"));
    }

    #[test]
    fn a_corrupt_trust_file_trusts_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("trust.json");
        fs::write(&file, "{ nope").unwrap();
        assert!(!TrustStore::load(file).is_trusted("a", "b", "c"));
    }
}
