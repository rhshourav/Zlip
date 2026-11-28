import { describe, expect, it } from 'vitest';
import { characters, charKind, describePattern, entropyBits, parsePattern, PATTERN_ERRORS, passwordLength, poolSize, rulesSummary, strength } from './password';
import type { PasswordGeneratorSettings } from './types';

const all: PasswordGeneratorSettings = { enabled: true, mode: 'random', length: 20, lowercase: true, uppercase: true, digits: true, symbols: true, pattern: 'LL######', avoid_lookalikes: false };
const shaped = (pattern: string, avoid_lookalikes = false): PasswordGeneratorSettings => ({ ...all, mode: 'pattern', pattern, avoid_lookalikes });

describe('password helpers', () => {
  it('counts the characters the generator can choose from', () => {
    expect(poolSize(all)).toBe(85);
    expect(poolSize({ ...all, symbols: false })).toBe(62);
    expect(poolSize({ ...all, lowercase: false, digits: false, symbols: false })).toBe(26);
    // O and I, o and l, 0 and 1 are left out.
    expect(poolSize({ ...all, avoid_lookalikes: true })).toBe(79);
    expect(poolSize({ ...all, lowercase: false, uppercase: false, digits: false, symbols: false })).toBe(0);
  });

  it('estimates the bits from the length and the pool', () => {
    expect(entropyBits(all)).toBe(128);
    expect(entropyBits({ ...all, length: 8, lowercase: false, digits: false, symbols: false })).toBe(38);
    expect(entropyBits({ ...all, lowercase: false, uppercase: false, digits: false, symbols: false })).toBe(0);
  });

  it('labels the strength', () => {
    expect([0, 39, 40, 59, 60, 79, 80, 200].map(strength)).toEqual(['weak', 'weak', 'fair', 'fair', 'good', 'good', 'strong', 'strong']);
  });

  it('says which kinds are on', () => {
    expect(rulesSummary(all)).toBe('a–z · A–Z · 0–9 · symbols');
    expect(rulesSummary({ ...all, lowercase: false, digits: false, symbols: false, avoid_lookalikes: true })).toBe('A–Z · no look-alikes');
  });

  it('splits a password into letters, digits and symbols', () => {
    expect(charKind('a')).toBe('letter');
    expect(charKind('Q')).toBe('letter');
    expect(charKind('7')).toBe('digit');
    expect(charKind('#')).toBe('symbol');
    expect(characters('aZ4!').map((c) => c.kind)).toEqual(['letter', 'letter', 'digit', 'symbol']);
    expect(characters('')).toEqual([]);
  });

  it('reads patterns the same way the Rust generator does', () => {
    // The same examples are tested in crates/zlip-config/src/password_pattern.rs.
    const kinds = (p: string): string[] | string => {
      const r = parsePattern(p);
      return r.ok ? r.slots.map((s) => (s.kind === 'literal' ? s.ch : s.kind)) : r.error;
    };
    expect(kinds('LL######')).toEqual(['capital', 'capital', ...Array(6).fill('digit')]);
    expect(kinds('Ll#*')).toEqual(['capital', 'small', 'digit', 'symbol']);
    expect(kinds('ZQ-#')).toEqual(['Z', 'Q', '-', 'digit']);
    expect(kinds('A1 #')).toEqual(['A', '1', ' ', 'digit']);
    expect(kinds('é#')).toEqual(['é', 'digit']);
    expect(kinds('\\L\\#\\*\\\\#')).toEqual(['L', '#', '*', '\\', 'digit']);
    expect(kinds('#\\')).toBe(PATTERN_ERRORS.trailingBackslash);
  });

  it('refuses patterns that cannot be used, with a reason', () => {
    // "AA123456" typed literally is a fixed word, not a pattern.
    for (const p of ['AA123456', '\\L\\#', '-']) expect(parsePattern(p)).toEqual({ ok: false, error: PATTERN_ERRORS.noPlaceholder });
    expect(parsePattern('')).toEqual({ ok: false, error: PATTERN_ERRORS.empty });
    for (const p of ['#\n#', '#\t', '\\\u0007']) expect(parsePattern(p)).toEqual({ ok: false, error: PATTERN_ERRORS.control });
  });

  it('holds a pattern to 128 positions and 256 typed characters', () => {
    expect(parsePattern('#'.repeat(128))).toMatchObject({ ok: true });
    expect(parsePattern('#'.repeat(129))).toEqual({ ok: false, error: PATTERN_ERRORS.tooLong });
    expect(parsePattern('\\-'.repeat(127) + '#')).toMatchObject({ ok: true }); // 255 typed, 128 positions
    expect(parsePattern('#'.repeat(257))).toEqual({ ok: false, error: PATTERN_ERRORS.tooLong });
  });

  it('describes what a pattern makes', () => {
    const say = (p: string): string => {
      const r = parsePattern(p);
      return r.ok ? describePattern(r.slots) : '';
    };
    expect(say('LL######')).toBe('2 capitals, 6 numbers');
    expect(say('L#')).toBe('1 capital, 1 number');
    expect(say('ZQ-l*#')).toBe('1 small letter, 1 number, 1 symbol, 3 typed characters');
  });

  it('estimates a pattern from its positions', () => {
    // AA123456: 2 x log2(26) + 6 x log2(10) = 29.3 bits, which is Weak. The screen says so.
    expect(entropyBits(shaped('LL######'))).toBe(29);
    expect(strength(entropyBits(shaped('LL######')))).toBe('weak');
    // Look-alikes leave 24 letters and 8 digits: 2 x log2(24) + 6 x log2(8) = 27.2.
    expect(entropyBits(shaped('LL######', true))).toBe(27);
    // Typed characters add nothing; a pattern that cannot be used holds nothing.
    expect(entropyBits(shaped('ZQ-######'))).toBe(entropyBits(shaped('######')));
    expect(entropyBits(shaped('AA123456'))).toBe(0);
    // Four of each placeholder: 4 x (2 x log2(26) + log2(10) + log2(23)) = 69.
    expect(entropyBits(shaped('Ll#*Ll#*Ll#*Ll#*'))).toBe(69);
  });

  it('counts the characters of a pattern password', () => {
    expect(passwordLength(shaped('LL######'))).toBe(8);
    expect(passwordLength(shaped('ZQ-####'))).toBe(7);
    expect(passwordLength(shaped('AA123456'))).toBe(0);
    expect(passwordLength(all)).toBe(20);
    expect(passwordLength({ ...all, length: 3 })).toBe(8);
  });

  it('shows the pattern in the summary', () => {
    expect(rulesSummary(shaped('LL######'))).toBe('Pattern LL######');
    expect(rulesSummary(shaped('LL######', true))).toBe('Pattern LL###### · no look-alikes');
  });
});
