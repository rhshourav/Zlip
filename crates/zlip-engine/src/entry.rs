//! One row of the in-RAM index. Only the first few KiB of each text are kept in
//! memory; the full content is decrypted from the store when needed.

use crate::view::ItemView;
use url::Url;
use zlip_core::{Category, ItemId, Payload, Rgba, classify, parse_color};
use zlip_highlight::Language;
use zlip_store::{IndexedItem, StoredKind};

pub(crate) const HEAD_BYTES: usize = 4096;
const PREVIEW_LINES: usize = 2;
const PREVIEW_CHARS: usize = 220;

#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub id: ItemId,
    pub category: Category,
    pub kind: StoredKind,
    /// Start of the text (or newline-joined paths). Empty for images.
    pub head: String,
    /// `head` is the whole content.
    pub complete: bool,
    pub created_ms: i64,
    pub pinned: bool,
    pub folder: Option<String>,
    pub size_bytes: u64,
    pub dims: Option<(u32, u32)>,
    pub source_exe: Option<String>,
    pub language: Option<Language>,
    pub color: Option<Rgba>,
    pub host: Option<String>,
    pub file_count: usize,
    pub preview: String,
    pub preview_html: Option<String>,
    /// Defined in the config file rather than stored in the history.
    pub read_only: bool,
}

pub(crate) fn head_of(s: &str) -> (String, bool) {
    if s.len() <= HEAD_BYTES {
        return (s.to_owned(), true);
    }
    let mut end = HEAD_BYTES;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_owned(), false)
}

fn first_lines(text: &str) -> String {
    let joined = text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .take(PREVIEW_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    match joined.char_indices().nth(PREVIEW_CHARS) {
        Some((i, _)) => format!("{}…", &joined[..i]),
        None => joined,
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/'])
        .find(|s| !s.is_empty())
        .unwrap_or(path)
}

fn link_host(text: &str) -> Option<String> {
    let t = text.trim();
    let t = if t.len() > 4 && t[..4].eq_ignore_ascii_case("www.") {
        format!("https://{t}")
    } else {
        t.to_owned()
    };
    let host = Url::parse(&t).ok()?.host_str()?.to_owned();
    Some(
        host.strip_prefix("www.")
            .map_or(host.clone(), str::to_owned),
    )
}

/// "C:\\Program Files\\Google\\Chrome\\chrome.exe" becomes "Chrome".
pub(crate) fn app_name(exe: &str) -> String {
    let name = file_name(exe);
    let stem = name
        .strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".EXE"))
        .unwrap_or(name);
    let mut chars = stem.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

impl Entry {
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        id: ItemId,
        category: Category,
        kind: StoredKind,
        text: Option<&str>,
        created_ms: i64,
        pinned: bool,
        folder: Option<String>,
        size_bytes: u64,
        dims: Option<(u32, u32)>,
        source_exe: Option<String>,
        language: Option<Language>,
        read_only: bool,
    ) -> Self {
        let full = text.unwrap_or_default();
        let (head, complete) = head_of(full);
        let file_count = if kind == StoredKind::Files {
            full.lines().count()
        } else {
            0
        };
        let preview = match kind {
            StoredKind::Files => {
                let mut names = full.lines().map(file_name);
                let first = names.next().unwrap_or_default().to_owned();
                match file_count {
                    0 | 1 => first,
                    n => format!("{first}  +{} more", n - 1),
                }
            }
            StoredKind::Image => {
                dims.map_or_else(|| "Image".to_owned(), |(w, h)| format!("{w} × {h}"))
            }
            StoredKind::Text => first_lines(&head),
        };
        let preview_html = (category == Category::Code && kind == StoredKind::Text)
            .then(|| zlip_highlight::highlight_html(&preview, language));
        let color = (category == Category::Color)
            .then(|| parse_color(&head))
            .flatten();
        let host = (category == Category::Link)
            .then(|| link_host(&head))
            .flatten();
        Self {
            id,
            category,
            kind,
            head,
            complete,
            created_ms,
            pinned,
            folder,
            size_bytes,
            dims,
            source_exe,
            language,
            color,
            host,
            file_count,
            preview,
            preview_html,
            read_only,
        }
    }

    pub fn from_indexed(ix: IndexedItem) -> Self {
        let m = ix.meta;
        let language = if m.category == Category::Code && m.kind == StoredKind::Text {
            ix.text.as_deref().and_then(zlip_highlight::detect)
        } else {
            None
        };
        let dims = m.width.zip(m.height);
        Self::build(
            m.id,
            m.category,
            m.kind,
            ix.text.as_deref(),
            m.created_at_ms,
            m.pinned,
            m.folder,
            m.size_bytes,
            dims,
            ix.source_exe,
            language,
            false,
        )
    }

    /// Entry for a payload that was just stored.
    pub fn from_capture(
        id: ItemId,
        payload: &Payload,
        category: Category,
        language: Option<Language>,
        source_exe: Option<String>,
        now_ms: i64,
    ) -> Self {
        let (kind, text, dims) = match payload {
            Payload::Text(t) => (StoredKind::Text, Some(t.clone()), None),
            Payload::Files(f) => (StoredKind::Files, Some(f.join("\n")), None),
            Payload::Image(i) => (StoredKind::Image, None, Some((i.width, i.height))),
        };
        Self::build(
            id,
            category,
            kind,
            text.as_deref(),
            now_ms,
            false,
            None,
            payload.size_bytes() as u64,
            dims,
            source_exe,
            language,
            false,
        )
    }

    /// Label shown under the preview.
    pub fn label(&self) -> Option<String> {
        match self.category {
            Category::Code => Some(self.language.map_or("Code", Language::label).to_owned()),
            Category::Link => self.host.clone(),
            Category::File => Some(if self.file_count == 1 {
                "1 file".to_owned()
            } else {
                format!("{} files", self.file_count)
            }),
            Category::Image => self.dims.map(|(w, h)| format!("{w} × {h}")),
            Category::Color => self.color.map(Rgba::to_hex),
            Category::Snippet => self.folder.clone(),
            Category::Text | Category::Sensitive => None,
        }
    }

    pub fn view(&self, queue_position: Option<usize>) -> ItemView {
        ItemView {
            id: self.id.0,
            category: self.category,
            label: self.label(),
            preview: self.preview.clone(),
            preview_html: self.preview_html.clone(),
            color: self.color.map(Rgba::to_hex),
            created_at_ms: self.created_ms,
            pinned: self.pinned,
            folder: self.folder.clone(),
            size_bytes: self.size_bytes,
            width: self.dims.map(|d| d.0),
            height: self.dims.map(|d| d.1),
            source_app: self.source_exe.as_deref().map(app_name),
            expires_in_ms: None,
            queue_position,
            read_only: self.read_only,
        }
    }
}

