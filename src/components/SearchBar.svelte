<script lang="ts">
  import Slot from './Slot.svelte';
  import { ov } from '../lib/overlay.svelte';

  let input: HTMLInputElement | undefined = $state();
  export function focus(): void {
    input?.focus();
    input?.select();
  }

  const placeholder = $derived(
    ov.privateMode ? 'Private: copies are not saved' : ov.queue.mode !== 'idle' ? 'Search' : 'Search your clipboard'
  );
</script>

<!-- Search sits first and is always focused: you can start typing the instant the overlay opens. -->
<div class="bar" class:private={ov.privateMode}>
  <Slot name="search" size={18} />
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

  {#if ov.queue.mode !== 'idle'}
    <button class="chip queue" onclick={() => void ov.clearQueue()} title="Clear the paste queue" aria-label="Clear the paste queue ({ov.queue.len} items)">
      <Slot name="queue" size={14} />
      <span>{ov.queue.mode === 'collecting' ? 'Collecting' : 'Pasting'} · {ov.queue.len}</span>
      <Slot name="clear" size={12} />
    </button>
  {/if}

  <button
    class="chip toggle"
    aria-pressed={ov.privateMode}
    onclick={() => void ov.setPrivate(!ov.privateMode)}
    title="Private mode pauses capturing. Nothing you copy is saved while it is on."
  >
    <Slot name="private" size={14} />
    <span>Private</span>
  </button>
</div>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 46px;
    padding: 0 8px 0 14px;
    border-radius: var(--r-lg);
    background: var(--field);
    border: 1px solid var(--hairline);
    box-shadow: inset 0 1px 0 var(--highlight);
    color: var(--ink-2);
    transition: border-color var(--dur-fast), box-shadow var(--dur) var(--ease), background var(--dur-fast);
  }
  .bar:focus-within {
    border-color: color-mix(in srgb, var(--accent) 65%, transparent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 20%, transparent), inset 0 1px 0 var(--highlight);
    color: var(--ink);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    outline: 0;
    background: transparent;
    font-size: 14.5px;
    letter-spacing: -0.005em;
    color: var(--ink);
  }
  input::placeholder {
    color: var(--ink-3);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border-radius: 999px;
    font-size: 11.5px;
    font-weight: 550;
    color: var(--ink-2);
    background: color-mix(in srgb, var(--ink) 6%, transparent);
    transition: background var(--dur-fast), color var(--dur-fast);
  }
  .chip:hover {
    background: color-mix(in srgb, var(--ink) 11%, transparent);
    color: var(--ink);
  }
  .queue {
    color: var(--ink);
    background: color-mix(in srgb, var(--cat-snippet) 35%, transparent);
  }
  .toggle[aria-pressed='true'] {
    color: var(--ink);
    background: color-mix(in srgb, var(--cat-code) 50%, transparent);
  }
  .private {
    border-color: color-mix(in srgb, var(--cat-code) 55%, var(--hairline));
  }
</style>
