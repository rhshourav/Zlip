// A believable fake backend for browser development and visual checks. Not shipped
// behaviour: the real engine is in Rust. Keep it small and obviously fake.

import { parsePattern } from './password';
import type { Category, FolderView, ItemDetail, ItemView, Notice, QueueView, Settings, SettingsView, StatusView, UiConfig } from './types';

const now = Date.now();
const min = 60_000;

const kw = (t: string) => `<span class="z-keyword z-control z-rust">${t}</span>`;
const fn = (t: string) => `<span class="z-entity z-name z-function z-rust">${t}</span>`;
const str = (t: string) => `<span class="z-string z-quoted z-double z-json">${t}</span>`;
const pun = (t: string) => `<span class="z-punctuation z-json">${t}</span>`;

const base: Omit<ItemView, 'id' | 'category' | 'preview' | 'created_at_ms'> = {
  label: null,
  preview_html: null,
  color: null,
  pinned: false,
  folder: null,
  size_bytes: 0,
  width: null,
  height: null,
  source_app: null,
  expires_in_ms: null,
  queue_position: null,
  read_only: false
};

let items: ItemView[] = [
  { ...base, id: 1, category: 'code', label: 'JSON', created_at_ms: now - 20_000, source_app: 'Code', size_bytes: 214,
    preview: '{\n  "name": "zlip", "version": "0.1.0"',
    preview_html: `${pun('{')}\n  ${str('"name"')}${pun(':')} ${str('"zlip"')}${pun(',')} ${str('"version"')}${pun(':')} ${str('"0.1.0"')}` },
  { ...base, id: 2, category: 'link', label: 'github.com', created_at_ms: now - 4 * min, source_app: 'Chrome', size_bytes: 41,
    preview: 'https://github.com/rhshourav/zlip/pull/42' },
  { ...base, id: 3, category: 'text', created_at_ms: now - 11 * min, source_app: 'Outlook', size_bytes: 91,
    preview: 'Meeting moved to 3pm. Bring the Q4 slides and the updated roadmap draft for review before Friday.' },
  { ...base, id: 4, category: 'color', label: '#C5CAF5', color: '#C5CAF5', created_at_ms: now - 25 * min, source_app: 'Figma',
    preview: '#C5CAF5' },
  { ...base, id: 5, category: 'image', label: '1920 × 1080', created_at_ms: now - 40 * min, source_app: 'Snipping tool', width: 1920, height: 1080,
    size_bytes: 412_000, preview: '1920 × 1080' },
  { ...base, id: 6, category: 'sensitive', label: 'API key', created_at_ms: now - 18_000, expires_in_ms: 42_000,
    preview: 'API key • 40 characters' },
  { ...base, id: 7, category: 'file', label: '3 files', created_at_ms: now - 2 * 3_600_000, source_app: 'Explorer', size_bytes: 180,
    preview: 'report-q4.pdf  +2 more' },
  { ...base, id: 8, category: 'code', label: 'Rust', created_at_ms: now - 3 * 3_600_000, source_app: 'Code', size_bytes: 96,
    preview: 'fn main() {\n    println!("hello");',
    preview_html: `${kw('fn')} ${fn('main')}${pun('()')} ${pun('{')}\n    ${fn('println!')}${pun('(')}${str('"hello"')}${pun(');')}` },
  { ...base, id: 9, category: 'text', created_at_ms: now - 26 * 3_600_000, pinned: true, source_app: 'Outlook', size_bytes: 24,
    preview: 'Best regards,\nRhshourav' },
  { ...base, id: 10, category: 'text', created_at_ms: now - 5 * 86_400_000, source_app: 'Terminal', size_bytes: 41,
    preview: 'kubectl rollout restart deployment/api' }
];

const snippets: ItemView[] = [
  { ...base, id: 1000, category: 'snippet', label: 'Terminal', folder: 'Terminal', created_at_ms: 0, read_only: true, size_bytes: 23,
    preview: 'Docker prune\ndocker system prune -af' },
  { ...base, id: 1001, category: 'snippet', label: 'Terminal', folder: 'Terminal', created_at_ms: 0, read_only: true, size_bytes: 24,
    preview: 'Undo last commit\ngit reset --soft HEAD~1' },
  { ...base, id: 1002, category: 'snippet', label: 'Signatures', folder: 'Signatures', created_at_ms: 0, read_only: true, size_bytes: 40,
    preview: 'Email signature\nBest, Rhshourav · {date}' }
];

