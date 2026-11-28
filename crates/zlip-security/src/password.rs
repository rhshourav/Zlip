//! Random passwords for the built-in generator.
//!
//! The random bytes come from the caller: the app passes the operating system's secure
//! generator, the tests pass a fixed sequence. Nothing here keeps or logs a password.

use zeroize::Zeroizing;
use zlip_config::{
    PASSWORD_MAX_LEN, PASSWORD_MIN_LEN, PasswordGenerator, PasswordMode, PatternError, PatternSlot,
    parse_password_pattern,
};

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
/// Symbols that nearly every site accepts in a password.
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.?";
/// Characters that are easy to confuse when read or typed by hand.
const LOOKALIKES: &str = "O0oIl1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PasswordError {
    #[error("turn on at least one kind of character in the password generator settings")]
    NoCharacters,
    #[error("the system could not provide random numbers")]
    NoRandomness,
    #[error("the password pattern cannot be used: {0}")]
    BadPattern(PatternError),
}

/// A random number in `0..n`, free of modulo bias: values from the unfair tail of the
/// range are thrown away and a fresh one is drawn.
fn below(n: usize, fill: &mut dyn FnMut(&mut [u8]) -> bool) -> Result<usize, PasswordError> {
    debug_assert!(n > 0 && n <= u32::MAX as usize);
    let n = n as u32;
    let limit = (u32::MAX / n) * n;
    // A real generator almost never rejects; the cap only stops a broken one from looping.
    for _ in 0..64 {
        let mut word = [0u8; 4];
        if !fill(&mut word) {
            return Err(PasswordError::NoRandomness);
        }
        let x = u32::from_le_bytes(word);
        if x < limit {
            return Ok((x % n) as usize);
        }
    }
    Err(PasswordError::NoRandomness)
}

fn pick(chars: &[u8], fill: &mut dyn FnMut(&mut [u8]) -> bool) -> Result<u8, PasswordError> {
    Ok(chars[below(chars.len(), fill)?])
}

/// The characters one kind contributes, minus the look-alikes when asked.
fn class(chars: &str, avoid_lookalikes: bool) -> Vec<u8> {
    chars
        .bytes()
        .filter(|b| !(avoid_lookalikes && LOOKALIKES.as_bytes().contains(b)))
        .collect()
}

/// Make a password that follows `rules`: a random mix, or the shape of the pattern,
/// depending on the mode.
pub fn generate_password(
    rules: &PasswordGenerator,
    fill: &mut dyn FnMut(&mut [u8]) -> bool,
) -> Result<Zeroizing<String>, PasswordError> {
    match rules.mode {
        PasswordMode::Random => random_password(rules, fill),
        PasswordMode::Pattern => patterned_password(rules, fill),
    }
}

/// Follow the pattern position by position. Each position is drawn on its own, evenly
/// from its kind, so there is nothing to shuffle and nothing is guaranteed beyond the
/// shape itself. The kinds and the length of random mode play no part here; only
/// `avoid_lookalikes` does.
fn patterned_password(
    rules: &PasswordGenerator,
    fill: &mut dyn FnMut(&mut [u8]) -> bool,
) -> Result<Zeroizing<String>, PasswordError> {
    let slots = parse_password_pattern(&rules.pattern).map_err(PasswordError::BadPattern)?;
    let avoid = rules.avoid_lookalikes;
    let (upper, lower) = (class(UPPER, avoid), class(LOWER, avoid));
    let (digits, symbols) = (class(DIGITS, avoid), class(SYMBOLS, avoid));
    // A position never writes more bytes than its pattern text, so this never grows (and
    // never leaves a copy of the password behind in a freed buffer).
    let mut out = Zeroizing::new(String::with_capacity(rules.pattern.len()));
    for slot in slots {
        match slot {
            PatternSlot::Capital => out.push(char::from(pick(&upper, fill)?)),
            PatternSlot::Small => out.push(char::from(pick(&lower, fill)?)),
            PatternSlot::Digit => out.push(char::from(pick(&digits, fill)?)),
            PatternSlot::Symbol => out.push(char::from(pick(&symbols, fill)?)),
            PatternSlot::Literal(c) => out.push(c),
        }
    }
    Ok(out)
}

