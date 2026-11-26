import { describe, expect, it } from 'vitest';
import { characters, charKind, entropyBits, poolSize, rulesSummary, strength } from './password';
import type { PasswordGeneratorSettings } from './types';

const all: PasswordGeneratorSettings = { enabled: true, length: 20, lowercase: true, uppercase: true, digits: true, symbols: true, avoid_lookalikes: false };

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
});
