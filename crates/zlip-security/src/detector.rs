use regex::{Regex, RegexSet};
use std::sync::LazyLock;
use url::Url;
use zlip_config::SecurityConfig;

/// What kind of secret was recognised. Drives the masked preview text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensitiveKind {
    PrivateKey,
    ApiKey,
    Jwt,
    PaymentCard,
    /// Long, random-looking, single token (passwords, generated secrets).
    Token,
}

impl SensitiveKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::PrivateKey => "Private key",
            Self::ApiKey => "API key",
            Self::Jwt => "Login token",
            Self::PaymentCard => "Payment card",
            Self::Token => "Secret",
        }
    }
}

/// Anchored vendor formats. Index order must match [`KEY_KINDS`].
static KEY_PATTERNS: LazyLock<RegexSet> =
    LazyLock::new(|| RegexSet::new(KEY_SOURCES).expect("static patterns compile"));

const KEY_SOURCES: [&str; 10] = [
    r"^AKIA[0-9A-Z]{16}$",                                    // AWS access key id
    r"^gh[pousr]_[A-Za-z0-9]{36,}$",                          // GitHub tokens
    r"^github_pat_[A-Za-z0-9_]{50,}$",                        // GitHub fine-grained
    r"^xox[baprs]-[A-Za-z0-9-]{10,}$",                        // Slack
    r"^eyJ[A-Za-z0-9_-]+[.][A-Za-z0-9_-]+[.][A-Za-z0-9_-]+$", // JWT
    r"-----BEGIN [A-Z ]*PRIVATE KEY-----",                    // PEM private key
    r"^sk_(live|test)_[0-9A-Za-z]{24,}$",                     // Stripe secret key
    r"^AIza[0-9A-Za-z_-]{35}$",                               // Google API key
    r"^npm_[A-Za-z0-9]{36}$",                                 // npm token
    r"^sk-[A-Za-z0-9_-]{20,}$",                               // OpenAI / Anthropic style
];
const KEY_KINDS: [SensitiveKind; 10] = [
    SensitiveKind::ApiKey,
    SensitiveKind::ApiKey,
    SensitiveKind::ApiKey,
    SensitiveKind::ApiKey,
    SensitiveKind::Jwt,
    SensitiveKind::PrivateKey,
    SensitiveKind::ApiKey,
    SensitiveKind::ApiKey,
    SensitiveKind::ApiKey,
    SensitiveKind::ApiKey,
];

static UUID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
        .expect("static pattern compiles")
});
static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[^@\s]+@[^@\s]+\.[A-Za-z]{2,}$").expect("static pattern compiles")
});

/// Shannon entropy in bits per byte.
fn shannon_bits_per_byte(s: &str) -> f64 {
    let mut counts = [0usize; 256];
    for b in s.bytes() {
        counts[usize::from(b)] += 1;
    }
    let n = s.len() as f64;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / n;
            -p * p.log2()
        })
        .sum()
}

fn luhn_ok(digits: &[u32]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let x = d * 2;
                if x > 9 { x - 9 } else { x }
            } else {
                d
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

fn is_payment_card(s: &str) -> bool {
    if !s
        .bytes()
        .all(|b| b.is_ascii_digit() || b == b' ' || b == b'-')
    {
        return false;
    }
    let d: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
    // Real card numbers start with 3-6, and all-identical digits are never cards.
    (13..=19).contains(&d.len())
        && (3..=6).contains(&d[0])
        && d.iter().any(|&x| x != d[0])
        && luhn_ok(&d)
}

const SECRET_QUERY_KEYS: [&str; 11] = [
    "token",
    "access_token",
    "key",
    "api_key",
    "apikey",
    "secret",
    "password",
    "pwd",
    "sig",
    "signature",
    "auth",
];

/// Things that *look* random to an entropy test but are ordinary data. Without
/// this, copying a link or a file path would make it vanish after a minute.
fn is_benign_structure(s: &str) -> bool {
    if UUID.is_match(s) || EMAIL.is_match(s) {
        return true;
    }
    let b = s.as_bytes();
    let windows_path = b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'\\' || b[2] == b'/');
    if windows_path || s.starts_with("\\\\") || (s.starts_with('/') && s[1..].contains('/')) {
        return true;
    }
    if let Ok(u) = Url::parse(s)
        && matches!(u.scheme(), "http" | "https")
        && u.host_str().is_some()
    {
        let has_userinfo = !u.username().is_empty() || u.password().is_some();
        let secret_param = u
            .query_pairs()
            .any(|(k, _)| SECRET_QUERY_KEYS.contains(&k.to_ascii_lowercase().as_str()));
        return !has_userinfo && !secret_param;
    }
    false
}

