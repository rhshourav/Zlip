#!/usr/bin/env node
// Release notes for a tag, kept in the repo as docs/release-notes/<tag>.md.
//
//   node scripts/release-notes.mjs check    --tag v0.1.0
//   node scripts/release-notes.mjs body     --tag v0.1.0 [--repo owner/name] [--signed true] [--out body.md]
//   node scripts/release-notes.mjs scaffold --version 0.1.0
//
// The notes file is plain Markdown with three conveniences:
//   {{tag}} {{version}} {{repo}}      replaced when the release body is rendered
//   <!-- winget -->…<!-- /winget -->        removed for pre-releases (they are not sent to Winget)
//   <!-- unsigned -->…<!-- /unsigned -->    removed when the installers are Authenticode-signed
//   <!-- prerelease -->…<!-- /prerelease --> kept only for pre-releases
// A "## Highlights" section is required: it becomes the Winget "ReleaseNotes" text.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const NOTES_DIR = 'docs/release-notes';
const TAG_RE = /^v(\d+\.\d+\.\d+)(-[0-9A-Za-z.-]+)?$/;

export function parseTag(tag) {
  const m = TAG_RE.exec(tag ?? '');
  if (!m) throw new Error(`Tag '${tag}' must look like v1.2.3 or v1.2.3-rc.1`);
  return { version: m[1] + (m[2] ?? ''), base: m[1], prerelease: Boolean(m[2]) };
}

/** The notes file for a tag. A pre-release may reuse the notes of the release it leads up to. */
export function resolveNotesFile(tag, root = '.') {
  const { base, prerelease } = parseTag(tag);
  const candidates = [`${tag}.md`, ...(prerelease ? [`v${base}.md`] : [])];
  for (const name of candidates) {
    const file = path.join(root, NOTES_DIR, name);
    if (fs.existsSync(file)) return file;
  }
  return null;
}

/** Everything that would make a release go out half-written. Empty array = fine. */
export function problemsIn(markdown) {
  const problems = [];
  if (!/^##\s+Highlights\s*$/im.test(markdown)) problems.push('missing a "## Highlights" section (it feeds the Winget release notes)');
  if (/\b(TODO|TBD|FIXME)\b/.test(markdown)) problems.push('still contains TODO/TBD/FIXME placeholders');
  return problems;
}

const block = (name) => new RegExp(`<!--\\s*${name}\\s*-->\\n?([\\s\\S]*?)<!--\\s*/${name}\\s*-->\\n?`, 'g');
const dropBlock = (text, name) => text.replace(block(name), '');
const keepBlock = (text, name) => text.replace(block(name), '$1');

export function renderBody({ markdown, tag, repo = 'rhshourav/zlip', signed = false }) {
  const { version, prerelease } = parseTag(tag);
  let out = markdown;
  out = prerelease ? dropBlock(out, 'winget') : keepBlock(out, 'winget');
  out = signed ? dropBlock(out, 'unsigned') : keepBlock(out, 'unsigned');
  out = prerelease ? keepBlock(out, 'prerelease') : dropBlock(out, 'prerelease');
  out = out.replaceAll('{{tag}}', tag).replaceAll('{{version}}', version).replaceAll('{{repo}}', repo);
  return out.replace(/\n{3,}/g, '\n\n').trimEnd() + '\n';
}

/** The "## Highlights" section as plain text, for the Winget ReleaseNotes field (max 10 000 chars). */
export function highlights(markdown, max = 4000) {
  const m = /^##\s+Highlights\s*\n([\s\S]*?)(?=^##\s|(?![\s\S]))/im.exec(markdown);
  if (!m) return null;
  const text = m[1]
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1') // [text](url) -> text
    .replace(/\*{1,2}|`/g, '') // **bold**, *italic*, `code`
    .replace(/(^|\W)_{1,2}([^_\n]+)_{1,2}(?=\W|$)/g, '$1$2') // _italic_, but never the underscore in snake_case
    .replace(/^\s*[-*]\s+/gm, '- ')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
  if (!text) return null;
  if (text.length <= max) return text;
  const cut = text.slice(0, max);
  return cut.slice(0, Math.max(cut.lastIndexOf('\n'), 0) || max).trimEnd();
}

export function scaffold(version) {
  return `> TODO: one sentence on what this release is.

## Highlights

- TODO: the two or three things a user will notice.

## What's new

- TODO

## Install

<!-- winget -->
**Winget:** \`winget install rhshourav.Zlip\`

<!-- /winget -->
**Installer:** download \`Zlip_{{version}}_x64-setup.exe\` or \`Zlip_{{version}}_x64_en-US.msi\` from the assets below.

Verify a download against \`SHA256SUMS.txt\`:

\`\`\`powershell
(Get-FileHash .\\Zlip_{{version}}_x64_en-US.msi -Algorithm SHA256).Hash
\`\`\`

## Known limitations

- TODO
`.replaceAll('{{version}}', version);
}

// ----------------------------------------------------------------------- CLI
function flags(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i++) {
    if (!argv[i].startsWith('--')) continue;
    const key = argv[i].slice(2);
    const next = argv[i + 1];
    out[key] = next === undefined || next.startsWith('--') ? 'true' : (i++, next);
  }
  return out;
}

function main(argv) {
  const [cmd, ...rest] = argv;
  const f = flags(rest);
  switch (cmd) {
    case 'check': {
      const file = resolveNotesFile(f.tag);
      if (!file) {
        const { base, prerelease } = parseTag(f.tag);
        throw new Error(`No release notes found. Create ${NOTES_DIR}/${f.tag}.md${prerelease ? ` (or ${NOTES_DIR}/v${base}.md)` : ''}.\nStart from: node scripts/release-notes.mjs scaffold --version ${base}`);
      }
      const problems = problemsIn(fs.readFileSync(file, 'utf8'));
      if (problems.length) throw new Error(`${file}: ${problems.join('; ')}`);
      console.log(`Release notes OK: ${file}`);
      return;
    }
    case 'body': {
      const file = resolveNotesFile(f.tag);
      if (!file) throw new Error(`No release notes for ${f.tag}`);
      const body = renderBody({ markdown: fs.readFileSync(file, 'utf8'), tag: f.tag, repo: f.repo, signed: f.signed === 'true' });
      if (f.out) fs.writeFileSync(f.out, body);
      else process.stdout.write(body);
      return;
    }
    case 'scaffold': {
      const file = path.join(NOTES_DIR, `v${f.version}.md`);
      if (fs.existsSync(file)) throw new Error(`${file} already exists`);
      fs.mkdirSync(path.dirname(file), { recursive: true });
      fs.writeFileSync(file, scaffold(f.version));
      console.log(`Created ${file}. Fill in every TODO: the release is refused while any remain.`);
      return;
    }
    default:
      throw new Error('Usage: release-notes.mjs <check|body|scaffold> [--tag vX.Y.Z] [--version X.Y.Z] [--repo owner/name] [--signed true] [--out file]');
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
