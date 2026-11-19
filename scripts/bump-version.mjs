#!/usr/bin/env node
// One command to change the version everywhere it lives, and one to prove it is consistent.
//
//   node scripts/bump-version.mjs 0.2.0            change it (Cargo.toml, Cargo.lock, package.json, package-lock.json)
//   node scripts/bump-version.mjs 0.2.0 --dry-run  show what would change
//   node scripts/bump-version.mjs --check 0.2.0    exit 1 unless every file already says 0.2.0 (the release workflow runs this)
//
// Why Cargo.lock too: CI builds with `--locked`, which refuses to run when Cargo.toml and
// Cargo.lock disagree, so a version bump that only edits Cargo.toml fails the release.
// tauri.conf.json deliberately has no "version" (Cargo.toml is the single source); if one is
// ever added it is checked and bumped as well.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;

/**
 * The MSI installer carries the version as numbers only (major.minor.patch.build), and
 * Tauri refuses to build it otherwise, AFTER the long compile. So a pre-release part has to be
 * a number (1.2.3-1 becomes MSI version 1.2.3.1); "rc.1" or "beta" is rejected here instead.
 * Returns a message, or null when the version is fine.
 */
export function msiProblem(version) {
  const m = /^(\d+)\.(\d+)\.(\d+)(?:-(.+))?$/.exec(version);
  if (!m) return null; // not a version at all: reported by the format check
  const [, major, minor, patch, pre] = m;
  if (Number(major) > 255) return `major version ${major} is above 255, the MSI limit`;
  if (Number(minor) > 255) return `minor version ${minor} is above 255, the MSI limit`;
  if (Number(patch) > 65535) return `patch version ${patch} is above 65535, the MSI limit`;
  if (pre !== undefined && !(/^\d+$/.test(pre) && Number(pre) <= 65535)) {
    return `the pre-release part '${pre}' must be a number up to 65535, because the MSI installer cannot carry text. Use ${major}.${minor}.${patch}-1 instead of ${major}.${minor}.${patch}-${pre}`;
  }
  return null;
}

const read = (root, rel) => fs.readFileSync(path.join(root, rel), 'utf8');
const exists = (root, rel) => fs.existsSync(path.join(root, rel));

