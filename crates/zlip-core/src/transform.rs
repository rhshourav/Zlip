use base64::{
    Engine as _, alphabet,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig, general_purpose::STANDARD},
};
use heck::{ToKebabCase, ToLowerCamelCase, ToShoutySnakeCase, ToSnakeCase, ToUpperCamelCase};
use serde::{Deserialize, Serialize};
use url::Url;

/// A reversible-at-paste-time text operation. The same names appear in the UI's
/// transform bar and in `[[automations]] do = [...]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Transform {
    Trim,
    CamelCase,
    PascalCase,
    SnakeCase,
    KebabCase,
    ScreamingSnakeCase,
    Upper,
    Lower,
    Base64Decode,
    Base64Encode,
    StripTrackingParams,
}

impl Transform {
    pub const ALL: [Self; 11] = [
        Self::Trim,
        Self::CamelCase,
        Self::PascalCase,
        Self::SnakeCase,
        Self::KebabCase,
        Self::ScreamingSnakeCase,
        Self::Upper,
        Self::Lower,
        Self::Base64Decode,
        Self::Base64Encode,
        Self::StripTrackingParams,
    ];

    /// Stable name, identical to the serde name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Trim => "trim",
            Self::CamelCase => "camel_case",
            Self::PascalCase => "pascal_case",
            Self::SnakeCase => "snake_case",
            Self::KebabCase => "kebab_case",
            Self::ScreamingSnakeCase => "screaming_snake_case",
            Self::Upper => "upper",
            Self::Lower => "lower",
            Self::Base64Decode => "base64_decode",
            Self::Base64Encode => "base64_encode",
            Self::StripTrackingParams => "strip_tracking_params",
        }
    }

    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == s)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TransformError {
    #[error("the text is not valid Base64")]
    NotBase64,
    #[error("the Base64 decodes to binary data, not text")]
    NotText,
}

/// Apply `transforms` in order. The first failure aborts so the UI can say why.
pub fn apply_transforms(transforms: &[Transform], input: &str) -> Result<String, TransformError> {
    let mut text = input.to_owned();
    for t in transforms {
        text = apply_one(*t, &text)?;
    }
    Ok(text)
}

fn apply_one(t: Transform, s: &str) -> Result<String, TransformError> {
    Ok(match t {
        Transform::Trim => s.trim().to_owned(),
        Transform::CamelCase => per_line(s, |l| l.to_lower_camel_case()),
        Transform::PascalCase => per_line(s, |l| l.to_upper_camel_case()),
        Transform::SnakeCase => per_line(s, |l| l.to_snake_case()),
        Transform::KebabCase => per_line(s, |l| l.to_kebab_case()),
        Transform::ScreamingSnakeCase => per_line(s, |l| l.to_shouty_snake_case()),
        Transform::Upper => s.to_uppercase(),
        Transform::Lower => s.to_lowercase(),
        Transform::Base64Decode => base64_decode(s)?,
        Transform::Base64Encode => STANDARD.encode(s.as_bytes()),
        Transform::StripTrackingParams => strip_tracking_params(s),
    })
}

/// heck treats newlines as word breaks; converting each line keeps line structure.
fn per_line(s: &str, f: impl Fn(&str) -> String) -> String {
    s.split_inclusive('\n')
        .map(|line| {
            let body = line.trim_end_matches(['\r', '\n']);
            format!("{}{}", f(body), &line[body.len()..])
        })
        .collect()
}

const LENIENT_STD: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);
const LENIENT_URL: GeneralPurpose = GeneralPurpose::new(
    &alphabet::URL_SAFE,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// Accepts standard and URL-safe alphabets, with or without padding, and ignores
/// whitespace and line wrapping (as found in PEM bodies and email attachments).
fn base64_decode(s: &str) -> Result<String, TransformError> {
    let compact: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if compact.is_empty() {
        return Err(TransformError::NotBase64);
    }
    let bytes = LENIENT_STD
        .decode(&compact)
        .or_else(|_| LENIENT_URL.decode(&compact))
        .map_err(|_| TransformError::NotBase64)?;
    String::from_utf8(bytes).map_err(|_| TransformError::NotText)
}

fn is_tracking_param(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.starts_with("utm_")
        || matches!(
            k.as_str(),
            "fbclid"
                | "gclid"
                | "dclid"
                | "msclkid"
                | "yclid"
                | "twclid"
                | "wbraid"
                | "gbraid"
                | "igshid"
                | "mc_eid"
                | "mc_cid"
                | "mkt_tok"
                | "vero_id"
                | "ref_src"
                | "_hsenc"
                | "_hsmi"
        )
}

/// Remove well-known tracking parameters from every URL token; other text,
/// spacing and non-tracking parameters are preserved.
fn strip_tracking_params(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        if !token.is_empty() {
            out.push_str(&clean_url_token(token));
            token.clear();
        }
    };
    for c in s.chars() {
        if c.is_whitespace() {
            flush(&mut token, &mut out);
            out.push(c);
        } else {
            token.push(c);
        }
    }
    flush(&mut token, &mut out);
    out
}

