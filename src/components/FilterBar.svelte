<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import { ov } from '../lib/overlay.svelte';
  import { CATEGORY_LABEL, CATEGORY_ORDER } from '../lib/types';

  // Only the kinds you actually have are offered: three kinds means three chips, not eight.
  const kinds = $derived(CATEGORY_ORDER.filter((c) => ov.available.includes(c) || ov.category === c));
  const visible = $derived(kinds.length > 1 || ov.folders.length > 0 || ov.category !== null || ov.folder !== null);

  // The row is wider than the window as soon as there are a few kinds plus folders. It scrolls sideways;
  // the mouse wheel, the arrows and the fades below are how a mouse user finds that out and gets there.
  let bar: HTMLDivElement | undefined = $state();
  let track: HTMLDivElement | undefined = $state();
  let more = $state({ before: false, after: false }); // is there anything hidden on that side?

  function measure(): void {
    if (!bar) return;
    const before = bar.scrollLeft > 1;
    const after = bar.scrollLeft + bar.clientWidth < bar.scrollWidth - 1;
    if (before !== more.before || after !== more.after) more = { before, after };
  }

  // Re-measure when the window or the chips change size (a folder appears, the window is resized).
  $effect(() => {
    if (!bar || !track) return;
    const watcher = new ResizeObserver(measure);
    watcher.observe(bar);
    watcher.observe(track);
    measure();
    return () => watcher.disconnect();
  });

  const calm = (): boolean => matchMedia('(prefers-reduced-motion: reduce)').matches || document.documentElement.dataset.motion === 'off';

  // A mouse wheel only turns vertically. Over this row, turn it sideways. A trackpad's own sideways
  // swipe is left alone, and at either end the wheel is handed back untouched.
  $effect(() => {
    const el = bar;
    if (!el) return;
    const onWheel = (e: WheelEvent): void => {
      if (el.scrollWidth <= el.clientWidth || Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
      const step = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY; // some wheels report lines, not pixels
      const next = Math.min(el.scrollWidth - el.clientWidth, Math.max(0, el.scrollLeft + step));
      if (next === el.scrollLeft) return;
      e.preventDefault();
      el.scrollLeft = next;
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  });

  function page(direction: 1 | -1): void {
    if (!bar) return;
    bar.scrollBy({ left: direction * Math.round(bar.clientWidth * 0.7), behavior: calm() ? 'auto' : 'smooth' });
  }

  // Choosing a filter (a click, or the next/previous category shortcut) never leaves it out of sight.
  $effect(() => {
    void ov.category;
    void ov.folder;
    void tick().then(() => {
      const chip = bar?.querySelector<HTMLElement>('[aria-pressed="true"]');
      if (!bar || !chip) return;
      const room = 36; // clear of the fade and the arrow
      const c = chip.getBoundingClientRect();
      const b = bar.getBoundingClientRect();
      const shift = c.left < b.left + room ? c.left - b.left - room : c.right > b.right - room ? c.right - b.right + room : 0;
      if (shift !== 0) bar.scrollBy({ left: shift, behavior: calm() ? 'auto' : 'smooth' });
    });
  });
</script>

{#if visible}
  <div class="wrap">
    <div
      class="bar"
      bind:this={bar}
      onscroll={measure}
      data-before={more.before}
      data-after={more.after}
      role="toolbar"
      aria-label="Filter by kind or folder"
    >
      <div class="track" bind:this={track}>
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
    </div>
    <!-- Pointer helpers only: the keyboard reaches every chip with Tab and the category shortcuts. -->
    {#if more.before}
      <button class="arrow before" tabindex="-1" aria-label="Show earlier filters" onclick={() => page(-1)}><Icon name="chevron-left" size={14} /></button>
    {/if}
    {#if more.after}
      <button class="arrow after" tabindex="-1" aria-label="Show more filters" onclick={() => page(1)}><Icon name="chevron-right" size={14} /></button>
    {/if}
  </div>
{/if}

<style>
  .wrap {
    position: relative;
    flex: none;
    height: 28px;
  }
  .bar {
    height: 100%;
    overflow-x: auto;
    overflow-y: hidden;
    overscroll-behavior-x: contain;
    scrollbar-width: none;
    /* Fade only the side that has more behind it, so the first and last chip are never dimmed at rest. */
    --fade-before: 0px;
    --fade-after: 0px;
    mask-image: linear-gradient(90deg, transparent, #000 var(--fade-before), #000 calc(100% - var(--fade-after)), transparent);
  }
  .bar[data-before='true'] {
    --fade-before: 30px;
  }
  .bar[data-after='true'] {
    --fade-after: 30px;
  }
  .bar::-webkit-scrollbar {
    display: none;
  }
  .track {
    display: flex;
    align-items: center;
    gap: 2px;
    width: max-content;
    height: 100%;
    padding: 0 1px;
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
  .arrow {
    position: absolute;
    top: 2px;
    display: grid;
    place-items: center;
    width: 22px;
    height: 24px;
    border-radius: 999px;
    color: var(--ink-2);
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .arrow:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .arrow.before {
    left: 0;
  }
  .arrow.after {
    right: 0;
  }
</style>
