//! "Start with Windows": a per-user entry under `HKCU\...\Run`. It needs no administrator
//! rights and shows up in Task Manager's Startup tab, where the user can also turn it off.
//!
//! It is read and written with the `reg.exe` that ships with Windows, so Zlip needs no extra
//! crate and no `unsafe` for it. Every function here blocks for a few milliseconds: call
//! them from a blocking task, never straight from the UI thread.

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Zlip";

/// `reg.exe` from the system folder, not whichever one comes first on `PATH`.
fn reg_exe() -> PathBuf {
    std::env::var_os("SystemRoot").map_or_else(
        || PathBuf::from("reg.exe"),
        |root| PathBuf::from(root).join("System32").join("reg.exe"),
    )
}

fn reg(args: &[&str]) -> Result<Output, String> {
    let mut cmd = Command::new(reg_exe());
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Without this a console window flashes up, because Zlip itself has none.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.output()
        .map_err(|e| format!("Could not run reg.exe: {e}"))
}

/// What Windows runs at sign-in: the full path of this program, in quotes (it may hold spaces).
fn run_command(exe: &Path) -> String {
    let path = exe.display().to_string();
    // `current_exe` can hand back a `\\?\C:\...` path; the Run key wants the plain one.
    let path = path.strip_prefix(r"\\?\").unwrap_or(&path);
    format!("\"{path}\"")
}

fn this_command() -> Result<String, String> {
    std::env::current_exe()
        .map(|exe| run_command(&exe))
        .map_err(|e| format!("Could not find where Zlip is installed: {e}"))
}

/// The data of the `Zlip` value in `reg query` output, if the output has one.
///
/// A value line reads `    Zlip    REG_SZ    "C:\...\zlip.exe"`: name, type and data, split by
/// runs of spaces. Everything after the type is the data, spaces inside it included.
fn parse_value(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix(VALUE_NAME)?;
        // The name must end here: "ZlipHelper" is a different value.
        let rest = rest.strip_prefix(char::is_whitespace)?.trim_start();
        let data = rest
            .strip_prefix("REG_SZ")
            .or_else(|| rest.strip_prefix("REG_EXPAND_SZ"))?;
        Some(data.trim().to_owned())
    })
}

/// Does the stored command start this copy of Zlip (and not an older or moved one)?
///
/// `reg.exe` prints in the console's code page, not UTF-8. When the text is not valid UTF-8
/// (a user name with accents, say) the paths cannot be compared reliably, so an entry that
/// exists is taken to be ours instead of reporting "off" for something that is on.
fn points_here(output: &[u8], expected: &str) -> bool {
    match std::str::from_utf8(output) {
        Ok(text) => parse_value(text).is_some_and(|stored| stored.eq_ignore_ascii_case(expected)),
        Err(_) => true,
    }
}

/// Is Zlip set to start with Windows?
pub fn is_enabled() -> Result<bool, String> {
    let out = reg(&["query", RUN_KEY, "/v", VALUE_NAME])?;
    // `reg query` exits with 1 when the value does not exist.
    Ok(out.status.success() && points_here(&out.stdout, &this_command()?))
}

/// Turn start-up on (pointing at this copy of Zlip) or off.
pub fn set_enabled(on: bool) -> Result<(), String> {
    let out = if on {
        let command = this_command()?;
        reg(&[
            "add", RUN_KEY, "/v", VALUE_NAME, "/t", "REG_SZ", "/d", &command, "/f",
        ])?
    } else {
        // Already off counts as done: `reg delete` fails on a value that is not there.
        if !reg(&["query", RUN_KEY, "/v", VALUE_NAME])?.status.success() {
            return Ok(());
        }
        reg(&["delete", RUN_KEY, "/v", VALUE_NAME, "/f"])?
    };
    if out.status.success() {
        Ok(())
    } else {
        let why = String::from_utf8_lossy(&out.stderr);
        Err(format!("Windows did not accept the change: {}", why.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = r"C:\Program Files\Zlip\zlip.exe";

    fn query_output(data_line: &str) -> String {
        format!(
            "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n{data_line}\r\n\r\n"
        )
    }

    #[test]
    fn quotes_the_path_so_spaces_survive() {
        assert_eq!(run_command(Path::new(EXE)), format!("\"{EXE}\""));
    }

    #[test]
    fn drops_the_extended_length_prefix() {
        let with_prefix = format!(r"\\?\{EXE}");
        assert_eq!(run_command(Path::new(&with_prefix)), format!("\"{EXE}\""));
    }

    #[test]
    fn reads_the_value_out_of_reg_output() {
        let out = query_output(&format!("    Zlip    REG_SZ    \"{EXE}\""));
        assert_eq!(parse_value(&out), Some(format!("\"{EXE}\"")));
    }

    #[test]
    fn ignores_other_values_and_empty_output() {
        let other = query_output("    ZlipHelper    REG_SZ    \"C:\\x.exe\"");
        assert_eq!(parse_value(&other), None);
        let unrelated = query_output("    OneDrive    REG_SZ    \"C:\\y.exe\"");
        assert_eq!(parse_value(&unrelated), None);
        assert_eq!(parse_value(""), None);
    }

    #[test]
    fn our_own_entry_is_recognised_whatever_the_case() {
        let ours = format!("\"{EXE}\"");
        let out = query_output(&format!("    Zlip    REG_SZ    {}", ours.to_uppercase()));
        assert!(points_here(out.as_bytes(), &ours));
    }

    #[test]
    fn an_entry_for_another_copy_is_not_ours() {
        let out = query_output("    Zlip    REG_SZ    \"D:\\old\\zlip.exe\"");
        assert!(!points_here(out.as_bytes(), &format!("\"{EXE}\"")));
    }

    #[test]
    fn text_in_another_code_page_counts_as_ours() {
        // 0xE9 is "é" in the common OEM/ANSI code pages and is not valid UTF-8 on its own.
        let mut out = b"    Zlip    REG_SZ    \"C:\\Users\\Jos".to_vec();
        out.extend_from_slice(&[0xE9, b'\\', b'z', b'.', b'e', b'x', b'e', b'"']);
        assert!(points_here(&out, "\"C:\\Users\\Jos\u{e9}\\z.exe\""));
    }
}
