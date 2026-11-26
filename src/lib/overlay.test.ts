import { describe, expect, it, vi } from 'vitest';
import { ov } from './overlay.svelte';

// Outside Tauri the store talks to the in-memory mock backend, so these run without Windows.
vi.stubGlobal('window', { matchMedia: () => ({ matches: false }) });

describe('filters', () => {
  it('picking a folder drops the kind, and picking a kind drops the folder', () => {
    ov.setCategory('file');
    expect(ov.category).toBe('file');

    ov.setFolder('@pinned');
    expect(ov.folder).toBe('@pinned');
    expect(ov.category).toBeNull();

    ov.setCategory('text');
    expect(ov.category).toBe('text');
    expect(ov.folder).toBeNull();
  });

  it('turning a folder off leaves the kind alone, and "All" clears everything', () => {
    ov.setFolder('@pinned');
    ov.setFolder('@pinned'); // the same chip again switches it off
    expect(ov.folder).toBeNull();

    ov.setCategory('code');
    ov.clearFilters();
    expect(ov.category).toBeNull();
    expect(ov.folder).toBeNull();
  });

  it('the next/previous kind shortcut leaves a folder', () => {
    ov.available = ['text', 'code']; // the kinds the history really has
    ov.setFolder('@pinned');
    ov.cycleCategory(1);
    expect(ov.category).toBe('text');
    expect(ov.folder).toBeNull();
  });
});