let settings: Settings = {
  schema: 1,
  hotkeys: {
    open_overlay: 'Win+V', open_overlay_backup: 'Ctrl+Alt+V', queue_collect: 'Ctrl+Shift+Q', queue_paste: 'Ctrl+V', paste_plain: 'Shift+Enter',
    quick_paste: 'Alt+{1..9}', expand: 'Tab', pin: 'Ctrl+P', queue_add: 'Ctrl+Q', delete: 'Delete', close: 'Escape',
    prev_category: 'Ctrl+Left', next_category: 'Ctrl+Right', generate_password: 'Ctrl+G'
  },
  theme: {
    mode: 'auto', tint_opacity: 0.14, corner_radius: 24, animations: true,
    categories: { text: '#C5CAF5', link: '#A9D6F5', code: '#D6C4F5', image: '#FAD0B8', color: '#F8C4D8', file: '#F7E3A6', snippet: '#BDEBD0', sensitive: '#F5B5AD' }
  },
  ui: { width: 360, height: 500, position: 'caret', search_limit: 50, close_on_blur: true, hint_hide_after: 5 },
  retention: { max_items: 500, max_age: '30d', max_image_mb: 25, max_text_kb: 1024 },
  security: { sensitive_ttl_secs: 60, min_token_len: 16, min_entropy_bits: 3.5, blacklist_exes: ['bitwarden.exe', '1password.exe'], blacklist_titles: ['Bitwarden', '1Password'] },
  password_generator: { enabled: true, mode: 'random', length: 20, lowercase: true, uppercase: true, digits: true, symbols: true, pattern: 'LL######', avoid_lookalikes: false },
  snippet_folders: [],
  actions: [],
  automations: []
};

const mockConfig = (): UiConfig => ({
  hotkeys: { ...settings.hotkeys },
  theme: { ...settings.theme },
  ui: { ...settings.ui },
  password_generator: { ...settings.password_generator },
  os: { rounded: true, native_backdrop: false, transparency: true, dark: window.matchMedia('(prefers-color-scheme: dark)').matches },
  action_names: ['Run lint hook']
});

