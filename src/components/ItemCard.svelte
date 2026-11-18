<script lang="ts">
  import Icon from './Icon.svelte';
  import QuickEditor from './QuickEditor.svelte';
  import type { IconName } from '../lib/icons';
  import { formatAge, formatBytes, formatCountdown } from '../lib/time';
  import type { Category, ItemDetail, ItemView } from '../lib/types';

  interface Props {
    item: ItemView;
    index: number;
    selected: boolean;
    expanded: boolean;
    editing: boolean;
    detail: ItemDetail | undefined;
    thumb: string | undefined;
    revealed: string | null;
    loadedAt: number;
    now: number;
    onselect: () => void;
    onpaste: () => void;
    onplain: () => void;
    onexpand: () => void;
    onpin: () => void;
    ondelete: () => void;
    onqueue: () => void;
    onedit: () => void;
    onreveal: () => void;
    onaction: (name: string) => void;
    onpasteedited: (text: string) => void;
    onsaveedited: (text: string) => void;
    oncanceledit: () => void;
  }

  let { item, index, selected, expanded, editing, detail, thumb, revealed, loadedAt, now, ...on }: Props = $props();

  // One glyph per kind, on a soft tile in that kind's colour. The tile replaces the old
  // uppercase label, the coloured dot and the accent bar: one small mark says it all.
  const GLYPH: Record<Category, IconName> = { text: 'text', link: 'link', code: 'code', image: 'image', color: 'swatch', file: 'file', snippet: 'bolt', sensitive: 'lock' };

  const remaining = $derived(item.expires_in_ms === null ? null : Math.max(0, item.expires_in_ms - (now - loadedAt)));
  // A snippet's preview is "title\nfirst line of body": show the title as a title.
  const snippetTitle = $derived(item.preview.split('\n')[0] ?? '');
  const snippetBody = $derived(item.preview.split('\n').slice(1).join(' '));
  const isText = $derived(item.category !== 'image' && item.category !== 'sensitive');
  const title = $derived(item.category === 'image' ? (item.width && item.height ? `${item.width} × ${item.height}` : 'Image') : item.preview);
  const sub = $derived(
    [
      item.category === 'link' || item.category === 'code' || item.category === 'file' ? item.label : null,
      item.source_app,
      remaining !== null ? `Wiped in ${formatCountdown(remaining)}` : item.read_only ? null : formatAge(item.created_at_ms, now)
    ]
      .filter(Boolean)
      .join(' · ')
  );
  const meta = $derived(
    [
      item.size_bytes ? formatBytes(item.size_bytes) : null,
      detail && item.category !== 'image' && item.category !== 'file' && item.category !== 'sensitive'
        ? `${detail.char_count.toLocaleString()} chars · ${detail.line_count} ${detail.line_count === 1 ? 'line' : 'lines'}`
        : null,
      detail?.reformatted ? 'pretty-printed' : null
    ]
      .filter(Boolean)
      .join(' · ')
  );
</script>

<!-- A row is one option in a listbox. Clicking it pastes; the controls inside do their own thing. -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="row"
  class:expanded
  role="option"
  tabindex="-1"
  aria-selected={selected}
  id="item-{item.id}"
  data-id={item.id}
  style="--cat: var(--cat-{item.category})"
  onpointermove={on.onselect}
  onclick={on.onpaste}
