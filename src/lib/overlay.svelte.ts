// All overlay behaviour lives here; components only render and forward events.

import { api, listen } from './api';
import { applyTheme } from './theme';
import { CATEGORY_ORDER } from './types';
import type { Category, FolderView, ItemDetail, ItemView, Notice, QueueView, Settings, SettingsView, Transform, UiConfig } from './types';

/** Only one of each group can be active: you pick *a* casing, *a* Base64 direction. */
const CASING: Transform[] = ['camel_case', 'pascal_case', 'snake_case', 'kebab_case', 'screaming_snake_case', 'upper', 'lower'];
const BASE64: Transform[] = ['base64_decode', 'base64_encode'];

const REVEAL_MS = 8000;
/** Settings save themselves this long after the last change. */
const SAVE_DELAY_MS = 300;
const NOTICE_MS = { info: 3200, warn: 6000, error: 9000 } as const;

class Overlay {
  config = $state<UiConfig | null>(null);
  items = $state<ItemView[]>([]);
  folders = $state<FolderView[]>([]);
  query = $state('');
  category = $state<Category | null>(null);
  folder = $state<string | null>(null);
  selectedId = $state<number | null>(null);
  expandedId = $state<number | null>(null);
  editing = $state(false);
  details = $state<Record<number, ItemDetail>>({});
  thumbs = $state<Record<number, string>>({});
  transforms = $state<Transform[]>([]);
  plain = $state(false);
  queue = $state<QueueView>({ mode: 'idle', len: 0, next_preview: null });
  privateMode = $state(false);
  notice = $state<Notice | null>(null);
  /** When the current list was fetched; secrets count down from here. */
  loadedAt = $state(Date.now());
  revealed = $state<{ id: number; text: string } | null>(null);
  /** Bumped on every show so the view can re-focus the search box. */
  shownTick = $state(0);
  /** Which screen is showing. */
  view = $state<'list' | 'settings' | 'generator'>('list');
  /** The password on show in the generator. Kept only while that screen is open. */
  password = $state('');
  /** The "Paste as" menu. */
  menuOpen = $state(false);
  /** Kinds present in the unfiltered history, so the filter row only offers what exists. */
  available = $state<Category[]>([]);
  settings = $state<SettingsView | null>(null);
  /** True while a shortcut is being recorded: Escape then belongs to the recorder. */
  recording = $state(false);

  /** Whether the generator is offered at all (Settings → Password generator). */
  generatorOn = $derived(this.config?.password_generator.enabled ?? false);

  selectedIndex = $derived(this.items.findIndex((i) => i.id === this.selectedId));
  selected = $derived<ItemView | null>(this.items[this.selectedIndex] ?? null);
  /** Transforms only make sense for things that become text. */
  canTransform = $derived(this.selected !== null && this.selected.category !== 'image');

  #seq = 0;
  #noticeTimer: ReturnType<typeof setTimeout> | undefined;
  #revealTimer: ReturnType<typeof setTimeout> | undefined;
  #unlisten: Array<() => void> = [];
  #saveTimer: ReturnType<typeof setTimeout> | undefined;

