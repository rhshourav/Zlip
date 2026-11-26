// Small helpers for showing a generated password. The password itself is made in Rust
// (crates/zlip-security/src/password.rs); the character counts below must match it.

import type { PasswordGeneratorSettings } from './types';

export const PASSWORD_MIN_LEN = 8;
export const PASSWORD_MAX_LEN = 128;

const LOWER = 26;
const UPPER = 26;
const DIGITS = 10;
const SYMBOLS = 23;
/** O 0 o I l 1: two of each letter pair and of the digits go when look-alikes are left out. */
const LOOKALIKES_PER_KIND = 2;

/** How many different characters the generator draws from with these settings. */
export function poolSize(g: PasswordGeneratorSettings): number {
  const drop = g.avoid_lookalikes ? LOOKALIKES_PER_KIND : 0;
  return (g.lowercase ? LOWER - drop : 0) + (g.uppercase ? UPPER - drop : 0) + (g.digits ? DIGITS - drop : 0) + (g.symbols ? SYMBOLS : 0);
}

/** Roughly how many bits of guesswork a password made with these settings holds. */
export function entropyBits(g: PasswordGeneratorSettings): number {
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

/** "a–z · A–Z · 0–9 · symbols", or whichever kinds are on. */
export function rulesSummary(g: PasswordGeneratorSettings): string {
  return [g.lowercase && 'a–z', g.uppercase && 'A–Z', g.digits && '0–9', g.symbols && 'symbols', g.avoid_lookalikes && 'no look-alikes'].filter(Boolean).join(' · ');
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
