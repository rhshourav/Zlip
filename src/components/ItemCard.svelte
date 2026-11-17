<script lang="ts">
  import Slot from './Slot.svelte';
  import QuickEditor from './QuickEditor.svelte';
  import { formatAge, formatBytes, formatCountdown } from '../lib/time';
  import { CATEGORY_NAME } from '../lib/types';
  import type { ItemDetail, ItemView } from '../lib/types';

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

  const remaining = $derived(item.expires_in_ms === null ? null : Math.max(0, item.expires_in_ms - (now - loadedAt)));
  // A snippet's preview is "title\nfirst line of body": show the title as a title.
  const snippetTitle = $derived(item.preview.split('\n')[0] ?? '');
  const snippetBody = $derived(item.preview.split('\n').slice(1).join(' '));
  const isText = $derived(item.category !== 'image' && item.category !== 'sensitive');
  const meta = $derived(
    [
      item.source_app,
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

<!-- A card is one option in a listbox. Whole-card click pastes; the controls inside do their own thing. -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="card"
  class:expanded
  role="option"
  tabindex="-1"
  aria-selected={selected}
  data-id={item.id}
  style="--cat: var(--cat-{item.category}); --i: {Math.min(index, 8)}"
  onpointermove={on.onselect}
  onclick={on.onpaste}
>
  <header>
    <span class="kind"><span class="dot"></span>{CATEGORY_NAME[item.category]}</span>
    {#if item.label}<span class="label">{item.label}</span>{/if}
    <span class="spacer"></span>
    {#if item.pinned}<span class="badge" title="Pinned"><Slot name="pin" size={12} />Pinned</span>{/if}
    {#if item.queue_position !== null}<span class="badge q">#{item.queue_position}</span>{/if}
    {#if index < 9 && selected}<span class="keycap" title="Quick paste">Alt {index + 1}</span>{/if}
    {#if remaining === null && !item.read_only}<time class="age">{formatAge(item.created_at_ms, now)}</time>{/if}
  </header>

  <div class="body" class:none={item.category === 'image' && expanded && !!detail?.image}>
    {#if item.category === 'color'}
      <span class="swatch" style:--sw={item.color ?? 'transparent'}></span>
      <span class="hex mono">{item.preview}</span>
    {:else if item.category === 'image'}
      <!-- Once the full-size preview is showing below, the thumbnail would just repeat it. -->
      {#if expanded && detail?.image}
        <!-- nothing: the header already says the dimensions -->
      {:else if thumb}<img class="thumb" src={thumb} alt="Copied item, {item.width} by {item.height} pixels" />{:else}<span class="thumb skeleton"></span>{/if}
    {:else if item.category === 'sensitive'}
      <Slot name="lock" size={16} />
      <span class="masked">{item.preview}</span>
      {#if remaining !== null}
        <span class="count" title="Wiped from memory automatically">{formatCountdown(remaining)}</span>
      {/if}
    {:else if item.category === 'snippet'}
      <div class="snippet">
        <strong>{snippetTitle}</strong>
        {#if snippetBody}<span class="mono">{snippetBody}</span>{/if}
      </div>
    {:else if item.category === 'code' && item.preview_html}
      <!-- eslint-disable-next-line svelte/no-at-html-tags -- HTML escaped and highlighted by zlip-highlight in Rust -->
      <pre class="code hl">{@html item.preview_html}</pre>
    {:else}
      <p class="text" class:mono={item.category === 'code' || item.category === 'file'}>{item.preview}</p>
    {/if}
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
            {#if meta}<p class="meta">{meta}</p>{/if}

            <div class="actions">
              <button class="primary" onclick={on.onpaste}>Paste <span class="keycap">↵</span></button>
              {#if isText}<button onclick={on.onplain}>Plain <span class="keycap">⇧↵</span></button>{/if}
              <button onclick={on.onqueue}><Slot name="queue" size={14} />Queue</button>
              {#if !item.read_only && item.category !== 'sensitive'}
                <button onclick={on.onpin}><Slot name="pin" size={14} />{item.pinned ? 'Unpin' : 'Pin'}</button>
              {/if}
              {#if detail?.editable_text != null}<button onclick={on.onedit}><Slot name="edit" size={14} />Edit</button>{/if}
              {#if item.category === 'sensitive'}
                <button onclick={on.onreveal}><Slot name="reveal" size={14} />{revealed !== null ? 'Hide' : 'Reveal 8 s'}</button>
              {/if}
              {#each detail?.actions ?? [] as name (name)}
                <button onclick={() => on.onaction(name)}><Slot name="action" size={14} />{name}</button>
              {/each}
              {#if !item.read_only}<button class="danger" onclick={on.ondelete}><Slot name="delete" size={14} />Delete</button>{/if}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </div>

  <button class="chev" aria-label={expanded ? 'Collapse' : 'Expand'} aria-expanded={expanded} tabindex="-1" onclick={(e) => { e.stopPropagation(); on.onexpand(); }}>
    <Slot name="chevron" size={14} />
    <span class="chev-glyph"></span>
  </button>
</div>

<style>
  .card {
    position: relative;
    padding: 10px 40px 10px 16px;
    border-radius: var(--r-lg);
    background: var(--card);
    border: 1px solid var(--hairline);
    box-shadow: inset 0 1px 0 var(--highlight);
    cursor: pointer;
    transition: background var(--dur-fast), border-color var(--dur-fast), box-shadow var(--dur) var(--ease), transform var(--dur) var(--ease);
    animation: rise var(--dur) var(--ease) both;
    animation-delay: calc(var(--i) * 16ms);
    content-visibility: auto;
    contain-intrinsic-size: auto 78px;
  }
  /* The pastel accent bar is what ties a card to its tab above. */
  .card::before {
    content: '';
    position: absolute;
    left: 6px;
    top: 11px;
    bottom: 11px;
    width: 3px;
    border-radius: 3px;
    background: var(--cat);
  }
  .card:hover {
    background: var(--card-hover);
  }
  .card[aria-selected='true'] {
    background: color-mix(in srgb, var(--cat) 13%, var(--card-hover));
    border-color: color-mix(in srgb, var(--cat) 58%, var(--hairline));
    box-shadow: var(--shadow-selected), inset 0 1px 0 var(--highlight);
    transform: translateY(-1px);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  header {
    display: flex;
    align-items: center;
    gap: 7px;
    min-height: 20px;
    font-size: 11px;
    color: var(--ink-3);
  }
  .kind {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 650;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: color-mix(in srgb, var(--cat) var(--kind-mix), var(--ink));
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--cat);
  }
  .label {
    font-weight: 500;
    color: var(--ink-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .label::before {
    content: '·';
    margin-right: 7px;
    color: var(--ink-3);
  }
  .spacer {
    flex: 1;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 18px;
    padding: 0 7px;
    border-radius: 999px;
    font-size: 10.5px;
    font-weight: 600;
    color: var(--ink);
    background: color-mix(in srgb, var(--cat-file) 45%, transparent);
  }
  .badge.q {
    background: color-mix(in srgb, var(--cat-snippet) 55%, transparent);
    font-variant-numeric: tabular-nums;
  }
  .age {
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .body {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 5px;
    min-width: 0;
  }
  .body.none {
    display: none;
  }
  .text {
    margin: 0;
    font-size: 13.5px;
    line-height: 1.42;
    letter-spacing: -0.004em;
    color: var(--ink);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  .text.mono {
    font-size: 12.5px;
    letter-spacing: 0;
  }
  .code {
    margin: 0;
    font: 12.5px/1.5 var(--font-mono);
    white-space: pre;
    overflow: hidden;
    text-overflow: ellipsis;
    max-height: 3em;
    min-width: 0;
    flex: 1;
    color: var(--ink);
  }
  .snippet {
    display: grid;
    gap: 1px;
    min-width: 0;
  }
  .snippet strong {
    font-size: 13.5px;
    font-weight: 620;
    letter-spacing: -0.004em;
  }
  .snippet span {
    font-size: 12px;
    color: var(--ink-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .swatch {
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: var(--r-md);
    /* Checkerboard under the colour so transparent values read as transparent. */
    background:
      linear-gradient(var(--sw), var(--sw)),
      conic-gradient(#0002 25%, #0000 0 50%, #0002 0 75%, #0000 0) 0 0 / 10px 10px;
    box-shadow: inset 0 0 0 1px var(--hairline-strong);
  }
  .hex {
    font-size: 13px;
    letter-spacing: 0.02em;
  }
  .thumb {
    display: block;
    max-width: 100%;
    max-height: 76px;
    border-radius: var(--r-sm);
    box-shadow: 0 0 0 1px var(--hairline);
    object-fit: cover;
  }
  .thumb.skeleton {
    width: 120px;
    height: 68px;
  }
  .skeleton {
    background: linear-gradient(90deg, var(--code-bg), color-mix(in srgb, var(--ink) 10%, transparent), var(--code-bg)) 0 0 / 200% 100%;
    animation: shimmer 1.4s linear infinite;
    border-radius: var(--r-sm);
  }
  .line {
    display: block;
    height: 14px;
    width: 70%;
  }
  @keyframes shimmer {
    to {
      background-position: -200% 0;
    }
  }
  .masked {
    font-weight: 550;
    letter-spacing: 0.01em;
  }
  .count {
    margin-left: auto;
    font: 600 11px/1 var(--font-ui);
    font-variant-numeric: tabular-nums;
    color: color-mix(in srgb, var(--cat-sensitive) 60%, var(--ink));
  }
  /* A hairline that drains over the secret's lifetime. */
  .drain {
    position: absolute;
    left: 14px;
    right: 14px;
    bottom: 4px;
    height: 2px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--cat-sensitive) 35%, transparent);
    transform-origin: left;
    transform: scaleX(var(--p));
    transition: transform 1s linear;
  }

  .more {
    display: grid;
    grid-template-rows: 0fr;
    transition: grid-template-rows var(--dur) var(--ease);
  }
  .more[data-open='true'] {
    grid-template-rows: 1fr;
  }
  .more-inner {
    overflow: hidden;
    min-height: 0;
  }
  .detail {
    display: grid;
    gap: 9px;
    padding-top: 10px;
    cursor: default;
  }
  .full {
    margin: 0;
    max-height: 230px;
    overflow: auto;
    padding: 10px 12px;
    border-radius: var(--r-md);
    background: var(--code-bg);
    font: 12px/1.55 var(--font-mono);
    white-space: pre;
    user-select: text;
  }
  .full.wrap {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font: 13px/1.5 var(--font-ui);
  }
  img.full {
    padding: 0;
    max-width: 100%;
    object-fit: contain;
  }
  .files {
    margin: 0;
    padding: 8px 12px;
    list-style: none;
    max-height: 150px;
    overflow: auto;
    border-radius: var(--r-md);
    background: var(--code-bg);
    font-size: 11.5px;
    line-height: 1.6;
    user-select: text;
  }
  .files li {
    overflow-wrap: anywhere;
  }
  .secret {
    margin: 0;
    padding: 10px 12px;
    border-radius: var(--r-md);
    background: color-mix(in srgb, var(--cat-sensitive) 22%, transparent);
    font-size: 12.5px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }
  .meta,
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
  .actions button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border-radius: var(--r-sm);
    font-size: 12px;
    font-weight: 550;
    background: color-mix(in srgb, var(--ink) 7%, transparent);
    transition: background var(--dur-fast), transform var(--dur-fast) var(--ease);
  }
  .actions button:hover {
    background: color-mix(in srgb, var(--ink) 13%, transparent);
  }
  .actions button:active {
    transform: scale(0.97);
  }
  .actions .primary {
    background: color-mix(in srgb, var(--cat) 45%, transparent);
  }
  .actions .primary:hover {
    background: color-mix(in srgb, var(--cat) 62%, transparent);
  }
  .actions .danger:hover {
    background: color-mix(in srgb, var(--cat-sensitive) 55%, transparent);
  }
  .keycap {
    min-width: 18px;
    height: 17px;
    padding: 0 5px;
    font-size: 10px;
  }

  .chev {
    position: absolute;
    top: 8px;
    right: 8px;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    color: var(--ink-3);
    transition: background var(--dur-fast), color var(--dur-fast), transform var(--dur) var(--ease);
  }
  .chev:hover {
    background: color-mix(in srgb, var(--ink) 10%, transparent);
    color: var(--ink);
  }
  .expanded .chev {
    transform: rotate(180deg);
  }
  .chev > :global(*) {
    grid-area: 1 / 1;
  }
  /* A drawn chevron until you map the `chevron` slot (see icons.css). */
  .chev-glyph {
    display: var(--fallback-chevron, block);
    width: 7px;
    height: 7px;
    border-right: 1.6px solid currentColor;
    border-bottom: 1.6px solid currentColor;
    transform: translateY(-2px) rotate(45deg);
  }
</style>
