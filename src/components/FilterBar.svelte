<script lang="ts">
  import Icon from './Icon.svelte';
  import { ov } from '../lib/overlay.svelte';
  import { CATEGORY_LABEL, CATEGORY_ORDER } from '../lib/types';

  // Only the kinds you actually have are offered: three kinds means three chips, not eight.
  const kinds = $derived(CATEGORY_ORDER.filter((c) => ov.available.includes(c) || ov.category === c));
  const visible = $derived(kinds.length > 1 || ov.folders.length > 0 || ov.category !== null || ov.folder !== null);
</script>

{#if visible}
  <div class="bar" role="toolbar" aria-label="Filter by kind or folder">
    <button class="chip" aria-pressed={ov.category === null && ov.folder === null} onclick={() => ov.clearFilters()}>All</button>
    {#each kinds as c (c)}
      <button class="chip" aria-pressed={ov.category === c} onclick={() => ov.setCategory(ov.category === c ? null : c)}>{CATEGORY_LABEL[c]}</button>
    {/each}
    {#if ov.folders.length > 0}
      <span class="sep" aria-hidden="true"></span>
      {#each ov.folders as f (f.filter)}
        <button class="chip" aria-pressed={ov.folder === f.filter} onclick={() => ov.setFolder(f.filter)}>
          <Icon name={f.kind === 'pinned' ? 'pin' : 'folder'} size={12} />
          {f.name}
          <span class="n">{f.count}</span>
        </button>
      {/each}
    {/if}
  </div>
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 28px;
    overflow-x: auto;
    padding: 0 1px;
    /* Fade the right edge: the row continues, and the fade says so. */
    mask-image: linear-gradient(90deg, #000 calc(100% - 24px), transparent);
    scrollbar-width: none;
  }
  .bar::-webkit-scrollbar {
    display: none;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
    height: 24px;
    padding: 0 10px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 500;
    color: var(--ink-2);
    white-space: nowrap;
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .chip:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .chip[aria-pressed='true'] {
    background: color-mix(in srgb, var(--ink) 10%, transparent);
    color: var(--ink);
    font-weight: 600;
  }
  .sep {
    flex: none;
    width: 1px;
    height: 14px;
    margin: 0 5px;
    background: var(--hairline-strong);
  }
  .n {
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
    color: var(--ink-3);
  }
</style>
