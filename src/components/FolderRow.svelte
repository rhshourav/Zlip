<script lang="ts">
  import Slot from './Slot.svelte';
  import { ov } from '../lib/overlay.svelte';
</script>

{#if ov.folders.length > 0}
  <!-- Snippet folders: pinned items and the folders from your config, one tap away. -->
  <div class="row" role="toolbar" aria-label="Snippet folders">
    {#each ov.folders as f (f.filter)}
      <button class="chip" class:pinned={f.kind === 'pinned'} aria-pressed={ov.folder === f.filter} onclick={() => ov.setFolder(f.filter)}>
        <Slot name={f.kind === 'pinned' ? 'folder-pinned' : 'folder'} size={14} />
        <span>{f.name}</span>
        <span class="n">{f.count}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .row {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    padding: 1px 2px;
    margin: 0 -2px;
    scrollbar-width: none;
  }
  .row::-webkit-scrollbar {
    display: none;
  }
  .chip {
    --c: var(--cat-snippet);
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
    height: 26px;
    padding: 0 10px;
    border-radius: var(--r-sm);
    font-size: 12px;
    font-weight: 550;
    color: var(--ink-2);
    background: color-mix(in srgb, var(--c) 14%, transparent);
    border: 1px dashed color-mix(in srgb, var(--c) 55%, transparent);
    transition: background var(--dur-fast), color var(--dur-fast), border-color var(--dur-fast);
  }
  .chip.pinned {
    --c: var(--cat-file);
  }
  .chip:hover {
    color: var(--ink);
    background: color-mix(in srgb, var(--c) 26%, transparent);
  }
  .chip[aria-pressed='true'] {
    color: var(--ink);
    background: color-mix(in srgb, var(--c) 40%, transparent);
    border-style: solid;
  }
  .n {
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
    color: var(--ink-3);
  }
</style>
