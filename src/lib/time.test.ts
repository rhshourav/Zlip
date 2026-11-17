import { describe, expect, it } from 'vitest';
import { formatAge, formatBytes, formatCountdown } from './time';

describe('time formatting', () => {
  const now = 1_700_000_000_000;
  it('ages', () => {
    expect(formatAge(now - 10_000, now)).toBe('just now');
    expect(formatAge(now - 5 * 60_000, now)).toBe('5 min');
    expect(formatAge(now - 3 * 3_600_000, now)).toBe('3 h');
    expect(formatAge(now - 2 * 86_400_000, now)).toBe('2 d');
    expect(formatAge(now - 21 * 86_400_000, now)).toBe('3 w');
    expect(formatAge(now + 5_000, now)).toBe('just now');
  });
  it('bytes', () => {
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(2048)).toBe('2.0 KB');
    expect(formatBytes(50 * 1024)).toBe('50 KB');
    expect(formatBytes(3 * 1024 * 1024)).toBe('3.0 MB');
  });
  it('countdown', () => {
    expect(formatCountdown(42_000)).toBe('0:42');
    expect(formatCountdown(61_000)).toBe('1:01');
    expect(formatCountdown(-5)).toBe('0:00');
  });
});
