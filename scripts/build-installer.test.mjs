import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { bundlesFor, checksumLines, findInstallers, planBuild } from './build-installer.mjs';

test('both installers by default, one on request', () => {
  assert.deepEqual(bundlesFor([]), ['nsis', 'msi']);
  assert.deepEqual(bundlesFor(['--msi-only']), ['msi']);
  assert.deepEqual(bundlesFor(['--nsis-only']), ['nsis']);
});

test('without the signing key the updater files are switched off, and the override is passed as a file', () => {
  const p = planBuild({ bundles: ['nsis', 'msi'], env: {}, configPath: '/tmp/c.json' });
  assert.deepEqual(p.args, ['build', '--bundles', 'nsis,msi', '--config', '/tmp/c.json']);
  assert.deepEqual(p.override, { bundle: { createUpdaterArtifacts: false } });
  assert.equal(p.signed, false);
});

test('with the signing key set the real config is used untouched', () => {
  const p = planBuild({ bundles: ['msi'], env: { TAURI_SIGNING_PRIVATE_KEY: 'k' }, configPath: '/tmp/c.json' });
  assert.deepEqual(p.args, ['build', '--bundles', 'msi']);
  assert.equal(p.override, null);
  assert.equal(p.signed, true);
});

test('finds installers and hashes them', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'inst-'));
  const b = path.join(root, 'target/release/bundle');
  fs.mkdirSync(path.join(b, 'nsis'), { recursive: true });
  fs.mkdirSync(path.join(b, 'msi'), { recursive: true });
  fs.writeFileSync(path.join(b, 'nsis/Zlip_0.1.0_x64-setup.exe'), 'nsis-bytes');
  fs.writeFileSync(path.join(b, 'msi/Zlip_0.1.0_x64_en-US.msi'), 'msi-bytes');
  fs.writeFileSync(path.join(b, 'nsis/notes.txt'), 'ignored');
  const found = findInstallers(root);
  assert.equal(found.nsis.length, 1);
  assert.equal(found.msi.length, 1);
  const [line] = checksumLines(found.msi);
  assert.equal(line, `${crypto.createHash('sha256').update('msi-bytes').digest('hex')}  Zlip_0.1.0_x64_en-US.msi`);
});

test('no bundle folder means no installers, not a crash', () => {
  const found = findInstallers(fs.mkdtempSync(path.join(os.tmpdir(), 'empty-')));
  assert.deepEqual([found.nsis, found.msi], [[], []]);
});
