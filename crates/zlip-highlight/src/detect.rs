use crate::{Language, MAX_ANALYZE_BYTES};
use quick_xml::{Reader, events::Event};
use sqlparser::{dialect::GenericDialect, parser::Parser};

/// Guess the language of `text`. Cheapest, most specific checks run first.
///
/// Every positive answer is *confirmed by a real parser*, never by regex alone,
/// so ordinary prose or JavaScript is not mislabelled as code.
#[must_use]
pub fn detect(text: &str) -> Option<Language> {
    let t = text.trim();
    if t.len() < 2 || t.len() > MAX_ANALYZE_BYTES {
        return None;
    }
    if is_json(t) {
        Some(Language::Json)
    } else if is_xml(t) {
        Some(Language::Xml)
    } else if is_sql(t) {
        Some(Language::Sql)
    } else if is_rust(t) {
        Some(Language::Rust)
    } else {
        None
    }
}

fn is_json(t: &str) -> bool {
    // Bare numbers/strings/literals are valid JSON but are not "code" to a user.
    matches!(t.as_bytes()[0], b'{' | b'[')
        && serde_json::from_str::<serde::de::IgnoredAny>(t).is_ok()
}

fn is_xml(t: &str) -> bool {
    if !(t.starts_with('<') && t.ends_with('>')) {
        return false;
    }
    let mut reader = Reader::from_str(t);
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = true;
    let (mut depth, mut roots) = (0usize, 0usize);
    loop {
        match reader.read_event() {
            Ok(Event::Start(_)) => {
                if depth == 0 {
                    roots += 1;
                }
                depth += 1;
            }
            Ok(Event::Empty(_)) => {
                if depth == 0 {
                    roots += 1;
                }
            }
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::Text(_) | Event::CData(_)) if depth == 0 => return false,
            Ok(Event::Eof) => return depth == 0 && roots == 1,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
}

const SQL_LEADERS: [&str; 11] = [
    "select", "insert", "update", "delete", "create", "alter", "drop", "with", "truncate",
    "explain", "merge",
];

fn is_sql(t: &str) -> bool {
    let first = t
        .split(|c: char| !c.is_ascii_alphabetic())
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !SQL_LEADERS.contains(&first.as_str()) {
        return false;
    }
    // A parse alone is not enough: "select a thing from the shop" is *valid* SQL
    // (`a AS thing FROM the AS shop`). Real queries also carry punctuation or a
    // clause keyword, which ordinary sentences almost never do.
    has_sql_structure(t)
        && Parser::parse_sql(&GenericDialect {}, t).is_ok_and(|stmts| !stmts.is_empty())
}

fn has_sql_structure(t: &str) -> bool {
    const PUNCT: [char; 5] = [';', '*', '(', '=', ','];
    const CLAUSES: [&str; 9] = [
        " where ",
        " join ",
        " group by ",
        " order by ",
        " values",
        " into ",
        " limit ",
        " having ",
        " union ",
    ];
    let lower = format!(" {} ", t.to_ascii_lowercase());
    t.contains(PUNCT) || CLAUSES.iter().any(|c| lower.contains(c)) || lower.contains(" table ")
}

/// One of these is enough evidence (together with a successful parse).
const RUST_STRONG: [&str; 11] = [
    "fn ", "impl ", "struct ", "enum ", "trait ", "let mut ", "#[", "&self", "&mut ", "pub fn",
    "unwrap()",
];
/// Common in other languages too, so two are needed: `use strict;` is valid Rust syntax.
const RUST_WEAK: [&str; 6] = ["use ", "mod ", "match ", "::", "->", "!("];

fn is_rust(t: &str) -> bool {
    let strong = RUST_STRONG.iter().any(|s| t.contains(*s));
    let weak = RUST_WEAK.iter().filter(|s| t.contains(**s)).count();
    if !strong && weak < 2 {
        return false;
    }
    syn::parse_file(t).is_ok() || syn::parse_file(&format!("fn __zlip() {{\n{t}\n}}")).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_objects_and_arrays_only() {
        assert_eq!(detect(r#"{"a": 1}"#), Some(Language::Json));
        assert_eq!(detect("[1, 2, 3]"), Some(Language::Json));
        assert_eq!(detect("42"), None);
        assert_eq!(detect(r#""just a string""#), None);
        assert_eq!(detect("{not json}"), None);
    }

    #[test]
    fn xml_needs_one_well_formed_root() {
        assert_eq!(detect("<a><b x=\"1\"/></a>"), Some(Language::Xml));
        assert_eq!(detect("<a><b></a>"), None);
        assert_eq!(detect("<a/><b/>"), None, "two roots is not a document");
        assert_eq!(detect("<br>"), None, "unclosed HTML void tag");
        assert_eq!(detect("<3 hearts>"), None);
    }

    #[test]
    fn sql_requires_a_real_parse() {
        assert_eq!(
            detect("SELECT id, name FROM users WHERE id = 1;"),
            Some(Language::Sql)
        );
        assert_eq!(detect("SELECT 1;"), Some(Language::Sql));
        assert_eq!(
            detect("delete from users where id = 3"),
            Some(Language::Sql)
        );
        assert_eq!(
            detect("select a thing from the shop"),
            None,
            "valid SQL grammar, but it is a sentence"
        );
        assert_eq!(
            detect("select 1"),
            None,
            "a bare two-word line is not worth calling code"
        );
        assert_eq!(detect("Update your profile picture today"), None);
    }

    #[test]
    fn rust_is_scored_then_parsed() {
        assert_eq!(detect("fn main() { run(); }"), Some(Language::Rust));
        assert_eq!(
            detect("let mut v = Vec::new();\nv.push(1);"),
            Some(Language::Rust),
            "statement snippets are accepted"
        );
        assert_eq!(detect("use std::io::Read;"), Some(Language::Rust));
        assert_eq!(
            detect("use strict;"),
            None,
            "valid Rust syntax, but one weak signal"
        );
        assert_eq!(detect("let f = (a) => a + 1; use strict;"), None);
        assert_eq!(detect("println!(\"hi\");"), None, "a single weak signal");
        assert_eq!(detect("pub fn broken( {"), None);
    }

    #[test]
    fn prose_is_not_code() {
        assert_eq!(detect("Meeting moved to 3pm, bring the slides."), None);
        assert_eq!(detect(""), None);
        assert_eq!(detect("x"), None);
    }
}
