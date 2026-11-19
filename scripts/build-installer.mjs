#!/usr/bin/env node
// Builds the Windows installers on your own machine:   npm run installer
//
//   target/release/bundle/nsis/Zlip_<version>_x64-setup.exe   (double-click installer)
//   target/release/bundle/msi/Zlip_<version>_x64_en-US.msi    (for managed PCs / Intune / Winget)
//   target/release/bundle/SHA256SUMS.txt
//
// Why a script and not just `npm run tauri build`:
//  * tauri.conf.json sets createUpdaterArtifacts, and Tauri then insists on
//    TAURI_SIGNING_PRIVATE_KEY and fails AFTER building the installers. This turns the
//    updater files off for a local build (unless the key is in your environment).
//  * It calls the Tauri CLI through Node directly, so there is no shell: no PowerShell
//    "running scripts is disabled" error (npm.ps1) and no mangled JSON quotes.
//
// Options: --msi-only | --nsis-only | --dry-run (print the command, build nothing)

import crypto from 'node:crypto';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

/** Which installer formats to build. */
export function bundlesFor(argv) {
  if (argv.includes('--msi-only')) return ['msi'];
  if (argv.includes('--nsis-only')) return ['nsis'];
  return ['nsis', 'msi'];
}

/** The tauri arguments, and the config override to merge (or null when the real key is present). */
export function planBuild({ bundles, env = process.env, configPath }) {
  const signed = Boolean(env.TAURI_SIGNING_PRIVATE_KEY);
  const args = ['build', '--bundles', bundles.join(',')];
  if (!signed) args.push('--config', configPath);
  return { args, signed, override: signed ? null : { bundle: { createUpdaterArtifacts: false } } };
}

const walk = (dir) =>
  !fs.existsSync(dir)
    ? []
    : fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(path.join(dir, e.name)) : [path.join(dir, e.name)]));

/** The installers a build produced. */
export function findInstallers(root = '.') {
  const bundle = path.join(root, 'target', 'release', 'bundle');
  const files = walk(bundle);
  return {
    bundle,
    nsis: files.filter((f) => /-setup\.exe$/i.test(f)),
    msi: files.filter((f) => /\.msi$/i.test(f))
  };
}

export function sha256File(file) {
  return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

export function checksumLines(files) {
  return files.map((f) => `${sha256File(f)}  ${path.basename(f)}`);
}

function resolveTauriCli(root) {
  const require = createRequire(path.join(root, 'package.json'));
  let pkgPath;
  try {
    pkgPath = require.resolve('@tauri-apps/cli/package.json');
  } catch {
    throw new Error('The Tauri CLI is not installed. Run:  npm ci   (or npm install)');
  }
  const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
  const bin = typeof pkg.bin === 'string' ? pkg.bin : pkg.bin?.tauri;
  if (!bin) throw new Error('Could not find the Tauri CLI entry point in @tauri-apps/cli/package.json');
  return path.join(path.dirname(pkgPath), bin);
}

function preflight() {
  if (process.platform !== 'win32') throw new Error('Windows installers can only be built on Windows. (CI builds them on windows-latest.)');
  const cargo = spawnSync('cargo', ['--version'], { encoding: 'utf8' });
  if (cargo.error || cargo.status !== 0) throw new Error('Rust is not on PATH (cargo not found). Install Rust and the Visual Studio C++ Build Tools, then open a NEW terminal.');
}

function main(argv) {
  const root = process.cwd();
  const dryRun = argv.includes('--dry-run');
  const bundles = bundlesFor(argv);
  const configPath = path.join(os.tmpdir(), `zlip-tauri-local-${process.pid}.json`);
  const plan = planBuild({ bundles, configPath });

  if (dryRun) {
    console.log(`node <tauri-cli> ${plan.args.join(' ')}`);
    if (plan.override) console.log(`with ${configPath} = ${JSON.stringify(plan.override)}`);
    return;
  }

  preflight();
  const cli = resolveTauriCli(root);
  if (plan.override) fs.writeFileSync(configPath, JSON.stringify(plan.override));
  console.log(plan.signed ? 'Building with updater signing (TAURI_SIGNING_PRIVATE_KEY is set)...' : 'Building without updater artifacts (no signing key needed)...');
  console.log('The first build compiles every Rust dependency in release mode: expect 10+ minutes.\n');

  let status;
  try {
    status = spawnSync(process.execPath, [cli, ...plan.args], { stdio: 'inherit', cwd: root, env: process.env }).status;
  } finally {
    fs.rmSync(configPath, { force: true });
  }
  if (status !== 0) throw new Error(`The build failed (exit code ${status}). The error is printed above.`);

  const found = findInstallers(root);
  const all = [...found.nsis, ...found.msi];
  if (!all.length) throw new Error(`The build finished but no installer was found under ${found.bundle}`);
  const lines = checksumLines(all);
  fs.writeFileSync(path.join(found.bundle, 'SHA256SUMS.txt'), lines.join('\n') + '\n');

  console.log('\nInstallers:');
  for (const f of all) console.log(`  ${path.relative(root, f)}  (${(fs.statSync(f).size / 1048576).toFixed(1)} MB)`);
  console.log('\nSHA-256:');
  for (const l of lines) console.log(`  ${l}`);
  console.log('\nThese are not code-signed, so Windows SmartScreen will warn on first run ("More info" > "Run anyway").');
  console.log('Quit Zlip from the tray before installing over an existing copy.');
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(`error: ${e.message}`);
    process.exit(1);
  }
}
