use crate::{Category, Payload};
use regex::Regex;
use std::sync::LazyLock;
use url::Url;
use zlip_highlight::Language;

/// An 8-bit RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    /// `#RRGGBB` (or `#RRGGBBAA` when not opaque), upper-case.
    #[must_use]
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }
}

/// Result of classifying a capture. Never `Sensitive`: that verdict belongs to
/// `zlip-security` and is applied *before* this runs (blueprint §2.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub category: Category,
    pub language: Option<Language>,
    pub color: Option<Rgba>,
}

impl Classification {
    fn of(category: Category) -> Self {
        Self {
            category,
            language: None,
            color: None,
        }
    }
}

/// Classify a payload. First match wins: image, file list, link, colour, code, text.
#[must_use]
pub fn classify(payload: &Payload) -> Classification {
    match payload {
        Payload::Image(_) => Classification::of(Category::Image),
        Payload::Files(_) => Classification::of(Category::File),
        Payload::Text(t) => classify_text(t),
    }
}

fn classify_text(text: &str) -> Classification {
    let t = text.trim();
    if is_link(t) {
        return Classification::of(Category::Link);
    }
    if let Some(color) = parse_color(t) {
        return Classification {
            category: Category::Color,
            language: None,
            color: Some(color),
        };
    }
    if let Some(language) = zlip_highlight::detect(t) {
        return Classification {
            category: Category::Code,
            language: Some(language),
            color: None,
        };
    }
    if looks_like_generic_code(t) {
        return Classification::of(Category::Code);
    }
    Classification::of(Category::Text)
}

// ---------------------------------------------------------------- links

fn is_link(t: &str) -> bool {
    if t.is_empty() || t.len() > 2048 || t.contains(char::is_whitespace) {
        return false;
    }
    if t.len() > 4 && t[..4].eq_ignore_ascii_case("www.") {
        return t[4..].contains('.') && Url::parse(&format!("https://{t}")).is_ok();
    }
    match Url::parse(t) {
        Ok(u) => match u.scheme() {
            "http" | "https" | "ftp" | "ftps" => u.host_str().is_some_and(|h| !h.is_empty()),
            "mailto" => !u.path().is_empty(),
            _ => false,
        },
        Err(_) => false,
    }
}

// --------------------------------------------------------------- colours

static FUNC_COLOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(rgba?|hsla?)\(\s*([^)]*?)\s*\)$").expect("static pattern compiles")
});

/// Parse `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()/rgba()` and `hsl()/hsla()`.
/// The *whole* string must be a colour; "color: red" or prose is not.
#[must_use]
pub fn parse_color(s: &str) -> Option<Rgba> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex);
    }
    let caps = FUNC_COLOR.captures(s)?;
    let name = caps[1].to_ascii_lowercase();
    let args: Vec<&str> = caps[2]
        .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .collect();
    if args.len() < 3 || args.len() > 4 {
        return None;
    }
    let alpha = match args.get(3) {
        Some(a) => parse_alpha(a)?,
        None => 255,
    };
    if name.starts_with("rgb") {
        Some(Rgba {
            r: channel(args[0])?,
            g: channel(args[1])?,
            b: channel(args[2])?,
            a: alpha,
        })
    } else {
        let h: f64 = args[0].trim_end_matches("deg").parse().ok()?;
        let sat = percent(args[1])?;
        let light = percent(args[2])?;
        let (r, g, b) = hsl_to_rgb(h.rem_euclid(360.0), sat, light);
        Some(Rgba { r, g, b, a: alpha })
    }
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let nib = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok().map(|v| v * 17);
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        3 => Some(Rgba {
            r: nib(0)?,
            g: nib(1)?,
            b: nib(2)?,
            a: 255,
        }),
        4 => Some(Rgba {
            r: nib(0)?,
            g: nib(1)?,
            b: nib(2)?,
            a: nib(3)?,
        }),
        6 => Some(Rgba {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: 255,
        }),
        8 => Some(Rgba {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: byte(6)?,
        }),
        _ => None,
    }
}

fn channel(s: &str) -> Option<u8> {
    if let Some(p) = s.strip_suffix('%') {
        let v: f64 = p.parse().ok()?;
        return (0.0..=100.0)
            .contains(&v)
            .then(|| (v * 255.0 / 100.0).round() as u8);
    }
    let v: f64 = s.parse().ok()?;
    (0.0..=255.0).contains(&v).then(|| v.round() as u8)
}

