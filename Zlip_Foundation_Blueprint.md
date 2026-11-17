# Zlip: Foundation Blueprint

*Architect: rhshourav · Stack: Rust + Tauri v2 · Target: Windows 10 (1903+) and 11, portable core*

Zlip replaces `Win+V` with a keyboard-first, glass-styled clipboard manager. This document covers the UI/UX blueprint, Rust architecture, security protocol and CI/CD design. Every choice carries its reason. The workflow files (`ci.yml`, `release.yml`, `audit.yml`) are delivered separately.

## 1. UI/UX blueprint

### 1.1 Principles

1. **Keyboard first.** The hands are already on the keyboard when `Win+V` fires; reaching for the mouse costs time.
2. **Content is the interface.** Typography carries hierarchy; color only classifies.
3. **Fast in, fast out.** Visible in under 100 ms, gone the moment you paste. The overlay window is created at startup and kept hidden, because showing a hidden window takes milliseconds while cold-starting a webview takes seconds.
4. **Motion explains.** Animation marks a state change, never decoration.

### 1.2 The liquid-glass material

Layers, back to front:

| # | Layer | Spec | Why |
| --- | --- | --- | --- |
| 1 | OS backdrop | Acrylic via `window-vibrancy`; system corner rounding on Win11, CSS radius on Win10 | `backdrop-filter` only blurs pixels inside the webview, never the desktop behind it. Real blending needs the OS compositor |
| 2 | Tint scrim | `rgba(255,255,255,.14)` light, `rgba(20,22,30,.42)` dark | Keeps text contrast above 4.5:1 on any wallpaper |
| 3 | Card glass | fill `rgba(255,255,255,.08)`, `backdrop-filter: blur(20px) saturate(170%)` | Each item reads as its own lens |
| 4 | Specular edge | 1 px border fading `rgba(255,255,255,.38)` to `.06` top to bottom, plus a 135° sheen from `rgba(255,255,255,.28)` to transparent at 42% | A fixed light direction creates the liquid depth |
| 5 | Grain | 3% opacity noise tile | Hides banding in blurred gradients |
| 6 | Shadow | `0 24px 64px rgba(0,0,0,.28), 0 2px 8px rgba(0,0,0,.12)` | Separates the panel from busy backgrounds |

**Performance guard:** `backdrop-filter` is GPU-heavy, so it is applied only to cards in the viewport (the list is virtualized). When Windows Transparency effects are off, or `prefers-reduced-transparency` is set, layers 1 and 3 fall back to a solid 92% opaque fill.

### 1.3 Design tokens

**Spacing** (4 px grid): 4 · 8 · 12 · 16 · 24 · 32. **Radius:** window 24, card 12, chip 999. Cards sit 12 px inside the window, so 24 − 12 = 12 keeps corners concentric, which is what makes nested rounded shapes look intentional.

**Type:** UI in Inter (bundled, OFL) falling back to `Segoe UI Variable`; code in JetBrains Mono. Sizes 12 / 13 / 15 / 17, weights 400 / 500 / 600, line-height 1.4, tabular numerals for timestamps. *Why:* Apple's SF fonts cannot be redistributed on Windows; Inter is the closest legal match and stays crisp at 13 px.

**Category pastels** (shared lightness so no category shouts; at least 30° hue gap between neighbours):

| Category | Hex | Name |
| --- | --- | --- |
| Text | `#C5CAF5` | Periwinkle |
| Link | `#A9D6F5` | Sky |
| Code | `#D6C4F5` | Lavender |
| Image | `#FAD0B8` | Peach |
| Color / Hex | `#F8C4D8` | Rose |
| File | `#F7E3A6` | Butter |
| Pinned snippet | `#BDEBD0` | Mint |
| Sensitive | `#F5B5AD` | Coral |

Chips use the pastel at 24% alpha with a solid 6 px dot, and the category name is always spelled out, so color is never the only signal (color-blind safe).

**Syntax tokens** (dark / light): keyword `#CDB4F7` / `#7B4FC9` · string `#A8E6C3` / `#1F8F5F` · number `#FAD0B8` / `#B5541C` · function `#A9D6F5` / `#1E6FB8` · comment `#8B90A8` / `#6B7088` · punctuation `#C9CCDD` / `#4B4F63`. Highlighting emits CSS classes bound to these variables, so a theme is just a variable set.