>
  <span class="tile" class:swatch={item.category === 'color'} style:--sw={item.color ?? undefined} aria-hidden="true">
    {#if item.category !== 'color'}<Icon name={GLYPH[item.category]} size={16} />{/if}
  </span>

  <div class="main">
    {#if item.category === 'snippet'}
      <p class="title strong">{snippetTitle}</p>
      {#if snippetBody}<p class="sub mono">{snippetBody}</p>{/if}
    {:else}
      {#if item.category === 'code' && item.preview_html}
        <!-- eslint-disable-next-line svelte/no-at-html-tags -- HTML escaped and highlighted by zlip-highlight in Rust -->
        <pre class="title code hl">{@html item.preview_html}</pre>
      {:else}
        <p class="title" class:mono={item.category === 'color' || item.category === 'file'} class:masked={item.category === 'sensitive'}>{title}</p>
      {/if}
      {#if item.category === 'image' && !(expanded && detail?.image)}
        {#if thumb}<img class="thumb" src={thumb} alt="Copied image, {item.width} by {item.height} pixels" />{:else}<span class="thumb skeleton"></span>{/if}
      {/if}
      {#if sub}<p class="sub">{sub}</p>{/if}
    {/if}
  </div>

  <div class="aside">
    {#if item.pinned}<span class="mark" title="Pinned"><Icon name="pin" size={12} /></span>{/if}
    {#if item.queue_position !== null}<span class="mark qn" title="Position in the paste queue">#{item.queue_position}</span>{/if}
    {#if index < 9 && selected && !expanded}<span class="keycap" title="Quick paste">Alt {index + 1}</span>{/if}
    <button class="chev" aria-label={expanded ? 'Collapse' : 'Expand'} aria-expanded={expanded} tabindex="-1" onclick={(e) => { e.stopPropagation(); on.onexpand(); }}>
      <span class="chev-glyph" class:up={expanded}><Icon name="chevron-down" size={14} /></span>
    </button>
  </div>

  {#if remaining !== null}
    <div class="drain" style:--p={Math.min(1, remaining / 60000)} aria-hidden="true"></div>
  {/if}

  <!-- Expanded part: animates its height with grid-template-rows, no layout measuring. -->
  <div class="more" data-open={expanded}>
    <div class="more-inner">
      {#if expanded}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="detail" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
          {#if editing && detail?.editable_text != null}
            <QuickEditor text={detail.editable_text} onpaste={on.onpasteedited} onsave={on.onsaveedited} oncancel={on.oncanceledit} />
          {:else}
            {#if revealed !== null}
              <pre class="secret mono">{revealed}</pre>
            {:else if detail?.image}
              <img class="full" src={detail.image} alt="Full-size preview of the copied item" />
            {:else if detail && detail.files.length > 0}
              <ul class="files mono">{#each detail.files as f (f)}<li>{f}</li>{/each}</ul>
            {:else if detail && item.category !== 'sensitive'}
              <!-- eslint-disable-next-line svelte/no-at-html-tags -- HTML escaped (and highlighted) by zlip-highlight in Rust -->
              <pre class="full hl" class:wrap={item.category !== 'code'}>{@html detail.html}</pre>
            {:else if !detail}
              <span class="skeleton line"></span>
            {/if}
            {#if detail?.truncated}<p class="note">Showing the start. The full text is still what gets pasted.</p>{/if}
            {#if meta}<p class="note">{meta}</p>{/if}

            <div class="actions">
              <button class="btn primary" onclick={on.onpaste}>Paste <span class="keycap">↵</span></button>
              {#if isText}<button class="btn" onclick={on.onplain}>Plain <span class="keycap">⇧↵</span></button>{/if}
              <button class="btn" onclick={on.onqueue}><Icon name="queue" size={13} />Queue</button>
              {#if !item.read_only && item.category !== 'sensitive'}
                <button class="btn" onclick={on.onpin}><Icon name="pin" size={13} />{item.pinned ? 'Unpin' : 'Pin'}</button>
              {/if}
              {#if detail?.editable_text != null}<button class="btn" onclick={on.onedit}><Icon name="edit" size={13} />Edit</button>{/if}
              {#if item.category === 'sensitive'}
                <button class="btn" onclick={on.onreveal}><Icon name="eye" size={13} />{revealed !== null ? 'Hide' : 'Reveal 8 s'}</button>
              {/if}
              {#each detail?.actions ?? [] as name (name)}
                <button class="btn" onclick={() => on.onaction(name)}><Icon name="bolt" size={13} />{name}</button>
              {/each}
              {#if !item.read_only}<button class="btn danger" onclick={on.ondelete}><Icon name="trash" size={13} />Delete</button>{/if}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .row {
    position: relative;
    display: grid;
    grid-template-columns: 30px minmax(0, 1fr) auto;
    column-gap: 10px;
    align-items: start;
    padding: 7px 8px;
    border-radius: var(--r-md);
    cursor: pointer;
    transition: background var(--dur-fast);
    content-visibility: auto;
    contain-intrinsic-size: auto 50px;
  }
  .row:hover {
    background: var(--hover);
  }
  .row[aria-selected='true'] {
    background: var(--sel);
  }

  /* ---- the kind tile ---- */
  .tile {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: var(--r-sm);
    background: color-mix(in srgb, var(--cat) 88%, transparent);
    color: var(--tile-ink);
  }
  .tile.swatch {
    background: var(--sw, transparent);
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.14);
  }

  /* ---- text ---- */
  .main {
    min-width: 0;
  }
  .title {
    margin: 0;
    font-size: 13px;
    line-height: 1.38;
    color: var(--ink);
    overflow: hidden;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .title.strong {
    font-weight: 600;
    -webkit-line-clamp: 1;
    line-clamp: 1;
  }
  .title.mono {
    font-size: 12px;
  }
  .title.masked {
    letter-spacing: 0.12em;
    color: var(--ink-2);
  }
  .title.code {
    font: 12px/1.45 var(--font-mono);
    max-height: calc(1.45em * 2);
  }
  .sub {
    margin: 1px 0 0;
    font-size: 11px;
    line-height: 1.35;
    color: var(--ink-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .thumb {
    display: block;
    max-width: 100%;
    max-height: 72px;
    margin-top: 4px;
    border-radius: var(--r-xs);
    object-fit: cover;
    box-shadow: 0 0 0 1px var(--hairline);
  }
  .thumb.skeleton {
    width: 120px;
    height: 56px;
  }
  .skeleton {
    background: linear-gradient(90deg, var(--hover), color-mix(in srgb, var(--ink) 12%, transparent), var(--hover));
    background-size: 200% 100%;
    animation: shimmer 1.4s linear infinite;
  }
  .skeleton.line {
    display: block;
    height: 12px;
    border-radius: 6px;
  }
  @keyframes shimmer {
    to {
      background-position: -200% 0;
    }
  }

  /* ---- right side ---- */
  .aside {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    color: var(--ink-3);
    font-size: 11px;
  }
  .mark {
    display: inline-flex;
    align-items: center;
    color: var(--ink-3);
  }
  .qn {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--accent-ink);
  }
  .chev {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    color: var(--ink-3);
    opacity: 0;
    transition:
      opacity var(--dur-fast),
      background var(--dur-fast);
  }
  .row:hover .chev,
  .row[aria-selected='true'] .chev,
  .chev:focus-visible {
    opacity: 1;
  }
  .chev:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .chev-glyph {
    display: grid;
    transition: transform var(--dur) var(--ease);
  }
  .chev-glyph.up {
    transform: rotate(180deg);
  }

  /* A hairline that drains over a secret's lifetime. */
  .drain {
    position: absolute;
    left: 8px;
    right: 8px;
    bottom: 2px;
    height: 2px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--cat-sensitive) 70%, transparent);
    transform-origin: left;
    transform: scaleX(var(--p));
    transition: transform 1s linear;
  }

  /* ---- expanded ---- */
  .more {
    grid-column: 1 / -1;
    display: grid;
    grid-template-rows: 0fr;
    transition: grid-template-rows var(--dur) var(--ease);
  }
  .more[data-open='true'] {
    grid-template-rows: 1fr;
  }
  .more-inner {
    min-height: 0;
    overflow: hidden;
  }
  .detail {
    display: grid;
    gap: 8px;
    padding-top: 8px;
    cursor: default;
  }
  .full {
    margin: 0;
    max-height: 190px;
    overflow: auto;
    padding: 8px 10px;
    border-radius: var(--r-sm);
    background: var(--code-bg);
    font: 12px/1.5 var(--font-mono);
    user-select: text;
    white-space: pre;
  }
  .full.wrap {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-family: var(--font-ui);
    font-size: 12.5px;
  }
  img.full {
    display: block;
    max-width: 100%;
    max-height: 220px;
    padding: 0;
    object-fit: contain;
    background: var(--code-bg);
  }
  .secret {
    margin: 0;
    padding: 8px 10px;
    border-radius: var(--r-sm);
    background: color-mix(in srgb, var(--cat-sensitive) 22%, transparent);
    font-size: 12px;
    user-select: text;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .files {
    margin: 0;
    padding: 8px 10px 8px 24px;
    border-radius: var(--r-sm);
    background: var(--code-bg);
    font-size: 11.5px;
    max-height: 120px;
    overflow: auto;
  }
  .note {
    margin: 0;
    font-size: 11px;
    color: var(--ink-3);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
</style>
