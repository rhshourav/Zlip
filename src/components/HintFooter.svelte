<script lang="ts">
  import { ov } from '../lib/overlay.svelte';
  import { chordCaps } from '../lib/chords';
  import { shouldShowHint } from '../lib/hints';

  // Re-evaluated each time the overlay opens, so hints fade between visits, never mid-use.
  const hints = $derived.by(() => {
    void ov.shownTick;
    const cfg = ov.config;
    if (!cfg) return [];
    const k = cfg.hotkeys;
    const after = cfg.ui.hint_hide_after;
    return [
      { id: 'paste', caps: ['↵'], label: 'Paste' },
      { id: 'plain', caps: chordCaps(k.paste_plain), label: 'Plain' },
      { id: 'expand', caps: chordCaps(k.expand), label: 'Expand' },
      { id: 'pin', caps: chordCaps(k.pin), label: 'Pin' },
      { id: 'queue', caps: chordCaps(k.queue_add), label: 'Queue' },
      { id: 'quick', caps: chordCaps(k.quick_paste), label: 'Quick paste' }
    ].filter((h) => shouldShowHint(h.id, after));
  });
</script>

{#if hints.length > 0}
  <footer aria-label="Keyboard shortcuts">
    {#each hints.slice(0, 4) as h (h.id)}
      <span class="hint">
        <span class="caps">{#each h.caps as c (c)}<span class="keycap">{c}</span>{/each}</span>
        {h.label}
      </span>
    {/each}
  </footer>
{/if}

<style>
  footer {
    display: flex;
    gap: 14px;
    align-items: center;
    flex-wrap: nowrap;
    overflow: hidden;
    padding-top: 8px;
    border-top: 1px solid var(--hairline);
    font-size: 11px;
    color: var(--ink-3);
    white-space: nowrap;
  }
  .hint {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .caps {
    display: inline-flex;
    gap: 2px;
  }
</style>
