// When to check for a new version and whether to offer it (zoo-release docs/UPDATE.md §2–3).

export const REPO_URL = 'https://github.com/zoosewu/zoo-downloader';
export const DAY_MS = 24 * 60 * 60 * 1000;

type Version = [major: number, minor: number, patch: number];

/** `major.minor.patch`; pre-releases and anything else are not versions to update to. */
export function parseVersion(text: string | null | undefined): Version | null {
  const m = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(text?.trim() ?? '');
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

/**
 * How `latest` differs from `current`: `minor` covers a new major too (offered on the main
 * screen), `patch` only on the About page, `none` when it is not newer or not a version.
 */
export function newerBy(current: string, latest: string | null | undefined): 'minor' | 'patch' | 'none' {
  const a = parseVersion(current);
  const b = parseVersion(latest);
  if (!a || !b) return 'none';
  if (b[0] !== a[0]) return b[0] > a[0] ? 'minor' : 'none';
  if (b[1] !== a[1]) return b[1] > a[1] ? 'minor' : 'none';
  return b[2] > a[2] ? 'patch' : 'none';
}

/** What "Skip this version" remembers: every `major.minor.x` is skipped with it. */
export function majorMinor(version: string): string {
  const v = parseVersion(version);
  return v ? `${v[0]}.${v[1]}` : version;
}

/** The main screen offers `latest`: a new major or minor that was not skipped. */
export function offerOnMainScreen(current: string, latest: string | null | undefined, skipped: string | null | undefined): boolean {
  return !!latest && newerBy(current, latest) === 'minor' && majorMinor(latest) !== skipped;
}

/** How long the last successful check stays good (a day); 0 when it is due now. */
export function checkDueIn(lastCheck: string | null | undefined, now: number): number {
  const at = lastCheck ? Date.parse(lastCheck) : NaN;
  if (Number.isNaN(at) || at > now) return 0;
  return Math.max(0, at + DAY_MS - now);
}

/** A failure worth naming (`update.reason.*`); anything else shows the updater's own message. */
export function failureReason(message: string): 'signature' | 'network' | null {
  if (/signature/i.test(message)) return 'signature';
  if (/error sending request|connect|timed? ?out|dns|network|status code/i.test(message)) return 'network';
  return null;
}

/** "today 14:31", "yesterday 09:05", else the date and time; 24-hour clock. */
export function formatCheckedAt(iso: string, language: string, now = new Date()): string {
  const at = new Date(iso);
  const time = at.toLocaleTimeString(language, { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' });
  const days = Math.round((startOfDay(now) - startOfDay(at)) / DAY_MS);
  if (days === 0 || days === 1) {
    const day = new Intl.RelativeTimeFormat(language, { numeric: 'auto' }).format(-days, 'day');
    return `${day} ${time}`;
  }
  return `${at.toLocaleDateString(language)} ${time}`;
}

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}