### 1.4 Wireframe (macro layout only)

Default 440 × 640, configurable. It opens beside the text caret when one is detectable, otherwise bottom-centre of the active monitor with a 24 px gap. *Why:* the paste target is where the eyes already are. `▢` marks a reserved 20 px slot; its contents are yours to assign.

```
┌────────────────────────────────────────────┐
│ ┌────────────────────────────────────────┐ │
│ │ ▢  Search clipboard…             Esc   │ │  A Search bar     44
│ └────────────────────────────────────────┘ │
│  All  Text  Code  Links  Images  Colors  › │  B Category rail  32
│  Pinned ─────────────────────────────────  │
│  [Terminal] [Boilerplate] [Signatures] [+] │  C Folders        36
│  Recent ─────────────────────────────────  │
│ ┌────────────────────────────────────────┐ │
│ │ ▢  CODE · Rust                  2 min  │ │  D Item card      64
│ │    fn main() { run(); }                │ │    (selected card grows)
│ └────────────────────────────────────────┘ │
│ ┌────────────────────────────────────────┐ │
│ │ ▢  LINK · github.com           14 min  │ │
│ │    https://github.com/rhshourav/zlip   │ │
│ └────────────────────────────────────────┘ │
│              … virtualized list …          │
│ ┌────────────────────────────────────────┐ │
│ │ Paste as  Plain  camelCase  snake_case │ │  E Transform bar  40
│ │ Trim  Base64 decode           Queue 3  │ │
│ └────────────────────────────────────────┘ │
│  ↑↓ move   Enter paste   Tab edit          │  F Hint footer    28
└────────────────────────────────────────────┘
```

- **A. Search bar, top, autofocused.** Typing starts the instant the panel appears, with no click-to-search step. Top position matches reading order: find, then shape, then commit. Fuzzy matching (`nucleo`) runs in Rust per keystroke and returns the top 50 IDs.
- **B. Category rail.** Scopes the search without a dropdown; `Ctrl+←/→` cycles it. One 32 px line replaces a sidebar that would cost 120 px of width.
- **C. Snippet folders above recents.** Frequently used commands and signatures are reachable without scrolling. The row collapses while typing, because search results must take priority.
- **D. Item card.** Two lines when collapsed: a metadata line (category chip, language, age) and a content line in the right face (mono for code, swatch for colors, thumbnail for images). Constant density makes scanning fast.
- **E. Transform bar at the bottom.** Filters (what) sit at the top and verbs (how) at the bottom, the two ends of a find, shape, commit path. Toggles act on the selected item and preview live in the card before anything is pasted.
- **F. Hint footer.** Teaches shortcuts without a manual and hides itself once each has been used five times (configurable).

**Expanded card:** height animates open to show the full pretty-printed, highlighted content (12 lines, then scroll), a metadata row (source app, size, captured-at) and an action row. **Quick edit** turns the body into a textarea in place with *Paste edited*, *Save as new* and *Cancel*, so the original is never silently overwritten.

**Paste queue HUD:** a small glass pill in the screen corner shows `3 queued · next: ...` while collecting or draining. *Why:* the queue is a mode, and modes must be visible.

### 1.5 Interaction and motion

| Event | Motion | Time | Why |
| --- | --- | --- | --- |
| Show | fade 0→1, scale .97→1, rise 8 px, `cubic-bezier(.22,1,.36,1)` | 180 ms | Fast start, soft landing |
| Hide | fade out | 120 ms | Exits feel better quicker than entrances |
| Hover | sheen shifts 12 px, fill +4% | 120 ms | Shows the target without moving layout |
| Expand | grid rows `0fr → 1fr`, content fades in after 80 ms | 220 ms | Animates height without measuring it in JS |
| Paste | card compresses to .98, window hides | 80 ms | Confirms the action |
| Queue add | badge bounces, HUD slides in | 240 ms | Draws the eye to the one thing that changed |

Only `transform` and `opacity` animate (compositor-only), except expand. With reduced motion everything becomes an 80 ms fade.

