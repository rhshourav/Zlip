// All overlay behaviour lives here; components only render and forward events.

import { api, listen } from './api';
import { applyTheme } from './theme';
import { CATEGORY_ORDER } from './types';
import type { Category, FolderView, ItemDetail, ItemView, Notice, QueueView, Transform, UiConfig } from './types';

/** Only one of each group can be active: you pick *a* casing, *a* Base64 direction. */
const CASING: Transform[] = ['camel_case', 'pascal_case', 'snake_case', 'kebab_case', 'screaming_snake_case', 'upper', 'lower'];
const BASE64: Transform[] = ['base64_decode', 'base64_encode'];

const REVEAL_MS = 8000;
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

  selectedIndex = $derived(this.items.findIndex((i) => i.id === this.selectedId));
  selected = $derived<ItemView | null>(this.items[this.selectedIndex] ?? null);
  /** Transforms only make sense for things that become text. */
  canTransform = $derived(this.selected !== null && this.selected.category !== 'image');

  #seq = 0;
  #noticeTimer: ReturnType<typeof setTimeout> | undefined;
  #revealTimer: ReturnType<typeof setTimeout> | undefined;
  #unlisten: Array<() => void> = [];

  async init(): Promise<void> {
    this.config = await api.uiConfig();
    applyTheme(this.config);
    this.#unlisten = await Promise.all([
      listen<null>('zlip:overlay-shown', () => void this.onShown()),
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
    this.shownTick++;
    const [status, notice] = await Promise.all([api.status(), api.takeNotice()]);
    this.privateMode = status.private_mode;
    this.queue = status.queue;
    if (notice) this.showNotice(notice);
    await this.refresh();
  }

  async refresh(): Promise<void> {
    const seq = ++this.#seq;
    const [items, folders] = await Promise.all([api.search(this.query, this.category, this.folder), api.folders()]);
    if (seq !== this.#seq) return; // a newer keystroke already replaced this result
    this.items = items;
    this.folders = folders;
    this.loadedAt = Date.now();
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

  setCategory(c: Category | null): void {
    this.category = c;
    this.collapse();
    void this.refresh();
  }

  setFolder(f: string | null): void {
    this.folder = this.folder === f ? null : f;
    this.collapse();
    void this.refresh();
  }

  cycleCategory(delta: 1 | -1): void {
    const order: Array<Category | null> = [null, ...CATEGORY_ORDER];
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
    if (this.editing) this.editing = false;
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
