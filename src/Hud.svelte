<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './components/Icon.svelte';
  import { api, listen } from './lib/api';
  import { applyTheme } from './lib/theme';
  import { chordCaps } from './lib/chords';
  import type { QueueView, UiConfig } from './lib/types';

  let q = $state<QueueView>({ mode: 'collecting', len: 0, next_preview: null });
  let cfg = $state<UiConfig | null>(null);

  onMount(() => {
    void api.uiConfig().then((c) => {
      cfg = c;
      applyTheme(c);
    });
    void api.status().then((s) => (q = s.queue));
    const un = listen<QueueView>('zlip:queue', (v) => (q = v));
    return () => void un.then((u) => u());
  });

  const collectKeys = $derived(chordCaps(cfg?.hotkeys.queue_collect ?? 'Ctrl+Shift+Q').join('+'));
  const pasteKeys = $derived(chordCaps(cfg?.hotkeys.queue_paste ?? 'Ctrl+V').join('+'));
  const hint = $derived(
    q.mode === 'draining'
      ? q.next_preview
        ? `Next: ${q.next_preview}`
        : `${pasteKeys} pastes the next item`
      : `${collectKeys} to start pasting`
  );
  const title = $derived(q.mode === 'draining' ? `Pasting · ${q.len} left` : `Collecting · ${q.len}`);
</script>

<!-- Click-through status pill: it never takes focus or clicks from the app you are pasting into. -->
<div class="pill" class:draining={q.mode === 'draining'} role="status" aria-live="polite">
  <span class="mark"><span class="pulse"></span><Icon name="queue" size={14} /></span>
  <span class="text">
    <strong>{title}</strong>
    <span class="next">{hint}</span>
  </span>
</div>

<style>
  .pill {
    display: flex;
    align-items: center;
    gap: 12px;
    height: calc(100% - 12px);
    margin: 6px;
    padding: 0 16px 0 14px;
    border-radius: 18px;
    background: color-mix(in srgb, var(--panel-solid) 90%, transparent);
    box-shadow:
      0 0 0 1px var(--hairline-strong),
      var(--shadow);
    --c: var(--cat-code);
  }
  .pill.draining {
    --c: var(--cat-snippet);
  }
  .mark {
    position: relative;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
  }
  .mark > :global(*) {
    grid-area: 1 / 1;
  }
  .pulse {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--c);
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--c) 60%, transparent);
    animation: pulse 1.8s var(--ease) infinite;
  }
  @keyframes pulse {
    70% {
      box-shadow: 0 0 0 9px transparent;
    }
    100% {
      box-shadow: 0 0 0 0 transparent;
    }
  }
  .text {
    display: grid;
    min-width: 0;
    line-height: 1.25;
  }
  strong {
    font-size: 13px;
    font-weight: 650;
  }
  .next {
    font-size: 11px;
    color: var(--ink-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