fn clean_url_token(token: &str) -> String {
    let Ok(mut url) = Url::parse(token) else {
        return token.to_owned();
    };
    if !matches!(url.scheme(), "http" | "https") || url.query().is_none() {
        return token.to_owned();
    }
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| !is_tracking_param(k))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.len() == url.query_pairs().count() {
        return token.to_owned(); // nothing to strip: keep the original spelling
    }
    if kept.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut()
            .clear()
            .extend_pairs(kept.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }
    url.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use Transform::*;

    fn run(t: &[Transform], s: &str) -> String {
        apply_transforms(t, s).unwrap()
    }

    #[test]
    fn casing() {
        assert_eq!(run(&[CamelCase], "hello big world"), "helloBigWorld");
        assert_eq!(run(&[PascalCase], "hello_big_world"), "HelloBigWorld");
        assert_eq!(run(&[SnakeCase], "HelloBigWorld"), "hello_big_world");
        assert_eq!(run(&[KebabCase], "HelloBigWorld"), "hello-big-world");
        assert_eq!(run(&[ScreamingSnakeCase], "hello big"), "HELLO_BIG");
    }

    #[test]
    fn casing_keeps_line_structure() {
        assert_eq!(
            run(&[SnakeCase], "FooBar\r\nBazQux\n"),
            "foo_bar\r\nbaz_qux\n"
        );
    }

    #[test]
    fn trim_and_chain() {
        assert_eq!(run(&[Trim, Upper], "  hi \n"), "HI");
    }

    #[test]
    fn base64_round_trip_and_leniency() {
        assert_eq!(run(&[Base64Encode], "héllo"), "aMOpbGxv");
        assert_eq!(run(&[Base64Decode], "aMOpbGxv"), "héllo");
        assert_eq!(run(&[Base64Decode], "aGVs\nbG8="), "hello", "wrapped lines");
        assert_eq!(run(&[Base64Decode], "aGVsbG8"), "hello", "missing padding");
        assert_eq!(run(&[Base64Decode], "Pz8+"), "??>", "standard alphabet");
        assert_eq!(run(&[Base64Decode], "Pz8-"), "??>", "URL-safe alphabet");
    }

    #[test]
    fn base64_errors_are_specific() {
        assert_eq!(
            apply_transforms(&[Base64Decode], "!!!"),
            Err(TransformError::NotBase64)
        );
        assert_eq!(
            apply_transforms(&[Base64Decode], ""),
            Err(TransformError::NotBase64)
        );
        assert_eq!(
            apply_transforms(&[Base64Decode], "/w=="),
            Err(TransformError::NotText)
        );
    }

    #[test]
    fn tracking_params_are_stripped() {
        assert_eq!(
            run(
                &[StripTrackingParams],
                "https://a.com/p?id=7&utm_source=x&fbclid=y"
            ),
            "https://a.com/p?id=7"
        );
        assert_eq!(
            run(&[StripTrackingParams], "https://a.com/p?utm_medium=z"),
            "https://a.com/p"
        );
        assert_eq!(
            run(
                &[StripTrackingParams],
                "read https://a.com/?gclid=1 and https://b.org/x end"
            ),
            "read https://a.com/ and https://b.org/x end"
        );
    }

    #[test]
    fn untouched_urls_keep_their_exact_spelling() {
        let u = "https://a.com/p?q=a%20b&x=1";
        assert_eq!(run(&[StripTrackingParams], u), u);
        assert_eq!(run(&[StripTrackingParams], "no urls here"), "no urls here");
    }

    #[test]
    fn names_round_trip_through_serde_and_lookup() {
        for t in Transform::ALL {
            assert_eq!(Transform::from_name(t.name()), Some(t));
        }
    }
}
