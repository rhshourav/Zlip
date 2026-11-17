use crate::trust::{TrustStore, sha256_file};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use zlip_config::Action;

/// Output beyond this per stream is dropped (and `truncated` is set).
const MAX_OUTPUT: usize = 64 * 1024;

/// Text handed to a script. Constructing one is a statement by the caller that
/// the item is *not* sensitive (sensitive items never leave the in-RAM vault).
#[derive(Debug, Clone, Copy)]
pub struct ActionInput<'a>(&'a str);

impl<'a> ActionInput<'a> {
    #[must_use]
    pub fn from_non_sensitive(text: &'a str) -> Self {
        Self(text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionResult {
    /// `None` when the process was killed or ended by a signal.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Finished(ActionResult),
    /// Nothing was started. Show the user this hash; if they approve, call
    /// [`TrustStore::trust`] and run again.
    NeedsTrust {
        sha256: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("`{0}` does not exist or is not a file")]
    NotAFile(String),
    #[error(
        "batch files ({0}) are run through the command interpreter, which re-parses arguments; use a .ps1 script or an .exe instead"
    )]
    BatchFile(String),
    #[error("could not start the action: {0}")]
    Spawn(#[from] std::io::Error),
}

fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Build the command. PowerShell scripts run through the *system* `powershell.exe`
/// by absolute path (never found via `PATH`, which could be hijacked).
fn command_for(action: &Action) -> Result<Command, ActionError> {
    match extension(&action.path).as_str() {
        "bat" | "cmd" => Err(ActionError::BatchFile(action.path.clone())),
        "ps1" => {
            let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
            let mut c = Command::new(format!(
                "{root}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"
            ));
            c.args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &action.path,
            ]);
            c.args(&action.args);
            Ok(c)
        }
        _ => {
            let mut c = Command::new(&action.path);
            c.args(&action.args);
            Ok(c)
        }
    }
}

/// How long to wait for the output pipes to drain after the process is gone.
/// A grandchild that inherited the pipe (a daemon, or a `sleep` under `sh`) can
/// keep it open for ever, so we take what arrived and move on.
const DRAIN_GRACE: Duration = Duration::from_millis(250);

struct Capture {
    data: Arc<Mutex<Vec<u8>>>,
    truncated: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
}

/// Keep at most `MAX_OUTPUT` bytes, but keep *reading* so the child never blocks on a full pipe.
fn capture(mut r: impl Read + Send + 'static) -> Capture {
    let data = Arc::new(Mutex::new(Vec::new()));
    let truncated = Arc::new(AtomicBool::new(false));
    let (d, t) = (Arc::clone(&data), Arc::clone(&truncated));
    let thread = thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = r.read(&mut buf) {
            if n == 0 {
                break;
            }
            let mut kept = d.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let room = MAX_OUTPUT.saturating_sub(kept.len());
            kept.extend_from_slice(&buf[..n.min(room)]);
            if n > room {
                t.store(true, Ordering::Relaxed);
            }
        }
    });
    Capture {
        data,
        truncated,
        thread,
    }
}

impl Capture {
    fn finish(self, deadline: Instant) -> (Vec<u8>, bool) {
        while !self.thread.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let kept = self
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        (kept, self.truncated.load(Ordering::Relaxed))
    }
}

/// Windows: `child.kill()` ends only the direct process, so end the whole tree first.
#[cfg(windows)]
fn kill_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let _ = Command::new(format!("{root}\\System32\\taskkill.exe"))
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(0x0800_0000)
        .output();
}
#[cfg(not(windows))]
fn kill_tree(_pid: u32) {}

/// Run `action` with `input` on stdin, unless its script has not been approved.
pub fn run_script(
    action: &Action,
    input: ActionInput<'_>,
    trust: &TrustStore,
) -> Result<RunOutcome, ActionError> {
    if !Path::new(&action.path).is_file() {
        return Err(ActionError::NotAFile(action.path.clone()));
    }
    let mut cmd = command_for(action)?; // refuses batch files before anything else
    let sha = sha256_file(Path::new(&action.path))?;
    if !trust.is_trusted(&action.name, &action.path, &sha) {
        return Ok(RunOutcome::NeedsTrust { sha256: sha });
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash
    }
    let mut child = spawn_with_retry(&mut cmd)?;

    let stdin = child.stdin.take();
    let text = input.0.to_owned();
    let writer = thread::spawn(move || {
        if let Some(mut s) = stdin {
            let _ = s.write_all(text.as_bytes()); // a script that ignores stdin closes the pipe: fine
        }
    });
    let out = child.stdout.take().map(capture);
    let err = child.stderr.take().map(capture);

    let deadline = Instant::now() + Duration::from_millis(action.timeout_ms);
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (Some(status), false);
        }
        if Instant::now() >= deadline {
            kill_tree(child.id());
            let _ = child.kill();
            let _ = child.wait();
            break (None, true);
        }
        thread::sleep(Duration::from_millis(10));
    };
    let _ = writer.join();
    let grace = Instant::now() + DRAIN_GRACE;
    let take = |c: Option<Capture>| c.map(|c| c.finish(grace)).unwrap_or_default();
    let ((out, t1), (err, t2)) = (take(out), take(err));
    Ok(RunOutcome::Finished(ActionResult {
        exit_code: status.and_then(|s| s.code()),
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err).into_owned(),
        timed_out,
        truncated: t1 || t2,
    }))
}

