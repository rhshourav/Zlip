#!/usr/bin/env node
// Generates the three Winget manifest files (version, installer, defaultLocale) and
// refuses to write them if anything is malformed. Used by .github/workflows/release.yml
// and runnable by hand:
//
//   node scripts/winget-manifest.mjs --version 0.1.0 --tag v0.1.0 \
//     --url https://github.com/rhshourav/zlip/releases/download/v0.1.0/Zlip_0.1.0_x64_en-US.msi \
//     --sha256 <64 hex> --product-code "{GUID}" --upgrade-code "{GUID}" --out winget-manifests
//
// Every flag falls back to an environment variable, so the workflow can pass its own env:
//   VERSION TAG GITHUB_REPOSITORY INSTALLER_URL INSTALLER_SHA256 PRODUCT_CODE UPGRADE_CODE
//   DISPLAY_VERSION PACKAGE_ID PACKAGE_LICENSE PACKAGE_SHORT_DESC
//
// Output: <out>/manifests/<first letter>/<Publisher>/<Package>/<version>/<Id>[.installer|.locale.en-US].yaml
// Test locally (admin PowerShell, once: winget settings --enable LocalManifestFiles):
//   winget validate --manifest <that folder>
//   winget install  --manifest <that folder>

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { highlights, resolveNotesFile } from './release-notes.mjs';

const DEFAULTS = {
  id: 'rhshourav.Zlip',
  repo: 'rhshourav/zlip',
  license: 'MIT',
  shortDesc: 'Fast, private, glass-styled clipboard manager with paste queues and snippets.',
  manifestVersion: '1.10.0', // still accepted by winget-pkgs; newer schemas exist (1.12.0)
  minOs: '10.0.18362.0',
  tags: ['clipboard', 'clipboard-manager', 'productivity', 'rust', 'tauri']
};

