//! Chord encoding and matching for the keyboard hook. Pure integer logic, so it
//! is cheap enough to run inside the hook callback and testable on any OS.

use zlip_config::hotkey::{Chord, Key, Mods};

pub const VK_SHIFT: u32 = 0x10;
pub const VK_CONTROL: u32 = 0x11;
pub const VK_MENU: u32 = 0x12; // Alt
pub const VK_LWIN: u32 = 0x5B;
pub const VK_RWIN: u32 = 0x5C;
pub const VK_V: u32 = 0x56;
pub const VK_LEFT: u32 = 0x25;
/// "Unassigned" virtual key, injected to tell Windows the Win key was used as a
/// modifier so Start does not open when it is released.
pub const VK_MASK: u32 = 0xE8;

/// Windows virtual-key code for a platform-neutral key.
#[must_use]
pub fn vk_for(key: Key) -> u32 {
    match key {
        Key::Letter(c) => u32::from(c.to_ascii_uppercase() as u8),
        Key::Digit(d) => 0x30 + u32::from(d),
        Key::F(n) => 0x70 + u32::from(n) - 1,
        Key::Enter => 0x0D,
        Key::Tab => 0x09,
        Key::Space => 0x20,
        Key::Escape => 0x1B,
        Key::Backspace => 0x08,
        Key::Delete => 0x2E,
        Key::Insert => 0x2D,
        Key::Home => 0x24,
        Key::End => 0x23,
        Key::PageUp => 0x21,
        Key::PageDown => 0x22,
        Key::Left => 0x25,
        Key::Up => 0x26,
        Key::Right => 0x27,
        Key::Down => 0x28,
    }
}

/// `(modifier bits << 16) | virtual key`. Zero means "slot disabled" (no real
/// chord encodes to zero because every VK used here is non-zero).
#[must_use]
pub fn encode(chord: Chord) -> u32 {
    (u32::from(chord.mods.bits()) << 16) | vk_for(chord.key)
}

/// The Win32 `MOD_*` flags (`RegisterHotKey`) for a modifier bit set. Zlip's bits
/// are Ctrl 1, Alt 2, Shift 4, Win 8; Windows uses Alt 1, Ctrl 2, Shift 4, Win 8.
#[must_use]
pub fn win_hotkey_mods(bits: u8) -> u32 {
    let m = Mods::from_bits(bits);
    let mut out = 0;
    if m.contains(Mods::CTRL) {
        out |= 0x0002; // MOD_CONTROL
    }
    if m.contains(Mods::ALT) {
        out |= 0x0001; // MOD_ALT
    }
    if m.contains(Mods::SHIFT) {
        out |= 0x0004; // MOD_SHIFT
    }
    if m.contains(Mods::WIN) {
        out |= 0x0008; // MOD_WIN
    }
    out
}

/// Exact match: the pressed modifiers must equal the chord's, so `Ctrl+Shift+Q`
/// does not fire on `Ctrl+Q` and `Win+V` does not fire on `Win+Shift+V`.
#[must_use]
pub fn matches(encoded: u32, pressed: Mods, vk: u32) -> bool {
    encoded != 0 && encoded & 0xFFFF == vk && (encoded >> 16) as u8 == pressed.bits()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(s: &str) -> Chord {
        s.parse().unwrap()
    }

    #[test]
    fn virtual_key_codes_match_the_windows_headers() {
        assert_eq!(vk_for(Key::Letter('V')), 0x56);
        assert_eq!(vk_for(Key::Digit(0)), 0x30);
        assert_eq!(vk_for(Key::Digit(9)), 0x39);
        assert_eq!(vk_for(Key::F(1)), 0x70);
        assert_eq!(vk_for(Key::F(12)), 0x7B);
        assert_eq!(vk_for(Key::Enter), 0x0D);
        assert_eq!(vk_for(Key::Escape), 0x1B);
        assert_eq!(vk_for(Key::Delete), 0x2E);
        assert_eq!(VK_V, vk_for(Key::Letter('V')));
        assert_eq!(VK_LEFT, vk_for(Key::Left));
    }

    #[test]
    fn matching_is_exact_on_modifiers() {
        let win_v = encode(chord("Win+V"));
        assert!(matches(win_v, Mods::WIN, VK_V));
        assert!(!matches(win_v, Mods::WIN | Mods::SHIFT, VK_V));
        assert!(!matches(win_v, Mods::NONE, VK_V));
        assert!(!matches(win_v, Mods::WIN, 0x42));
        let q = encode(chord("Ctrl+Shift+Q"));
        assert!(matches(q, Mods::CTRL | Mods::SHIFT, 0x51));
        assert!(!matches(q, Mods::CTRL, 0x51));
    }

    #[test]
    fn modifier_bits_map_to_the_win32_flags() {
        assert_eq!(win_hotkey_mods(Mods::CTRL.bits()), 0x0002);
        assert_eq!(win_hotkey_mods(Mods::ALT.bits()), 0x0001);
        assert_eq!(
            win_hotkey_mods((Mods::CTRL | Mods::ALT).bits()),
            0x0002 | 0x0001
        );
        assert_eq!(win_hotkey_mods((Mods::SHIFT | Mods::WIN).bits()), 0x000C);
        assert_eq!(win_hotkey_mods(Mods::NONE.bits()), 0);
    }

    #[test]
    fn a_disabled_slot_never_matches() {
        assert!(!matches(0, Mods::NONE, 0));
    }

    #[test]
    fn encoding_round_trips_the_pieces() {
        let e = encode(chord("Ctrl+Alt+F5"));
        assert_eq!(e & 0xFFFF, 0x74);
        assert_eq!((e >> 16) as u8, (Mods::CTRL | Mods::ALT).bits());
    }
}