/** Stand-in for the Rust generator, only so the screen can be designed in a browser. Not for real passwords. */
function mockPassword(g: Settings['password_generator']): string {
  if (!g.enabled) throw new Error('The password generator is turned off in Settings.');
  const skip = g.avoid_lookalikes ? 'O0oIl1' : '';
  const choose = (from: string[]): string => from[crypto.getRandomValues(new Uint32Array(1))[0]! % from.length]!;
  if (g.mode === 'pattern') {
    const parsed = parsePattern(g.pattern);
    if (!parsed.ok) throw new Error(`The password pattern cannot be used: ${parsed.error}`);
    const sets = {
      capital: [...'ABCDEFGHIJKLMNOPQRSTUVWXYZ'],
      small: [...'abcdefghijklmnopqrstuvwxyz'],
      digit: [...'0123456789'],
      symbol: [...'!@#$%^&*()-_=+[]{};:,.?']
    };
    return parsed.slots.map((s) => (s.kind === 'literal' ? s.ch : choose(sets[s.kind].filter((c) => !skip.includes(c))))).join('');
  }
  const kinds = [g.lowercase && 'abcdefghijklmnopqrstuvwxyz', g.uppercase && 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', g.digits && '0123456789', g.symbols && '!@#$%^&*()-_=+[]{};:,.?']
    .filter((k): k is string => typeof k === 'string')
    .map((k) => [...k].filter((c) => !skip.includes(c)));
  if (kinds.length === 0) throw new Error('Turn on at least one kind of character in the password generator settings.');
  const pool = kinds.flat();
  const out = [...kinds.map(choose)];
  while (out.length < g.length) out.push(choose(pool));
  for (let i = out.length - 1; i > 0; i--) {
    const j = crypto.getRandomValues(new Uint32Array(1))[0]! % (i + 1);
    [out[i], out[j]] = [out[j]!, out[i]!];
  }
  return out.join('');
}

let privateMode = false;
let autostart = false;
let queue: QueueView = { mode: 'idle', len: 0, next_preview: null };
const queued: number[] = [];

const listeners = new Map<string, Set<(p: unknown) => void>>();
export function mockListen(event: string, handler: (p: unknown) => void): () => void {
  const set = listeners.get(event) ?? new Set();
  set.add(handler);
  listeners.set(event, set);
  return () => set.delete(handler);
}
function emit(event: string, payload: unknown = null) {
  listeners.get(event)?.forEach((h) => h(payload));
}

function thumb(): string {
  const svg =
    '<svg xmlns="http://www.w3.org/2000/svg" width="160" height="90"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">' +
    '<stop offset="0" stop-color="#C5CAF5"/><stop offset="1" stop-color="#FAD0B8"/></linearGradient></defs>' +
    '<rect width="160" height="90" fill="url(#g)"/><circle cx="120" cy="28" r="12" fill="#fff" fill-opacity=".7"/>' +
    '<path d="M0 90 L50 48 L82 72 L116 40 L160 80 V90Z" fill="#fff" fill-opacity=".55"/></svg>';
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function withQueue(list: ItemView[]): ItemView[] {
  return list.map((i) => ({ ...i, queue_position: queued.includes(i.id) ? queued.indexOf(i.id) + 1 : null }));
}

function refreshQueue() {
  queue = { mode: queued.length ? (queue.mode === 'idle' ? 'collecting' : queue.mode) : 'idle', len: queued.length, next_preview: queued.length ? (items.find((i) => i.id === queued[0])?.preview.split('\n')[0] ?? null) : null };
  emit('zlip:queue', queue);
}

export async function mockCall<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const id = args.id as number | undefined;
  const result = ((): unknown => {
    switch (cmd) {
      case 'search': {
        const q = String(args.query ?? '').toLowerCase().split(/\s+/).filter(Boolean);
        const category = args.category as Category | null;
        const folder = args.folder as string | null;
        let pool: ItemView[] = items;
        if (folder === '@pinned') pool = items.filter((i) => i.pinned);
        else if (folder) pool = snippets.filter((s) => s.folder === folder);
        else if (q.length) pool = [...items, ...snippets];
        return withQueue(
          pool
            .filter((i) => !category || i.category === category)
            .filter((i) => q.every((w) => `${i.preview} ${i.label ?? ''}`.toLowerCase().includes(w)))
            .sort((a, b) => b.created_at_ms - a.created_at_ms)
        );
      }
      case 'folders': {
        const out: FolderView[] = [];
        const pinned = items.filter((i) => i.pinned).length;
        if (pinned) out.push({ name: 'Pinned', count: pinned, kind: 'pinned', filter: '@pinned' });
        out.push({ name: 'Terminal', count: 2, kind: 'snippets', filter: 'Terminal' });
        out.push({ name: 'Signatures', count: 1, kind: 'snippets', filter: 'Signatures' });
        return out;
      }
      case 'item_detail': {
        const it = [...items, ...snippets].find((i) => i.id === id);
        if (!it) return null;
        const text = it.preview;
        const detail: ItemDetail = {
          id: it.id, html: it.preview_html ? it.preview_html + '\n' + (it.category === 'code' ? '  <span class="z-comment">// …</span>' : '') : escapeHtml(text),
          language: it.category === 'code' ? it.label : null, reformatted: it.category === 'code', truncated: false,
          editable_text: it.category === 'image' || it.category === 'sensitive' || it.category === 'file' ? null : text,
          files: it.category === 'file' ? ['C:\\Users\\rhs\\Documents\\report-q4.pdf', 'C:\\Users\\rhs\\Documents\\budget.xlsx', 'C:\\Users\\rhs\\Pictures\\logo.png'] : [],
          image: it.category === 'image' ? thumb() : null, line_count: text.split('\n').length, char_count: text.length,
          size_bytes: it.size_bytes, source_app: it.source_app, actions: it.category === 'sensitive' || it.category === 'image' ? [] : ['Run lint hook']
        };
        return detail;
      }
      case 'thumbnail': return thumb();
      case 'reveal': return 'AKIAIOSFODNN7EXAMPLE-demo-secret';
      case 'paste':
      case 'paste_text':
        console.info('[mock] paste', args);
        emit('zlip:notice', { level: 'info', message: 'Pasted (mock backend).' } satisfies Notice);
        return null;
      case 'save_text': {
        const text = String(args.text ?? '');
        items = [{ ...base, id: Date.now(), category: 'text', created_at_ms: Date.now(), preview: text, size_bytes: text.length, source_app: 'Zlip' }, ...items];
        emit('zlip:items-changed');
        return null;
      }
      case 'toggle_pin': {
        const it = items.find((i) => i.id === id);
        if (!it) return null;
        it.pinned = !it.pinned;
        return it.pinned;
      }
      case 'delete_item': {
        const had = items.some((i) => i.id === id);
        items = items.filter((i) => i.id !== id);
        return had;
      }
      case 'clear_history': {
        const n = items.filter((i) => !i.pinned).length;
        items = items.filter((i) => i.pinned);
        return n;
      }
      case 'queue_add':
        if (id !== undefined && !queued.includes(id)) queued.push(id);
        refreshQueue();
        return queue;
      case 'queue_clear':
        queued.length = 0;
        queue = { mode: 'idle', len: 0, next_preview: null };
        emit('zlip:queue', queue);
        return null;
      case 'status': return { private_mode: privateMode, queue } satisfies StatusView;
      case 'set_private_mode': privateMode = Boolean(args.on); return null;
      case 'generate_password': return mockPassword(settings.password_generator);
      case 'copy_text':
        console.info('[mock] copy_text');
        return null;
      case 'get_autostart': return autostart;
      case 'set_autostart': autostart = Boolean(args.on); return null;
      case 'ui_config': return mockConfig();
      case 'get_settings':
        return { config: structuredClone(settings), version: '0.1.0', repo_url: 'https://github.com/rhshourav/zlip', config_path: 'C:\\Users\\you\\AppData\\Roaming\\zlip\\config.toml', log_dir: 'C:\\Users\\you\\AppData\\Local\\zlip\\logs', backup_shortcut: settings.hotkeys.open_overlay_backup ? 'active' : 'off' } satisfies SettingsView;
      case 'save_settings':
        settings = structuredClone(args.settings as Settings);
        emit('zlip:config', mockConfig());
        return null;
      case 'repair_shortcuts':
      case 'pause_shortcuts':
      case 'open_url':
      case 'open_logs':
      case 'open_config':
        return null;
      case 'take_notice': return null;
      case 'run_action': return { status: 'needs_trust', action: String(args.name), path: 'C:/tools/zlip/lint.ps1', sha256: '9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08' };
      default: return null;
    }
  })();
  return result as T;
}
