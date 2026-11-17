// Parses the same shortcut strings the Rust side validates ("Ctrl+Shift+Q", "Alt+{1..9}").

export interface Chord {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
  /** Canonical key: 'A'-'Z', '0'-'9', 'F1'.., 'Enter', 'Tab', 'Escape', 'Delete', 'Left', ... */
  key: string;
}

const KEY_ALIASES: Record<string, string> = {
  esc: 'Escape',
  escape: 'Escape',
  del: 'Delete',
  delete: 'Delete',
  return: 'Enter',
  enter: 'Enter',
  tab: 'Tab',
  space: 'Space',
  backspace: 'Backspace',
  insert: 'Insert',
  ins: 'Insert',
  home: 'Home',
  end: 'End',
  pageup: 'PageUp',
  pgup: 'PageUp',
  pagedown: 'PageDown',
  pgdn: 'PageDown',
  up: 'Up',
  arrowup: 'Up',
  down: 'Down',
  arrowdown: 'Down',
  left: 'Left',
  arrowleft: 'Left',
  right: 'Right',
  arrowright: 'Right'
};

function canonicalKey(raw: string): string | null {
  const lower = raw.toLowerCase();
  const named = KEY_ALIASES[lower];
  if (named) return named;
  if (/^[a-z]$/.test(lower)) return lower.toUpperCase();
  if (/^[0-9]$/.test(lower)) return lower;
  if (/^f([1-9]|1[0-9]|2[0-4])$/.test(lower)) return lower.toUpperCase();
  return null;
}

interface Mods {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
}

function splitMods(text: string): { mods: Mods; key: string } | null {
  const parts = text.split('+').map((p) => p.trim());
  const key = parts.pop();
  if (!key) return null;
  const mods: Mods = { ctrl: false, alt: false, shift: false, meta: false };
  for (const p of parts) {
    switch (p.toLowerCase()) {
      case 'ctrl':
      case 'control':
        mods.ctrl = true;
        break;
      case 'alt':
      case 'option':
        mods.alt = true;
        break;
      case 'shift':
        mods.shift = true;
        break;
      case 'win':
      case 'super':
      case 'meta':
      case 'cmd':
        mods.meta = true;
        break;
      default:
        return null;
    }
  }
  return { mods, key };
}

export function parseChord(text: string): Chord | null {
  const s = splitMods(text);
  if (!s) return null;
  const key = canonicalKey(s.key);
  return key ? { ...s.mods, key } : null;
}

/** "Alt+{1..9}": the modifier set that turns the digit keys into quick-paste. */
export function parseQuickPaste(text: string): Mods | null {
  const s = splitMods(text);
  return s && s.key === '{1..9}' ? s.mods : null;
}

/** Key of an event in the same canonical form as [`Chord.key`]. */
export function eventKey(e: Pick<KeyboardEvent, 'key' | 'code'>): string | null {
  // With Alt held, `key` can be a composed character; the physical key is reliable.
  const digit = /^(?:Digit|Numpad)([0-9])$/.exec(e.code);
  if (digit && digit[1]) return digit[1];
  const letter = /^Key([A-Z])$/.exec(e.code);
  if (letter && letter[1] && e.key.length !== 1) return letter[1];
  return canonicalKey(e.key === ' ' ? 'space' : e.key);
}

export function matchesChord(e: KeyboardEvent, c: Chord): boolean {
  return (
    e.ctrlKey === c.ctrl && e.altKey === c.alt && e.shiftKey === c.shift && e.metaKey === c.meta && eventKey(e) === c.key
  );
}

/** 1 to 9 when the event is the quick-paste modifier plus a digit, else null. */
export function quickPasteIndex(e: KeyboardEvent, mods: Mods): number | null {
  if (e.ctrlKey !== mods.ctrl || e.altKey !== mods.alt || e.shiftKey !== mods.shift || e.metaKey !== mods.meta) {
    return null;
  }
  const k = eventKey(e);
  return k && /^[1-9]$/.test(k) ? Number(k) : null;
}

const SYMBOL: Record<string, string> = {
  Enter: '↵',
  Escape: 'Esc',
  Left: '←',
  Right: '→',
  Up: '↑',
  Down: '↓',
  Delete: 'Del',
  Backspace: '⌫',
  Space: 'Space'
};

/** Keycap labels for the hint footer: "Ctrl+Shift+Q" becomes ['Ctrl', 'Shift', 'Q']. */
export function chordCaps(text: string): string[] {
  const s = splitMods(text);
  if (!s) return [text];
  const caps: string[] = [];
  if (s.mods.meta) caps.push('Win');
  if (s.mods.ctrl) caps.push('Ctrl');
  if (s.mods.alt) caps.push('Alt');
  if (s.mods.shift) caps.push('Shift');
  const key = s.key === '{1..9}' ? '1–9' : (canonicalKey(s.key) ?? s.key);
  caps.push(SYMBOL[key] ?? key);
  return caps;
}
