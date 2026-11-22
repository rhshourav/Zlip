// Checking GitHub for a newer release.
//
// This only ever runs when the person presses "Check" in Settings: Zlip never contacts the
// network on its own. It asks for the project's latest published release and compares version
// numbers. Nothing is downloaded or installed; a newer version is opened as a web page.

export type UpdateState =
  | { status: 'idle' }
  | { status: 'checking' }
  | { status: 'current'; latest: string }
  | { status: 'available'; latest: string; url: string }
  | { status: 'error'; message: string };

export type CheckResult = Exclude<UpdateState, { status: 'idle' | 'checking' }>;

interface Parsed {
  core: [number, number, number];
  /** The dot-separated parts after the hyphen of "1.2.3-beta.1", or null for a final release. */
  pre: string[] | null;
}

const TIMEOUT_MS = 8000;

/** "1.2.3", "v1.2.3" and "1.2.3-beta.1" (a "+build" suffix is ignored). Anything else is null. */
export function parseVersion(text: string): Parsed | null {
  const m = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z.-]+)?$/.exec(text.trim());
  if (!m) return null;
  return { core: [Number(m[1]), Number(m[2]), Number(m[3])], pre: m[4] ? m[4].split('.') : null };
}

function comparePre(a: string[], b: string[]): number {
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const x = a[i];
    const y = b[i];
    if (x === undefined) return -1; // fewer parts is the lower version: 1.0.0-beta < 1.0.0-beta.1
    if (y === undefined) return 1;
    const xNum = /^\d+$/.test(x);
    const yNum = /^\d+$/.test(y);
    if (xNum && yNum) {
      const d = Number(x) - Number(y);
      if (d !== 0) return Math.sign(d);
    } else if (xNum !== yNum) {
      return xNum ? -1 : 1; // numbers sort before words: 1.0.0-1 < 1.0.0-beta
    } else if (x !== y) {
      return x < y ? -1 : 1;
    }
  }
  return 0;
}

/** Negative when `a` is older than `b`, 0 when equal, positive when newer. Null if either is not a version. */
export function compareVersions(a: string, b: string): number | null {
  const pa = parseVersion(a);
  const pb = parseVersion(b);
  if (!pa || !pb) return null;
  for (let i = 0; i < 3; i++) {
    const d = (pa.core[i] ?? 0) - (pb.core[i] ?? 0);
    if (d !== 0) return Math.sign(d);
  }
  if (pa.pre && pb.pre) return comparePre(pa.pre, pb.pre);
  if (pa.pre) return -1; // 1.0.0-beta is older than 1.0.0
  if (pb.pre) return 1;
  return 0;
}

/** "https://github.com/owner/repo" becomes "owner/repo". */
export function repoSlug(url: string): string | null {
  const m = /^https:\/\/github\.com\/([\w.-]+)\/([\w.-]+?)\/?$/.exec(url.trim());
  return m ? `${m[1]}/${m[2]}` : null;
}

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null;
const fail = (message: string): CheckResult => ({ status: 'error', message });

/**
 * Ask GitHub for the latest release of `repoUrl` and compare it with `current`.
 * Never throws: every failure comes back as `{ status: 'error' }` with a sentence to show.
 * `request` is `fetch`; it is a parameter so tests can answer without a network.
 */
export async function checkForUpdate(repoUrl: string, current: string, request: typeof fetch = fetch): Promise<CheckResult> {
  const base = repoUrl.replace(/\/+$/, '');
  const slug = repoSlug(base);
  if (!slug) return fail('This build doesn’t point at a GitHub repository.');
  if (!parseVersion(current)) return fail('Couldn’t read the installed version number.');

  let res: Response;
  try {
    res = await request(`https://api.github.com/repos/${slug}/releases/latest`, {
      // `Accept` is a plain header, so the browser sends the request without a preflight.
      headers: { Accept: 'application/vnd.github+json' },
      signal: AbortSignal.timeout(TIMEOUT_MS)
    });
  } catch (e) {
    return fail(e instanceof DOMException && e.name === 'TimeoutError' ? 'GitHub took too long to answer.' : 'Couldn’t reach GitHub. Check your connection.');
  }
  if (res.status === 404) return fail('No release has been published yet.');
  if (res.status === 403 || res.status === 429) return fail('GitHub is limiting requests right now. Try again later.');
  if (!res.ok) return fail(`GitHub answered with error ${res.status}.`);

  let body: unknown;
  try {
    body = await res.json();
  } catch {
    return fail('GitHub’s answer couldn’t be read.');
  }
  const tag = isRecord(body) && typeof body.tag_name === 'string' ? body.tag_name : '';
  if (!parseVersion(tag)) return fail('The latest release isn’t numbered like a version.');
  const latest = tag.trim().replace(/^v/, '');

  // Only ever point at a page of this project, whatever the answer says.
  const page = isRecord(body) && typeof body.html_url === 'string' && body.html_url.startsWith(`${base}/`) ? body.html_url : `${base}/releases/latest`;

  const order = compareVersions(latest, current);
  return order !== null && order > 0 ? { status: 'available', latest, url: page } : { status: 'current', latest };
}
