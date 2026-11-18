// The only file that talks to the Rust side. Outside Tauri (plain `npm run dev` in a
// browser) every call is answered by ./mock so the UI can be designed without Windows.

import type {
  ActionReport,
  Category,
  FolderView,
  ItemDetail,
  ItemView,
  Notice,
  QueueView,
  Settings,
  SettingsView,
  StatusView,
  Transform,
  UiConfig
} from './types';

export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) return (await import('./mock')).mockCall<T>(cmd, args);
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

/** Subscribe to a backend event. Resolves to the unsubscribe function. */
export async function listen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (!inTauri) return (await import('./mock')).mockListen(event, handler as (p: unknown) => void);
  const { listen: tauriListen } = await import('@tauri-apps/api/event');
  return tauriListen<T>(event, (e) => handler(e.payload));
}

export const api = {
  search: (query: string, category: Category | null, folder: string | null) =>
    call<ItemView[]>('search', { query, category, folder }),
  folders: () => call<FolderView[]>('folders'),
  itemDetail: (id: number) => call<ItemDetail | null>('item_detail', { id }),
  thumbnail: (id: number) => call<string | null>('thumbnail', { id }),
  reveal: (id: number) => call<string | null>('reveal', { id }),
  paste: (id: number, transforms: Transform[], plain: boolean) => call<void>('paste', { id, transforms, plain }),
  pasteText: (text: string, transforms: Transform[]) => call<void>('paste_text', { text, transforms }),
  saveText: (text: string) => call<void>('save_text', { text }),
  togglePin: (id: number) => call<boolean | null>('toggle_pin', { id }),
  deleteItem: (id: number) => call<boolean>('delete_item', { id }),
  clearHistory: () => call<number>('clear_history'),
  queueAdd: (id: number) => call<QueueView>('queue_add', { id }),
  queueClear: () => call<void>('queue_clear'),
  status: () => call<StatusView>('status'),
  setPrivateMode: (on: boolean) => call<void>('set_private_mode', { on }),
  uiConfig: () => call<UiConfig>('ui_config'),
  takeNotice: () => call<Notice | null>('take_notice'),
  hideOverlay: () => call<void>('hide_overlay'),
  openConfig: () => call<void>('open_config'),
  openLogs: () => call<void>('open_logs'),
  getSettings: () => call<SettingsView>('get_settings'),
  saveSettings: (settings: Settings) => call<void>('save_settings', { settings }),
  repairShortcuts: () => call<void>('repair_shortcuts'),
  pauseShortcuts: (paused: boolean) => call<void>('pause_shortcuts', { paused }),
  runAction: (id: number, name: string) => call<ActionReport>('run_action', { id, name }),
  trustAction: (name: string, sha256: string) => call<void>('trust_action', { name, sha256 })
};
