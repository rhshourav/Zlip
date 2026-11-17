<script lang="ts">
  import { ov } from '../lib/overlay.svelte';
  import type { Transform } from '../lib/types';

  const CHOICES: Array<{ t: Transform; label: string; hint: string }> = [
    { t: 'trim', label: 'Trim', hint: 'Remove spaces and blank lines at both ends' },
    { t: 'camel_case', label: 'camelCase', hint: 'Join words, lower-case first' },
    { t: 'snake_case', label: 'snake_case', hint: 'Lower-case words joined by underscores' },
    { t: 'kebab_case', label: 'kebab-case', hint: 'Lower-case words joined by hyphens' },
    { t: 'upper', label: 'UPPER', hint: 'Upper-case everything' },
    { t: 'base64_decode', label: 'Base64 → text', hint: 'Decode Base64 before pasting' }
  ];
  const clean = { t: 'strip_tracking_params' as Transform, label: 'Clean link', hint: 'Remove utm_*, fbclid and other tracking from URLs' };

  const choices = $derived(ov.selected?.category === 'link' ? [clean, ...CHOICES.slice(0, 1)] : CHOICES);
  const active = $derived(ov.canTransform);
</script>

<!-- Format on the way in: toggles apply to the next paste, and reset when the overlay closes. -->
<div class="bar" class:off={!active} role="group" aria-label="Paste options">
  <span class="label">Paste as</span>
  <div class="pills">
    <button class="pill" aria-pressed={ov.plain} disabled={!active} title="Paste text only, without formatting" onclick={() => (ov.plain = !ov.plain)}>Plain</button>
    {#each choices as c (c.t)}
      <button class="pill" aria-pressed={ov.transforms.includes(c.t)} disabled={!active} title={c.hint} onclick={() => ov.toggleTransform(c.t)}>{c.label}</button>
    {/each}
  </div>
</div>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    transition: opacity var(--dur) var(--ease);
  }
  .bar.off {
    opacity: 0.45;
  }
  .label {
    flex: none;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-3);
  }
  .pills {
    display: flex;
    gap: 4px;
    overflow-x: auto;
    scrollbar-width: none;
    padding: 1px;
    mask-image: linear-gradient(90deg, #000 calc(100% - 20px), transparent);
  }
  .pills::-webkit-scrollbar {
    display: none;
  }
  .pill {
    flex: none;
    height: 26px;
    padding: 0 10px;
    border-radius: var(--r-sm);
    font-size: 11.5px;
    font-weight: 550;
    color: var(--ink-2);
    background: color-mix(in srgb, var(--ink) 6%, transparent);
    border: 1px solid transparent;
    transition: background var(--dur-fast), color var(--dur-fast), border-color var(--dur-fast);
  }
  .pill:enabled:hover {
    background: color-mix(in srgb, var(--ink) 11%, transparent);
    color: var(--ink);
  }
  .pill[aria-pressed='true'] {
    color: var(--ink);
    background: color-mix(in srgb, var(--accent) 32%, transparent);
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .pill:disabled {
    cursor: default;
  }
</style>