/// Classify `raw` as a secret, or `None`. Runs *before* anything is stored.
#[must_use]
pub fn detect_sensitive(raw: &str, cfg: &SecurityConfig) -> Option<SensitiveKind> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 4096 {
        return None;
    }
    if let Some(i) = KEY_PATTERNS.matches(s).iter().next() {
        return Some(KEY_KINDS[i]);
    }
    if is_payment_card(s) {
        return Some(SensitiveKind::PaymentCard);
    }
    if s.contains(char::is_whitespace) || s.len() < cfg.min_token_len || is_benign_structure(s) {
        return None;
    }
    // Random-looking single token: long, 3+ character classes, high entropy.
    // Identifiers like `getUserResetToken` have only two classes.
    let classes = [
        s.bytes().any(|b| b.is_ascii_lowercase()),
        s.bytes().any(|b| b.is_ascii_uppercase()),
        s.bytes().any(|b| b.is_ascii_digit()),
        s.bytes().any(|b| b.is_ascii_punctuation()),
    ]
    .iter()
    .filter(|&&x| x)
    .count();
    (classes >= 3 && shannon_bits_per_byte(s) >= cfg.min_entropy_bits)
        .then_some(SensitiveKind::Token)
}

#[must_use]
pub fn is_sensitive(raw: &str, cfg: &SecurityConfig) -> bool {
    detect_sensitive(raw, cfg).is_some()
}