// ------------------------------------------------------------- Cargo.toml
const WS_VERSION = /(\[workspace\.package\](?:(?!\n\[)[\s\S])*?\nversion\s*=\s*")([^"]+)(")/;

export function cargoTomlVersion(text) {
  return WS_VERSION.exec(text)?.[2] ?? null;
}
export function setCargoTomlVersion(text, version) {
  if (!WS_VERSION.test(text)) throw new Error('Cargo.toml: no version under [workspace.package]');
  return text.replace(WS_VERSION, `$1${version}$3`);
}

// -------------------------------------------------------------- Cargo.lock
// Workspace crates are the only packages without a `source = ...` line.
function lockChunks(text) {
  return text.split('\n\n').map((chunk) => {
    if (!chunk.startsWith('[[package]]') || /\nsource\s*=/.test(chunk)) return null;
    const name = /\nname\s*=\s*"([^"]+)"/.exec(chunk)?.[1];
    const version = /\nversion\s*=\s*"([^"]+)"/.exec(chunk)?.[1];
    return name && version ? { name, version } : null;
  });
}
export function cargoLockVersions(text) {
  return lockChunks(text).filter(Boolean);
}
export function setCargoLockVersion(text, from, to) {
  const info = lockChunks(text);
  return text
    .split('\n\n')
    .map((chunk, i) => (info[i]?.version === from ? chunk.replace(/(\nversion\s*=\s*")[^"]+(")/, `$1${to}$2`) : chunk))
    .join('\n\n');
}

// ----------------------------------------------------------- JSON files
// npm writes these as JSON.stringify(x, null, 2) + "\n", so a round trip keeps them byte-identical.
const json = (text) => JSON.parse(text);
const dump = (obj) => JSON.stringify(obj, null, 2) + '\n';

// ----------------------------------------------------------------- api
/** What every file currently says. */
export function readVersions(root = '.') {
  const out = [];
  out.push({ file: 'Cargo.toml', version: cargoTomlVersion(read(root, 'Cargo.toml')) });
  for (const p of cargoLockVersions(read(root, 'Cargo.lock'))) out.push({ file: `Cargo.lock (${p.name})`, version: p.version });
  out.push({ file: 'package.json', version: json(read(root, 'package.json')).version });
  const lock = json(read(root, 'package-lock.json'));
  out.push({ file: 'package-lock.json', version: lock.version });
  if (lock.packages?.['']) out.push({ file: 'package-lock.json (root package)', version: lock.packages[''].version });
  if (exists(root, 'src-tauri/tauri.conf.json')) {
    const conf = json(read(root, 'src-tauri/tauri.conf.json'));
    if (conf.version) out.push({ file: 'src-tauri/tauri.conf.json', version: conf.version });
  }
  return out;
}

/** Every file that does not say `version`. Empty array = consistent. */
export function check(version, root = '.') {
  if (!SEMVER.test(version)) return [`'${version}' is not a version like 1.2.3 or 1.2.3-1`];
  const msi = msiProblem(version);
  if (msi) return [`${version}: ${msi}`];
  return readVersions(root)
    .filter((v) => v.version !== version)
    .map((v) => `${v.file} says ${v.version ?? '(nothing)'}, expected ${version}`);
}

/** Rewrite every file. Returns the list of files touched. */
export function bump(version, { root = '.', dryRun = false } = {}) {
  if (!SEMVER.test(version)) throw new Error(`'${version}' is not a version like 1.2.3 or 1.2.3-1`);
  const msi = msiProblem(version);
  if (msi) throw new Error(`${version}: ${msi}`);
  const current = cargoTomlVersion(read(root, 'Cargo.toml'));
  if (!current) throw new Error('Cargo.toml: no version under [workspace.package]');
  const touched = [];
  const write = (rel, text) => {
    if (text === read(root, rel)) return;
    touched.push(rel);
    if (!dryRun) fs.writeFileSync(path.join(root, rel), text);
  };

  write('Cargo.toml', setCargoTomlVersion(read(root, 'Cargo.toml'), version));
  write('Cargo.lock', setCargoLockVersion(read(root, 'Cargo.lock'), current, version));

  const pkg = json(read(root, 'package.json'));
  pkg.version = version;
  write('package.json', dump(pkg));

  const lock = json(read(root, 'package-lock.json'));
  lock.version = version;
  if (lock.packages?.['']) lock.packages[''].version = version;
  write('package-lock.json', dump(lock));

  if (exists(root, 'src-tauri/tauri.conf.json')) {
    const conf = json(read(root, 'src-tauri/tauri.conf.json'));
    if (conf.version) {
      conf.version = version;
      write('src-tauri/tauri.conf.json', dump(conf));
    }
  }
  return { from: current, to: version, touched };
}

// ----------------------------------------------------------------- CLI
function main(argv) {
  const args = argv.filter((a) => !a.startsWith('--'));
  const dryRun = argv.includes('--dry-run');
  if (argv.includes('--check')) {
    const problems = check(args[0]);
    if (problems.length) {
      // Only a plain mismatch is fixed by bumping; a malformed or MSI-illegal version has to be re-chosen.
      const fixable = problems.every((p) => p.includes('expected '));
      throw new Error(`Versions are not consistent:\n - ${problems.join('\n - ')}${fixable ? `\nFix with: node scripts/bump-version.mjs ${args[0]}` : ''}`);
    }
    console.log(`All version fields say ${args[0]}.`);
    return;
  }
  if (!args[0]) throw new Error('Usage: bump-version.mjs <X.Y.Z> [--dry-run]   |   bump-version.mjs --check <X.Y.Z>');
  const r = bump(args[0], { dryRun });
  console.log(`${dryRun ? 'Would change' : 'Changed'} ${r.from} -> ${r.to} in: ${r.touched.join(', ') || '(nothing: already at that version)'}`);
  if (!dryRun && r.touched.length) {
    console.log(`\nNext:\n  node scripts/release-notes.mjs scaffold --version ${r.to}   # then fill in docs/release-notes/v${r.to}.md\n  git commit -am "Release ${r.to}" && git tag v${r.to} && git push --follow-tags`);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(`error: ${e.message}`);
    process.exit(1);
  }
}
