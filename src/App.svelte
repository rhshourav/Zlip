<script lang="ts">
  import { onMount, tick } from 'svelte';
  import FilterBar from './components/FilterBar.svelte';
  import Footer from './components/Footer.svelte';
  import ItemCard from './components/ItemCard.svelte';
  import SearchBar from './components/SearchBar.svelte';
  import Settings from './components/Settings.svelte';
  import Toast from './components/Toast.svelte';
  import { api } from './lib/api';
  import { type Chord, chordCaps, matchesChord, parseChord, parseQuickPaste, quickPasteIndex } from './lib/chords';
  import { recordHintUse } from './lib/hints';
  import { ov } from './lib/overlay.svelte';

  let search: SearchBar | undefined = $state();
  let listEl: HTMLElement | undefined = $state();
  let now = $state(Date.now());

  // The clock only needs to tick fast while a secret is counting down.
  const hasSecret = $derived(ov.items.some((i) => i.expires_in_ms !== null));
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), hasSecret ? 1000 : 20_000);
    return () => clearInterval(t);
  });

  onMount(() => {
    void ov.init();
    return () => ov.destroy();
  });

  // Opening the overlay (or coming back from Settings) always lands the caret in the search box.
  $effect(() => {
    void ov.shownTick;
    if (ov.view === 'list') void tick().then(() => search?.focus());
  });

  // Keep the selected card on screen as the selection moves.
  $effect(() => {
    const id = ov.selectedId;
    if (id === null) return;
    queueMicrotask(() => listEl?.querySelector(`[data-id="${id}"]`)?.scrollIntoView({ block: 'nearest' }));
  });

  // After a card finishes expanding, make sure the whole thing (buttons included) is visible.
  $effect(() => {
    const id = ov.expandedId;
    if (id === null) return;
    const t = setTimeout(() => listEl?.querySelector(`[data-id="${id}"]`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }), 260);
    return () => clearTimeout(t);
  });

  // Shortcuts, parsed once per config (and again on hot reload).
  const keys = $derived.by(() => {
    const h = ov.config?.hotkeys;
    if (!h) return null;
    return {
      close: parseChord(h.close),
      plain: parseChord(h.paste_plain),
      expand: parseChord(h.expand),
      pin: parseChord(h.pin),
      queue: parseChord(h.queue_add),
      del: parseChord(h.delete),
      prev: parseChord(h.prev_category),
      next: parseChord(h.next_category),
      quick: parseQuickPaste(h.quick_paste)
    };
  });

  function onKeydown(e: KeyboardEvent): void {
    if (!keys || e.isComposing || ov.editing) return; // the quick editor owns the keyboard
    const hit = (c: Chord | null) => c !== null && matchesChord(e, c);
    const did = (hint?: string) => {
      e.preventDefault();
      e.stopPropagation();
      if (hint) recordHintUse(hint);
    };
    const bare = !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey;

    // Settings: only "back" applies. A shortcut being recorded owns every key, Escape included.
    if (ov.view === 'settings') {
      if (!ov.recording && hit(keys.close)) return did(), void ov.back();
      return;
    }

    if (hit(keys.close)) return did(), void ov.back();
    if (e.key === 'ArrowDown' && bare) return did(), ov.move(1);
    if (e.key === 'ArrowUp' && bare) return did(), ov.move(-1);
    if (hit(keys.plain)) return did('plain'), void ov.pasteSelected(true);
    if (e.key === 'Enter' && bare) return did('paste'), void ov.pasteSelected();
    const n = keys.quick ? quickPasteIndex(e, keys.quick) : null;
    if (n !== null) return did('quick'), void ov.pasteNth(n);
    if (hit(keys.expand)) return did('expand'), void ov.toggleExpand();
    if (hit(keys.pin)) return did('pin'), void ov.togglePin();
    if (hit(keys.queue)) return did('queue'), void ov.queueAddSelected();
    if (hit(keys.prev)) return did(), ov.cycleCategory(-1);
    if (hit(keys.next)) return did(), ov.cycleCategory(1);
    // A bare Delete edits the search text while there is some; it removes the item only when the box is empty.
    if (hit(keys.del) && (ov.query === '' || !keys.del || keys.del.ctrl || keys.del.alt || keys.del.shift || keys.del.meta)) {
      return did(), void ov.remove();
    }
    // Type-to-search from anywhere: focus the box and let the keystroke land in it.
    if (e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey && !(e.target instanceof HTMLInputElement)) search?.focus();
  }

  async function runAction(id: number, name: string): Promise<void> {
    const r = await api.runAction(id, name);
    if (r.status === 'done') {
      const out = (r.stdout || r.stderr).trim();
      ov.showNotice({
        level: r.exit_code === 0 ? 'info' : 'warn',
        message: r.timed_out ? `“${name}” took too long and was stopped.` : `“${name}” finished (exit ${r.exit_code ?? '?'})${out ? `\n${out.slice(0, 400)}` : ''}`
      });
    } else if (r.status === 'needs_trust') {
      // Nothing has run. Show exactly what will run and require an explicit yes.
      const ok = window.confirm(`Run this script for the first time?\n\n${r.path}\n\nSHA-256:\n${r.sha256}\n\nZlip remembers this exact file. If it changes you will be asked again.`);
      if (ok) {
        try {
          await api.trustAction(r.action, r.sha256);
          await runAction(id, name);
        } catch (e) {
          ov.showNotice({ level: 'error', message: String(e) });
        }
      }
    } else {
      ov.showNotice({ level: 'error', message: r.message });
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main class="panel" aria-label="Zlip clipboard">
  {#if ov.view === 'settings'}
    <Settings />
  {:else}
    <SearchBar bind:this={search} />
    <FilterBar />

    <div class="list" role="listbox" tabindex="-1" aria-label="Clipboard items" aria-activedescendant={ov.selectedId === null ? undefined : `item-${ov.selectedId}`} bind:this={listEl}>
      {#each ov.items as item, i (item.id)}
        <ItemCard
          {item}
          index={i}
          selected={item.id === ov.selectedId}
          expanded={item.id === ov.expandedId}
          editing={ov.editing && item.id === ov.expandedId}
          detail={ov.details[item.id]}
          thumb={ov.thumbs[item.id]}
          revealed={ov.revealed?.id === item.id ? ov.revealed.text : null}
          loadedAt={ov.loadedAt}
          {now}
          onselect={() => ov.select(item.id)}
          onpaste={() => void ov.pasteSelected()}
          onplain={() => void ov.pasteSelected(true)}
          onexpand={() => void ov.toggleExpand()}
          onpin={() => void ov.togglePin()}
          ondelete={() => void ov.remove()}
          onqueue={() => void ov.queueAddSelected()}
          onedit={() => void ov.startEdit()}
          onreveal={() => (ov.revealed?.id === item.id ? ov.hideSecret() : void ov.reveal(item.id))}
          onaction={(name) => void runAction(item.id, name)}
          onpasteedited={(t) => void ov.pasteEdited(t)}
          onsaveedited={(t) => void ov.saveEdited(t)}
          oncanceledit={() => (ov.editing = false)}
        />
      {:else}
        <div class="empty">
          {#if ov.query}
            <strong>No results for “{ov.query}”</strong>
            <span>Search tolerates gaps, so “dkr” finds “docker”.</span>
          {:else if ov.folder}
            <strong>This folder is empty</strong>
            <span>Pin items with <span class="keycap">Ctrl</span><span class="keycap">P</span> or add snippets in your settings file.</span>
          {:else if ov.privateMode}
            <strong>Private mode is on</strong>
            <span>Nothing you copy is being saved.</span>
          {:else}
            <strong>Nothing copied yet</strong>
            <span>Copy something, then press {#each chordCaps(ov.config?.hotkeys.open_overlay ?? 'Win+V') as c, i (i)}<span class="keycap">{c}</span>{/each}</span>
          {/if}
        </div>
      {/each}
    </div>

    <Footer />
  {/if}
  <Toast />
</main>

<style>
  .panel {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
    height: 100%;
    padding: 10px 8px 0;
    /* The OS draws the blurred backdrop; this layer adds the tint and a lit edge. */
    background: var(--panel);
    box-shadow:
      inset 0 0 0 1px var(--hairline-strong),
      inset 0 1px 0 var(--highlight);
    border-radius: 8px; /* matches the radius Windows 11 gives the window */
    overflow: hidden;
  }
  :global(:root[data-rounded='no']) .panel {
    border-radius: 0;
  }

  .list {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    align-content: start;
    gap: 1px;
    overflow-y: auto;
    /* Let the list use the panel's full width: the scrollbar sits in the padding. */
    margin: 0 -6px;
    padding: 2px 6px 8px;
  }

  .empty {
    display: grid;
    gap: 4px;
    justify-items: center;
    align-content: center;
    min-height: 180px;
    padding: 24px;
    text-align: center;
    color: var(--ink-3);
    font-size: 12px;
    animation: fade var(--dur) var(--ease) both;
  }
  .empty strong {
    color: var(--ink-2);
    font-size: 13.5px;
    font-weight: 600;
  }
  .empty span {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
</style>