**Default keys** (all remappable): `Win+V` open · type to search · `↑ ↓` move · `Enter` paste · `Shift+Enter` paste plain · `Tab` expand/edit · `Ctrl+P` pin · `Ctrl+Q` queue · `Alt+1..9` quick paste · `Del` delete · `Esc` close. *Why Alt+digits:* bare digits would collide with typing in search.

## 2. Rust architecture

### 2.1 Workspace layout

```
zlip/
├─ Cargo.toml                workspace, shared lints, release profile
├─ package.json  vite.config.ts
├─ src/                      Svelte 5 UI: overlay, settings, queue HUD
├─ src-tauri/                app shell: windows, tray, commands, wiring
├─ crates/
│  ├─ zlip-core/             items, categorizer, transforms, placeholders, queue, fuzzy search
│  ├─ zlip-config/           serde schema, validation, hot reload, JSON Schema export
│  ├─ zlip-security/         detector, vault, expiry, blacklist
│  ├─ zlip-highlight/        language detection, pretty-print, syntect HTML
│  ├─ zlip-store/            SQLite + AEAD encryption, retention janitor
│  ├─ zlip-actions/          script and HTTP actions with allowlists
│  └─ zlip-platform/         the only crate with OS calls: hook, clipboard, paste, foreground
├─ .github/workflows/        ci.yml  release.yml  audit.yml
└─ .cargo/audit.toml         documented advisory ignores
```

*Why this split:* core, config, security and highlight have no OS dependency, so CI tests them on Linux. `unsafe` is confined to `zlip-platform`, so the audit surface is one crate. Svelte 5 compiles to direct DOM updates with no virtual-DOM runtime, which gives a smaller bundle and faster first paint.

### 2.2 Crates

