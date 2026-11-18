<script lang="ts">
  import Icon from './Icon.svelte';
  import Segmented from './settings/Segmented.svelte';
  import ShortcutField from './settings/ShortcutField.svelte';
  import Toggle from './settings/Toggle.svelte';
  import { api } from '../lib/api';
  import { hasModifier, hasWin, withWin } from '../lib/chords';
  import { ov } from '../lib/overlay.svelte';

  interface Opt<T> {
    value: T;
    label: string;
  }

  const c = $derived(ov.settings?.config ?? null);

  // ---- window size: three presets, because a number box is the wrong tool for this ----
  const SIZES = [
    { id: 'compact', label: 'Compact', w: 320, h: 420 },
    { id: 'standard', label: 'Standard', w: 360, h: 500 },
    { id: 'large', label: 'Large', w: 420, h: 600 }
  ] as const;
  type SizeId = (typeof SIZES)[number]['id'];
  const sizeId = $derived<SizeId | null>(c ? (SIZES.find((s) => s.w === c.ui.width && s.h === c.ui.height)?.id ?? null) : null);

  function setSize(id: SizeId): void {
    const s = SIZES.find((x) => x.id === id);
    if (s)
      ov.change((cfg) => {
        cfg.ui.width = s.w;
        cfg.ui.height = s.h;
      });
  }

  // ---- choices for the pop-up menus ----
  const ITEMS: Opt<number>[] = [100, 250, 500, 1000, 5000].map((n) => ({ value: n, label: `${n.toLocaleString()} items` }));
  const AGES: Opt<string>[] = [
    { value: '1d', label: '1 day' },
    { value: '7d', label: '1 week' },
    { value: '30d', label: '1 month' },
    { value: '365d', label: '1 year' },
    { value: 'never', label: 'Forever' }
  ];
  const WIPE: Opt<number>[] = [
    { value: 30, label: '30 seconds' },
    { value: 60, label: '1 minute' },
    { value: 120, label: '2 minutes' },
    { value: 300, label: '5 minutes' }
  ];
  /** A value someone typed into the settings file by hand still has to show up, not vanish. */
  function withCurrent<T extends string | number>(options: Opt<T>[], current: T): Opt<T>[] {
    return options.some((o) => o.value === current) ? options : [...options, { value: current, label: `${current} (custom)` }];
  }

  // ---- shortcuts ----
  function toggleWin(on: boolean): void {
    if (!c) return;
    const next = withWin(c.hotkeys.open_overlay, on);
    if (!hasModifier(next)) {
      ov.showNotice({ level: 'warn', message: 'Record a shortcut that uses Ctrl, Alt or Shift first. Then you can turn the Windows key off.' });
      return;
    }
    ov.change((cfg) => (cfg.hotkeys.open_overlay = next));
  }

  // ---- clearing history asks twice, in place, instead of opening a dialog ----
  let confirming = $state(false);
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;
  function clearHistory(): void {
    if (!confirming) {
      confirming = true;
      confirmTimer = setTimeout(() => (confirming = false), 3500);
      return;
    }
    clearTimeout(confirmTimer);
    confirming = false;
    void ov.clearHistory();
  }
</script>

