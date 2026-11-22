//! Opening links. Zlip opens one family of addresses: its own repository's pages.

/// The project page. The settings screen links to it and looks there for newer releases,
/// and `open_url` opens this address and the pages under it, nothing else.
pub const REPO_URL: &str = "https://github.com/rhshourav/zlip";

/// `true` for the repository's own pages and nothing else.
fn is_repo_url(url: &str) -> bool {
    let inside = url
        .strip_prefix(REPO_URL)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'));
    inside && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Open a page of the Zlip repository in the default browser. Any other address is refused,
/// so nothing (not even a reply from GitHub) can make Zlip open an arbitrary link.
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    if !is_repo_url(&url) {
        return Err("Zlip only opens pages of its own repository.".to_owned());
    }
    zlip_platform::open_in_shell(&url);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_the_repository_and_its_pages() {
        for ok in [
            REPO_URL,
            "https://github.com/rhshourav/zlip/",
            "https://github.com/rhshourav/zlip/releases/latest",
            "https://github.com/rhshourav/zlip/releases/tag/v1.2.3",
        ] {
            assert!(is_repo_url(ok), "{ok}");
        }
    }

    #[test]
    fn refuses_everything_else() {
        for bad in [
            "",
            "https://github.com/rhshourav/zlip-evil",
            "https://github.com/rhshourav/zlipx/releases",
            "https://github.com/rhshourav/zlip@evil.example",
            "https://github.com/rhshourav",
            "http://github.com/rhshourav/zlip",
            "https://github.com.evil.example/rhshourav/zlip",
            "https://evil.example/?https://github.com/rhshourav/zlip",
            "file:///C:/Windows/System32/calc.exe",
            "calc.exe",
            "https://github.com/rhshourav/zlip/ calc.exe",
            "https://github.com/rhshourav/zlip/\ncalc.exe",
        ] {
            assert!(!is_repo_url(bad), "{bad:?}");
        }
    }
}