fn parse_alpha(s: &str) -> Option<u8> {
    if let Some(p) = s.strip_suffix('%') {
        let v: f64 = p.parse().ok()?;
        return (0.0..=100.0)
            .contains(&v)
            .then(|| (v * 255.0 / 100.0).round() as u8);
    }
    let v: f64 = s.parse().ok()?;
    (0.0..=1.0).contains(&v).then(|| (v * 255.0).round() as u8)
}

fn percent(s: &str) -> Option<f64> {
    let v: f64 = s.strip_suffix('%')?.parse().ok()?;
    (0.0..=100.0).contains(&v).then_some(v / 100.0)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to8 = |v: f64| ((v + m) * 255.0).round() as u8;
    (to8(r), to8(g), to8(b))
}

// ------------------------------------------------------- generic "code"

/// Conservative fallback for languages we cannot parse (JS, Python, shell...).
/// Needs several lines *and* several distinct signals, so prose stays prose.
fn looks_like_generic_code(t: &str) -> bool {
    if t.lines().count() < 2 {
        return false;
    }
    const SIGNALS: [&str; 14] = [
        "{",
        "}",
        "=>",
        "();",
        "def ",
        "function ",
        "import ",
        "const ",
        "return ",
        "class ",
        "#include",
        "</",
        "if (",
        "for (",
    ];
    let hits = SIGNALS.iter().filter(|s| t.contains(**s)).count();
    let semicolons = t.lines().filter(|l| l.trim_end().ends_with(';')).count();
    hits + usize::from(semicolons >= 2) >= 3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ImageData;

    fn text(s: &str) -> Classification {
        classify(&Payload::Text(s.into()))
    }

    #[test]
    fn links() {
        for s in [
            "https://github.com/rhshourav/zlip",
            "http://localhost:3000/a?b=c",
            "www.example.com/x",
            "mailto:a@b.co",
        ] {
            assert_eq!(text(s).category, Category::Link, "{s}");
        }
        for s in [
            "not a link",
            "https://",
            "javascript:alert(1)",
            "see https://example.com now",
            "example.com",
        ] {
            assert_ne!(text(s).category, Category::Link, "{s}");
        }
    }

    #[test]
    fn colours_all_notations() {
        assert_eq!(
            parse_color("#fff"),
            Some(Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 255
            })
        );
        assert_eq!(
            parse_color("#C5CAF5").map(Rgba::to_hex).as_deref(),
            Some("#C5CAF5")
        );
        assert_eq!(parse_color("#11223344").map(|c| c.a), Some(0x44));
        assert_eq!(
            parse_color("rgb(255, 0, 128)"),
            Some(Rgba {
                r: 255,
                g: 0,
                b: 128,
                a: 255
            })
        );
        assert_eq!(parse_color("rgb(255 0 0 / 50%)").map(|c| c.a), Some(128));
        assert_eq!(parse_color("rgba(0,0,0,0.5)").map(|c| c.a), Some(128));
        assert_eq!(
            parse_color("hsl(120, 100%, 50%)"),
            Some(Rgba {
                r: 0,
                g: 255,
                b: 0,
                a: 255
            })
        );
        assert_eq!(
            parse_color("hsl(0 100% 50%)"),
            Some(Rgba {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            })
        );
    }

    #[test]
    fn non_colours_are_rejected() {
        for s in [
            "#12",
            "#12345",
            "#gggggg",
            "rgb(300,0,0)",
            "rgb(1,2)",
            "color: red",
            "hsl(1,2,3)",
            "#",
        ] {
            assert_eq!(parse_color(s), None, "{s}");
        }
        assert_eq!(text("#ff00aa").category, Category::Color);
    }

    #[test]
    fn code_uses_real_parsers_first() {
        let c = text("{\"a\": [1, 2]}");
        assert_eq!(
            (c.category, c.language),
            (Category::Code, Some(Language::Json))
        );
        let c = text("SELECT * FROM t");
        assert_eq!(c.language, Some(Language::Sql));
    }

    #[test]
    fn generic_code_needs_several_signals() {
        let js = "const add = (a, b) => {\n  return a + b;\n};\nexport default add;";
        assert_eq!(text(js).category, Category::Code);
        assert_eq!(
            text("Dear team,\nsee you at lunch.").category,
            Category::Text
        );
        assert_eq!(text("one line { only }").category, Category::Text);
    }

    #[test]
    fn payload_kinds() {
        assert_eq!(
            classify(&Payload::Files(vec!["C:/a.txt".into()])).category,
            Category::File
        );
        let img = Payload::Image(ImageData {
            png: vec![],
            width: 1,
            height: 1,
        });
        assert_eq!(classify(&img).category, Category::Image);
    }
}