| Purpose | Crates |
| --- | --- |
| Shell, update | `tauri` 2, `tauri-plugin-single-instance`, `tauri-plugin-autostart`, `tauri-plugin-updater` (signed updates) |
| Acrylic window | `window-vibrancy` |
| Win32 | `windows` (windows-rs): hook, clipboard listener, `SendInput`, process queries |
| Config | `serde`, `toml`, `schemars` (editor autocomplete), `notify` (hot reload) |
| Search, transforms | `nucleo-matcher` (Helix's matcher), `heck` (casing), `base64`, `unicode-segmentation` |
| Highlight, detect | `syntect`, `serde_json`, `quick-xml`, `sqlformat`, `prettyplease`, `sqlparser`, `url`, `regex` |
| Storage, secrets | `rusqlite` (bundled), `chacha20poly1305`, `keyring`, `blake3`, `zeroize`, `secrecy` |
| Async | `tokio`, `tokio-util` (`DelayQueue` for expiry), `crossbeam-channel`, `time` |

### 2.3 Global hook and runtime model

**Why not `RegisterHotKey` or `rdev`:** the shell owns `Win+V`, so `RegisterHotKey` cannot claim it. A low-level keyboard hook (`WH_KEYBOARD_LL`) sees the key first and can swallow it. `rdev::grab` wraps the same API but hides the injected-event flag and the Start-menu masking we need, so Zlip calls windows-rs directly.

Two dedicated OS threads run Win32 message loops: **hook** and **listener** (`AddClipboardFormatListener` on a message-only window). Both only classify an event and send it down a channel. Windows silently removes a low-level hook whose callback exceeds its timeout (a few hundred ms), so callbacks must return in microseconds. A 5-second watchdog re-installs the hook if its heartbeat stops.

Hook rules: ignore events flagged `LLKHF_INJECTED` or carrying Zlip's `dwExtraInfo` marker, so our own `SendInput` never loops back. When `V` is swallowed while Win is held, inject an unassigned key (`VK 0xE8`) so Windows sees Win used as a modifier and does not open Start on release. Known limit: a non-elevated process receives no keystrokes while an elevated window has focus (UIPI). Plan: a `uiAccess` manifest from a signed Program Files install in v1.1.

### 2.4 Paste pipeline

`Win+V` → hook thread → channel → controller stores the foreground `HWND` and shows the warm window → UI calls `search(query, filter, limit)` → `Enter` → `paste(id, transforms)` → controller hides the window, restores focus to the stored `HWND`, resolves placeholders and transforms in `zlip-core`, writes the clipboard with exclusion flags (Zlip's own write is not re-captured), sends `Ctrl+V`, then restores the previous clipboard after a short delay.

**Placeholders:** `{date}`, `{date:%Y-%m-%d}`, `{time}`, `{clipboard}` (alias `{clipboard_state}`), `{cursor}`, `{{` for a literal brace. `{cursor}` pastes the text, then sends Left-arrow once per grapheme cluster that followed the marker. It works in nearly every text control without per-app integrations.

**Paste queue (FIFO):** *Collect* mode (`Ctrl+Shift+Q`) appends every copy. *Drain* mode intercepts `Ctrl+V` in the hook, swallows it, loads the head item onto the clipboard, injects `Ctrl+V` and pops.

**Categorization (first match wins):** sensitive → image → file list → link → color → code → text. Sensitive goes first so a token is never indexed as plain text. Code detection: JSON by `serde_json`, XML by `quick-xml`, SQL by `sqlparser`, Rust by keyword scoring confirmed with `syn`; pretty-printing uses `serde_json`, `quick-xml`, `sqlformat` and `prettyplease`.

## 3. Security protocol

### 3.1 Threat model

In scope: shoulder-surfing, disk theft, pagefile and crash-dump leakage, a compromised webview, malicious clipboard content, hostile config or webhook settings. Out of scope: malware running as the same user (it can read the clipboard directly). The best defence there is minimizing how long secrets exist.

### 3.2 Password-manager bypass: three layers

1. **Format flags (Microsoft's own contract).** Skip any copy whose formats include `ExcludeClipboardContentFromMonitorProcessing`, or where `CanIncludeInClipboardHistory` or `CanUploadToCloudClipboard` is 0. Desktop password managers set these.
2. **Source process.** `GetClipboardOwner` → `GetWindowThreadProcessId` → `QueryFullProcessImageNameW`, then match the lowercase exe name against a blacklist. Defaults: `bitwarden.exe`, `1password.exe`, `keepass.exe`, `keepassxc.exe`, `dashlane.exe`, `enpass.exe`, `nordpass.exe` (confirm against current installers at each release; the list is user-editable). A spoofed exe name can only make Zlip skip a copy, so this fails safe.
3. **Browser extensions.** They copy from inside the browser process, so layers 1 and 2 cannot see them. Layer 3 is the detector below plus a foreground-window-title match. It is a heuristic, not a guarantee, and the settings page says so.

### 3.3 High-entropy detector

Runs before anything is stored; a match marks the item *sensitive*.

```rust
use std::sync::LazyLock;
use regex::RegexSet;

static KEY_PATTERNS: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new([
        r"^AKIA[0-9A-Z]{16}$",                // AWS access key id
        r"^gh[pousr]_[A-Za-z0-9]{36,}$",      // GitHub tokens
        r"^xox[baprs]-[A-Za-z0-9-]{10,}$",    // Slack tokens
        r"^eyJ[A-Za-z0-9_-]+[.][A-Za-z0-9_-]+[.][A-Za-z0-9_-]+$", // JWT
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----", // PEM private keys
    ])
    .expect("static patterns compile")
});

fn shannon_bits_per_byte(s: &str) -> f64 {
    let mut counts = [0usize; 256];
    for b in s.bytes() { counts[b as usize] += 1; }
    let n = s.len() as f64;
    counts.iter().filter(|&&c| c > 0)
        .map(|&c| { let p = c as f64 / n; -p * p.log2() })
        .sum()
}

fn luhn_ok(digits: &[u32]) -> bool {
    let sum: u32 = digits.iter().rev().enumerate()
        .map(|(i, &d)| if i % 2 == 1 { let x = d * 2; if x > 9 { x - 9 } else { x } } else { d })
        .sum();
    sum % 10 == 0
}

pub fn is_sensitive(raw: &str, cfg: &SecurityConfig) -> bool {
    let s = raw.trim();
    if s.is_empty() || s.len() > 4096 { return false; }
    if KEY_PATTERNS.is_match(s) { return true; }

    // Payment card: digits, spaces, dashes only; 13-19 digits; passes Luhn.
    if s.bytes().all(|b| b.is_ascii_digit() || b == b' ' || b == b'-') {
        let d: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
        if (13..=19).contains(&d.len()) && luhn_ok(&d) { return true; }
    }

    // Random-looking single token: long, 3+ character classes, high entropy.
    let classes = [
        s.bytes().any(|b| b.is_ascii_lowercase()),
        s.bytes().any(|b| b.is_ascii_uppercase()),
        s.bytes().any(|b| b.is_ascii_digit()),
        s.bytes().any(|b| b.is_ascii_punctuation()),
    ].iter().filter(|&&x| x).count();
    !s.contains(char::is_whitespace)
        && s.len() >= cfg.min_token_len              // default 16
        && classes >= 3
        && shannon_bits_per_byte(s) >= cfg.min_entropy_bits // default 3.5
}
```

*Why 3+ character classes:* identifiers such as `getUserResetToken` have two classes and are not secrets. Thresholds live in config.

### 3.4 Sensitive-item lifecycle (60-second expiry)

- Sensitive items live only in an in-RAM `SensitiveVault`. They are never written to SQLite or image files, and no webhook can receive them.
- The UI gets a masked preview and a short-lived *reveal* command.
- A `DelayQueue` fires at 60 s (configurable): zeroize and remove the item, then clear the system clipboard **only if** `GetClipboardSequenceNumber()` still equals the value recorded at capture. *Why:* if the user has copied something else since, wiping it would destroy their data.

```rust
use zeroize::Zeroizing;

pub struct SensitiveItem {
    pub id: ItemId,
    secret: Zeroizing<Vec<u8>>, // zeroed on drop
    clip_seq: u32,              // clipboard sequence number at capture
}

impl SensitiveItem {
    pub fn expire(self, platform: &impl Platform) {
        if platform.clipboard_sequence() == self.clip_seq {
            platform.clear_clipboard(); // EmptyClipboard()
        }
        // `self` drops here and Zeroizing wipes the buffer
    }
}
```

### 3.5 Memory hygiene: what we do and what we cannot

**Do:** `Zeroizing` and `SecretString` for secrets; allocate sensitive buffers once with `with_capacity` so reallocation leaves no stray copies; never `format!` or log a secret; `VirtualLock` the vault pages so they are not paged to disk; release profile with `panic = "abort"`, `lto = "fat"`, `strip = true`. **Cannot:** scrub the OS clipboard's own copy (hence the clear above), WebView DOM copies (hence masked IPC), or copies the target app makes after you paste. The settings page states this plainly.

### 3.6 Data at rest

Non-sensitive history goes to SQLite with each payload sealed by XChaCha20-Poly1305 (random 24-byte nonce). The 32-byte key is generated on first run and stored in Windows Credential Manager through `keyring` (DPAPI, bound to the Windows user). *Why not SQLCipher:* it drags an OpenSSL/Perl build chain into CI and cannot keep sensitive items out of the database file. Images are encrypted content-addressed files (BLAKE3 names, so duplicates collapse). A janitor enforces retention by age, count and size. **Private mode** pauses capture entirely.

### 3.7 Webview and IPC hardening

- CSP: `default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; script-src 'self'`. No remote code, no `eval`.
- Tauri v2 capabilities grant the overlay only Zlip's own commands; no filesystem, shell or HTTP plugin is reachable from JS. DevTools are off in release.
- **Clipboard text is attacker-controlled.** All highlighting is generated in Rust with HTML escaping; the UI never inserts raw clipboard text as HTML.
- Commands validate inputs and take item IDs, not content.

### 3.8 Action webhooks

Actions are declared in config only, never built from clipboard text. **Scripts:** absolute path, spawned directly (no `cmd /c`, no shell parsing), clipboard delivered on **stdin**, timeout then kill, and a first-run confirmation showing the file's SHA-256 that re-prompts if the file changes. **HTTP:** HTTPS only, host allowlist, 5 s timeout, no cross-host redirects, optional HMAC-SHA256 header with the secret in Credential Manager. Sensitive items are never eligible.

### 3.9 Supply chain

Signed updater bundles (minisign), Authenticode signing, build-provenance attestations, `cargo audit` on every push plus a daily scan, `--locked` builds, and Dependabot for Cargo, npm and GitHub Actions. Once Dependabot is on, pin third-party actions to commit SHAs.

## 4. Customization engine

Config lives at `%APPDATA%\\zlip\\config.toml`. Layers: embedded defaults, then the user file. A JSON Schema is exported for editor autocomplete, and a watcher hot-reloads on save. On a parse error Zlip keeps the last good config and shows a toast. The settings GUI reads and writes this same file, so there is one source of truth.

```toml
#:schema ./zlip.schema.json
schema = 1

[hotkeys]
open_overlay  = "Win+V"
paste_plain   = "Shift+Enter"
queue_collect = "Ctrl+Shift+Q"
quick_paste   = "Alt+{1..9}"

[theme]
mode          = "auto"   # auto | light | dark
tint_opacity  = 0.14
corner_radius = 24

[theme.categories]
text = "#C5CAF5"
link = "#A9D6F5"
code = "#D6C4F5"

[retention]
max_items    = 500
max_age      = "30d"
max_image_mb = 25

[security]
sensitive_ttl_secs = 60
blacklist_exes     = ["bitwarden.exe", "1password.exe", "keepass.exe", "keepassxc.exe"]
min_token_len      = 16
min_entropy_bits   = 3.5

[[snippet_folders]]
name  = "Terminal"
items = [{ title = "Docker prune", body = "docker system prune -af" }]

[[actions]]
name       = "Run lint hook"
kind       = "script"
path       = "C:/tools/zlip/lint.ps1"
timeout_ms = 5000

[[automations]]
when = { category = "link" }
do   = ["strip_tracking_params"]
```

## 5. CI/CD pipeline

| File | Trigger | Does |
| --- | --- | --- |
| `ci.yml` | push to main, pull requests, called by release | rustfmt, clippy with `-D warnings`, tests on Windows, portable crates on Linux, frontend lint/check/build, `cargo audit` and `npm audit`, release-cache warm-up on main |
| `release.yml` | tag `vX.Y.Z` | checks tag against Cargo version, CI gate, signed MSI + NSIS via Tauri/WiX, SHA256SUMS, provenance attestation, publish, Winget manifests and PR |
| `audit.yml` | daily 03:17 UTC, lockfile changes | `cargo audit`; opens an issue on failure |

- **Draft, then publish, then Winget.** Assets upload to a draft release and go public only after every build succeeds. Winget needs public URLs, and the SHA256 is computed from the public URL's bytes, so it cannot disagree with what users download.
- **MSI is the Winget installer.** It has standard silent switches and a ProductCode for clean upgrades. The workflow reads `ProductCode`, `UpgradeCode` and `ProductVersion` from the MSI itself and writes the three manifest files (version, installer, defaultLocale).
- **PR without cloning winget-pkgs.** That repo is huge, so the workflow forks it, branches off upstream `master` and commits the three files through the Contents API.
- **Cache scope.** A cache written on a tag is invisible to the next tag. So `ci.yml` builds the release profile on `main` under the same cache key, and tag builds restore from it. npm uses the `setup-node` cache.
- **Injection safety.** Untrusted values pass through `env:`, never interpolated into scripts. Default permissions are read-only; each job elevates only what it needs.

**One-time setup**

1. `npm run tauri signer generate`; store the private key and password as `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, and put the public key in `tauri.conf.json`.
2. In `tauri.conf.json` set `bundle.createUpdaterArtifacts: true` and `bundle.targets: ["nsis", "msi"]`.
3. Create a classic PAT with `public_repo` and save it as `WINGET_PKGS_TOKEN`. Winget moderators review the first version of a new package by hand; later versions are mostly automated.
4. Optional: base64-encode a code-signing `.pfx` into `WINDOWS_CERTIFICATE` plus `WINDOWS_CERTIFICATE_PASSWORD`. Unsigned installers work but trigger SmartScreen and are likelier to be flagged during Winget validation.
5. Set `PACKAGE_LICENSE` in `release.yml` to your real SPDX id and add a `LICENSE` file.

## 6. Suggested build order

1. **Spike:** the hook swallows `Win+V` without opening Start, and the warm window shows in under 100 ms with Acrylic.
2. Clipboard listener, categorizer, store, fuzzy search.
3. Overlay UI: cards, expand, transform bar.
4. Security layers first-class: tests for entropy, Luhn and the expiry-versus-sequence-number race.
5. Paste queue, placeholders, quick edit, highlighting, snippet folders, actions.
6. Settings GUI over the TOML file, then a release dry run on a `v0.0.1-rc.1` tag.
