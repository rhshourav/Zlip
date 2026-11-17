// A footer hint disappears once its shortcut has been used a few times.
// Counters live in localStorage: they are a UI nicety, never anything private.

const KEY = 'zlip.hints.v1';

function load(): Record<string, number> {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? (JSON.parse(raw) as Record<string, number>) : {};
  } catch {
    return {};
  }
}

export function hintUses(id: string): number {
  return load()[id] ?? 0;
}

export function recordHintUse(id: string): void {
  try {
    const all = load();
    all[id] = (all[id] ?? 0) + 1;
    localStorage.setItem(KEY, JSON.stringify(all));
  } catch {
    /* storage unavailable: hints simply never fade */
  }
}

/** `hideAfter` of 0 means "never hide". */
export function shouldShowHint(id: string, hideAfter: number): boolean {
  return hideAfter === 0 || hintUses(id) < hideAfter;
}
