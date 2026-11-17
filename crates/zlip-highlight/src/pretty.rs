use crate::Language;
use quick_xml::{Reader, Writer, events::Event};
use sqlformat::{FormatOptions, Indent, QueryParams};

/// Reformat `text` as `lang`. Returns `None` when the text does not parse, so
/// callers fall back to showing the original untouched.
#[must_use]
pub fn pretty(text: &str, lang: Language) -> Option<String> {
    let t = text.trim();
    match lang {
        Language::Json => pretty_json(t),
        Language::Xml => pretty_xml(t),
        Language::Sql => Some(pretty_sql(t)),
        Language::Rust => syn::parse_file(t).ok().map(|f| prettyplease::unparse(&f)),
    }
}

/// Lexical re-indenter: validates with serde_json, then lays out the *original*
/// tokens so numbers, escapes and key order are never altered.
fn pretty_json(t: &str) -> Option<String> {
    serde_json::from_str::<serde::de::IgnoredAny>(t).ok()?;
    let mut out = String::with_capacity(t.len() * 2);
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    let mut chars = t.chars().peekable();
    let indent = |out: &mut String, d: usize| {
        out.push('\n');
        for _ in 0..d {
            out.push_str("  ");
        }
    };
    while let Some(c) = chars.next() {
        if in_str {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
            }
            '{' | '[' => {
                out.push(c);
                let close = if c == '{' { '}' } else { ']' };
                while chars.peek().is_some_and(|n| n.is_whitespace()) {
                    chars.next();
                }
                if chars.peek() == Some(&close) {
                    out.push(close);
                    chars.next();
                } else {
                    depth += 1;
                    indent(&mut out, depth);
                }
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                indent(&mut out, depth);
                out.push(c);
            }
            ',' => {
                out.push(',');
                indent(&mut out, depth);
            }
            ':' => out.push_str(": "),
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    Some(out)
}

fn pretty_xml(t: &str) -> Option<String> {
    let mut reader = Reader::from_str(t);
    reader.config_mut().trim_text(true);
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    loop {
        match reader.read_event().ok()? {
            Event::Eof => break,
            ev => writer.write_event(ev).ok()?,
        }
    }
    String::from_utf8(writer.into_inner()).ok()
}

fn pretty_sql(t: &str) -> String {
    let opts = FormatOptions {
        indent: Indent::Spaces(2),
        uppercase: Some(true),
        ..FormatOptions::default()
    };
    sqlformat::format(t, &QueryParams::None, &opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_preserves_key_order_and_number_spelling() {
        let out = pretty(
            r#"{"z":1.50,"a":12345678901234567890123,"m":[]}"#,
            Language::Json,
        )
        .unwrap();
        assert_eq!(
            out,
            "{\n  \"z\": 1.50,\n  \"a\": 12345678901234567890123,\n  \"m\": []\n}"
        );
    }

    #[test]
    fn json_strings_with_structural_chars_are_untouched() {
        let src = r#"{"k":"a,b:{c}[d] \" end"}"#;
        let out = pretty(src, Language::Json).unwrap();
        assert!(out.contains(r#""a,b:{c}[d] \" end""#));
        let again: serde_json::Value = serde_json::from_str(&out).unwrap();
        let orig: serde_json::Value = serde_json::from_str(src).unwrap();
        assert_eq!(again, orig, "pretty-printing must be semantically lossless");
    }

    #[test]
    fn json_nested_layout() {
        let out = pretty(r#"[{"a":{"b":[1,2]}}]"#, Language::Json).unwrap();
        assert_eq!(
            out,
            "[\n  {\n    \"a\": {\n      \"b\": [\n        1,\n        2\n      ]\n    }\n  }\n]"
        );
    }

    #[test]
    fn invalid_json_is_not_reformatted() {
        assert_eq!(pretty("{oops", Language::Json), None);
    }

    #[test]
    fn xml_is_indented() {
        let out = pretty("<a><b>1</b><c/></a>", Language::Xml).unwrap();
        assert_eq!(out, "<a>\n  <b>1</b>\n  <c/>\n</a>");
    }

    #[test]
    fn sql_is_laid_out_over_lines() {
        let out = pretty("select id,name from users where id=1", Language::Sql).unwrap();
        assert!(out.contains("SELECT"));
        assert!(out.lines().count() >= 3, "{out}");
    }

    #[test]
    fn rust_goes_through_prettyplease() {
        let out = pretty("fn main(){let x=1;println!(\"{}\",x);}", Language::Rust).unwrap();
        assert!(out.contains("fn main() {\n    let x = 1;"), "{out}");
    }
}