  async init(): Promise<void> {
    try {
      this.config = await api.uiConfig();
      applyTheme(this.config);
    } catch (e) {
      this.showNotice({ level: 'error', message: `Could not load settings: ${String(e)}` });
    }
    this.#unlisten = await Promise.all([
      listen<null>('zlip:overlay-shown', () => void this.onShown()),
      listen<null>('zlip:open-settings', () => void this.openSettings()),
      listen<null>('zlip:items-changed', () => void this.refresh()),
      listen<QueueView>('zlip:queue', (q) => (this.queue = q)),
      listen<UiConfig>('zlip:config', (c) => {
        this.config = c;
        applyTheme(c);
      }),
      listen<Notice>('zlip:notice', (n) => this.showNotice(n))
    ]);
    await this.onShown();
  }

  destroy(): void {
    this.#unlisten.forEach((u) => u());
  }

  // ------------------------------------------------------------- lifecycle

  /** Every time the overlay opens it starts clean: no stale search, no half-open card. */
  async onShown(): Promise<void> {
    this.query = '';
    this.category = null;
    this.folder = null;
    this.expandedId = null;
    this.editing = false;
    this.transforms = [];
    this.plain = false;
    this.revealed = null;
    this.selectedId = null;
    this.view = 'list';
    this.password = '';
    this.menuOpen = false;
    this.recording = false;
    this.shownTick++;
    // Nothing below may stop the overlay from being usable: each step fails on its own.
    try {
      const [status, notice] = await Promise.all([api.status(), api.takeNotice()]);
      this.privateMode = status.private_mode;
      this.queue = status.queue;
      if (notice) this.showNotice(notice);
    } catch (e) {
      console.error('status', e);
    }
    await this.refresh();
  }

  async refresh(): Promise<void> {
    const seq = ++this.#seq;
    let items: ItemView[];
    let folders: FolderView[];
    try {
      [items, folders] = await Promise.all([api.search(this.query, this.category, this.folder), api.folders()]);
    } catch (e) {
      console.error('search', e);
      return;
    }
    if (seq !== this.#seq) return; // a newer keystroke already replaced this result
    this.items = items;
    this.folders = folders;
    this.loadedAt = Date.now();
    if (!this.category && !this.folder && !this.query) this.available = [...new Set(items.map((i) => i.category))];
    if (!items.some((i) => i.id === this.selectedId)) this.selectedId = items[0]?.id ?? null;
    if (this.expandedId !== null && !items.some((i) => i.id === this.expandedId)) this.collapse();
    for (const it of items) if (it.category === 'image') void this.loadThumb(it.id);
  }

  async loadThumb(id: number): Promise<void> {
    if (this.thumbs[id]) return;
    const url = await api.thumbnail(id);
    if (url) this.thumbs[id] = url;
  }

  // ------------------------------------------------------------- filtering

  setQuery(q: string): void {
    this.query = q;
    this.collapse();
    void this.refresh();
  }

  // A kind (Text, Files...) and a folder (Pinned, a snippet folder) are one choice, not two:
  // together they would only show the pinned *files*, which is usually nothing at all.
  setCategory(c: Category | null): void {
    this.category = c;
    if (c !== null) this.folder = null;
    this.collapse();
    void this.refresh();
  }

  setFolder(f: string | null): void {
    this.folder = this.folder === f ? null : f;
    if (this.folder !== null) this.category = null;
    this.collapse();
    void this.refresh();
  }

  clearFilters(): void {
    this.category = null;
    this.folder = null;
    this.collapse();
    void this.refresh();
  }

  cycleCategory(delta: 1 | -1): void {
    // Only the kinds that exist: the filter row hides the others, so skipping through them would look broken.
    const order: Array<Category | null> = [null, ...CATEGORY_ORDER.filter((c) => this.available.includes(c) || c === this.category)];
    const at = order.indexOf(this.category);
    this.setCategory(order[(at + delta + order.length) % order.length] ?? null);
  }

  // ------------------------------------------------------------- selection

  select(id: number): void {
    if (this.selectedId === id) return;
    this.selectedId = id;
    this.collapse();
  }

  move(delta: number): void {
    if (!this.items.length) return;
    const at = this.selectedIndex < 0 ? 0 : this.selectedIndex;
    const next = Math.min(this.items.length - 1, Math.max(0, at + delta));
    const id = this.items[next]?.id ?? null;
    if (id !== null) this.select(id);
  }

  // ------------------------------------------------------------ expansion

  collapse(): void {
    this.expandedId = null;
    this.editing = false;
    this.hideSecret();
  }

  async toggleExpand(): Promise<void> {
    const it = this.selected;
    if (!it) return;
    if (this.expandedId === it.id) {
      this.collapse();
      return;
    }
    this.expandedId = it.id;
    this.editing = false;
    if (!this.details[it.id]) {
      const d = await api.itemDetail(it.id);
      if (d) this.details[it.id] = d;
    }
  }

  async startEdit(): Promise<void> {
    if (this.selectedId === null) return;
    if (this.expandedId !== this.selectedId) await this.toggleExpand();
    const text = this.details[this.selectedId]?.editable_text;
    if (typeof text === 'string') this.editing = true;
  }

  /** Escape peels back one layer at a time and only closes the overlay at the end. */
  async back(): Promise<void> {
    if (this.menuOpen) this.menuOpen = false;
    else if (this.view === 'settings') await this.closeSettings();
    else if (this.view === 'generator') this.closeGenerator();
    else if (this.editing) this.editing = false;
    else if (this.revealed) this.hideSecret();
    else if (this.expandedId !== null) this.collapse();
    else if (this.query || this.category || this.folder) {
      this.query = '';
      this.category = null;
      this.folder = null;
      await this.refresh();
    } else await api.hideOverlay();
  }

  // ---------------------------------------------------------------- paste

  async pasteSelected(forcePlain = false): Promise<void> {
    const it = this.selected;
    if (it) await api.paste(it.id, this.canTransform ? this.transforms : [], forcePlain || this.plain);
  }

  async pasteNth(n: number): Promise<void> {
    const it = this.items[n - 1];
    if (it) await api.paste(it.id, it.category === 'image' ? [] : this.transforms, this.plain);
  }

  async pasteEdited(text: string): Promise<void> {
    this.editing = false;
    await api.pasteText(text, this.transforms);
  }

  async saveEdited(text: string): Promise<void> {
    try {
      await api.saveText(text);
      this.editing = false;
      this.showNotice({ level: 'info', message: 'Saved as a new item.' });
    } catch (e) {
      this.showNotice({ level: 'error', message: String(e) });
    }
  }

  // ----------------------------------------------------------- item actions

  async togglePin(): Promise<void> {
    const it = this.selected;
    if (!it || it.read_only || it.category === 'sensitive') return;
    await api.togglePin(it.id);
    await this.refresh();
  }

  async remove(): Promise<void> {
    const it = this.selected;
    if (!it || it.read_only) return;
    const at = this.selectedIndex;
    await api.deleteItem(it.id);
    delete this.details[it.id];
    this.selectedId = this.items[at + 1]?.id ?? this.items[at - 1]?.id ?? null;
    this.collapse();
    await this.refresh();
  }

  async queueAddSelected(): Promise<void> {
    const it = this.selected;
    if (!it) return;
    this.queue = await api.queueAdd(it.id);
    await this.refresh();
  }

  async clearQueue(): Promise<void> {
    await api.queueClear();
    await this.refresh();
  }

  async setPrivate(on: boolean): Promise<void> {
    await api.setPrivateMode(on);
    this.privateMode = on;
  }

  // -------------------------------------------------------------- settings

  async openSettings(): Promise<void> {
    this.menuOpen = false;
    this.password = '';
    this.view = 'settings';
    await this.loadSettings();
  }

  async closeSettings(): Promise<void> {
    await this.flushSettings();
    this.view = 'list';
    await this.refresh();
  }

  async loadSettings(): Promise<void> {
    try {
      this.settings = await api.getSettings();
    } catch (e) {
      this.showNotice({ level: 'error', message: String(e) });
    }
  }

  /** Edit a setting. It applies to the screen at once and is saved a moment later. */
  change(mutate: (c: Settings) => void): void {
    if (!this.settings) return;
    mutate(this.settings.config);
    this.#previewSettings();
    clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(() => void this.persistSettings(), SAVE_DELAY_MS);
  }

  /** Show look-and-feel changes immediately; the saved copy comes back from Rust shortly after. */
  #previewSettings(): void {
    const c = this.settings?.config;
    if (!c || !this.config) return;
    this.config = {
      ...this.config,
      hotkeys: $state.snapshot(c.hotkeys),
      theme: $state.snapshot(c.theme),
      ui: $state.snapshot(c.ui),
      password_generator: $state.snapshot(c.password_generator)
    };
    applyTheme(this.config);
  }

  async flushSettings(): Promise<void> {
    if (this.#saveTimer === undefined) return;
    clearTimeout(this.#saveTimer);
    this.#saveTimer = undefined;
    await this.persistSettings();
  }

  async persistSettings(): Promise<void> {
    this.#saveTimer = undefined;
    if (!this.settings) return;
    try {
      await api.saveSettings($state.snapshot(this.settings.config) as Settings);
      // Windows reports whether it accepted the backup shortcut a moment after it is set.
      setTimeout(() => void this.refreshBackupStatus(), 1300);
    } catch (e) {
      // Nothing was saved: show what the file really says, and why.
      this.showNotice({ level: 'error', message: String(e) });
      await this.loadSettings();
      if (this.settings && this.config) {
        this.config = {
          ...this.config,
          hotkeys: $state.snapshot(this.settings.config.hotkeys),
          theme: $state.snapshot(this.settings.config.theme),
          ui: $state.snapshot(this.settings.config.ui),
          password_generator: $state.snapshot(this.settings.config.password_generator)
        };
        applyTheme(this.config);
      }
    }
  }

  async refreshBackupStatus(): Promise<void> {
    try {
      const fresh = await api.getSettings();
      if (this.settings) this.settings.backup_shortcut = fresh.backup_shortcut;
    } catch {
      /* the status line just stays as it was */
    }
  }

  async repairShortcuts(): Promise<void> {
    try {
      await api.repairShortcuts();
      this.showNotice({ level: 'info', message: 'Shortcuts reconnected.' });
    } catch (e) {
      this.showNotice({ level: 'error', message: String(e) });
    }
  }

  async clearHistory(): Promise<void> {
    try {
      const n = await api.clearHistory();
      this.showNotice({ level: 'info', message: `${n === 1 ? '1 item' : `${n} items`} cleared. Pinned items were kept.` });
      await this.refresh();
    } catch (e) {
      this.showNotice({ level: 'error', message: String(e) });
    }
  }

  // ------------------------------------------------------- password generator

  async openGenerator(): Promise<void> {
    if (!this.generatorOn) return;
    this.menuOpen = false;
    this.collapse();
    this.view = 'generator';
    await this.newPassword();
  }

  closeGenerator(): void {
    this.password = '';
    this.view = 'list';
    void this.refresh();
  }

  /** Make a fresh password from the saved generator settings (the random numbers come from Windows). */
  async newPassword(): Promise<void> {
    try {
      this.password = await api.generatePassword();
    } catch (e) {
      this.password = '';
      this.showNotice({ level: 'error', message: String(e) });
    }
  }

  /** An ordinary copy: Zlip's clipboard watcher saves it like anything else you copy (not in private mode). */
  async copyPassword(): Promise<void> {
    if (!this.password) return;
    try {
      await api.copyText(this.password);
      this.showNotice({ level: 'info', message: this.privateMode ? 'Copied. Private mode is on, so it is not saved.' : 'Copied.' });
    } catch (e) {
      this.showNotice({ level: 'error', message: String(e) });
    }
  }

  /** Paste it where you were typing. Pasting does not go through the clipboard watcher, so it is saved here. */
  async pastePassword(): Promise<void> {
    const text = this.password;
    if (!text) return;
    if (!this.privateMode) {
      try {
        await api.saveText(text);
      } catch {
        /* not being saved must never stop the paste */
      }
    }
    this.password = '';
    await api.pasteText(text, []);
  }

  // --------------------------------------------------------------- secrets

  async reveal(id: number): Promise<void> {
    const text = await api.reveal(id);
    clearTimeout(this.#revealTimer);
    if (text === null) {
      this.showNotice({ level: 'warn', message: 'That secret has already expired.' });
      await this.refresh();
      return;
    }
    this.revealed = { id, text };
    this.#revealTimer = setTimeout(() => this.hideSecret(), REVEAL_MS);
  }

  hideSecret(): void {
    clearTimeout(this.#revealTimer);
    this.revealed = null;
  }

  // ------------------------------------------------------------ transforms

  toggleTransform(t: Transform): void {
    const on = this.transforms.includes(t);
    let next = this.transforms.filter((x) => x !== t);
    if (!on) {
      const group = CASING.includes(t) ? CASING : BASE64.includes(t) ? BASE64 : [];
      next = next.filter((x) => !group.includes(x));
      next.push(t);
    }
    this.transforms = next;
  }

  // --------------------------------------------------------------- notices

  showNotice(n: Notice): void {
    this.notice = n;
    clearTimeout(this.#noticeTimer);
    this.#noticeTimer = setTimeout(() => (this.notice = null), NOTICE_MS[n.level]);
  }

  dismissNotice(): void {
    clearTimeout(this.#noticeTimer);
    this.notice = null;
  }
}

export const ov = new Overlay();
