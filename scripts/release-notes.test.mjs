import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { highlights, parseTag, problemsIn, renderBody, resolveNotesFile, scaffold } from './release-notes.mjs';

const NOTES = `> Summary.

## Highlights

- **Fast:** opens in [one keypress](https://example.com).
- \`Private\` by design.

### Detail

Nested heading stays inside Highlights.

## Install

<!-- winget -->
**Winget:** \`winget install rhshourav.Zlip\`
<!-- /winget -->
Download {{tag}} ({{version}}) from {{repo}}.

<!-- unsigned -->
Not signed: SmartScreen will warn.
<!-- /unsigned -->
<!-- prerelease -->
This is a pre-release.
<!-- /prerelease -->
`;

test('tags: stable, pre-release, and nonsense', () => {
  assert.deepEqual(parseTag('v1.2.3'), { version: '1.2.3', base: '1.2.3', prerelease: false });
  assert.deepEqual(parseTag('v1.2.3-rc.1'), { version: '1.2.3-rc.1', base: '1.2.3', prerelease: true });
  for (const bad of ['1.2.3', 'v1.2', 'v1.2.3.4', '', undefined]) assert.throws(() => parseTag(bad));
});

test('a pre-release may reuse the notes of the release it leads to, a stable tag may not borrow', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'notes-'));
  fs.mkdirSync(path.join(root, 'docs/release-notes'), { recursive: true });
  fs.writeFileSync(path.join(root, 'docs/release-notes/v1.0.0.md'), NOTES);
  assert.ok(resolveNotesFile('v1.0.0', root));
  assert.ok(resolveNotesFile('v1.0.0-rc.1', root), 'rc falls back to the base notes');
  assert.equal(resolveNotesFile('v1.1.0', root), null);
  assert.equal(resolveNotesFile('v1.1.0-rc.1', root), null);
  fs.writeFileSync(path.join(root, 'docs/release-notes/v1.0.0-rc.1.md'), 'exact');
  assert.match(resolveNotesFile('v1.0.0-rc.1', root), /v1\.0\.0-rc\.1\.md$/, 'an exact file wins');
});

test('the release gate refuses missing highlights and leftover TODOs', () => {
  assert.deepEqual(problemsIn(NOTES), []);
  assert.equal(problemsIn('## What changed\n- x').length, 1);
  assert.equal(problemsIn('## Highlights\n- TODO: write this').length, 1);
  assert.equal(problemsIn('nothing TBD here\n').length, 2);
});

test('a freshly scaffolded file is refused until it is filled in', () => {
  const fresh = scaffold('1.2.3');
  assert.ok(problemsIn(fresh).some((p) => p.includes('TODO')));
  assert.ok(fresh.includes('Zlip_1.2.3_x64-setup.exe'));
  assert.equal(problemsIn(fresh.replaceAll('TODO', 'Done')).length, 0);
});

test('body: stable release', () => {
  const body = renderBody({ markdown: NOTES, tag: 'v1.2.3', repo: 'me/app', signed: false });
  assert.match(body, /winget install rhshourav\.Zlip/);
  assert.match(body, /Download v1\.2\.3 \(1\.2\.3\) from me\/app\./);
  assert.match(body, /SmartScreen will warn/);
  assert.doesNotMatch(body, /pre-release/);
  assert.doesNotMatch(body, /<!--|{{/);
});

test('body: pre-release has no Winget line (the pipeline does not submit it) and says so', () => {
  const body = renderBody({ markdown: NOTES, tag: 'v1.2.3-rc.1' });
  assert.doesNotMatch(body, /winget install/);
  assert.match(body, /This is a pre-release/);
  assert.match(body, /\(1\.2\.3-rc\.1\)/);
});

test('body: signed installers drop the SmartScreen warning', () => {
  assert.doesNotMatch(renderBody({ markdown: NOTES, tag: 'v1.2.3', signed: true }), /SmartScreen/);
});

test('highlights become plain Winget text and stop at the next section', () => {
  const h = highlights(NOTES);
  assert.match(h, /^- Fast: opens in one keypress\.$/m);
  assert.match(h, /^- Private by design\.$/m);
  assert.match(h, /Nested heading stays inside Highlights/);
  assert.doesNotMatch(h, /Install|winget|\*|`|\]\(/);
  assert.equal(highlights('## Install\nx'), null);
});

test('highlights keep identifiers like snake_case and strip real emphasis', () => {
  const h = highlights('## Highlights\n- Formats as `snake_case` or camelCase, _quietly_ and __boldly__ (my_var_name stays).\n');
  assert.equal(h, '- Formats as snake_case or camelCase, quietly and boldly (my_var_name stays).');
});

test('highlights are cut at a line, never mid-word, when too long', () => {
  const long = '## Highlights\n' + Array.from({ length: 50 }, (_, i) => `- item number ${i} with some words`).join('\n');
  const h = highlights(long, 200);
  assert.ok(h.length <= 200);
  assert.match(h, /words$/);
});