// ------------------------------------------------------------------ YAML out
/** A YAML scalar that parses back to the same string. Plain when safe, double-quoted otherwise. */
export function scalar(value) {
  const s = String(value);
  const plainSafe =
    s.length > 0 &&
    s === s.trim() &&
    !/^[-?:,[\]{}#&*!|>'"%@`]/.test(s) &&
    !/: |:$| #|\t|\n|\r/.test(s) &&
    !/^(true|false|yes|no|on|off|null|~)$/i.test(s) &&
    !/^[+-]?(\d[\d_]*\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(s) && // would parse as a number
    !/^0[xob]/i.test(s) &&
    !/^\d{4}-\d{1,2}-\d{1,2}([Tt\s]|$)/.test(s); // would parse as a date
  if (plainSafe) return s;
  return `"${s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '\\r').replace(/\t/g, '\\t')}"`;
}

/** A multi-line text as a literal block scalar (`|-`), indented under its key. */
function block(text, indent = '  ') {
  return `|-\n${text
    .split('\n')
    .map((l) => (l.length ? indent + l : ''))
    .join('\n')}`;
}

// ---------------------------------------------------------------- validation
// The schema forbids control characters in identifiers, so the patterns have to name them.
// eslint-disable-next-line no-control-regex
const ID_RE = /^[^.\s\\/:*?"<>|\x01-\x1f]{1,32}(\.[^.\s\\/:*?"<>|\x01-\x1f]{1,32}){1,7}$/;
// eslint-disable-next-line no-control-regex
const VERSION_RE = /^[^\\/:*?"<>|\x01-\x1f]{1,128}$/;
const GUID_RE = /^\{[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}\}$/;

/** Collects every problem instead of stopping at the first, so one run shows them all. */
export function validate(m) {
  const p = [];
  const need = (cond, msg) => {
    if (!cond) p.push(msg);
  };
  need(ID_RE.test(m.id), `PackageIdentifier '${m.id}' must look like Publisher.Package (no spaces, max 32 chars per part)`);
  need(VERSION_RE.test(m.version), `PackageVersion '${m.version}' is empty, too long, or has a forbidden character`);
  need(/^[A-Fa-f0-9]{64}$/.test(m.sha256 ?? ''), `InstallerSha256 must be 64 hex characters (got '${m.sha256}')`);
  let url;
  try {
    url = new URL(m.url);
  } catch {
    /* reported below */
  }
  need(url && url.protocol === 'https:' && m.url.length <= 2048, `InstallerUrl must be an https URL (got '${m.url}')`);
  need(['wix', 'msi', 'nullsoft', 'inno', 'exe'].includes(m.installerType), `InstallerType '${m.installerType}' is not supported by this script`);
  need(['machine', 'user'].includes(m.scope), `Scope must be machine or user (got '${m.scope}')`);
  need(/^\d{4}-\d{2}-\d{2}$/.test(m.released), `ReleaseDate must be YYYY-MM-DD (got '${m.released}')`);
  if (['wix', 'msi'].includes(m.installerType)) {
    need(GUID_RE.test(m.productCode ?? ''), `ProductCode must be a {GUID} for an MSI (got '${m.productCode}')`);
    if (m.upgradeCode) need(GUID_RE.test(m.upgradeCode), `UpgradeCode must be a {GUID} (got '${m.upgradeCode}')`);
  }
  need(m.shortDesc.length >= 1 && m.shortDesc.length <= 256, `ShortDescription must be 1-256 characters (is ${m.shortDesc.length})`);
  need(m.license.length >= 1 && m.license.length <= 512, 'License must be 1-512 characters');
  need(!m.releaseNotes || m.releaseNotes.length <= 10000, `ReleaseNotes must be at most 10000 characters (is ${m.releaseNotes?.length})`);
  need(m.tags.length <= 16, 'At most 16 Tags');
  for (const t of m.tags) need(/^\S{1,40}$/.test(t), `Tag '${t}' must have no spaces and be at most 40 characters`);
  need(/^[\w.-]+\/[\w.-]+$/.test(m.repo), `Repository '${m.repo}' must be owner/name`);
  return p;
}

// ---------------------------------------------------------------- generation
export function buildManifests(input) {
  const m = {
    ...DEFAULTS,
    scope: 'machine',
    installerType: 'wix',
    arch: 'x64',
    tags: DEFAULTS.tags,
    ...input
  };
  m.tag ??= `v${m.version}`;
  const problems = validate(m);
  if (problems.length) throw new Error(`Refusing to write manifests:\n - ${problems.join('\n - ')}`);

  const [publisher, ...rest] = m.id.split('.');
  const pkg = rest.join('.');
  const repoUrl = `https://github.com/${m.repo}`;
  const schema = (kind) => `# yaml-language-server: $schema=https://aka.ms/winget-manifest.${kind}.${m.manifestVersion}.schema.json`;
  const isMsi = ['wix', 'msi'].includes(m.installerType);

  const version = [
    schema('version'),
    `PackageIdentifier: ${m.id}`,
    `PackageVersion: ${scalar(m.version)}`,
    'DefaultLocale: en-US',
    'ManifestType: version',
    `ManifestVersion: ${m.manifestVersion}`
  ];

  const installer = [
    schema('installer'),
    `PackageIdentifier: ${m.id}`,
    `PackageVersion: ${scalar(m.version)}`,
    'InstallerLocale: en-US',
    `MinimumOSVersion: ${m.minOs}`,
    `InstallerType: ${m.installerType}`,
    `Scope: ${m.scope}`,
    'InstallModes:',
    '- interactive',
    '- silent',
    '- silentWithProgress',
    'UpgradeBehavior: install',
    `ReleaseDate: ${m.released}`,
    'Installers:',
    `- Architecture: ${m.arch}`,
    `  InstallerUrl: ${m.url}`,
    `  InstallerSha256: ${scalar(m.sha256.toUpperCase())}`
  ];
  if (isMsi) {
    installer.push(
      `  ProductCode: ${scalar(m.productCode)}`,
      '  AppsAndFeaturesEntries:',
      `  - DisplayName: ${scalar(pkg)}`,
      `    Publisher: ${scalar(publisher)}`,
      `    DisplayVersion: ${scalar(m.displayVersion ?? m.version)}`,
      `    ProductCode: ${scalar(m.productCode)}`
    );
    if (m.upgradeCode) installer.push(`    UpgradeCode: ${scalar(m.upgradeCode)}`);
    installer.push(`    InstallerType: ${m.installerType}`);
  }
  installer.push('ManifestType: installer', `ManifestVersion: ${m.manifestVersion}`);

  const locale = [
    schema('defaultLocale'),
    `PackageIdentifier: ${m.id}`,
    `PackageVersion: ${scalar(m.version)}`,
    'PackageLocale: en-US',
    `Publisher: ${scalar(publisher)}`,
    `PublisherUrl: https://github.com/${publisher}`,
    `PublisherSupportUrl: ${repoUrl}/issues`,
    `Author: ${scalar(publisher)}`,
    `PackageName: ${scalar(pkg)}`,
    `PackageUrl: ${repoUrl}`,
    `License: ${scalar(m.license)}`,
    `LicenseUrl: ${repoUrl}/blob/${m.tag}/LICENSE`,
    `ShortDescription: ${scalar(m.shortDesc)}`,
    'Tags:',
    ...m.tags.map((t) => `- ${scalar(t)}`)
  ];
  if (m.releaseNotes) locale.push(`ReleaseNotes: ${block(m.releaseNotes)}`);
  locale.push(`ReleaseNotesUrl: ${repoUrl}/releases/tag/${m.tag}`, 'ManifestType: defaultLocale', `ManifestVersion: ${m.manifestVersion}`);

  const dir = path.join('manifests', publisher[0].toLowerCase(), publisher, pkg, m.version);
  return {
    dir,
    files: {
      [`${m.id}.yaml`]: version.join('\n') + '\n',
      [`${m.id}.installer.yaml`]: installer.join('\n') + '\n',
      [`${m.id}.locale.en-US.yaml`]: locale.join('\n') + '\n'
    }
  };
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

export function inputFromArgs(argv, env = process.env) {
  const f = flags(argv);
  const pick = (flag, envName) => f[flag] ?? env[envName] ?? undefined;
  const version = pick('version', 'VERSION');
  // Release notes: an explicit file, else the file for this tag if there is one.
  let notesFile = f['notes-file'];
  if (!notesFile) {
    try {
      notesFile = resolveNotesFile(pick('tag', 'TAG') ?? `v${version}`);
    } catch {
      notesFile = null; // the tag is not a release tag: no notes
    }
  }
  const releaseNotes = notesFile ? (highlights(fs.readFileSync(notesFile, 'utf8')) ?? undefined) : undefined;
  const input = {
    id: pick('id', 'PACKAGE_ID'),
    version,
    tag: pick('tag', 'TAG'),
    repo: pick('repo', 'GITHUB_REPOSITORY'),
    url: pick('url', 'INSTALLER_URL'),
    sha256: pick('sha256', 'INSTALLER_SHA256'),
    installerType: pick('installer-type', 'INSTALLER_TYPE'),
    scope: pick('scope', 'INSTALLER_SCOPE'),
    productCode: pick('product-code', 'PRODUCT_CODE'),
    upgradeCode: pick('upgrade-code', 'UPGRADE_CODE'),
    displayVersion: pick('display-version', 'DISPLAY_VERSION'),
    license: pick('license', 'PACKAGE_LICENSE'),
    shortDesc: pick('short-desc', 'PACKAGE_SHORT_DESC'),
    manifestVersion: pick('manifest-version', 'MANIFEST_VERSION'),
    released: f.released ?? new Date().toISOString().slice(0, 10),
    releaseNotes
  };
  // Drop unset keys so they do not overwrite DEFAULTS with `undefined`.
  return { input: Object.fromEntries(Object.entries(input).filter(([, v]) => v !== undefined && v !== '')), out: f.out ?? 'winget-manifests' };
}

function main(argv) {
  const { input, out } = inputFromArgs(argv);
  const { dir, files } = buildManifests(input);
  const target = path.join(out, dir);
  fs.mkdirSync(target, { recursive: true });
  for (const [name, text] of Object.entries(files)) {
    fs.writeFileSync(path.join(target, name), text, 'utf8');
    console.log(`--- ${name}\n${text}`);
  }
  console.log(`Wrote ${Object.keys(files).length} manifests to ${target}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(`error: ${e.message}`);
    process.exit(1);
  }
}
