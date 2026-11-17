import { describe, expect, it } from 'vitest';
import { chordCaps, eventKey, matchesChord, parseChord, parseQuickPaste, quickPasteIndex } from './chords';

const ev = (init: Partial<KeyboardEvent>): KeyboardEvent =>
  ({ key: '', code: '', ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...init }) as KeyboardEvent;

describe('chords', () => {
  it('parses modifiers and keys in any case', () => {
    expect(parseChord('ctrl + shift + q')).toEqual({ ctrl: true, shift: true, alt: false, meta: false, key: 'Q' });
    expect(parseChord('Win+V')?.meta).toBe(true);
    expect(parseChord('Esc')?.key).toBe('Escape');
    expect(parseChord('Ctrl+Left')?.key).toBe('Left');
    expect(parseChord('F12')?.key).toBe('F12');
  });

  it('rejects nonsense', () => {
    for (const bad of ['', 'Ctrl+', 'Hyper+V', 'Ctrl+Nope', 'F99']) expect(parseChord(bad)).toBeNull();
  });

  it('matches exactly on modifiers', () => {
    const pin = parseChord('Ctrl+P')!;
    expect(matchesChord(ev({ key: 'p', code: 'KeyP', ctrlKey: true }), pin)).toBe(true);
    expect(matchesChord(ev({ key: 'P', code: 'KeyP', ctrlKey: true, shiftKey: true }), pin)).toBe(false);
    expect(matchesChord(ev({ key: 'p', code: 'KeyP' }), pin)).toBe(false);
  });

  it('maps DOM key names to canonical names', () => {
    expect(matchesChord(ev({ key: 'ArrowLeft', code: 'ArrowLeft', ctrlKey: true }), parseChord('Ctrl+Left')!)).toBe(true);
    expect(matchesChord(ev({ key: 'Enter', code: 'Enter', shiftKey: true }), parseChord('Shift+Enter')!)).toBe(true);
    expect(matchesChord(ev({ key: 'Delete', code: 'Delete' }), parseChord('Del')!)).toBe(true);
  });

  it('uses the physical digit under Alt, where `key` can be a composed character', () => {
    expect(eventKey(ev({ key: '¡', code: 'Digit1', altKey: true }))).toBe('1');
    const mods = parseQuickPaste('Alt+{1..9}')!;
    expect(quickPasteIndex(ev({ key: '¡', code: 'Digit1', altKey: true }), mods)).toBe(1);
    expect(quickPasteIndex(ev({ key: '9', code: 'Digit9', altKey: true }), mods)).toBe(9);
    expect(quickPasteIndex(ev({ key: '0', code: 'Digit0', altKey: true }), mods)).toBeNull();
    expect(quickPasteIndex(ev({ key: '1', code: 'Digit1' }), mods)).toBeNull();
  });

  it('quick paste pattern needs the {1..9} placeholder', () => {
    expect(parseQuickPaste('Alt+5')).toBeNull();
    expect(parseQuickPaste('Ctrl+Alt+{1..9}')).toEqual({ ctrl: true, alt: true, shift: false, meta: false });
  });

  it('draws keycaps', () => {
    expect(chordCaps('Ctrl+Shift+Q')).toEqual(['Ctrl', 'Shift', 'Q']);
    expect(chordCaps('Alt+{1..9}')).toEqual(['Alt', '1–9']);
    expect(chordCaps('Shift+Enter')).toEqual(['Shift', '↵']);
    expect(chordCaps('Escape')).toEqual(['Esc']);
  });
});