/// Re-derive category and language for text (used after automations rewrite it).
pub(crate) fn classify_text(text: &str) -> (Category, Option<Language>) {
    let c = classify(&Payload::Text(text.to_owned()));
    (c.category, c.language)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_are_two_trimmed_lines_with_an_ellipsis_cap() {
        assert_eq!(first_lines("one\n\n  two  \nthree"), "one\n  two");
        let long = "x".repeat(500);
        let p = first_lines(&long);
        assert_eq!(p.chars().count(), PREVIEW_CHARS + 1);
        assert!(p.ends_with('…'));
    }

    #[test]
    fn head_cuts_on_a_char_boundary() {
        let s = "é".repeat(HEAD_BYTES);
        let (h, complete) = head_of(&s);
        assert!(!complete && h.len() <= HEAD_BYTES && s.starts_with(&h));
        assert_eq!(head_of("short"), ("short".to_owned(), true));
    }

    #[test]
    fn app_names_are_friendly() {
        assert_eq!(
            app_name(r"C:\Program Files\Google\Chrome\chrome.exe"),
            "Chrome"
        );
        assert_eq!(app_name("/usr/bin/NOTEPAD.EXE"), "NOTEPAD");
        assert_eq!(app_name(""), "");
    }

    #[test]
    fn link_hosts() {
        assert_eq!(
            link_host("https://www.github.com/a/b").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            link_host("www.example.org/x").as_deref(),
            Some("example.org")
        );
        assert_eq!(link_host("nope"), None);
    }

    #[test]
    fn file_previews_name_the_first_file_and_count_the_rest() {
        let e = Entry::build(
            ItemId(1),
            Category::File,
            StoredKind::Files,
            Some("C:\\a\\report.pdf\nC:\\a\\b.txt\nC:\\c.png"),
            0,
            false,
            None,
            10,
            None,
            None,
            None,
            false,
        );
        assert_eq!(e.preview, "report.pdf  +2 more");
        assert_eq!(e.label().as_deref(), Some("3 files"));
    }
}
