<script lang="ts">
  import { onMount } from 'svelte';

  let { text, onpaste, onsave, oncancel }: { text: string; onpaste: (t: string) => void; onsave: (t: string) => void; oncancel: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let value = $state(text); // the editor starts from the item's text and then owns its own copy
  let area: HTMLTextAreaElement | undefined = $state();
  let row: HTMLElement | undefined = $state();

  onMount(() => {
    area?.focus();
    area?.setSelectionRange(value.length, value.length);
    // Keep the buttons reachable without scrolling by hand.
    queueMicrotask(() => row?.scrollIntoView({ block: 'nearest' }));
  });

  function onkeydown(e: KeyboardEvent) {
    // The editor owns the keyboard while it is open: nothing leaks to the list behind it.
    e.stopPropagation();
    if (e.key === 'Escape') {
      e.preventDefault();
      oncancel();
    } else if (e.key === 'Enter' && e.ctrlKey) {
      e.preventDefault();
      onpaste(value);
    } else if (e.key.toLowerCase() === 's' && e.ctrlKey) {
      e.preventDefault();
      onsave(value);
    }
  }
</script>

<div class="editor">
  <textarea bind:this={area} bind:value {onkeydown} spellcheck="false" rows="5" aria-label="Edit before pasting"></textarea>
  <div class="row" bind:this={row}>
    <button class="primary" onclick={() => onpaste(value)}>Paste edited <span class="keycap">Ctrl</span><span class="keycap">↵</span></button>
    <button onclick={() => onsave(value)}>Save as new <span class="keycap">Ctrl</span><span class="keycap">S</span></button>
    <button class="quiet" onclick={oncancel}>Cancel <span class="keycap">Esc</span></button>
  </div>
</div>

<style>
  .editor {
    display: grid;
    gap: 8px;
  }
  textarea {
    width: 100%;
    resize: vertical;
    min-height: 84px;
    max-height: 280px;
    padding: 10px 12px;
    border-radius: var(--r-md);
    border: 1px solid color-mix(in srgb, var(--accent) 50%, var(--hairline));
    background: var(--code-bg);
    font: 12.5px/1.55 var(--font-mono);
    outline: 0;
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .row {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border-radius: var(--r-sm);
    font-size: 12px;
    font-weight: 550;
    background: color-mix(in srgb, var(--ink) 8%, transparent);
    transition: background var(--dur-fast);
  }
  button:hover {
    background: color-mix(in srgb, var(--ink) 14%, transparent);
  }
  .primary {
    background: color-mix(in srgb, var(--accent) 42%, transparent);
  }
  .primary:hover {
    background: color-mix(in srgb, var(--accent) 56%, transparent);
  }
  .quiet {
    background: transparent;
    color: var(--ink-2);
  }
  .keycap {
    min-width: 18px;
    height: 17px;
    font-size: 10px;
  }
</style>
