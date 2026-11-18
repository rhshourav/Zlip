<script lang="ts">
  import Icon from './Icon.svelte';
  import { ov } from '../lib/overlay.svelte';

  let input: HTMLInputElement | undefined = $state();
  export function focus(): void {
    input?.focus();
    input?.select();
  }

  const placeholder = $derived(ov.privateMode ? 'Search (nothing you copy is being saved)' : 'Search');
</script>

<!-- Search sits first and is always focused: you can start typing the instant the overlay opens. -->
<div class="top">
  <div class="field">
    <Icon name="search" size={15} />
    <input
      bind:this={input}
      type="text"
      role="searchbox"
      aria-label="Search clipboard history"
      {placeholder}
      autocomplete="off"
      spellcheck="false"
      value={ov.query}
      oninput={(e) => ov.setQuery(e.currentTarget.value)}
    />
    <!-- Status tags only exist while they are true, so the bar stays empty the rest of the time. -->
    {#if ov.queue.mode !== 'idle'}
      <button class="tag" onclick={() => void ov.clearQueue()} title="Clear the paste queue" aria-label="Clear the paste queue ({ov.queue.len} items)">
        <Icon name="queue" size={12} />
        {ov.queue.mode === 'collecting' ? 'Collecting' : 'Pasting'}
        {ov.queue.len}
        <Icon name="x" size={10} />
      </button>
    {/if}
    {#if ov.privateMode}
      <button class="tag" onclick={() => void ov.setPrivate(false)} title="Private mode is on. Click to turn it off." aria-label="Private mode is on. Turn it off">
        <Icon name="lock" size={12} />
        Private
      </button>
    {/if}
  </div>
  <button class="gear" onclick={() => void ov.openSettings()} title="Settings" aria-label="Settings">
    <Icon name="sliders" size={17} />
  </button>
</div>

<style>
  .top {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .field {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 6px 0 10px;
    border-radius: var(--r-md);
    background: var(--field);
    color: var(--ink-3);
    transition:
      box-shadow var(--dur) var(--ease),
      color var(--dur-fast);
  }
  .field:focus-within {
    color: var(--ink-2);
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--accent) 60%, transparent),
      0 0 0 4px color-mix(in srgb, var(--accent) 16%, transparent);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    outline: 0;
    background: transparent;
    font-size: 13.5px;
    color: var(--ink);
  }
  input::placeholder {
    color: var(--ink-3);
  }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
    height: 22px;
    padding: 0 8px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 600;
    color: var(--accent-ink);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    transition: background var(--dur-fast);
  }
  .tag:hover {
    background: color-mix(in srgb, var(--accent) 24%, transparent);
  }
  .gear {
    display: grid;
    place-items: center;
    flex: none;
    width: 34px;
    height: 34px;
    border-radius: var(--r-md);
    color: var(--ink-2);
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .gear:hover {
    background: var(--hover);
    color: var(--ink);
  }
</style>
