//! Platform-neutral keyboard chords: "Ctrl+Shift+Q", "Win+V", "Alt+{1..9}".
//! The Windows crate maps [`Key`] to virtual-key codes; the UI parses the same strings.

use std::{fmt, str::FromStr};

/// Modifier bit set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Self = Self(0);
    pub const CTRL: Self = Self(1);
    pub const ALT: Self = Self(2);
    pub const SHIFT: Self = Self(4);
    pub const WIN: Self = Self(8);

    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    #[must_use]
    pub fn bits(self) -> u8 {
        self.0
    }
    #[must_use]
    pub fn from_bits(bits: u8) -> Self {
        Self(bits & 0b1111)
    }
}

impl std::ops::BitOr for Mods {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// A non-modifier key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Letter(char), // 'A'..='Z'
    Digit(u8),    // 0..=9
    F(u8),        // 1..=24
    Enter,
    Tab,
    Space,
    Escape,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ParseError {}

impl FromStr for Key {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, ParseError> {
        let lower = s.to_ascii_lowercase();
        let named = match lower.as_str() {
            "enter" | "return" => Some(Self::Enter),
            "tab" => Some(Self::Tab),
            "space" => Some(Self::Space),
            "escape" | "esc" => Some(Self::Escape),
            "backspace" => Some(Self::Backspace),
            "delete" | "del" => Some(Self::Delete),
            "insert" | "ins" => Some(Self::Insert),
            "home" => Some(Self::Home),
            "end" => Some(Self::End),
            "pageup" | "pgup" => Some(Self::PageUp),
            "pagedown" | "pgdn" => Some(Self::PageDown),
            "up" | "arrowup" => Some(Self::Up),
            "down" | "arrowdown" => Some(Self::Down),
            "left" | "arrowleft" => Some(Self::Left),
            "right" | "arrowright" => Some(Self::Right),
            _ => None,
        };
        if let Some(k) = named {
            return Ok(k);
        }
        let mut chars = s.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            if c.is_ascii_alphabetic() {
                return Ok(Self::Letter(c.to_ascii_uppercase()));
            }
            if let Some(d) = c.to_digit(10) {
                return Ok(Self::Digit(d as u8));
            }
        }
        if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
            && (1..=24).contains(&n)
        {
            return Ok(Self::F(n));
        }
        Err(ParseError(format!("unknown key `{s}`")))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Letter(c) => write!(f, "{c}"),
            Self::Digit(d) => write!(f, "{d}"),
            Self::F(n) => write!(f, "F{n}"),
            Self::Enter => f.write_str("Enter"),
            Self::Tab => f.write_str("Tab"),
            Self::Space => f.write_str("Space"),
            Self::Escape => f.write_str("Escape"),
            Self::Backspace => f.write_str("Backspace"),
            Self::Delete => f.write_str("Delete"),
            Self::Insert => f.write_str("Insert"),
            Self::Home => f.write_str("Home"),
            Self::End => f.write_str("End"),
            Self::PageUp => f.write_str("PageUp"),
            Self::PageDown => f.write_str("PageDown"),
            Self::Up => f.write_str("Up"),
            Self::Down => f.write_str("Down"),
            Self::Left => f.write_str("Left"),
            Self::Right => f.write_str("Right"),
        }
    }
}

/// Modifiers plus one key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    pub mods: Mods,
    pub key: Key,
}

fn parse_mod(part: &str) -> Option<Mods> {
    match part.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(Mods::CTRL),
        "alt" | "option" => Some(Mods::ALT),
        "shift" => Some(Mods::SHIFT),
        "win" | "super" | "meta" | "cmd" => Some(Mods::WIN),
        _ => None,
    }
}

/// Split `a+b+c` into modifiers and the final key text. `+` itself cannot be a key.
fn split(s: &str) -> Result<(Mods, &str), ParseError> {
    let parts: Vec<&str> = s.split('+').map(str::trim).collect();
    let (key, mods) = parts
        .split_last()
        .ok_or_else(|| ParseError("empty shortcut".into()))?;
    if key.is_empty() {
        return Err(ParseError(format!("`{s}` has no key")));
    }
    let mut all = Mods::NONE;
    for m in mods {
        let bit =
            parse_mod(m).ok_or_else(|| ParseError(format!("unknown modifier `{m}` in `{s}`")))?;
        if all.contains(bit) {
            return Err(ParseError(format!("modifier `{m}` repeated in `{s}`")));
        }
        all = all | bit;
    }
    Ok((all, key))
}

impl FromStr for Chord {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, ParseError> {
        let (mods, key) = split(s)?;
        Ok(Self {
            mods,
            key: key.parse()?,
        })
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (bit, name) in [
            (Mods::WIN, "Win"),
            (Mods::CTRL, "Ctrl"),
            (Mods::ALT, "Alt"),
            (Mods::SHIFT, "Shift"),
        ] {
            if self.mods.contains(bit) {
                write!(f, "{name}+")?;
            }
        }
        write!(f, "{}", self.key)
    }
}

/// `Alt+{1..9}`: a modifier set applied to the digits 1 to 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuickPaste {
    pub mods: Mods,
}

impl FromStr for QuickPaste {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, ParseError> {
        let (mods, key) = split(s)?;
        if key != "{1..9}" {
            return Err(ParseError(format!(
                "`{s}` must end in {{1..9}}, for example Alt+{{1..9}}"
            )));
        }
        if mods.is_empty() {
            return Err(ParseError(
                "quick paste needs a modifier, or digits would clash with typing in search".into(),
            ));
        }
        Ok(Self { mods })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_chords() {
        let c: Chord = "Win+V".parse().unwrap();
        assert_eq!((c.mods, c.key), (Mods::WIN, Key::Letter('V')));
        let c: Chord = "ctrl + shift + q".parse().unwrap();
        assert_eq!(c.mods, Mods::CTRL | Mods::SHIFT);
        assert_eq!("Esc".parse::<Chord>().unwrap().key, Key::Escape);
        assert_eq!("Alt+5".parse::<Chord>().unwrap().key, Key::Digit(5));
        assert_eq!("Ctrl+F12".parse::<Chord>().unwrap().key, Key::F(12));
    }

    #[test]
    fn display_is_canonical_and_reparses() {
        for s in [
            "Win+V",
            "Ctrl+Shift+Q",
            "Alt+Left",
            "Delete",
            "Ctrl+Alt+Shift+Win+F5",
        ] {
            let c: Chord = s.parse().unwrap();
            assert_eq!(c.to_string().parse::<Chord>().unwrap(), c);
        }
        assert_eq!(
            "shift+ctrl+q".parse::<Chord>().unwrap().to_string(),
            "Ctrl+Shift+Q"
        );
    }

    #[test]
    fn rejects_garbage_with_a_useful_message() {
        for bad in [
            "",
            "Ctrl+",
            "Hyper+V",
            "Ctrl+Ctrl+V",
            "Ctrl+F99",
            "Ctrl+Nope",
            "Ctrl+Shift",
        ] {
            assert!(bad.parse::<Chord>().is_err(), "{bad:?}");
        }
        assert!("Hyper+V".parse::<Chord>().unwrap_err().0.contains("Hyper"));
    }

    #[test]
    fn quick_paste_pattern() {
        assert_eq!("Alt+{1..9}".parse::<QuickPaste>().unwrap().mods, Mods::ALT);
        assert!(
            "{1..9}".parse::<QuickPaste>().is_err(),
            "bare digits clash with typing"
        );
        assert!("Alt+5".parse::<QuickPaste>().is_err());
    }
}
