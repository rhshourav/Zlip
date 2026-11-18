# Zlip

A fast, private, glass-styled clipboard manager for Windows. `Win+V` opens a
translucent overlay with fuzzy search, colour-coded kinds, snippet folders, a
paste queue, paste-time formatting and a quick editor. Everything is configurable
from one TOML file that reloads the moment you save it.

Built with Rust + Tauri 2 (Svelte 5 UI). Design and architecture rationale:
[`Zlip_Foundation_Blueprint.md`](./Zlip_Foundation_Blueprint.md).

> **Status: foundation milestone.** The engine, security layers, storage, UI and CI
> are built and tested. The Windows-only layer (keyboard hook, clipboard, paste,
> DWM backdrop) has been **type-checked and linted but never executed**, because
> it was written without access to a Windows machine. Run the
> [first-run checklist](#first-run-on-windows-do-this-first) before anything else.

## What is verified, and how

| Area | Evidence |
| --- | --- |
| Capture pipeline, search, paste preparation, queue, retention, vault expiry, actions | 180 Rust unit tests, run on Linux (`cargo test --workspace --exclude zlip --all-features`) |
| Secret handling: never in the database file, search index, logs or `Debug` output | Tests read the SQLite file and WAL for the plaintext |
| Win32 layer (`zlip-platform`) | Compiles against `windows-sys` and is clippy-clean with `--features typecheck` on Linux. Pure logic (chord matching, hook re-install policy) is unit-tested. **Behaviour on real Windows is untested.** |
| Tauri shell (`src-tauri`) | `cargo clippy --workspace --all-targets --all-features -D warnings` is clean |
| UI | `svelte-check` (0 errors, 0 warnings), ESLint, 10 Vitest tests, production build, and screenshots in Chromium in light and dark with the mock backend. **Not yet seen inside WebView2.** |

## Run it

Prerequisites: Windows 10/11, Rust (stable), Node 22, and the
[WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (preinstalled on Windows 11).

```powershell
npm install
npm run tauri dev        # the real app (tray icon; Win+V)
```

Design the UI without Windows: `npm run dev` and open the page. A built-in mock
backend answers every call, so the whole overlay works in a normal browser
(`/hud.html` previews the queue pill).

| Task | Command |
| --- | --- |
| Format / lint / test Rust | `cargo fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace` |
| Check / lint / test / build the UI | `npm run check`, `npm run lint`, `npm test`, `npm run build` |
| Type-check the Windows layer on Linux | `cargo check -p zlip --features zlip-platform/typecheck` |
| Regenerate the config JSON Schema | `cargo run -p zlip-config --example export_schema > zlip.schema.json` |

## First run on Windows: do this first

Work through these in order; each one proves a layer the tests cannot.

1. **Starts cleanly.** A tray icon appears; `%LOCALAPPDATA%\zlip\logs\zlip.log` has no errors.
2. **The hotkey.** `Win+V` opens *Zlip's* overlay (not Windows' clipboard panel), and releasing `Win` does **not** open the Start menu.
3. **The glass.** Windows 11 22H2+: native acrylic with rounded corners. Windows 10: square acrylic. Check legibility over a bright wallpaper; adjust `theme.tint_opacity`.
4. **Capture.** Copy text in Notepad and a browser, copy files in Explorer, take a screenshot (`Win+Shift+S`). Each shows up with the right kind.
5. **Paste.** Paste into Notepad, a browser field, Windows Terminal and VS Code. Try a snippet with `{cursor}`. (Apps running *as administrator* ignore a non-elevated Zlip, by Windows design.)
6. **Privacy.** Copy from Bitwarden/KeePass: nothing is captured. Copy `AKIAIOSFODNN7EXAMPLE`: it appears under **Secret**, counts down, and the clipboard is cleared at 0:00.
7. **Queue.** `Ctrl+Shift+Q`, copy three things, `Ctrl+Shift+Q`, then `Ctrl+V` three times. The HUD pill follows along.
8. **Config.** Put a typo in `%APPDATA%\zlip\config.toml`: a toast explains it and the old settings stay live. Fix it: it reloads.
9. **Resilience.** Lock/unlock and sleep/wake, then try `Win+V` again. Try 150% display scaling and a second monitor.
10. **Hold the keys.** Hold `Win+V` for two seconds: the overlay must open once and stay open (no flicker). Then press the backup shortcut (`Ctrl+Alt+V`); it must work even if `Win+V` is made to fail.
11. **Settings.** Click the sliders button (or tray → Settings…). Change the theme, size and shortcuts; each applies at once and survives a restart.

## Configuration

`%APPDATA%\zlip\config.toml` is created on first run, fully commented. Unknown keys
are errors (a typo never silently does nothing) and every problem is reported with
its path. A `zlip.schema.json` is written next to it, so editors with TOML schema
support (for example Even Better TOML) autocomplete and validate it.

Shortcuts, retention, the security thresholds and blacklist, pastel colours, snippet
folders, script actions and automations (such as stripping tracking parameters from
copied links) all live there.

## Security model

* **Secrets** (vendor API keys, JWTs, PEM keys, Luhn-valid cards, random-looking
  tokens) go to an in-memory vault, never to disk. They expire (default 60 s); the
  clipboard is cleared only if it still holds what was captured; buffers are zeroed.
* **Password managers** are skipped three ways: Microsoft's clipboard exclusion
  formats, the owning program's name, and the active window title.
* **History** is SQLite with every payload sealed by XChaCha20-Poly1305 (row id bound
  as associated data). The key lives in Windows Credential Manager.
* **Scripts** run only from your config, with the text on stdin, never in a shell,
  with a timeout, and only after you approve the file's SHA-256. Batch files are refused.
* **UI**: strict CSP, a least-privilege Tauri capability (listen/unlisten only), and
  clipboard-derived HTML is escaped in Rust before it reaches the webview.

Limits: malware running as your user can read the clipboard directly. The defence
is to keep secrets alive as briefly as possible, not to claim otherwise.

## Releases

`.github/workflows/` contains CI, the daily audit and the release pipeline (tag
`vX.Y.Z` → installers → draft release → Winget PR). One-time setup:

1. Bump `version` in the root `Cargo.toml` (it is the single source: `tauri.conf.json` deliberately has no `version`).
2. `npm run tauri signer generate -- -w zlip.key`, then store the key and password as the
   `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets (see the header of `release.yml`).
3. A local installer build needs that key too, or turn updater artifacts off for it:
   `npm run tauri build -- --config '{"bundle":{"createUpdaterArtifacts":false}}'`.

The placeholder icons in `src-tauri/icons/` exist only so builds work; replace them
with `npm run tauri icon <your-1024px.png>`. In-app icons are a built-in set in
`src/lib/icons.ts` (plain path data on a 24 px grid; add a glyph there and use
`<Icon name="..." />`).

## Layout

```
crates/
  zlip-highlight  language detection, pretty-print, escaped highlighting
  zlip-core       categoriser, transforms, placeholders, queue, fuzzy search, images
  zlip-config     TOML schema, validation, JSON Schema, hot reload
  zlip-security   secret detector, in-RAM vault, privacy blacklist
  zlip-store      encrypted SQLite history, retention
  zlip-actions    hash-pinned, time-limited script actions
  zlip-engine     the decisions: capture pipeline, index, paste prep (no OS, no UI)
  zlip-platform   the only crate with `unsafe`: hook, clipboard, input, DWM, credentials
src-tauri/        thin shell: windows, tray, controller thread, commands
src/              Svelte UI: overlay (index.html) and queue HUD (hud.html)
```

## Where this differs from the blueprint, and why

* **`windows-sys`, not `windows`.** Same Microsoft project, raw bindings, and (unlike `windows`) it type-checks on Linux, so CI can verify the Win32 code without Windows.
* **Credential Manager called directly**, not through `keyring` (v4 split into per-platform crates). About 40 lines, same DPAPI-backed store.
* **No tokio.** Timing is `crossbeam` channels and threads; the expiry scheduler is a pure, testable function.
* **`chrono`, not `time`**, because `{date:%Y-%m-%d}` is strftime syntax.
* **Hook watchdog = scheduled replacement.** Nothing reports a dropped hook, and mouse input confuses `GetLastInputInfo`, so the hook is re-installed every 30 s (new first, then old; skipped while a modifier is held).
* **Windows 10 gets a square panel.** Its acrylic cannot be clipped to rounded corners; Windows 11 22H2+ uses the native, corner-clipped backdrop. Cards use layered translucency rather than `backdrop-filter` (a transparent window has nothing behind it to blur).
* **Detector exemptions.** URLs, UUIDs, emails and file paths are not "random tokens"; without this, copying a link would make it vanish in a minute.
* **`VirtualLock` is deferred.** It is page-granular and does not nest, so per-secret locking is subtly wrong; it needs a dedicated locked arena.
* **New crate `zlip-engine`**, so the decision logic is tested anywhere.

## Not done yet

Settings GUI (edit the TOML meanwhile), in-app updater (artifacts are produced; the
plugin is not wired), autostart at login, HTTP webhook actions (script actions
exist), rich-text/HTML capture (text, files and images are captured), user-created
snippet folders in the UI (config folders and pins work), and the locked-memory arena.
