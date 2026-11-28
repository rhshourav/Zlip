// Mirrors the DTOs in crates/zlip-engine/src/view.rs. Field names are snake_case on purpose.

export type Category = 'text' | 'link' | 'code' | 'image' | 'color' | 'file' | 'snippet' | 'sensitive';

export type Transform =
  | 'trim'
  | 'camel_case'
  | 'pascal_case'
  | 'snake_case'
  | 'kebab_case'
  | 'screaming_snake_case'
  | 'upper'
  | 'lower'
  | 'base64_decode'
  | 'base64_encode'
  | 'strip_tracking_params';

export interface ItemView {
  id: number;
  category: Category;
  label: string | null;
  preview: string;
  preview_html: string | null;
  color: string | null;
  created_at_ms: number;
  pinned: boolean;
  folder: string | null;
  size_bytes: number;
  width: number | null;
  height: number | null;
  source_app: string | null;
  expires_in_ms: number | null;
  queue_position: number | null;
  read_only: boolean;
}

export interface ItemDetail {
  id: number;
  html: string;
  language: string | null;
  reformatted: boolean;
  truncated: boolean;
  editable_text: string | null;
  files: string[];
  image: string | null;
  line_count: number;
  char_count: number;
  size_bytes: number;
  source_app: string | null;
  actions: string[];
}

export interface QueueView {
  mode: 'idle' | 'collecting' | 'draining';
  len: number;
  next_preview: string | null;
}

export interface StatusView {
  private_mode: boolean;
  queue: QueueView;
}

export interface FolderView {
  name: string;
  count: number;
  kind: 'pinned' | 'snippets';
  filter: string;
}

export type ActionReport =
  | { status: 'done'; exit_code: number | null; stdout: string; stderr: string; timed_out: boolean; truncated: boolean }
  | { status: 'needs_trust'; action: string; path: string; sha256: string }
  | { status: 'failed'; message: string };

export interface Hotkeys {
  open_overlay: string;
  /** A second shortcut that works even if the first stops responding. Empty = off. */
  open_overlay_backup: string;
  queue_collect: string;
  queue_paste: string;
  paste_plain: string;
  quick_paste: string;
  expand: string;
  pin: string;
  queue_add: string;
  delete: string;
  close: string;
  prev_category: string;
  next_category: string;
  /** In the overlay: open the password generator, and make a new password while it is open. */
  generate_password: string;
}

/** The built-in password generator's settings. */
export interface PasswordGeneratorSettings {
  /** Off hides the key button and ignores the shortcut. */
  enabled: boolean;
  /** `random` mixes the kinds below; `pattern` follows the shape written in `pattern`. */
  mode: 'random' | 'pattern';
  /** Random mode: 8 to 128 characters. */
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  /** Pattern mode: `L` capital, `l` small letter, `#` number, `*` symbol, `\x` keeps x, anything else is kept. */
  pattern: string;
  /** Leave out O 0 o I l 1, which are easy to mix up. */
  avoid_lookalikes: boolean;
}

export type CategoryColors = Record<Category, string>;

export interface UiConfig {
  hotkeys: Hotkeys;
  theme: {
    mode: 'auto' | 'light' | 'dark';
    tint_opacity: number;
    corner_radius: number;
    animations: boolean;
    categories: CategoryColors;
  };
  ui: { width: number; height: number; position: 'caret' | 'center'; search_limit: number; close_on_blur: boolean; hint_hide_after: number };
  password_generator: PasswordGeneratorSettings;
  os: { rounded: boolean; native_backdrop: boolean; transparency: boolean; dark: boolean };
  action_names: string[];
}

/** The whole settings file, as the Settings screen edits it. Sections it does not show are carried through untouched. */
export interface Settings {
  schema: number;
  hotkeys: Hotkeys;
  theme: UiConfig['theme'];
  ui: UiConfig['ui'];
  retention: { max_items: number; max_age: string; max_image_mb: number; max_text_kb: number };
  security: { sensitive_ttl_secs: number; min_token_len: number; min_entropy_bits: number; blacklist_exes: string[]; blacklist_titles: string[] };
  password_generator: PasswordGeneratorSettings;
  snippet_folders: unknown[];
  actions: unknown[];
  automations: unknown[];
}

export interface SettingsView {
  config: Settings;
  version: string;
  /** The project page on GitHub. It comes from the app itself, not from the settings file. */
  repo_url: string;
  config_path: string;
  log_dir: string;
  /** `unavailable`: Windows refused the backup shortcut (another program uses it). */
  backup_shortcut: 'off' | 'active' | 'unavailable';
}

export interface Notice {
  level: 'info' | 'warn' | 'error';
  message: string;
}

export const CATEGORY_LABEL: Record<Category, string> = {
  text: 'Text',
  link: 'Links',
  code: 'Code',
  image: 'Images',
  color: 'Colors',
  file: 'Files',
  snippet: 'Snippets',
  sensitive: 'Secrets'
};

/** Singular form for a single card ("LINK"), where the rail uses plurals ("Links"). */
export const CATEGORY_NAME: Record<Category, string> = {
  text: 'Text',
  link: 'Link',
  code: 'Code',
  image: 'Image',
  color: 'Color',
  file: 'Files',
  snippet: 'Snippet',
  sensitive: 'Secret'
};

/** Order of the category rail (after "All"). */
export const CATEGORY_ORDER: Category[] = ['text', 'code', 'link', 'image', 'color', 'file', 'snippet', 'sensitive'];