/// Preview text safe to show: never contains secret characters, except the last
/// four digits of a card (what banks print on receipts).
#[must_use]
pub fn mask(kind: SensitiveKind, raw: &str) -> String {
    let s = raw.trim();
    match kind {
        SensitiveKind::PaymentCard => {
            let digits: Vec<char> = s.chars().filter(char::is_ascii_digit).collect();
            let last4: String = digits[digits.len().saturating_sub(4)..].iter().collect();
            format!("•••• •••• •••• {last4}")
        }
        SensitiveKind::PrivateKey => "••••••••••••••••".to_owned(),
        _ => format!("{} • {} characters", kind.label(), s.chars().count()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SecurityConfig {
        SecurityConfig::default()
    }
    fn kind(s: &str) -> Option<SensitiveKind> {
        detect_sensitive(s, &cfg())
    }

    #[test]
    fn vendor_formats() {
        assert_eq!(kind("AKIAIOSFODNN7EXAMPLE"), Some(SensitiveKind::ApiKey));
        assert_eq!(
            kind(&format!("ghp_{}", "a1B2".repeat(9))),
            Some(SensitiveKind::ApiKey)
        );
        assert_eq!(
            kind("xoxb-123456789012-abcdefghij"),
            Some(SensitiveKind::ApiKey)
        );
        assert_eq!(
            kind(&format!("sk_live_{}", "x".repeat(24))),
            Some(SensitiveKind::ApiKey)
        );
        assert_eq!(
            kind("npm_abcdefghijklmnopqrstuvwxyz0123456789"),
            Some(SensitiveKind::ApiKey)
        );
        assert_eq!(
            kind("sk-ant-api03-AbCdEfGhIjKlMnOpQrSt"),
            Some(SensitiveKind::ApiKey)
        );
    }

    #[test]
    fn jwt_and_pem() {
        assert_eq!(
            kind(
                "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.dBjftJeZ4CVPmB92K27uhbUJU1p1r_wW1gFWFOEjXk"
            ),
            Some(SensitiveKind::Jwt)
        );
        assert_eq!(
            kind("-----BEGIN RSA PRIVATE KEY-----\nMIIE..."),
            Some(SensitiveKind::PrivateKey)
        );
        assert_eq!(
            kind("-----BEGIN PUBLIC KEY-----\nMIIB..."),
            None,
            "public keys are not secrets"
        );
    }

    #[test]
    fn payment_cards_need_luhn_and_a_plausible_prefix() {
        assert_eq!(
            kind("4111 1111 1111 1111"),
            Some(SensitiveKind::PaymentCard)
        );
        assert_eq!(
            kind("5500-0000-0000-0004"),
            Some(SensitiveKind::PaymentCard)
        );
        assert_eq!(kind("378282246310005"), Some(SensitiveKind::PaymentCard));
        assert_eq!(kind("4111 1111 1111 1112"), None, "fails Luhn");
        assert_eq!(
            kind("0000 0000 0000 0000"),
            None,
            "bad prefix and identical digits"
        );
        assert_eq!(kind("1234567890123"), None, "13 digits but prefix 1");
        assert_eq!(kind("4111 1111"), None, "too short");
    }

    #[test]
    fn random_tokens_are_caught() {
        assert_eq!(kind("q7G!x2Lp9#Zr4Wm8Kd5Ty"), Some(SensitiveKind::Token));
        assert_eq!(kind("Xk9$mP2vL8nQ4wR7"), Some(SensitiveKind::Token));
    }

    #[test]
    fn ordinary_developer_text_is_not_a_secret() {
        for s in [
            "getUserResetToken",
            "the quick brown fox jumps over the lazy dog",
            "550e8400-e29b-41d4-a716-446655440000",
            "https://example.com/Path/To/Page1?ref=Home",
            "https://github.com/rhshourav/zlip/pull/42",
            "C:\\Users\\Rhshourav\\Documents\\Report2024.pdf",
            "/usr/local/Share/App1/config.json",
            "rhshourav.dev2024@example.com",
            "da39a3ee5e6b4b0d3255bfef95601890afd80709",
            "short",
            "",
        ] {
            assert_eq!(kind(s), None, "false positive: {s:?}");
        }
    }

    #[test]
    fn urls_carrying_credentials_are_still_caught() {
        assert!(kind("https://admin:Zx9!kQ2mPw4Rt7@host.example.com/x").is_some());
        assert!(kind("https://api.example.com/v1/data?api_key=Zx9kQ2mPw4Rt7LpN3vB8").is_some());
        assert!(kind("https://x.com/cb?access_token=Zx9kQ2mPw4Rt7LpN3vB8cD5eF").is_some());
    }

    #[test]
    fn thresholds_come_from_config() {
        let mut strict = cfg();
        strict.min_token_len = 40;
        assert_eq!(detect_sensitive("Xk9$mP2vL8nQ4wR7", &strict), None);
        let mut lax = cfg();
        lax.min_entropy_bits = 6.0;
        assert_eq!(detect_sensitive("Xk9$mP2vL8nQ4wR7", &lax), None);
    }

    #[test]
    fn oversized_input_is_ignored_cheaply() {
        assert_eq!(kind(&"Aa1!".repeat(2000)), None);
    }

    #[test]
    fn masks_hide_everything_but_card_tail() {
        assert_eq!(
            mask(SensitiveKind::PaymentCard, "4111 1111 1111 1111"),
            "•••• •••• •••• 1111"
        );
        let m = mask(SensitiveKind::ApiKey, "AKIAIOSFODNN7EXAMPLE");
        assert_eq!(m, "API key • 20 characters");
        assert!(!m.contains("AKIA"));
        assert!(!mask(SensitiveKind::PrivateKey, "-----BEGIN PRIVATE KEY-----").contains("BEGIN"));
    }

    #[test]
    fn entropy_math_sanity() {
        assert!(shannon_bits_per_byte("aaaaaaaa") < 0.01);
        assert!((shannon_bits_per_byte("abcdefgh") - 3.0).abs() < 1e-9);
    }
}
