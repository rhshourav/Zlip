<script lang="ts">
  import Slot from './Slot.svelte';
  import { ov } from '../lib/overlay.svelte';
  import { CATEGORY_LABEL, CATEGORY_ORDER } from '../lib/types';
</script>

<!-- Colour-coded tabs: the pastel you see here is the pastel on the cards below. -->
<div class="rail" role="toolbar" aria-label="Filter by kind">
  <button class="pill" style="--c: var(--accent)" aria-pressed={ov.category === null} onclick={() => ov.setCategory(null)}>
    <span class="lead"><span class="dot"></span><Slot name="category-all" size={14} /></span>
    All
  </button>
  {#each CATEGORY_ORDER as c (c)}
    <button class="pill" style="--c: var(--cat-{c})" aria-pressed={ov.category === c} onclick={() => ov.setCategory(ov.category === c ? null : c)}>
      <span class="lead"><span class="dot"></span><Slot name="category-{c}" size={14} /></span>
      {CATEGORY_LABEL[c]}
    </button>
  {/each}
</div>

<style>
  .rail {
    display: flex;
    gap: 4px;
    overflow-x: auto;
    padding: 1px 2px 3px;
    margin: 0 -2px;
    /* Fade the right edge: the rail continues, and the user can see that it does. */
    mask-image: linear-gradient(90deg, #000 calc(100% - 28px), transparent);
    scrollbar-width: none;
  }
  .rail::-webkit-scrollbar {
    display: none;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    flex: none;
    height: 30px;
    padding: 0 12px 0 9px;
    border-radius: 999px;
    border: 1px solid transparent;
    font-size: 12.5px;
    font-weight: 550;
    letter-spacing: -0.003em;
    color: var(--ink-2);
    transition: background var(--dur-fast), color var(--dur-fast), border-color var(--dur-fast), transform var(--dur-fast) var(--ease);
  }
  .pill:hover {
    background: color-mix(in srgb, var(--c) 16%, transparent);
    color: var(--ink);
  }
  .pill:active {
    transform: scale(0.97);
  }
  .pill[aria-pressed='true'] {
    background: color-mix(in srgb, var(--c) 30%, transparent);
    border-color: color-mix(in srgb, var(--c) 62%, transparent);
    color: var(--ink);
  }
  /* The leading mark: a pastel dot until you map an icon to the slot (see icons.css). */
  .lead {
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
  }
  .lead > :global(*) {
    grid-area: 1 / 1;
  }
  .dot {
    display: var(--fallback-dot, block);
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--c) 30%, transparent);
  }
</style>
