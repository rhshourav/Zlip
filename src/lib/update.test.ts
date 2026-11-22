import { describe, expect, it } from 'vitest';
import { checkForUpdate, compareVersions, parseVersion, repoSlug } from './update';

const REPO = 'https://github.com/rhshourav/zlip';

/** A fake `fetch` that answers with a fixed status and JSON body. */
const answer =
  (status: number, body: unknown): typeof fetch =>
  async () =>
    new Response(typeof body === 'string' ? body : JSON.stringify(body), { status });

describe('versions', () => {
  it('parses plain, v-prefixed and pre-release versions', () => {
    expect(parseVersion('1.2.3')).toEqual({ core: [1, 2, 3], pre: null });
    expect(parseVersion('v0.10.4')).toEqual({ core: [0, 10, 4], pre: null });
    expect(parseVersion('1.0.0-beta.2')).toEqual({ core: [1, 0, 0], pre: ['beta', '2'] });
    expect(parseVersion('1.0.0+build.5')).toEqual({ core: [1, 0, 0], pre: null });
  });

  it('rejects anything that is not a version', () => {
    for (const bad of ['', 'v1', '1.2', '1.2.x', 'release-1.2.3', '1.2.3.4', '1.2.3-']) expect(parseVersion(bad)).toBeNull();
  });

  it('compares numbers as numbers, not as text', () => {
    expect(compareVersions('0.10.0', '0.9.0')).toBe(1);
    expect(compareVersions('1.2.3', '1.2.3')).toBe(0);
    expect(compareVersions('v1.2.3', '1.2.3')).toBe(0);
    expect(compareVersions('1.0.9', '1.1.0')).toBe(-1);
    expect(compareVersions('2.0.0', '1.99.99')).toBe(1);
  });

  it('puts a pre-release before its release', () => {
    expect(compareVersions('1.0.0-beta', '1.0.0')).toBe(-1);
    expect(compareVersions('1.0.0', '1.0.0-rc.1')).toBe(1);
    expect(compareVersions('1.0.0-beta.2', '1.0.0-beta.11')).toBe(-1);
    expect(compareVersions('1.0.0-alpha', '1.0.0-beta')).toBe(-1);
    expect(compareVersions('1.0.0-beta', '1.0.0-beta.1')).toBe(-1);
    expect(compareVersions('1.0.0-1', '1.0.0-beta')).toBe(-1);
  });

  it('gives null when either side is not a version', () => {
    expect(compareVersions('nightly', '1.0.0')).toBeNull();
  });
});

describe('repository address', () => {
  it('extracts owner and name', () => {
    expect(repoSlug(REPO)).toBe('rhshourav/zlip');
    expect(repoSlug(`${REPO}/`)).toBe('rhshourav/zlip');
  });
  it('refuses anything that is not a plain github.com repository', () => {
    for (const bad of ['', 'http://github.com/a/b', 'https://example.com/a/b', 'https://github.com/a', 'https://github.com/a/b/c']) expect(repoSlug(bad)).toBeNull();
  });
});

describe('checking for an update', () => {
  it('reports a newer release with its page', async () => {
    const r = await checkForUpdate(REPO, '0.1.0', answer(200, { tag_name: 'v0.2.0', html_url: `${REPO}/releases/tag/v0.2.0` }));
    expect(r).toEqual({ status: 'available', latest: '0.2.0', url: `${REPO}/releases/tag/v0.2.0` });
  });

  it('reports up to date for the same or an older release', async () => {
    expect(await checkForUpdate(REPO, '0.2.0', answer(200, { tag_name: 'v0.2.0' }))).toEqual({ status: 'current', latest: '0.2.0' });
    expect(await checkForUpdate(REPO, '0.3.0', answer(200, { tag_name: 'v0.2.0' }))).toEqual({ status: 'current', latest: '0.2.0' });
  });

  it('asks GitHub for this repository only, with a plain Accept header', async () => {
    let seen: { url: string; init?: RequestInit } | undefined;
    const spy: typeof fetch = async (url, init) => {
      seen = { url: String(url), init };
      return new Response(JSON.stringify({ tag_name: 'v0.1.0' }), { status: 200 });
    };
    await checkForUpdate(`${REPO}/`, '0.1.0', spy);
    expect(seen?.url).toBe('https://api.github.com/repos/rhshourav/zlip/releases/latest');
    expect(seen?.init?.headers).toEqual({ Accept: 'application/vnd.github+json' });
  });

  it('never points outside the project, whatever the answer says', async () => {
    const r = await checkForUpdate(REPO, '0.1.0', answer(200, { tag_name: 'v0.2.0', html_url: 'https://evil.example/download.exe' }));
    expect(r).toEqual({ status: 'available', latest: '0.2.0', url: `${REPO}/releases/latest` });
    const r2 = await checkForUpdate(REPO, '0.1.0', answer(200, { tag_name: 'v0.2.0', html_url: 'https://github.com/rhshourav/zlip-evil/x' }));
    expect(r2).toMatchObject({ url: `${REPO}/releases/latest` });
  });

  it('says what went wrong in plain words', async () => {
    const msg = async (p: Promise<{ status: string; message?: string }>) => (await p).message;
    expect(await msg(checkForUpdate(REPO, '0.1.0', answer(404, {})))).toMatch(/No release/);
    expect(await msg(checkForUpdate(REPO, '0.1.0', answer(403, {})))).toMatch(/limiting/);
    expect(await msg(checkForUpdate(REPO, '0.1.0', answer(500, {})))).toMatch(/error 500/);
    expect(await msg(checkForUpdate(REPO, '0.1.0', answer(200, 'not json')))).toMatch(/couldn’t be read/);
    expect(await msg(checkForUpdate(REPO, '0.1.0', answer(200, { tag_name: 'nightly' })))).toMatch(/isn’t numbered/);
    expect(await msg(checkForUpdate(REPO, '0.1.0', answer(200, [])))).toMatch(/isn’t numbered/);
    expect(await msg(checkForUpdate(REPO, '0.1.0', async () => Promise.reject(new TypeError('Failed to fetch'))))).toMatch(/Couldn’t reach GitHub/);
    expect(await msg(checkForUpdate(REPO, '0.1.0', async () => Promise.reject(new DOMException('timeout', 'TimeoutError'))))).toMatch(/too long/);
  });

  it('refuses a build whose address or version is unusable, without asking the network', async () => {
    const never: typeof fetch = async () => {
      throw new Error('should not be called');
    };
    expect((await checkForUpdate('https://example.com/x/y', '0.1.0', never)).status).toBe('error');
    expect((await checkForUpdate(REPO, 'dev', never)).status).toBe('error');
  });
});
