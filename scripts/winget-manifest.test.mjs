import assert from 'node:assert/strict';
import test from 'node:test';
import { buildManifests, inputFromArgs, scalar, validate } from './winget-manifest.mjs';

const GOOD = {
  version: '0.1.0',
  tag: 'v0.1.0',
  url: 'https://github.com/rhshourav/zlip/releases/download/v0.1.0/Zlip_0.1.0_x64_en-US.msi',
  sha256: 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
  productCode: '{6F9619FF-8B86-D011-B42D-00C04FC964FF}',
  upgradeCode: '{A1B2C3D4-1111-2222-3333-444455556666}',
  released: '2026-11-18'
};

test('writes the three files where winget-pkgs expects them', () => {
  const { dir, files } = buildManifests(GOOD);
  assert.equal(dir.replaceAll('\\', '/'), 'manifests/r/rhshourav/Zlip/0.1.0');
  assert.deepEqual(Object.keys(files).sort(), ['rhshourav.Zlip.installer.yaml', 'rhshourav.Zlip.locale.en-US.yaml', 'rhshourav.Zlip.yaml']);
});

test('installer manifest: MSI details, upper-case hash, quoted GUIDs', () => {
  const y = buildManifests(GOOD).files['rhshourav.Zlip.installer.yaml'];
  assert.match(y, /^InstallerType: wix$/m);
  assert.match(y, /^Scope: machine$/m);
  assert.match(y, /InstallerSha256: E3B0C442/);
  assert.match(y, /ProductCode: "\{6F9619FF-8B86-D011-B42D-00C04FC964FF\}"/);
  assert.match(y, /UpgradeCode: "\{A1B2C3D4/);
  assert.match(y, /^ReleaseDate: 2026-11-18$/m, 'the date stays unquoted so it is a YAML date');
  assert.match(y, /^ManifestType: installer\nManifestVersion: 1\.10\.0\n$/m);
});

test('an NSIS installer has no product code', () => {
  const y = buildManifests({ ...GOOD, installerType: 'nullsoft', productCode: undefined, upgradeCode: undefined }).files['rhshourav.Zlip.installer.yaml'];
  assert.match(y, /^InstallerType: nullsoft$/m);
  assert.doesNotMatch(y, /ProductCode|AppsAndFeaturesEntries/);
});

test('release notes become a literal block under ReleaseNotes', () => {
  const y = buildManifests({ ...GOOD, releaseNotes: '- one\n- two: with colon\n\n- three' }).files['rhshourav.Zlip.locale.en-US.yaml'];
  assert.match(y, /^ReleaseNotes: \|-\n {2}- one\n {2}- two: with colon\n\n {2}- three\nReleaseNotesUrl: /m);
  assert.doesNotMatch(buildManifests(GOOD).files['rhshourav.Zlip.locale.en-US.yaml'], /^ReleaseNotes:/m);
});

test('scalars that YAML would misread are quoted', () => {
  assert.equal(scalar('0.1.0'), '0.1.0');
  assert.equal(scalar('1.0'), '"1.0"', 'would parse as a float');
  assert.equal(scalar('12E4'), '"12E4"', 'would parse as a number');
  assert.equal(scalar('yes'), '"yes"');
  assert.equal(scalar('{GUID}'), '"{GUID}"');
  assert.equal(scalar('a: b'), '"a: b"');
  assert.equal(scalar('say "hi"'), 'say "hi"', 'a quote is only special at the start');
  assert.equal(scalar('"hi" there'), '"\\"hi\\" there"');
  assert.equal(scalar('2026-11-18'), '"2026-11-18"', 'would parse as a date');
  assert.equal(scalar('Fast, private clipboard'), 'Fast, private clipboard');
});

test('every problem is reported at once, and nothing is built', () => {
  const bad = { ...GOOD, id: 'NoDot', sha256: 'abc', url: 'http://insecure.example/x.msi', productCode: 'nope', released: 'yesterday' };
  const problems = validate({ id: 'rhshourav.Zlip', scope: 'machine', installerType: 'wix', repo: 'rhshourav/zlip', license: 'MIT', shortDesc: 'x', tags: [], ...bad });
  assert.ok(problems.length >= 5, problems.join('\n'));
  assert.throws(() => buildManifests(bad), /Refusing to write manifests/);
});

test('limits from the winget schema are enforced', () => {
  assert.throws(() => buildManifests({ ...GOOD, shortDesc: 'x'.repeat(257) }), /ShortDescription/);
  assert.throws(() => buildManifests({ ...GOOD, releaseNotes: 'x'.repeat(10001) }), /ReleaseNotes/);
  assert.throws(() => buildManifests({ ...GOOD, tags: ['has space'] }), /Tag/);
  assert.throws(() => buildManifests({ ...GOOD, version: 'a/b' }), /PackageVersion/);
});

test('flags beat environment variables; unset values keep the defaults', () => {
  const env = { VERSION: '9.9.9', INSTALLER_URL: GOOD.url, INSTALLER_SHA256: GOOD.sha256, PRODUCT_CODE: GOOD.productCode, GITHUB_REPOSITORY: 'someone/else', TAG: 'v9.9.9' };
  const { input, out } = inputFromArgs(['--version', '1.2.3', '--out', 'o'], env);
  assert.equal(input.version, '1.2.3');
  assert.equal(input.repo, 'someone/else');
  assert.equal(input.id, undefined, 'left out so the default id applies');
  assert.equal(out, 'o');
  assert.equal(buildManifests({ ...input, tag: 'v1.2.3' }).dir.replaceAll('\\', '/').split('/').pop(), '1.2.3');
});