/// Linux can report "text file busy" if another thread still holds a freshly
/// written script open for writing; a brief retry makes that a non-event.
fn spawn_with_retry(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    let mut tries = 0;
    loop {
        match cmd.spawn() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 20 => {
                tries += 1;
                thread::sleep(Duration::from_millis(25));
            }
            other => return other,
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
    use zlip_config::ActionKind;

    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        p
    }
    fn action(path: &Path, timeout_ms: u64) -> Action {
        Action {
            name: "t".into(),
            kind: ActionKind::Script,
            path: path.display().to_string(),
            args: vec![],
            timeout_ms,
        }
    }
    fn trusted(dir: &Path, a: &Action) -> TrustStore {
        let mut t = TrustStore::load(dir.join("trust.json"));
        t.trust(&a.name, &a.path, &sha256_file(Path::new(&a.path)).unwrap())
            .unwrap();
        t
    }
    fn finished(o: RunOutcome) -> ActionResult {
        match o {
            RunOutcome::Finished(r) => r,
            RunOutcome::NeedsTrust { .. } => panic!("expected the script to run"),
        }
    }

    #[test]
    fn clipboard_goes_to_stdin_and_output_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let a = action(&script(dir.path(), "echo.sh", "tr a-z A-Z"), 5000);
        let r = finished(
            run_script(
                &a,
                ActionInput::from_non_sensitive("hello"),
                &trusted(dir.path(), &a),
            )
            .unwrap(),
        );
        assert_eq!(
            (r.exit_code, r.stdout.as_str(), r.timed_out),
            (Some(0), "HELLO", false)
        );
    }

    #[test]
    fn an_unapproved_or_modified_script_does_not_run() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("ran");
        let path = script(dir.path(), "s.sh", &format!("touch {}", marker.display()));
        let a = action(&path, 5000);
        let mut store = TrustStore::load(dir.path().join("trust.json"));
        let RunOutcome::NeedsTrust { sha256 } =
            run_script(&a, ActionInput::from_non_sensitive("x"), &store).unwrap()
        else {
            panic!("must need approval first");
        };
        assert!(!marker.exists(), "nothing may run before approval");
        store.trust(&a.name, &a.path, &sha256).unwrap();
        finished(run_script(&a, ActionInput::from_non_sensitive("x"), &store).unwrap());
        assert!(marker.exists());

        fs::remove_file(&marker).unwrap();
        script(
            dir.path(),
            "s.sh",
            &format!("touch {} # edited", marker.display()),
        );
        assert!(matches!(
            run_script(&a, ActionInput::from_non_sensitive("x"), &store).unwrap(),
            RunOutcome::NeedsTrust { .. }
        ));
        assert!(!marker.exists(), "an edited script must be re-approved");
    }

    #[test]
    fn runaway_scripts_are_killed() {
        let dir = tempfile::tempdir().unwrap();
        let a = action(&script(dir.path(), "slow.sh", "sleep 30"), 200);
        let start = Instant::now();
        let r = finished(
            run_script(
                &a,
                ActionInput::from_non_sensitive(""),
                &trusted(dir.path(), &a),
            )
            .unwrap(),
        );
        assert!(r.timed_out && r.exit_code.is_none());
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn huge_output_is_capped_without_deadlocking() {
        let dir = tempfile::tempdir().unwrap();
        let a = action(
            &script(
                dir.path(),
                "big.sh",
                "head -c 1000000 /dev/zero | tr '\\0' x",
            ),
            10_000,
        );
        let r = finished(
            run_script(
                &a,
                ActionInput::from_non_sensitive(""),
                &trusted(dir.path(), &a),
            )
            .unwrap(),
        );
        assert!(r.truncated);
        assert_eq!(r.stdout.len(), MAX_OUTPUT);
    }

    #[test]
    fn large_stdin_to_a_script_that_ignores_it_does_not_hang() {
        let dir = tempfile::tempdir().unwrap();
        let a = action(&script(dir.path(), "ignore.sh", "echo done"), 10_000);
        let big = "z".repeat(2_000_000);
        let r = finished(
            run_script(
                &a,
                ActionInput::from_non_sensitive(&big),
                &trusted(dir.path(), &a),
            )
            .unwrap(),
        );
        assert_eq!(r.stdout.trim(), "done");
    }

    #[test]
    fn nonzero_exit_and_stderr_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let a = action(
            &script(dir.path(), "fail.sh", "echo oops >&2; exit 3"),
            5000,
        );
        let r = finished(
            run_script(
                &a,
                ActionInput::from_non_sensitive(""),
                &trusted(dir.path(), &a),
            )
            .unwrap(),
        );
        assert_eq!((r.exit_code, r.stderr.trim()), (Some(3), "oops"));
    }

    #[test]
    fn arguments_are_passed_verbatim_with_no_shell_expansion() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = action(&script(dir.path(), "args.sh", "printf '%s|' \"$@\""), 5000);
        a.args = vec!["$HOME".into(), "a b".into(), "; rm -rf /".into()];
        let r = finished(
            run_script(
                &a,
                ActionInput::from_non_sensitive(""),
                &trusted(dir.path(), &a),
            )
            .unwrap(),
        );
        assert_eq!(r.stdout, "$HOME|a b|; rm -rf /|");
    }

    #[test]
    fn batch_files_and_missing_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let bat = dir.path().join("x.bat");
        fs::write(&bat, "echo hi").unwrap();
        let store = TrustStore::load(dir.path().join("t.json"));
        assert!(matches!(
            run_script(
                &action(&bat, 1000),
                ActionInput::from_non_sensitive(""),
                &store
            ),
            Err(ActionError::BatchFile(_))
        ));
        assert!(matches!(
            run_script(
                &action(&dir.path().join("nope.sh"), 1000),
                ActionInput::from_non_sensitive(""),
                &store
            ),
            Err(ActionError::NotAFile(_))
        ));
    }
}