/// Every kind of character that is turned on appears at least once, the rest is drawn
/// evenly from all of them, and the order is shuffled so the guaranteed characters do not
/// sit at the front. The length is held between [`PASSWORD_MIN_LEN`] and
/// [`PASSWORD_MAX_LEN`].
fn random_password(
    rules: &PasswordGenerator,
    fill: &mut dyn FnMut(&mut [u8]) -> bool,
) -> Result<Zeroizing<String>, PasswordError> {
    let kinds: Vec<Vec<u8>> = [
        (rules.lowercase, LOWER),
        (rules.uppercase, UPPER),
        (rules.digits, DIGITS),
        (rules.symbols, SYMBOLS),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, chars)| class(chars, rules.avoid_lookalikes))
    .filter(|chars| !chars.is_empty())
    .collect();
    if kinds.is_empty() {
        return Err(PasswordError::NoCharacters);
    }

    let length = rules.length.clamp(PASSWORD_MIN_LEN, PASSWORD_MAX_LEN) as usize;
    let all: Zeroizing<Vec<u8>> = Zeroizing::new(kinds.iter().flatten().copied().collect());

    let mut out: Zeroizing<Vec<u8>> = Zeroizing::new(Vec::with_capacity(length));
    for kind in &kinds {
        out.push(pick(kind, fill)?);
    }
    while out.len() < length {
        out.push(pick(&all, fill)?);
    }
    // Fisher-Yates: every arrangement is equally likely.
    for i in (1..out.len()).rev() {
        let j = below(i + 1, fill)?;
        out.swap(i, j);
    }
    // Only ASCII went in, so this cannot fail; the fallback keeps the function total.
    String::from_utf8(out.to_vec())
        .map(Zeroizing::new)
        .map_err(|_| PasswordError::NoCharacters)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small deterministic generator (xorshift64*), good enough to exercise the logic.
    struct Fake(u64);
    impl Fake {
        fn fill(&mut self, buf: &mut [u8]) -> bool {
            for b in buf {
                self.0 ^= self.0 >> 12;
                self.0 ^= self.0 << 25;
                self.0 ^= self.0 >> 27;
                *b = (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 56) as u8;
            }
            true
        }
    }

    fn make(rules: &PasswordGenerator, seed: u64) -> String {
        let mut f = Fake(seed | 1);
        generate_password(rules, &mut |b| f.fill(b))
            .expect("a password")
            .to_string()
    }

    fn rules() -> PasswordGenerator {
        PasswordGenerator::default()
    }

    #[test]
    fn has_exactly_the_requested_length() {
        for len in [8, 9, 20, 64, 128] {
            let r = PasswordGenerator {
                length: len,
                ..rules()
            };
            assert_eq!(make(&r, u64::from(len)).len(), len as usize);
        }
    }

    #[test]
    fn the_length_is_held_inside_the_allowed_range() {
        let short = PasswordGenerator {
            length: 1,
            ..rules()
        };
        let long = PasswordGenerator {
            length: 5000,
            ..rules()
        };
        assert_eq!(make(&short, 1).len(), PASSWORD_MIN_LEN as usize);
        assert_eq!(make(&long, 1).len(), PASSWORD_MAX_LEN as usize);
    }

    #[test]
    fn every_kind_that_is_on_appears_every_time() {
        let r = PasswordGenerator {
            length: 8,
            ..rules()
        };
        for seed in 1..400 {
            let p = make(&r, seed);
            assert!(p.bytes().any(|b| b.is_ascii_lowercase()), "{p}");
            assert!(p.bytes().any(|b| b.is_ascii_uppercase()), "{p}");
            assert!(p.bytes().any(|b| b.is_ascii_digit()), "{p}");
            assert!(p.bytes().any(|b| SYMBOLS.as_bytes().contains(&b)), "{p}");
        }
    }

    #[test]
    fn only_capitals() {
        let r = PasswordGenerator {
            lowercase: false,
            digits: false,
            symbols: false,
            ..rules()
        };
        for seed in 1..50 {
            assert!(make(&r, seed).bytes().all(|b| b.is_ascii_uppercase()));
        }
    }

    #[test]
    fn only_small_letters() {
        let r = PasswordGenerator {
            uppercase: false,
            digits: false,
            symbols: false,
            ..rules()
        };
        for seed in 1..50 {
            assert!(make(&r, seed).bytes().all(|b| b.is_ascii_lowercase()));
        }
    }

    #[test]
    fn symbols_only_appear_when_they_are_on() {
        let off = PasswordGenerator {
            symbols: false,
            ..rules()
        };
        for seed in 1..100 {
            assert!(make(&off, seed).bytes().all(|b| b.is_ascii_alphanumeric()));
        }
        let on = rules();
        assert!((1..100).any(|seed| make(&on, seed).bytes().any(|b| !b.is_ascii_alphanumeric())));
    }

    #[test]
    fn look_alikes_are_left_out_when_asked() {
        let r = PasswordGenerator {
            avoid_lookalikes: true,
            length: 128,
            ..rules()
        };
        for seed in 1..100 {
            let p = make(&r, seed);
            assert!(!p.contains(|c| LOOKALIKES.contains(c)), "{p}");
        }
        let normal = PasswordGenerator {
            length: 128,
            ..rules()
        };
        assert!((1..100).any(|seed| make(&normal, seed).contains(|c| LOOKALIKES.contains(c))));
    }

    #[test]
    fn every_character_of_every_kind_can_come_up() {
        let r = PasswordGenerator {
            length: 128,
            ..rules()
        };
        let mut seen = std::collections::HashSet::new();
        for seed in 1..200 {
            seen.extend(make(&r, seed).bytes());
        }
        let expected = LOWER.len() + UPPER.len() + DIGITS.len() + SYMBOLS.len();
        assert_eq!(seen.len(), expected);
    }

    #[test]
    fn the_counts_match_the_strength_estimate_on_the_settings_screen() {
        // src/lib/password.ts works out "how strong" from these numbers.
        assert_eq!(class(LOWER, false).len(), 26);
        assert_eq!(class(UPPER, false).len(), 26);
        assert_eq!(class(DIGITS, false).len(), 10);
        assert_eq!(class(SYMBOLS, false).len(), 23);
        assert_eq!(class(LOWER, true).len(), 24);
        assert_eq!(class(UPPER, true).len(), 24);
        assert_eq!(class(DIGITS, true).len(), 8);
        assert_eq!(class(SYMBOLS, true).len(), 23);
    }

    fn shaped(pattern: &str, avoid_lookalikes: bool, seed: u64) -> String {
        let r = PasswordGenerator {
            mode: PasswordMode::Pattern,
            pattern: pattern.to_owned(),
            avoid_lookalikes,
            ..rules()
        };
        make(&r, seed)
    }

    #[test]
    fn two_capitals_then_six_numbers() {
        // The shape the user asked for: AA123456.
        for seed in 1..300 {
            let p = shaped("LL######", false, seed);
            let b = p.as_bytes();
            assert_eq!(b.len(), 8, "{p}");
            assert!(b[..2].iter().all(u8::is_ascii_uppercase), "{p}");
            assert!(b[2..].iter().all(u8::is_ascii_digit), "{p}");
        }
    }

    #[test]
    fn every_placeholder_draws_from_its_own_kind() {
        for seed in 1..200 {
            let p = shaped("Ll#*", false, seed);
            let b = p.as_bytes();
            assert!(b[0].is_ascii_uppercase(), "{p}");
            assert!(b[1].is_ascii_lowercase(), "{p}");
            assert!(b[2].is_ascii_digit(), "{p}");
            assert!(SYMBOLS.as_bytes().contains(&b[3]), "{p}");
        }
    }

    #[test]
    fn other_characters_in_a_pattern_stay_where_they_are() {
        for seed in 1..50 {
            let p = shaped("ZQ-####-é", false, seed);
            assert!(p.starts_with("ZQ-") && p.ends_with("-é"), "{p}");
            assert_eq!(p.chars().count(), 9);
            assert!(p[3..7].bytes().all(|b| b.is_ascii_digit()), "{p}");
        }
        assert_eq!(
            shaped(r"\L\#\*#", false, 3)
                .chars()
                .take(3)
                .collect::<String>(),
            "L#*"
        );
    }

    #[test]
    fn a_pattern_ignores_the_kinds_and_the_length_of_random_mode() {
        let r = PasswordGenerator {
            mode: PasswordMode::Pattern,
            pattern: "LL######".to_owned(),
            length: 64,
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: false,
            ..rules()
        };
        let p = make(&r, 9);
        assert_eq!(p.len(), 8);
        assert!(p[..2].bytes().all(|b| b.is_ascii_uppercase()), "{p}");
    }

    #[test]
    fn look_alikes_are_left_out_of_a_pattern_too_but_typed_characters_are_not() {
        for seed in 1..200 {
            let p = shaped("O1-LlL#####", true, seed);
            assert!(p.starts_with("O1-"), "{p}");
            assert!(!p[3..].contains(|c| LOOKALIKES.contains(c)), "{p}");
        }
        assert!(
            (1..200)
                .any(|seed| shaped("LlL#####", false, seed).contains(|c| LOOKALIKES.contains(c)))
        );
    }

    #[test]
    fn every_digit_and_capital_can_come_up_in_a_pattern() {
        let mut seen = std::collections::HashSet::new();
        for seed in 1..200 {
            seen.extend(shaped(&"L#".repeat(64), false, seed).bytes());
        }
        assert_eq!(seen.len(), UPPER.len() + DIGITS.len());
    }

    #[test]
    fn patterns_make_different_passwords_each_time() {
        assert_ne!(shaped("LL######", false, 1), shaped("LL######", false, 2));
    }

    #[test]
    fn a_pattern_that_cannot_be_used_is_an_error() {
        for (pattern, why) in [
            ("", PatternError::Empty),
            ("AA123456", PatternError::NoPlaceholder),
            ("#\\", PatternError::TrailingBackslash),
        ] {
            let r = PasswordGenerator {
                mode: PasswordMode::Pattern,
                pattern: pattern.to_owned(),
                ..rules()
            };
            let got = generate_password(&r, &mut |_| true);
            assert_eq!(
                got.unwrap_err(),
                PasswordError::BadPattern(why),
                "{pattern:?}"
            );
        }
    }

    #[test]
    fn a_failing_random_source_stops_a_pattern_too() {
        let r = PasswordGenerator {
            mode: PasswordMode::Pattern,
            ..rules()
        };
        let got = generate_password(&r, &mut |_| false);
        assert_eq!(got.unwrap_err(), PasswordError::NoRandomness);
    }

    #[test]
    fn two_passwords_differ() {
        assert_ne!(make(&rules(), 1), make(&rules(), 2));
    }

    #[test]
    fn nothing_turned_on_is_an_error() {
        let r = PasswordGenerator {
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: false,
            ..rules()
        };
        let got = generate_password(&r, &mut |_| true);
        assert_eq!(got.unwrap_err(), PasswordError::NoCharacters);
    }

    #[test]
    fn a_failing_random_source_is_an_error_not_a_weak_password() {
        let got = generate_password(&rules(), &mut |_| false);
        assert_eq!(got.unwrap_err(), PasswordError::NoRandomness);
        // A source that only ever returns the unusable tail value must not loop forever.
        let got = generate_password(&rules(), &mut |b| {
            b.fill(0xFF);
            true
        });
        assert_eq!(got.unwrap_err(), PasswordError::NoRandomness);
    }

    #[test]
    fn rejection_sampling_removes_the_modulo_bias() {
        // 0xFFFF_FFFF is in the unfair tail for n = 3, so it must be redrawn; 7 % 3 = 1.
        let mut words = vec![[0xFF, 0xFF, 0xFF, 0xFF], [7, 0, 0, 0]].into_iter();
        let n = below(3, &mut |b| {
            b.copy_from_slice(&words.next().expect("a word"));
            true
        });
        assert_eq!(n, Ok(1));
    }

    #[test]
    fn choices_are_spread_evenly() {
        // 6000 draws over 6 values: each should land near 1000.
        let mut f = Fake(0x9E37_79B9_7F4A_7C15);
        let mut counts = [0u32; 6];
        for _ in 0..6000 {
            counts[below(6, &mut |b| f.fill(b)).expect("a number")] += 1;
        }
        for c in counts {
            assert!((850..=1150).contains(&c), "{counts:?}");
        }
    }
}
