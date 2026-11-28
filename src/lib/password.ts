// Small helpers for showing a generated password. The password itself is made in Rust
// (crates/zlip-security/src/password.rs); the character counts below must match it, and so
// must the pattern rules, which live in crates/zlip-config/src/password_pattern.rs.

import type { PasswordGeneratorSettings } from './types';

export const PASSWORD_MIN_LEN = 8;
export const PASSWORD_MAX_LEN = 128;
/** Most characters a pattern may be written with (escapes included). */
export const PASSWORD_PATTERN_MAX_RAW = 256;
/** The pattern a fresh install starts with: two capitals, then six numbers (AA123456). */
export const DEFAULT_PATTERN = 'LL######';

const LOWER = 26;
const UPPER = 26;
const DIGITS = 10;
const SYMBOLS = 23;
/** O 0 o I l 1: two of each letter pair and of the digits go when look-alikes are left out. */
const LOOKALIKES_PER_KIND = 2;

/** How many different characters the generator draws from with these settings (random mode). */
export function poolSize(g: PasswordGeneratorSettings): number {
  const drop = g.avoid_lookalikes ? LOOKALIKES_PER_KIND : 0;
  return (g.lowercase ? LOWER - drop : 0) + (g.uppercase ? UPPER - drop : 0) + (g.digits ? DIGITS - drop : 0) + (g.symbols ? SYMBOLS : 0);
}

// ---- patterns: `LL######` is two capitals and six numbers ----

export type SlotKind = 'capital' | 'small' | 'digit' | 'symbol';
export type PatternSlot = { kind: SlotKind } | { kind: 'literal'; ch: string };
export type PatternParse = { ok: true; slots: PatternSlot[] } | { ok: false; error: string };

const PLACEHOLDER: Record<string, SlotKind> = { L: 'capital', l: 'small', '#': 'digit', '*': 'symbol' };

export const PATTERN_ERRORS = {
  empty: 'The pattern is empty.',
  noPlaceholder: 'Add at least one L (capital), l (small letter), # (number) or * (symbol).',
  tooLong: `A password can have at most ${PASSWORD_MAX_LEN} characters.`,
  trailingBackslash: 'A backslash must be followed by the character to keep.',
  control: 'That character cannot be typed into a password.'
} as const;

/** Read a pattern into its positions, or say what is wrong with it. Same rules as the Rust parser. */
export function parsePattern(pattern: string): PatternParse {
  const chars = [...pattern]; // by code point, like Rust's chars()
  if (chars.length === 0) return { ok: false, error: PATTERN_ERRORS.empty };
  if (chars.length > PASSWORD_PATTERN_MAX_RAW) return { ok: false, error: PATTERN_ERRORS.tooLong };
  const slots: PatternSlot[] = [];
  for (let i = 0; i < chars.length; i++) {
    let ch = chars[i]!;
    let typed = false;
    if (ch === '\\') {
      const next = chars[++i];
      if (next === undefined) return { ok: false, error: PATTERN_ERRORS.trailingBackslash };
      ch = next;
      typed = true;
    }
    const kind = typed ? undefined : PLACEHOLDER[ch];
    if (kind) {
      slots.push({ kind });
      continue;
    }
    if (/\p{Cc}/u.test(ch)) return { ok: false, error: PATTERN_ERRORS.control };
    slots.push({ kind: 'literal', ch });
  }
  if (slots.length > PASSWORD_MAX_LEN) return { ok: false, error: PATTERN_ERRORS.tooLong };
  if (slots.every((s) => s.kind === 'literal')) return { ok: false, error: PATTERN_ERRORS.noPlaceholder };
  return { ok: true, slots };
}

function slotPool(kind: SlotKind, avoidLookalikes: boolean): number {
  const drop = avoidLookalikes ? LOOKALIKES_PER_KIND : 0;
  return kind === 'capital' ? UPPER - drop : kind === 'small' ? LOWER - drop : kind === 'digit' ? DIGITS - drop : SYMBOLS;
}

const SLOT_NAME: Record<PatternSlot['kind'], [one: string, many: string]> = {
  capital: ['capital', 'capitals'],
  small: ['small letter', 'small letters'],
  digit: ['number', 'numbers'],
  symbol: ['symbol', 'symbols'],
  literal: ['typed character', 'typed characters']
};

/** "2 capitals, 6 numbers": what a pattern makes, in words. */
export function describePattern(slots: PatternSlot[]): string {
  return (['capital', 'small', 'digit', 'symbol', 'literal'] as const)
    .map((kind) => [kind, slots.filter((s) => s.kind === kind).length] as const)
    .filter(([, n]) => n > 0)
    .map(([kind, n]) => `${n} ${SLOT_NAME[kind][n === 1 ? 0 : 1]}`)
    .join(', ');
}

/** How many characters a password made with these settings has (0 for a pattern that cannot be used). */
export function passwordLength(g: PasswordGeneratorSettings): number {
  if (g.mode === 'pattern') {
    const p = parsePattern(g.pattern);
    return p.ok ? p.slots.length : 0;
  }
  return Math.min(PASSWORD_MAX_LEN, Math.max(PASSWORD_MIN_LEN, g.length));
}

/** Roughly how many bits of guesswork a password made with these settings holds. */
export function entropyBits(g: PasswordGeneratorSettings): number {
  if (g.mode === 'pattern') {
    const p = parsePattern(g.pattern);
    if (!p.ok) return 0;
    return Math.round(p.slots.reduce((sum, s) => (s.kind === 'literal' ? sum : sum + Math.log2(slotPool(s.kind, g.avoid_lookalikes))), 0));
  }
  const pool = poolSize(g);
  return pool > 1 ? Math.round(g.length * Math.log2(pool)) : 0;
}

export type Strength = 'weak' | 'fair' | 'good' | 'strong';

export function strength(bits: number): Strength {
  if (bits < 40) return 'weak';
  if (bits < 60) return 'fair';
  if (bits < 80) return 'good';
  return 'strong';
}

export const STRENGTH_LABEL: Record<Strength, string> = { weak: 'Weak', fair: 'Fair', good: 'Good', strong: 'Strong' };

/** "a–z · A–Z · 0–9 · symbols", or whichever kinds are on; in pattern mode, the pattern itself. */
export function rulesSummary(g: PasswordGeneratorSettings): string {
  const lookalikes = g.avoid_lookalikes && 'no look-alikes';
  const kinds = g.mode === 'pattern' ? [`Pattern ${g.pattern}`] : [g.lowercase && 'a–z', g.uppercase && 'A–Z', g.digits && '0–9', g.symbols && 'symbols'];
  return [...kinds, lookalikes].filter(Boolean).join(' · ');
}

export type CharKind = 'letter' | 'digit' | 'symbol';

export function charKind(ch: string): CharKind {
  if (/[0-9]/.test(ch)) return 'digit';
  if (/[A-Za-z]/.test(ch)) return 'letter';
  return 'symbol';
}

/** One entry per character, so digits and symbols can be coloured and the password is easy to read aloud. */
export function characters(password: string): Array<{ ch: string; kind: CharKind }> {
  return [...password].map((ch) => ({ ch, kind: charKind(ch) }));
}
