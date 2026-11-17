use crate::Language;
use std::sync::LazyLock;
use syntect::{
    html::{ClassStyle, ClassedHTMLGenerator},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

/// Prefix on every emitted class, e.g. `z-keyword`. The UI maps these classes to
/// the design-token CSS variables, so a theme is just a different variable set.
const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "z-" };

/// Escape the five HTML-significant characters.
#[must_use]
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn syntax_name(lang: Language) -> &'static str {
    match lang {
        Language::Json => "JSON",
        Language::Xml => "XML",
        Language::Sql => "SQL",
        Language::Rust => "Rust",
    }
}

/// Highlight `text` as `lang` into escaped, class-annotated HTML. With no
/// language (or if the grammar is missing) the text is only escaped.
#[must_use]
pub fn highlight_html(text: &str, lang: Option<Language>) -> String {
    let Some(syntax) = lang.and_then(|l| SYNTAXES.find_syntax_by_name(syntax_name(l))) else {
        return escape_html(text);
    };
    let mut generator = ClassedHTMLGenerator::new_with_class_style(syntax, &SYNTAXES, CLASS_STYLE);
    for line in LinesWithEndings::from(text) {
        if generator
            .parse_html_for_line_which_includes_newline(line)
            .is_err()
        {
            return escape_html(text);
        }
    }
    generator.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_grammar_we_advertise_exists() {
        for l in [Language::Json, Language::Xml, Language::Sql, Language::Rust] {
            assert!(
                SYNTAXES.find_syntax_by_name(syntax_name(l)).is_some(),
                "{l:?}"
            );
        }
    }

    #[test]
    fn keywords_get_prefixed_classes() {
        let h = highlight_html("fn main() {}", Some(Language::Rust));
        assert!(h.contains("class=\"z-"), "{h}");
    }

    #[test]
    fn markup_in_clipboard_text_cannot_escape() {
        let evil = "<script>alert(1)</script>";
        assert_eq!(
            highlight_html(evil, None),
            "&lt;script&gt;alert(1)&lt;/script&gt;"
        );
        let json = r#"{"x":"<img src=x onerror=alert(1)>"}"#;
        let h = highlight_html(json, Some(Language::Json));
        assert!(!h.contains("<img"), "{h}");
        let xml = "<a><![CDATA[<script>x</script>]]></a>";
        let h = highlight_html(xml, Some(Language::Xml));
        assert!(!h.contains("<script"), "{h}");
    }

    #[test]
    fn every_open_tag_is_one_of_ours() {
        let h = highlight_html("SELECT 'a<b' FROM t -- <x>", Some(Language::Sql));
        for part in h.split('<').skip(1) {
            assert!(
                part.starts_with("span") || part.starts_with("/span"),
                "stray tag: <{part}"
            );
        }
    }
}
