<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import { chordCaps } from '../lib/chords';
  import { ov } from '../lib/overlay.svelte';
  import { characters, entropyBits, passwordLength, rulesSummary, STRENGTH_LABEL, strength } from '../lib/password';

  const g = $derived(ov.config?.password_generator ?? null);
  const chars = $derived(characters(ov.password));
  const bits = $derived(g ? entropyBits(g) : 0);
  const level = $derived(strength(bits));
  const newCaps = $derived(chordCaps(ov.config?.hotkeys.generate_password ?? 'Ctrl+G'));

  // The first password lands the focus on "Paste", so Enter does what the hint says. After that the
  // focus stays wherever the user put it (pressing "New" must not jump away from "New").
  let pasteButton: HTMLButtonElement | undefined = $state();
  let focused = false;
  $effect(() => {
    if (ov.password && !focused) {
      focused = true;
      void tick().then(() => pasteButton?.focus());
    }
  });
</script>

<div class="gen">
  <header>
    <button class="back" onclick={() => void ov.back()} aria-label="Back to clipboard history">
      <Icon name="chevron-left" size={16} />
      Back
    </button>
    <h1>Password generator</h1>
    <span class="balance" aria-hidden="true"></span>
  </header>

  <div class="body">
    {#if g}
      <output class="pw" class:none={!ov.password} aria-label="Generated password">
        {#key ov.password}
          <span class="text">{#each chars as c, i (i)}<span class={c.kind}>{c.ch}</span>{/each}</span>
        {/key}
      </output>

      <div class="meter" role="meter" aria-label="Strength" aria-valuemin="0" aria-valuemax="128" aria-valuenow={bits} aria-valuetext={STRENGTH_LABEL[level]}>
        <span class="fill {level}" style:width="{Math.min(100, (bits / 128) * 100)}%"></span>
      </div>
      <p class="meta">
        <span class="level {level}">{STRENGTH_LABEL[level]}</span>
        <span aria-hidden="true">·</span>
        {passwordLength(g)} characters
        <span aria-hidden="true">·</span>
        about {bits} bits
      </p>

      <div class="actions">
        <button class="btn" onclick={() => void ov.newPassword()}><Icon name="refresh" size={13} />New</button>
        <button class="btn" disabled={!ov.password} onclick={() => void ov.copyPassword()}><Icon name="copy" size={13} />Copy</button>
        <button class="btn primary" bind:this={pasteButton} disabled={!ov.password} onclick={() => void ov.pastePassword()}>Paste <span class="keycap">↵</span></button>
      </div>

      <p class="rules">
        {rulesSummary(g)}
        <button class="link" onclick={() => void ov.openSettings()}>Change</button>
      </p>
    {/if}
  </div>

  <div class="hints" aria-label="Keyboard shortcuts">
    <span class="hint"><span class="keycap">Ctrl</span><span class="keycap">C</span> Copy</span>
    <span class="hint">
      {#each newCaps as k (k)}<span class="keycap">{k}</span>{/each}
      New
    </span>
  </div>
</div>

<style>
  .gen {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    flex: 1;
  }

  header {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    height: 34px;
    flex: none;
  }
  h1 {
    margin: 0;
    font-size: 13.5px;
    font-weight: 600;
  }
  .back {
    display: inline-flex;
    align-items: center;
    justify-self: start;
    gap: 2px;
    height: 28px;
    padding: 0 8px 0 4px;
    border-radius: var(--r-sm);
    font-size: 13px;
    color: var(--accent-ink);
    transition: background var(--dur-fast);
  }
  .back:hover {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    align-content: start;
    gap: 10px;
    padding: 10px 2px;
  }

  /* The password: big, monospaced and selectable, with digits and symbols in colour so it is easy to read out. */
  .pw {
    display: grid;
    align-items: center;
    min-height: 92px;
    padding: 14px 16px;
    border-radius: var(--r-lg);
    background: var(--group);
    box-shadow: 0 0 0 1px var(--hairline);
    font-family: var(--font-mono);
    font-size: 17px;
    line-height: 1.55;
    letter-spacing: 0.04em;
    overflow-wrap: anywhere;
    user-select: text;
    cursor: text;
  }
  .text {
    animation: settle var(--dur) var(--ease) both;
  }
  .none {
    color: var(--ink-3);
  }
  .letter {
    color: var(--ink);
  }
  .digit {
    color: var(--accent-ink);
  }
  .symbol {
    color: color-mix(in srgb, var(--danger) 82%, var(--ink));
  }
  @keyframes settle {
    from {
      opacity: 0;
      transform: translateY(2px);
    }
  }

  .meter {
    height: 4px;
    margin: 0 2px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--ink) 10%, transparent);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
    transition:
      width var(--dur) var(--ease),
      background var(--dur);
  }
  .fill.weak {
    background: var(--danger);
  }
  .fill.fair {
    background: #d68a1f;
  }
  .fill.strong {
    background: #2c9d64;
  }
  .meta {
    margin: -2px 2px 0;
    font-size: 11.5px;
    color: var(--ink-3);
  }
  .level {
    font-weight: 600;
    color: var(--accent-ink);
  }
  .level.weak {
    color: var(--danger);
  }
  .level.fair {
    color: #d68a1f;
  }
  .level.strong {
    color: #2c9d64;
  }

  .actions {
    display: grid;
    grid-template-columns: 1fr 1fr 1.4fr;
    gap: 6px;
    margin-top: 2px;
  }
  .actions .btn {
    height: 32px;
  }

  .rules {
    margin: 2px 2px 0;
    font-size: 11.5px;
    color: var(--ink-3);
  }
  .link {
    margin-left: 4px;
    color: var(--accent-ink);
  }
  .link:hover {
    text-decoration: underline;
  }

  .hints {
    display: flex;
    justify-content: flex-end;
    gap: 14px;
    flex: none;
    padding: 8px 6px 10px;
    border-top: 1px solid var(--hairline);
    font-size: 11.5px;
    color: var(--ink-3);
  }
  .hint {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
</style>
