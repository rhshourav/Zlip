//! Dynamic placeholders for pinned snippets.
//!
//! Recognised: `{date}`, `{date:%Y-%m-%d}`, `{time}`, `{time:%H:%M}`,
//! `{clipboard}` (alias `{clipboard_state}`), `{cursor}`; `{{` and `}}` are
//! literal braces. Any other `{...}` stays literal, because snippets are often
//! code and braces are everywhere in code.

use chrono::{
    NaiveDateTime,
    format::{Item, StrftimeItems},
};
use unicode_segmentation::UnicodeSegmentation;

/// Values a snippet can pull in at paste time.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    /// Local wall-clock time (the caller owns the clock, which keeps this testable).
    pub now: NaiveDateTime,
    /// Text on the clipboard just before Zlip overwrote it.
    pub clipboard: Option<&'a str>,
}

/// The expanded snippet plus where the caret should end up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded {
    pub text: String,
    /// Number of Left-arrow presses (one per grapheme cluster) needed after
    /// pasting to land where `{cursor}` was. Zero when there is no marker.
    pub cursor_back: usize,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlaceholderError {
    #[error("`{0}` is not a valid date/time format")]
    BadFormat(String),
}

/// `true` when `template` contains at least one *recognised* placeholder.
///
/// Escapes alone (`{{`) do not count: a pinned code snippet such as a Go
/// template (`{{ .Name }}`) must be pasted verbatim unless it really uses one.
#[must_use]
pub fn has_placeholders(template: &str) -> bool {
    parse(template)
        .iter()
        .any(|t| !matches!(t, Token::Lit(_) | Token::OpenBrace | Token::CloseBrace))
}

