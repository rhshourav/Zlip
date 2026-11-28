//! The shape a pattern-mode password follows: `LL######` makes two capitals and six digits.
//!
//! The rules live here, in one place, so the settings check and the generator can never
//! disagree about what a pattern means. `src/lib/password.ts` mirrors them to preview the
//! strength while the pattern is typed; both are tested against the same examples.
//!
//! | write | you get                                                  |
//! |-------|----------------------------------------------------------|
//! | `L`   | a capital letter                                         |
//! | `l`   | a small letter                                           |
//! | `#`   | a number                                                 |
//! | `*`   | a symbol                                                 |
//! | `\x`  | the character `x` as typed (`\#` is a `#`, `\\` a `\`)   |
//! | other | itself, so `ZQ-####` keeps the `ZQ-`                     |

use crate::model::PASSWORD_MAX_LEN;

/// Most characters a pattern may be written with (escapes included).
pub const PASSWORD_PATTERN_MAX_RAW: usize = 256;

/// One position of a generated password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternSlot {
    /// `L`: a capital letter.
    Capital,
    /// `l`: a small letter.
    Small,
    /// `#`: a number.
    Digit,
    /// `*`: a symbol.
    Symbol,
    /// Any other character, kept exactly as written.
    Literal(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PatternError {
    #[error("the pattern is empty")]
    Empty,
    #[error(
        "the pattern needs at least one L (capital), l (small letter), # (number) or * (symbol)"
    )]
    NoPlaceholder,
    #[error("the pattern is too long: a password can have at most {PASSWORD_MAX_LEN} characters")]
    TooLong,
    #[error("the pattern ends with a backslash: write \\\\ for a backslash itself")]
    TrailingBackslash,
    #[error("the pattern contains a character that cannot be typed into a password")]
    ControlCharacter,
}

/// Read a pattern into its positions, or say what is wrong with it.
pub fn parse_password_pattern(pattern: &str) -> Result<Vec<PatternSlot>, PatternError> {
    if pattern.is_empty() {
        return Err(PatternError::Empty);
    }
    if pattern.chars().count() > PASSWORD_PATTERN_MAX_RAW {
        return Err(PatternError::TooLong);
    }
    let literal = |c: char| {
        if c.is_control() {
            Err(PatternError::ControlCharacter)
        } else {
            Ok(PatternSlot::Literal(c))
        }
    };
    let mut slots = Vec::new();
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        slots.push(match c {
            'L' => PatternSlot::Capital,
            'l' => PatternSlot::Small,
            '#' => PatternSlot::Digit,
            '*' => PatternSlot::Symbol,
            '\\' => literal(chars.next().ok_or(PatternError::TrailingBackslash)?)?,
            other => literal(other)?,
        });
    }
    if slots.len() > PASSWORD_MAX_LEN as usize {
        return Err(PatternError::TooLong);
    }
    if slots.iter().all(|s| matches!(s, PatternSlot::Literal(_))) {
        return Err(PatternError::NoPlaceholder);
    }
    Ok(slots)
}

#[cfg(test)]
mod tests {
    use super::PatternSlot::{Capital, Digit, Literal, Small, Symbol};
    use super::*;

    #[test]
    fn two_capitals_and_six_numbers() {
        assert_eq!(
            parse_password_pattern("LL######"),
            Ok(vec![
                Capital, Capital, Digit, Digit, Digit, Digit, Digit, Digit
            ])
        );
    }

    #[test]
    fn every_placeholder_means_one_kind() {
        assert_eq!(
            parse_password_pattern("Ll#*"),
            Ok(vec![Capital, Small, Digit, Symbol])
        );
    }

    #[test]
    fn other_characters_are_kept_as_typed() {
        assert_eq!(
            parse_password_pattern("ZQ-#"),
            Ok(vec![Literal('Z'), Literal('Q'), Literal('-'), Digit])
        );
        assert_eq!(
            parse_password_pattern("A1 #"),
            Ok(vec![Literal('A'), Literal('1'), Literal(' '), Digit])
        );
        assert_eq!(parse_password_pattern("é#"), Ok(vec![Literal('é'), Digit]));
    }

    #[test]
    fn a_backslash_keeps_the_next_character_as_typed() {
        assert_eq!(
            parse_password_pattern(r"\L\#\*\\#"),
            Ok(vec![
                Literal('L'),
                Literal('#'),
                Literal('*'),
                Literal('\\'),
                Digit
            ])
        );
        assert_eq!(
            parse_password_pattern(r"#\"),
            Err(PatternError::TrailingBackslash)
        );
    }

    #[test]
    fn a_pattern_without_a_placeholder_is_refused() {
        // "AA123456" is a fixed word, not a pattern: the user wants LL######.
        for p in ["AA123456", r"\L\#", "-"] {
            assert_eq!(
                parse_password_pattern(p),
                Err(PatternError::NoPlaceholder),
                "{p}"
            );
        }
    }

    #[test]
    fn empty_and_untypeable_patterns_are_refused() {
        assert_eq!(parse_password_pattern(""), Err(PatternError::Empty));
        assert_eq!(
            parse_password_pattern("#\n#"),
            Err(PatternError::ControlCharacter)
        );
        assert_eq!(
            parse_password_pattern("#\t"),
            Err(PatternError::ControlCharacter)
        );
        assert_eq!(
            parse_password_pattern("\\\u{7}"),
            Err(PatternError::ControlCharacter)
        );
    }

    #[test]
    fn the_longest_password_is_128_characters() {
        assert_eq!(parse_password_pattern(&"#".repeat(128)).unwrap().len(), 128);
        assert_eq!(
            parse_password_pattern(&"#".repeat(129)),
            Err(PatternError::TooLong)
        );
        // An escape counts once: 128 positions written with 255 characters are still fine.
        assert_eq!(
            parse_password_pattern(&format!("{}#", r"\-".repeat(127)))
                .unwrap()
                .len(),
            128
        );
        assert_eq!(
            parse_password_pattern(&"#".repeat(PASSWORD_PATTERN_MAX_RAW + 1)),
            Err(PatternError::TooLong)
        );
    }
}
