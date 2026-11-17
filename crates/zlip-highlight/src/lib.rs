#![forbid(unsafe_code)]
//! Language detection, pretty-printing and HTML highlighting for clipboard text.
//!
//! # Security
//! Clipboard text is attacker-controlled. Every string that leaves this crate as
//! HTML is escaped here, in Rust, so the webview never has to insert raw
//! clipboard text as markup (blueprint §3.7).
//!
//! # Fidelity
//! Pretty-printing is for *display*. The JSON printer is lexical (it never
//! round-trips through a value tree), so key order, number spelling and string
//! escapes are preserved exactly.

mod detect;
mod html;
mod pretty;

use serde::{Deserialize, Serialize};

pub use detect::detect;
pub use html::{escape_html, highlight_html};
pub use pretty::pretty;

/// Inputs larger than this are never parsed or highlighted (bounds CPU and DOM size).
pub const MAX_ANALYZE_BYTES: usize = 256 * 1024;
/// Highlighted output is cut at a line boundary once it reaches this many bytes.
pub const MAX_RENDER_BYTES: usize = 64 * 1024;

/// Languages Zlip can recognise and pretty-print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Json,
    Xml,
    Sql,
    Rust,
}

impl Language {
    /// Human-readable name shown in the card's metadata line.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Xml => "XML",
            Self::Sql => "SQL",
            Self::Rust => "Rust",
        }
    }
}

/// Everything the UI needs to draw a code card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rendered {
    pub language: Option<Language>,
    /// Pretty-printed text (equals the input when it could not be reformatted).
    pub pretty: String,
    /// Escaped, class-annotated HTML of `pretty` (possibly truncated).
    pub html: String,
    /// `true` when `pretty` differs from the input.
    pub reformatted: bool,
    /// `true` when `html` was cut short at [`MAX_RENDER_BYTES`].
    pub truncated: bool,
}

/// Detect, pretty-print and highlight `text` in one call.
#[must_use]
pub fn render(text: &str) -> Rendered {
    if text.len() > MAX_ANALYZE_BYTES {
        let (clipped, truncated) = clip_at_line(text, MAX_RENDER_BYTES);
        return Rendered {
            language: None,
            pretty: text.to_owned(),
            html: escape_html(clipped),
            reformatted: false,
            truncated,
        };
    }
    let language = detect(text);
    let pretty_text = language.and_then(|l| pretty(text, l));
    let reformatted = pretty_text.as_deref().is_some_and(|p| p != text);
    let pretty = pretty_text.unwrap_or_else(|| text.to_owned());
    let (clipped, truncated) = clip_at_line(&pretty, MAX_RENDER_BYTES);
    Rendered {
        language,
        html: highlight_html(clipped, language),
        pretty,
        reformatted,
        truncated,
    }
}

/// Cut `s` to at most `max` bytes on a line (or at least char) boundary.
fn clip_at_line(s: &str, max: usize) -> (&str, bool) {
    if s.len() <= max {
        return (s, false);
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    let cut = s[..end].rfind('\n').unwrap_or(end);
    (&s[..cut], true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_json_end_to_end() {
        let r = render(r#"{"b":1,"a":[true,null]}"#);
        assert_eq!(r.language, Some(Language::Json));
        assert!(r.reformatted);
        assert!(r.pretty.starts_with("{\n  \"b\": 1,"));
        assert!(r.html.contains("<span"));
    }

    #[test]
    fn render_plain_text_is_escaped_not_highlighted() {
        let r = render("hello <b>world</b> & friends");
        assert_eq!(r.language, None);
        assert!(!r.reformatted);
        assert_eq!(r.html, "hello &lt;b&gt;world&lt;/b&gt; &amp; friends");
    }

    #[test]
    fn oversized_input_skips_analysis() {
        let big = "x\n".repeat(MAX_ANALYZE_BYTES);
        let r = render(&big);
        assert_eq!(r.language, None);
        assert!(r.truncated);
        assert!(r.html.len() <= MAX_RENDER_BYTES + 16);
    }

    #[test]
    fn clip_respects_char_boundaries() {
        let s = "é".repeat(100);
        let (c, t) = clip_at_line(&s, 51);
        assert!(t);
        assert!(s.starts_with(c));
    }
}