enum Token<'a> {
    Lit(&'a str),
    OpenBrace,
    CloseBrace,
    Date(Option<&'a str>),
    Time(Option<&'a str>),
    Clipboard,
    Cursor,
}

fn push_lit<'a>(tokens: &mut Vec<Token<'a>>, template: &'a str, from: usize, to: usize) {
    if to > from {
        tokens.push(Token::Lit(&template[from..to]));
    }
}

fn parse(template: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let bytes = template.as_bytes();
    let (mut lit_start, mut i) = (0, 0);
    while i < bytes.len() {
        match bytes[i] {
            b'{' if bytes.get(i + 1) == Some(&b'{') => {
                push_lit(&mut tokens, template, lit_start, i);
                tokens.push(Token::OpenBrace);
                i += 2;
                lit_start = i;
            }
            b'}' if bytes.get(i + 1) == Some(&b'}') => {
                push_lit(&mut tokens, template, lit_start, i);
                tokens.push(Token::CloseBrace);
                i += 2;
                lit_start = i;
            }
            b'{' => {
                if let Some(rel) = template[i + 1..].find('}') {
                    let inner = &template[i + 1..i + 1 + rel];
                    if let Some(tok) = recognise(inner) {
                        push_lit(&mut tokens, template, lit_start, i);
                        tokens.push(tok);
                        i += rel + 2;
                        lit_start = i;
                        continue;
                    }
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    push_lit(&mut tokens, template, lit_start, bytes.len());
    tokens
}

fn recognise(inner: &str) -> Option<Token<'_>> {
    let (name, arg) = match inner.split_once(':') {
        Some((n, a)) => (n, Some(a)),
        None => (inner, None),
    };
    match (name, arg) {
        ("date", a) => Some(Token::Date(a)),
        ("time", a) => Some(Token::Time(a)),
        ("clipboard" | "clipboard_state", None) => Some(Token::Clipboard),
        ("cursor", None) => Some(Token::Cursor),
        _ => None,
    }
}

fn format_time(now: NaiveDateTime, fmt: &str) -> Result<String, PlaceholderError> {
    // chrono panics when *displaying* an invalid specifier, so validate first.
    if StrftimeItems::new(fmt).any(|i| matches!(i, Item::Error)) {
        return Err(PlaceholderError::BadFormat(fmt.to_owned()));
    }
    Ok(now.format(fmt).to_string())
}

/// Expand every placeholder in `template`.
pub fn expand_placeholders(
    template: &str,
    ctx: &Context<'_>,
) -> Result<Expanded, PlaceholderError> {
    let mut out = String::with_capacity(template.len());
    let mut cursor_at: Option<usize> = None;
    for token in parse(template) {
        match token {
            Token::Lit(s) => out.push_str(s),
            Token::OpenBrace => out.push('{'),
            Token::CloseBrace => out.push('}'),
            Token::Date(f) => out.push_str(&format_time(ctx.now, f.unwrap_or("%Y-%m-%d"))?),
            Token::Time(f) => out.push_str(&format_time(ctx.now, f.unwrap_or("%H:%M"))?),
            // Inserted verbatim and never re-scanned, so clipboard text cannot inject placeholders.
            Token::Clipboard => out.push_str(ctx.clipboard.unwrap_or_default()),
            Token::Cursor => {
                cursor_at.get_or_insert(out.len()); // only the first marker counts
            }
        }
    }
    let cursor_back = cursor_at.map_or(0, |at| out[at..].graphemes(true).count());
    Ok(Expanded {
        text: out,
        cursor_back,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn ctx(clip: Option<&str>) -> Context<'_> {
        Context {
            now: NaiveDate::from_ymd_opt(2026, 11, 12)
                .unwrap()
                .and_hms_opt(9, 5, 7)
                .unwrap(),
            clipboard: clip,
        }
    }
    fn ex(t: &str) -> Expanded {
        expand_placeholders(t, &ctx(Some("CLIP"))).unwrap()
    }

    #[test]
    fn dates_and_times() {
        assert_eq!(ex("{date}").text, "2026-11-12");
        assert_eq!(ex("{time}").text, "09:05");
        assert_eq!(
            ex("{date:%d/%m/%Y} {time:%H:%M:%S}").text,
            "12/11/2026 09:05:07"
        );
    }

    #[test]
    fn clipboard_aliases_and_no_reinjection() {
        assert_eq!(
            ex("[{clipboard}] [{clipboard_state}]").text,
            "[CLIP] [CLIP]"
        );
        let e = expand_placeholders("{clipboard}", &ctx(Some("{date}"))).unwrap();
        assert_eq!(e.text, "{date}", "clipboard content must stay literal");
        assert_eq!(
            expand_placeholders("{clipboard}", &ctx(None)).unwrap().text,
            ""
        );
    }

    #[test]
    fn cursor_counts_graphemes_after_marker() {
        let e = ex("fn {cursor}()");
        assert_eq!((e.text.as_str(), e.cursor_back), ("fn ()", 2));
        assert_eq!(ex("{cursor}").cursor_back, 0);
        assert_eq!(ex("abc").cursor_back, 0);
    }

    #[test]
    fn cursor_handles_emoji_and_crlf_clusters() {
        assert_eq!(
            ex("a{cursor}👨‍👩‍👧b").cursor_back,
            2,
            "family emoji is one cluster"
        );
        assert_eq!(ex("a{cursor}\r\nb").cursor_back, 2, "CRLF is one cluster");
    }

    #[test]
    fn only_the_first_cursor_counts() {
        let e = ex("a{cursor}b{cursor}c");
        assert_eq!((e.text.as_str(), e.cursor_back), ("abc", 2));
    }

    #[test]
    fn doubled_braces_are_literal() {
        assert_eq!(ex("{{date}}").text, "{date}");
        assert_eq!(ex("{{ }}").text, "{ }");
    }

    #[test]
    fn code_braces_survive_untouched() {
        let src = "function f() { return {a: 1}; }\nif (x) { y(); }";
        assert_eq!(ex(src).text, src);
        assert_eq!(ex("{unknown} {date:%Y}x").text, "{unknown} 2026x");
    }

    #[test]
    fn bad_format_is_an_error_not_a_panic() {
        let err = expand_placeholders("{date:%Q}", &ctx(None)).unwrap_err();
        assert_eq!(err, PlaceholderError::BadFormat("%Q".into()));
    }

    #[test]
    fn detects_when_expansion_matters() {
        assert!(has_placeholders("hi {date}"));
        assert!(has_placeholders("{cursor}"));
        assert!(
            !has_placeholders("{{x}}"),
            "escapes alone are not placeholders"
        );
        assert!(!has_placeholders("{{ .Name }} fn() { }"));
    }
}
