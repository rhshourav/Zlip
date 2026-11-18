<script lang="ts">
  import { onDestroy } from 'svelte';
  import { api } from '../../lib/api';
  import { chordCaps, chordFromEvent, hasModifier } from '../../lib/chords';
  import { ov } from '../../lib/overlay.svelte';

  let {
    value,
    label,
    clearable = false,
    win = false,
    onchange
  }: {
    value: string;
    label: string;
    /** Backspace removes the shortcut (turns it off). */
    clearable?: boolean;
    /** Add the Windows key to whatever is recorded (the browser cannot see it reliably). */
    win?: boolean;
    onchange: (value: string) => void;
  } = $props();

  let recording = $state(false);
  let problem = $state('');

  // While recording, Zlip's own global shortcuts are paused, so pressing one records it
  // instead of triggering it.
  async function start(): Promise<void> {
    recording = true;
    ov.recording = true;
    problem = '';
    await api.pauseShortcuts(true).catch(() => {});
  }

  async function stop(): Promise<void> {
    if (!recording) return;
    recording = false;
    ov.recording = false;
    await api.pauseShortcuts(false).catch(() => {});
  }

  function onkeydown(e: KeyboardEvent): void {
    if (!recording) return;
    // The recorder owns the keyboard: nothing reaches the rest of the app.
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape') return void stop();
    if (clearable && (e.key === 'Backspace' || e.key === 'Delete')) {
      onchange('');
      return void stop();
    }
    if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) return; // wait for the real key
    const chord = chordFromEvent(e, win);
    if (!chord) {
      problem = 'That key can’t be used.';
    } else if (!hasModifier(chord)) {
      problem = 'Hold Ctrl, Alt or Shift too.';
    } else {
      onchange(chord);
      void stop();
    }
  }

  onDestroy(() => {
    if (recording) void api.pauseShortcuts(false).catch(() => {});
    ov.recording = false;
  });
</script>

<button
  class="field"
  class:rec={recording}
  aria-label="{label}: {value || 'none'}. Press to record a new shortcut."
  onclick={() => (recording ? void stop() : void start())}
  {onkeydown}
  onblur={() => void stop()}
>
  {#if recording}
    <span class="prompt" class:bad={!!problem}>{problem || 'Press new shortcut…'}</span>
  {:else if value}
    <span class="caps">{#each chordCaps(value) as c, i (i)}<span class="keycap">{c}</span>{/each}</span>
  {:else}
    <span class="prompt">None</span>
  {/if}
</button>

<style>
  .field {
    display: inline-flex;
    align-items: center;
    justify-content: flex-end;
    min-width: 96px;
    max-width: 190px;
    height: 26px;
    padding: 0 6px;
    border-radius: var(--r-sm);
    background: var(--field);
    transition:
      background var(--dur-fast),
      box-shadow var(--dur-fast);
  }
  .field:hover {
    background: color-mix(in srgb, var(--ink) 10%, transparent);
  }
  .field.rec {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    box-shadow: 0 0 0 1.5px var(--accent);
  }
  .caps {
    display: inline-flex;
    gap: 3px;
  }
  .prompt {
    padding: 0 2px;
    font-size: 12px;
    color: var(--ink-3);
    white-space: nowrap;
  }
  .rec .prompt {
    color: var(--accent-ink);
  }
  .prompt.bad {
    color: var(--danger);
  }
</style>