<div class="settings">
  <header>
    <button class="back" onclick={() => void ov.closeSettings()} aria-label="Back to clipboard history">
      <Icon name="chevron-left" size={16} />
      Back
    </button>
    <h1>Settings</h1>
    <span class="balance" aria-hidden="true"></span>
  </header>

  <div class="body">
    {#if c && ov.settings}
      <section>
        <h2>Shortcuts</h2>
        <div class="group">
          <div class="r">
            <span class="l">Open Zlip</span>
            <ShortcutField label="Open Zlip" value={c.hotkeys.open_overlay} win={hasWin(c.hotkeys.open_overlay)} onchange={(v) => ov.change((cfg) => (cfg.hotkeys.open_overlay = v))} />
          </div>
          <div class="r">
            <span class="l">
              Windows key
              <small>Win+V replaces the Windows clipboard panel.</small>
            </span>
            <Toggle label="Use the Windows key" checked={hasWin(c.hotkeys.open_overlay)} onchange={toggleWin} />
          </div>
          <div class="r">
            <span class="l">
              Backup shortcut
              <small>Still works if the main one stops responding.</small>
            </span>
            <ShortcutField label="Backup shortcut" value={c.hotkeys.open_overlay_backup} clearable onchange={(v) => ov.change((cfg) => (cfg.hotkeys.open_overlay_backup = v))} />
          </div>
          {#if ov.settings.backup_shortcut === 'unavailable'}
            <div class="r bad">
              <span class="l">Windows didn’t accept the backup shortcut. Another app is probably using it, so try a different one.</span>
            </div>
          {/if}
          <div class="r">
            <span class="l">
              Shortcut not responding?
              <small>Reconnects the keyboard hook.</small>
            </span>
            <button class="btn" onclick={() => void ov.repairShortcuts()}>Reconnect</button>
          </div>
        </div>
      </section>

      <section>
        <h2>Appearance</h2>
        <div class="group">
          <div class="r">
            <span class="l">Theme</span>
            <Segmented
              label="Theme"
              value={c.theme.mode}
              options={[
                { id: 'auto', label: 'Auto' },
                { id: 'light', label: 'Light' },
                { id: 'dark', label: 'Dark' }
              ]}
              onchange={(v) => ov.change((cfg) => (cfg.theme.mode = v))}
            />
          </div>
          <div class="r">
            <span class="l">
              Window size
              {#if sizeId === null}<small>Custom: {c.ui.width} × {c.ui.height}</small>{/if}
            </span>
            <Segmented label="Window size" value={sizeId} options={SIZES.map((s) => ({ id: s.id, label: s.label }))} onchange={setSize} />
          </div>
          <div class="r">
            <span class="l">Glass tint</span>
            <input
              class="range"
              type="range"
              min="0"
              max="0.8"
              step="0.02"
              aria-label="Glass tint"
              style:--p="{(c.theme.tint_opacity / 0.8) * 100}%"
              value={c.theme.tint_opacity}
              oninput={(e) => ov.change((cfg) => (cfg.theme.tint_opacity = Number(e.currentTarget.value)))}
            />
          </div>
          <div class="r">
            <span class="l">Animations</span>
            <Toggle label="Animations" checked={c.theme.animations} onchange={(v) => ov.change((cfg) => (cfg.theme.animations = v))} />
          </div>
        </div>
      </section>

      <section>
        <h2>Behavior</h2>
        <div class="group">
          <div class="r">
            <span class="l">
              Open at the text cursor
              <small>Otherwise it opens at the bottom of the screen.</small>
            </span>
            <Toggle label="Open at the text cursor" checked={c.ui.position === 'caret'} onchange={(v) => ov.change((cfg) => (cfg.ui.position = v ? 'caret' : 'center'))} />
          </div>
          <div class="r">
            <span class="l">Close when clicking away</span>
            <Toggle label="Close when clicking away" checked={c.ui.close_on_blur} onchange={(v) => ov.change((cfg) => (cfg.ui.close_on_blur = v))} />
          </div>
          <div class="r">
            <span class="l">
              Private mode
              <small>Don’t save anything you copy.</small>
            </span>
            <Toggle label="Private mode" checked={ov.privateMode} onchange={(v) => void ov.setPrivate(v)} />
          </div>
        </div>
      </section>

      <section>
        <h2>History</h2>
        <div class="group">
          <div class="r">
            <span class="l">Keep up to</span>
            <select aria-label="Keep up to" value={c.retention.max_items} onchange={(e) => ov.change((cfg) => (cfg.retention.max_items = Number(e.currentTarget.value)))}>
              {#each withCurrent(ITEMS, c.retention.max_items) as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
            </select>
          </div>
          <div class="r">
            <span class="l">Keep for</span>
            <select aria-label="Keep for" value={c.retention.max_age} onchange={(e) => ov.change((cfg) => (cfg.retention.max_age = e.currentTarget.value))}>
              {#each withCurrent(AGES, c.retention.max_age) as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
            </select>
          </div>
          <div class="r">
            <span class="l">
              Wipe secrets after
              <small>Passwords and tokens are never written to disk.</small>
            </span>
            <select aria-label="Wipe secrets after" value={c.security.sensitive_ttl_secs} onchange={(e) => ov.change((cfg) => (cfg.security.sensitive_ttl_secs = Number(e.currentTarget.value)))}>
              {#each withCurrent(WIPE, c.security.sensitive_ttl_secs) as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
            </select>
          </div>
          <div class="r">
            <span class="l">
              Clear history
              <small>Pinned items stay.</small>
            </span>
            <button class="btn danger" onclick={clearHistory}>{confirming ? 'Click again to confirm' : 'Clear…'}</button>
          </div>
        </div>
      </section>

      <section>
        <h2>About</h2>
        <div class="group">
          <div class="r">
            <span class="l">Zlip</span>
            <span class="val">Version {ov.settings.version}</span>
          </div>
          <div class="r">
            <span class="l">
              Settings file
              <small class="path">{ov.settings.config_path}</small>
            </span>
            <button class="btn" onclick={() => void api.openConfig()}>Open</button>
          </div>
          <div class="r">
            <span class="l">
              Log files
              <small>Attach these if something goes wrong.</small>
            </span>
            <button class="btn" onclick={() => void api.openLogs()}>Open</button>
          </div>
        </div>
        <p class="note">Changes are saved as you make them.</p>
      </section>
    {:else}
      <p class="note center">Loading…</p>
    {/if}
  </div>
</div>

<style>
  .settings {
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
    overflow-y: auto;
    display: grid;
    /* minmax(0, 1fr): a long path or label must shrink, not widen the whole screen. */
    grid-template-columns: minmax(0, 1fr);
    align-content: start;
    gap: 16px;
    padding: 6px 2px 12px;
  }

  h2 {
    margin: 0 0 5px;
    padding: 0 10px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--ink-3);
  }
  .group {
    border-radius: var(--r-lg);
    background: var(--group);
    box-shadow: 0 0 0 1px var(--hairline);
    overflow: hidden;
  }
  .r {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 40px;
    padding: 7px 12px;
  }
  /* A hairline between rows, inset from the left like a native list. */
  .r + .r::before {
    content: '';
    position: absolute;
    top: 0;
    left: 12px;
    right: 0;
    height: 1px;
    background: var(--hairline);
  }
  .l {
    display: grid;
    gap: 1px;
    min-width: 0;
    font-size: 13px;
  }
  small {
    font-size: 11px;
    line-height: 1.3;
    color: var(--ink-3);
  }
  small.path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 190px;
    font-family: var(--font-mono);
    font-size: 10px;
  }
  .r.bad .l {
    color: var(--danger);
    font-size: 12px;
  }
  .val {
    font-size: 12px;
    color: var(--ink-3);
  }
  .note {
    margin: 6px 0 0;
    padding: 0 10px;
    font-size: 11px;
    color: var(--ink-3);
  }
  .note.center {
    text-align: center;
    padding-top: 24px;
  }

  select {
    appearance: none;
    height: 26px;
    max-width: 150px;
    padding: 0 24px 0 9px;
    border: 0;
    border-radius: var(--r-sm);
    background: var(--field) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='14' viewBox='0 0 10 14' fill='none' stroke='%23888' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m2 5.5 3-3 3 3M2 8.5l3 3 3-3'/%3E%3C/svg%3E") no-repeat right 7px center;
    font-size: 12px;
    color: var(--ink);
    cursor: pointer;
  }
  select:hover {
    background-color: color-mix(in srgb, var(--ink) 10%, transparent);
  }
  option {
    background: var(--panel-solid);
    color: var(--ink);
  }

  .range {
    appearance: none;
    width: 124px;
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(90deg, var(--accent) var(--p), color-mix(in srgb, var(--ink) 16%, transparent) var(--p));
    cursor: pointer;
  }
  .range::-webkit-slider-thumb {
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    box-shadow:
      0 0 0 0.5px rgb(0 0 0 / 0.15),
      0 1px 3px rgb(0 0 0 / 0.3);
  }
</style>
