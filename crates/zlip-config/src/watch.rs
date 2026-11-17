use crate::{io::ConfigError, model::Config};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, RwLock, mpsc},
    thread,
    time::Duration,
};

/// Cheap, shareable view of the *current good* config.
#[derive(Debug, Clone)]
pub struct ConfigHandle(Arc<RwLock<Arc<Config>>>);

impl ConfigHandle {
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self(Arc::new(RwLock::new(Arc::new(config))))
    }

    /// An immutable snapshot; hold it as long as you like, it never changes under you.
    #[must_use]
    pub fn snapshot(&self) -> Arc<Config> {
        self.0
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn set(&self, config: Config) {
        *self
            .0
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::new(config);
    }
}

#[derive(Debug, Clone)]
pub enum ReloadEvent {
    /// A new, valid config is now live.
    Reloaded(Arc<Config>),
    /// The saved file is broken; the previous config stays live. Holds the reason.
    Rejected(String),
}

/// Keeps the file watcher alive; dropping it stops hot reload.
#[must_use = "dropping the watcher stops hot reload"]
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
}

impl std::fmt::Debug for ConfigWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConfigWatcher")
    }
}

const DEBOUNCE: Duration = Duration::from_millis(150);

/// Watch `path` and apply valid changes to `handle`.
///
/// The *directory* is watched, not the file, because editors save by writing a
/// temp file and renaming it, which would orphan a file-level watch. Duplicate
/// and unrelated events are harmless: a reload only fires when the text changed.
pub fn watch(
    path: PathBuf,
    handle: ConfigHandle,
    on_event: impl Fn(ReloadEvent) + Send + 'static,
) -> Result<ConfigWatcher, ConfigError> {
    let dir = path.parent().map(PathBuf::from).unwrap_or_default();
    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    })
    .map_err(to_io)?;
    watcher
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(to_io)?;

    thread::Builder::new()
        .name("zlip-config-watch".into())
        .spawn(move || {
            let mut last_text = fs::read_to_string(&path).ok();
            while rx.recv().is_ok() {
                while rx.recv_timeout(DEBOUNCE).is_ok() {} // wait for the burst to settle
                let Ok(text) = fs::read_to_string(&path) else {
                    continue;
                };
                if last_text.as_deref() == Some(text.as_str()) {
                    continue;
                }
                last_text = Some(text.clone());
                match Config::from_toml(&text) {
                    Ok(cfg) => {
                        handle.set(cfg);
                        on_event(ReloadEvent::Reloaded(handle.snapshot()));
                    }
                    Err(e) => {
                        tracing::warn!("config rejected, keeping the previous one: {e}");
                        on_event(ReloadEvent::Rejected(e.to_string()));
                    }
                }
            }
        })?;
    Ok(ConfigWatcher { _watcher: watcher })
}

fn to_io(e: notify::Error) -> ConfigError {
    ConfigError::Io(std::io::Error::other(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn wait_for(rx: &mpsc::Receiver<ReloadEvent>) -> ReloadEvent {
        rx.recv_timeout(Duration::from_secs(10))
            .expect("no reload event within 10 s")
    }

    #[test]
    fn hot_reload_applies_good_edits_and_keeps_the_last_good_on_bad_ones() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, Config::default_toml()).unwrap();
        let handle = ConfigHandle::new(Config::default());
        let (tx, rx) = mpsc::channel();
        let _w = watch(path.clone(), handle.clone(), move |e| {
            let _ = tx.send(e);
        })
        .unwrap();
        thread::sleep(Duration::from_millis(300)); // let the watcher settle

        fs::write(&path, "[ui]\nwidth = 500").unwrap();
        assert!(matches!(wait_for(&rx), ReloadEvent::Reloaded(_)));
        assert_eq!(handle.snapshot().ui.width, 500);

        fs::write(&path, "[ui]\nwidth = oops").unwrap();
        assert!(matches!(wait_for(&rx), ReloadEvent::Rejected(_)));
        assert_eq!(
            handle.snapshot().ui.width,
            500,
            "a broken save must not change live settings"
        );

        fs::write(&path, "[ui]\nwidth = 520").unwrap();
        assert!(matches!(wait_for(&rx), ReloadEvent::Reloaded(_)));
        assert_eq!(handle.snapshot().ui.width, 520);
    }

    #[test]
    fn rewriting_identical_text_does_not_fire() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[ui]\nwidth = 450").unwrap();
        let handle = ConfigHandle::new(Config::default());
        let (tx, rx) = mpsc::channel();
        let _w = watch(path.clone(), handle, move |e| {
            let _ = tx.send(e);
        })
        .unwrap();
        thread::sleep(Duration::from_millis(300));
        fs::write(&path, "[ui]\nwidth = 450").unwrap();
        let start = Instant::now();
        assert!(
            rx.recv_timeout(Duration::from_millis(800)).is_err(),
            "unexpected reload after {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn handle_snapshots_are_stable() {
        let h = ConfigHandle::new(Config::default());
        let before = h.snapshot();
        let mut next = Config::default();
        next.ui.width = 600;
        h.set(next);
        assert_eq!(before.ui.width, 440);
        assert_eq!(h.snapshot().ui.width, 600);
    }
}
