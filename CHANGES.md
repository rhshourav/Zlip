# What changed

## Win+V stopped working after the first use

Reasoned from the code (the Win32 layer had never been run on Windows):

1. **Held-key re-fire.** The hook asked `GetAsyncKeyState` whether `V` was still down to detect
   auto-repeat. Windows never updates that state for a key the hook swallowed, so holding `V` for
   even half a second re-fired the hotkey and toggled the overlay open/closed in a loop.
   The hook now tracks the swallowed key itself (`crates/zlip-platform/src/win/hook.rs`).
2. **One thread did everything.** Hotkeys and clipboard capture shared the controller thread. A busy
   clipboard can block capture for ~800 ms, and one panic silently killed the thread while the hook
   kept swallowing Win+V. Capture now has its own thread, every handler is wrapped in
   `catch_unwind`, and panics are written to the log (`controller.rs`, `logging.rs`).
   Release builds now unwind instead of abort (`Cargo.toml`) so that guard works there too.
3. **Fragile focus.** The overlay could be "visible" but without keyboard focus. `force_foreground`
   now lifts the foreground lock properly, focus is verified and retried, and pressing the shortcut
   while the overlay is visible-but-unfocused brings it forward instead of hiding it (`windows.rs`).
4. **No second path.** A backup shortcut (`Ctrl+Alt+V`, configurable, `""` = off) is registered with
   `RegisterHotKey`, which Windows never silently removes. Using it also refreshes the hook.
   Tray -> "Shortcut not working? Reconnect" and Settings -> Reconnect do the same on demand.
5. Rapid repeats of the open/close request (< 250 ms) are ignored.

## UI

* Rebuilt as a compact, Apple-style interface: quiet neutral surfaces, one blue accent, a small tile
  per item kind instead of labels/dots/accent bars, one filter row (only kinds you actually have),
  one footer (a "Paste as" menu plus at most two hints), status tags only when relevant.
* New Settings screen (sliders button, or tray -> Settings...). Everything saves as you change it:
  shortcuts (with a recorder), theme, window size (Compact / Standard / Large), glass tint, animations,
  open-at-cursor, close-on-blur, private mode, history limits, secret wipe time, clear history,
  open settings file / logs.
* Default window is now 360 x 500 (was 440 x 640). A config file that still holds exactly the old
  untouched 440 x 640 is upgraded once, in place, keeping its comments.
* Built-in icon set (`src/lib/icons.ts`) replaces the empty icon slots.

## Dev noise

`file watcher error: EBUSY ...` is harmless: `tauri dev` was watching `target/` while the compiler
wrote to it. `.taurignore` (project root and `src-tauri/`) stops that.
Quit Zlip from the tray (closing the window only hides it) before re-running `tauri dev`.

## Verify (nothing below could be compiled or run where this was written)

```powershell
cargo fmt --all
cargo test --workspace          # includes new tests for the config upgrade, backup chord, key mapping
npm run check ; npm test        # svelte-check, eslint, vitest (new tests in chords.test.ts)
npm.cmd run tauri dev
```
`zlip.schema.json` was edited by hand for the new field and the new default size. If the
`the_committed_json_schema_is_current` test complains, regenerate it:
`cargo run -p zlip-config --example export_schema > zlip.schema.json`
