<script lang="ts">
  import Icon from './Icon.svelte';
  import { ov } from '../lib/overlay.svelte';
  import { chordCaps } from '../lib/chords';
  import { shouldShowHint } from '../lib/hints';
  import type { Transform } from '../lib/types';

  const CHOICES: Array<{ t: Transform; label: string; hint: string }> = [
    { t: 'trim', label: 'Trim spaces', hint: 'Remove spaces and blank lines at both ends' },
    { t: 'camel_case', label: 'camelCase', hint: 'Join words, lower-case first' },
    { t: 'snake_case', label: 'snake_case', hint: 'Lower-case words joined by underscores' },
    { t: 'kebab_case', label: 'kebab-case', hint: 'Lower-case words joined by hyphens' },
    { t: 'upper', label: 'UPPER CASE', hint: 'Upper-case everything' },
    { t: 'base64_decode', label: 'Decode Base64', hint: 'Decode Base64 before pasting' }
  ];
  const clean = { t: 'strip_tracking_params' as Transform, label: 'Clean link', hint: 'Remove utm_*, fbclid and other tracking from URLs' };

  const choices = $derived(ov.selected?.category === 'link' ? [clean, CHOICES[0]!] : CHOICES);
  const usable = $derived(ov.canTransform);
  const count = $derived(ov.transforms.length + (ov.plain ? 1 : 0));

  // Hints are re-evaluated each time the overlay opens, so they fade between visits, never mid-use.
  const hints = $derived.by(() => {
    void ov.shownTick;
    const cfg = ov.config;
    if (!cfg) return [];
    const k = cfg.hotkeys;
    return [
      { id: 'paste', caps: ['↵'], label: 'Paste' },
      { id: 'expand', caps: chordCaps(k.expand), label: 'Expand' },
      { id: 'plain', caps: chordCaps(k.paste_plain), label: 'Plain' },
      { id: 'pin', caps: chordCaps(k.pin), label: 'Pin' },
      { id: 'queue', caps: chordCaps(k.queue_add), label: 'Queue' }
    ].filter((h) => shouldShowHint(h.id, cfg.ui.hint_hide_after));
  });

  let root: HTMLElement | undefined = $state();
  // A click anywhere outside the menu closes it.
  function outside(e: PointerEvent): void {
    if (ov.menuOpen && root && !root.contains(e.target as Node)) ov.menuOpen = false;
  }
</script>

<svelte:window onpointerdown={outside} />

<footer bind:this={root}>
  <button class="fmt" class:on={count > 0} aria-haspopup="menu" aria-expanded={ov.menuOpen} disabled={!usable} title="Change how the next paste is formatted" onclick={() => (ov.menuOpen = !ov.menuOpen)}>
    Paste as
    {#if count > 0}<span class="count">{count}</span>{/if}
    <Icon name="chevron-up" size={12} />
  </button>

  {#if ov.menuOpen}
    <div class="menu" role="menu" aria-label="Paste as">
      <button class="mi" role="menuitemcheckbox" aria-checked={ov.plain} title="Paste text only, without formatting" onclick={() => (ov.plain = !ov.plain)}>
        <span>Plain text</span>
        {#if ov.plain}<Icon name="check" size={14} />{/if}
      </button>
      <hr />
      {#each choices as c (c.t)}
        <button class="mi" role="menuitemcheckbox" aria-checked={ov.transforms.includes(c.t)} title={c.hint} onclick={() => ov.toggleTransform(c.t)}>
          <span>{c.label}</span>
          {#if ov.transforms.includes(c.t)}<Icon name="check" size={14} />{/if}
        </button>
      {/each}
    </div>
  {/if}

  <div class="hints" aria-label="Keyboard shortcuts">
    {#each hints.slice(0, 2) as h (h.id)}
      <span class="hint">
        <span class="caps">{#each h.caps as c (c)}<span class="keycap">{c}</span>{/each}</span>
        {h.label}
      </span>
    {/each}
  </div>
</footer>

<style>
  footer {
    position: relative;
    /* Lets the hints react to the window's width (see the @container rule below). */
    container-type: inline-size;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    margin: 0 -8px;
    padding: 0 10px;
    border-top: 1px solid var(--hairline);
  }
  .fmt {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
    height: 24px;
    padding: 0 8px;
    border-radius: var(--r-xs);
    font-size: 12px;
    font-weight: 500;
    color: var(--ink-2);
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .fmt:hover:not(:disabled),
  .fmt[aria-expanded='true'] {
    background: var(--hover);
    color: var(--ink);
  }
  .fmt:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .fmt.on {
    color: var(--accent-ink);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .count {
    display: grid;
    place-items: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 700;
    color: #fff;
    background: var(--accent);
  }

  .menu {
    position: absolute;
    left: 8px;
    bottom: calc(100% + 6px);
    z-index: 8; /* above a toast: the menu is something you just asked for */
    width: 208px;
    padding: 4px;
    border-radius: var(--r-lg);
    background: color-mix(in srgb, var(--panel-solid) 94%, transparent);
    backdrop-filter: blur(24px) saturate(1.4);
    box-shadow:
      0 0 0 1px var(--hairline-strong),
      var(--shadow);
    animation: pop var(--dur) var(--ease);
    transform-origin: bottom left;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(4px) scale(0.98);
    }
  }
  .mi {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border-radius: var(--r-xs);
    font-size: 12.5px;
    text-align: left;
  }
  .mi:hover {
    background: var(--accent);
    color: #fff;
  }
  hr {
    height: 1px;
    margin: 4px 6px;
    border: 0;
    background: var(--hairline);
  }

  .hints {
    display: flex;
    gap: 12px;
    margin-left: auto;
    overflow: hidden;
    white-space: nowrap;
    font-size: 11px;
    color: var(--ink-3);
  }
  .hint {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  /* In the Compact size there is room for one hint, not two: drop the extras rather than clip them. */
  @container (max-width: 320px) {
    .hint:nth-child(n + 2) {
      display: none;
    }
  }
  .caps {
    display: inline-flex;
    gap: 2px;
  }
</style>
