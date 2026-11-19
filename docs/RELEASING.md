# Releasing Zlip

A release is a tag. The pipeline in `.github/workflows/release.yml` does the rest:

```
tag vX.Y.Z  ->  prepare  ->  CI gate  ->  build (NSIS + MSI)  ->  verify draft  ->  publish  ->  Winget PR
               (fails in     (fmt, clippy,   draft release with      every asset     public      stable tags
                seconds)      tests)          notes + SHA256SUMS       present                     only)
```

`prepare` refuses the tag, in seconds, if any version field disagrees, the version is not legal
for an MSI installer, or the release notes are missing or unfinished. That is deliberate: the
build behind it takes half an hour.

## Releasing

```powershell
# 1. Change the version everywhere (Cargo.toml, Cargo.lock, package.json, package-lock.json).
npm run release:bump -- 0.2.0

# 2. Write the notes. The scaffold has TODO markers; the release is refused while any remain.
npm run release:notes -- scaffold --version 0.2.0
#    then edit docs/release-notes/v0.2.0.md

# 3. Commit, tag, push.
git add -A ; git commit -m "Release 0.2.0"
git tag v0.2.0 ; git push --follow-tags
```

Watch **Actions -> Release**. The draft release appears when the build finishes, becomes public
after the asset check, and (for a stable tag) the Winget pull request follows.

> **Why `bump-version` and not just editing `Cargo.toml`?** CI builds with `--locked`, which fails
> when `Cargo.toml` and `Cargo.lock` disagree. The script updates the workspace crates in
> `Cargo.lock` and nothing else.

### Pre-releases

Tag `v0.2.0-1`, `v0.2.0-2`, ... A pre-release is published, flagged as a pre-release on GitHub, and
**never sent to Winget**. It reuses `docs/release-notes/v0.2.0.md` unless you add a file with the
exact tag name.

The part after the dash **must be a number** (not `rc.1`, not `beta`): an MSI version is four
numbers, and Tauri fails the build otherwise. `0.2.0-1` becomes MSI version `0.2.0.1`.
`npm run release:bump -- 0.2.0-rc.1` explains this and refuses.

### Release notes

`docs/release-notes/<tag>.md` is the GitHub release body. It is plain Markdown plus:

| Marker | Effect |
| --- | --- |
| `{{tag}}` `{{version}}` `{{repo}}` | replaced when rendered |
| `<!-- winget -->...<!-- /winget -->` | removed from pre-releases (they are not on Winget) |
| `<!-- unsigned -->...<!-- /unsigned -->` | removed when the installers are code-signed |
| `<!-- prerelease -->...<!-- /prerelease -->` | kept only in pre-releases |
| the `## Highlights` section | **required**; becomes the Winget `ReleaseNotes` text |

Preview exactly what GitHub will show:

```powershell
npm run release:notes -- body --tag v0.1.0
```

## Build the installers on your own PC

```powershell
npm ci                 # once
npm run installer      # NSIS + MSI; add --msi-only or --nsis-only
```

Output is under `target\release\bundle\` with a `SHA256SUMS.txt`. The first build compiles every
dependency in release mode (10+ minutes); later ones are much faster.

Why not `npm run tauri build`? `tauri.conf.json` sets `createUpdaterArtifacts`, and Tauri then needs
`TAURI_SIGNING_PRIVATE_KEY` and fails *after* building the installers. `npm run installer` switches
those files off for local builds, and calls the Tauri CLI through Node, so it also avoids
PowerShell's "running scripts is disabled" error and its mangling of JSON quotes.

Local builds are not code-signed: SmartScreen shows "Windows protected your PC". Choose
**More info -> Run anyway**.

Test the install silently the way Winget will:

```powershell
msiexec /i target\release\bundle\msi\Zlip_0.1.0_x64_en-US.msi /qn     # install (admin)
msiexec /x target\release\bundle\msi\Zlip_0.1.0_x64_en-US.msi /qn     # uninstall
```

Quit Zlip from the tray before installing over a copy that is running.

## Winget

### How it works

After a stable release is public, the `winget` job:

1. downloads the MSI, hashes it, and reads its ProductCode / UpgradeCode / version from the file;
2. runs `scripts/winget-manifest.mjs`, which writes the three manifest files and **refuses to write
   them if anything is malformed** (bad hash, non-https URL, bad GUID, over-long fields, ...);
3. forks `microsoft/winget-pkgs` (if needed), pushes a branch, and opens the pull request. It is a
   *new package* PR the first time and a *new version* PR after that.

Moderators and a validation pipeline review the PR (usually a few days for a new package). Once it
merges, `winget install rhshourav.Zlip` works. The manifests are also kept as a workflow artifact.

### One-time setup

All three secrets go in a GitHub **Environment named `release`** (Settings -> Environments):

| Secret | What |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | from `npm run tauri signer generate -- -w zlip.key` |
| `WINGET_PKGS_TOKEN` | a *classic* personal access token with the `public_repo` scope |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` | optional: a base64 `.pfx` for Authenticode signing |

No manual fork is needed: the job forks `winget-pkgs` itself.

### Try the manifests on your PC before releasing

You need a published MSI (a GitHub release asset) because Winget installs from an `https` URL.
Get its SHA-256 from `SHA256SUMS.txt`, and its ProductCode / UpgradeCode from the build log
(step *Read ProductCode / UpgradeCode / ProductVersion from the MSI*).

```powershell
npm run winget -- --version 0.1.0 --tag v0.1.0 `
  --url https://github.com/rhshourav/zlip/releases/download/v0.1.0/Zlip_0.1.0_x64_en-US.msi `
  --sha256 <hash> --product-code "{...}" --upgrade-code "{...}" --out winget-test

# once, in an administrator PowerShell:
winget settings --enable LocalManifestFiles

winget validate --manifest winget-test\manifests\r\rhshourav\Zlip\0.1.0
winget install  --manifest winget-test\manifests\r\rhshourav\Zlip\0.1.0
```

Environment variables work as flags (`INSTALLER_URL`, `INSTALLER_SHA256`, `PRODUCT_CODE`, ...);
that is how the workflow calls it. `--installer-type nullsoft --scope user` generates a manifest
for the NSIS installer instead.

### If the Winget PR is rejected

* **"Installer is not signed" / SmartScreen / Defender findings.** Winget validation runs the
  installer through Windows' reputation checks. An unsigned installer from a new publisher can be
  flagged. Code-signing (the `WINDOWS_CERTIFICATE` secret) is the real fix; otherwise reply on the
  PR, since moderators can usually advise.
* **Hash mismatch.** The release asset changed after the PR was opened. Never re-upload an
  asset of a published version; cut a new version instead.
* **Anything else:** the PR comment from the validation bot names the failing check.

## Troubleshooting

| Symptom | Cause / fix |
| --- | --- |
| `prepare` fails: "Versions are not consistent" | run `npm run release:bump -- <version>` and commit |
| `prepare` fails: "pre-release part ... must be a number" | use `v1.2.3-1`, not `v1.2.3-rc.1` |
| `prepare` fails: "No release notes found" / "TODO" | finish `docs/release-notes/<tag>.md` |
| `cargo ... --locked` fails: "lock file needs to be updated" | the version was edited by hand; use the bump script |
| Build fails at "signing" | `TAURI_SIGNING_PRIVATE_KEY` is missing from the `release` environment |
| Winget job: "WINGET_PKGS_TOKEN is not set" | add the secret to the `release` environment |
