import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { bump, cargoLockVersions, check, msiProblem, readVersions, setCargoLockVersion, setCargoTomlVersion } from './bump-version.mjs';

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

const TOML = `[workspace]
members = ["a"]

[workspace.package]
version = "1.0.0"
authors = ["me"]

[workspace.dependencies]
serde = { version = "1.0.0" }
`;
const LOCK = `# generated
version = 4

[[package]]
name = "mine"
version = "1.0.0"
dependencies = [
 "serde",
]

[[package]]
name = "serde"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "abc"
`;

function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'bump-'));
  const w = (f, t) => fs.writeFileSync(path.join(root, f), t);
  w('Cargo.toml', TOML);
  w('Cargo.lock', LOCK);
  w('package.json', JSON.stringify({ name: 'x', version: '1.0.0' }, null, 2) + '\n');
  w('package-lock.json', JSON.stringify({ name: 'x', version: '1.0.0', packages: { '': { name: 'x', version: '1.0.0' } } }, null, 2) + '\n');
  return root;
}

test('Cargo.toml: only the workspace version changes, not a dependency that shares the number', () => {
  const out = setCargoTomlVersion(TOML, '2.0.0');
  assert.match(out, /\[workspace\.package\]\nversion = "2\.0\.0"/);
  assert.match(out, /serde = \{ version = "1\.0\.0" \}/);
});

test('Cargo.lock: workspace crates change, registry crates do not (even at the same version)', () => {
  assert.deepEqual(cargoLockVersions(LOCK), [{ name: 'mine', version: '1.0.0' }]);
  const out = setCargoLockVersion(LOCK, '1.0.0', '2.0.0');
  assert.match(out, /name = "mine"\nversion = "2\.0\.0"/);
  assert.match(out, /name = "serde"\nversion = "1\.0\.0"/);
});

test('bump changes every file, check agrees, and a second bump is a no-op', () => {
  const root = fixture();
  assert.deepEqual(check('1.0.0', root), []);
  assert.ok(check('2.0.0', root).length >= 4);
  const r = bump('2.0.0', { root });
  assert.deepEqual(r.touched.sort(), ['Cargo.lock', 'Cargo.toml', 'package-lock.json', 'package.json']);
  assert.deepEqual(check('2.0.0', root), []);
  assert.deepEqual(bump('2.0.0', { root }).touched, []);
});

test('dry run writes nothing', () => {
  const root = fixture();
  assert.ok(bump('2.0.0', { root, dryRun: true }).touched.length > 0);
  assert.deepEqual(check('1.0.0', root), []);
});

test('rejects things that are not versions', () => {
  const root = fixture();
  for (const v of ['2', 'v2.0.0', '2.0', 'latest']) {
    assert.throws(() => bump(v, { root }));
    assert.equal(check(v, root).length, 1);
  }
});

test('the MSI installer only allows a numeric pre-release part (Tauri fails the build otherwise)', () => {
  assert.equal(msiProblem('1.2.3'), null);
  assert.equal(msiProblem('1.2.3-1'), null, 'becomes MSI version 1.2.3.1');
  assert.equal(msiProblem('1.2.3-65535'), null);
  for (const bad of ['1.2.3-rc.1', '1.2.3-beta', '1.2.3-65536', '256.0.0', '1.256.0', '1.0.65536']) assert.ok(msiProblem(bad), bad);
  assert.match(msiProblem('1.2.3-rc.1'), /Use 1\.2\.3-1 instead of 1\.2\.3-rc\.1/);
  const root = fixture();
  assert.throws(() => bump('2.0.0-rc.1', { root }), /numeric|number/);
  assert.equal(check('2.0.0-rc.1', root).length, 1);
  assert.ok(bump('2.0.0-1', { root }).touched.length > 0);
  assert.deepEqual(check('2.0.0-1', root), []);
});

test('THIS repository: every version field agrees right now', () => {
  const versions = readVersions(REPO);
  const expected = versions[0].version;
  assert.ok(expected, 'Cargo.toml has a workspace version');
  assert.deepEqual(check(expected, REPO), [], 'run: node scripts/bump-version.mjs ' + expected);
});
