// App updates (tauri-plugin-updater reading the release's latest.json), following zoo-release's
// docs/UPDATE.md: checked a minute after start and once a day while enabled (a check from the
// last day is reused); downloading and restarting wait for the user. Shared state, so the
// banner and the About page show the same update.
import { useCallback, useEffect } from 'react';
import { atom, useAtomValue, useSetAtom, useStore } from 'jotai';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { getVersion } from '@tauri-apps/api/app';
import { backend } from '../backend';
import { settingsAtom } from '../stores/settingsStore';
import { useUpdateSettings } from './useUpdateSettings';
import { DAY_MS, checkDueIn, majorMinor, newerBy } from '../utils/appUpdate';

const FIRST_CHECK_MS = 60 * 1000;

export type UpdateState =
  /** Nothing known yet. */
  | { kind: 'idle' }
  /** A check the user asked for. Automatic checks show nothing while they run. */
  | { kind: 'checking' }
  | { kind: 'upToDate' }
  | { kind: 'available'; version: string }
  /** `total` is null when the server does not say. */
  | { kind: 'downloading'; version: string; done: number; total: number | null }
  | { kind: 'ready'; version: string }
  | { kind: 'restarting'; version: string }
  /** A check the user asked for failed (automatic ones fail silently). */
  | { kind: 'checkFailed' }
  /** Downloading or installing failed. */
  | { kind: 'failed'; version: string; message: string };

export const updateStateAtom = atom<UpdateState>({ kind: 'idle' });
/** The running app's version; empty until read. */
export const appVersionAtom = atom('');
/** "Later" on the banner: not shown again until the next start. */
export const bannerClosedAtom = atom(false);

/** The update the last check found, held by the updater until installed or replaced. */
let pending: Update | null = null;
/** Bumped by cancel: a download that finishes after it is thrown away. */
let downloadRun = 0;

function hold(update: Update | null) {
  if (pending && pending !== update) pending.close().catch(() => {});
  pending = update;
}

const busy = (s: UpdateState) => s.kind === 'checking' || s.kind === 'downloading' || s.kind === 'ready' || s.kind === 'restarting';
const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function useAppUpdate() {
  const store = useStore();
  const setState = useSetAtom(updateStateAtom);
  const setBannerClosed = useSetAtom(bannerClosedAtom);
  const updateSettings = useUpdateSettings();

  /** `manual`: from the About page, which shows progress and failures; automatic checks are silent. */
  const checkNow = useCallback(
    async (manual: boolean) => {
      if (busy(store.get(updateStateAtom))) return;
      if (manual) setState({ kind: 'checking' });
      try {
        const found = await check();
        hold(found);
        const current = await getVersion();
        await updateSettings({ updateLastCheck: new Date().toISOString(), updateLatest: found?.version ?? current });
        setState(found ? { kind: 'available', version: found.version } : { kind: 'upToDate' });
      } catch (e) {
        if (manual) setState({ kind: 'checkFailed' });
        else console.warn('[update] check failed:', errorText(e));
      }
    },
    [store, setState, updateSettings],
  );

  /** Download it; the user restarts when it is ready. */
  const install = useCallback(async () => {
    const run = ++downloadRun;
    setBannerClosed(false);
    let version = '';
    try {
      // The last answer may be a saved one, with nothing held to download.
      if (!pending) hold(await check());
      const update = pending;
      if (!update) return setState({ kind: 'upToDate' });
      version = update.version;
      let done = 0;
      let total: number | null = null;
      setState({ kind: 'downloading', version, done, total });
      await update.download((event) => {
        if (run !== downloadRun) return;
        if (event.event === 'Started') total = event.data.contentLength ?? null;
        if (event.event === 'Progress') done += event.data.chunkLength;
        setState({ kind: 'downloading', version, done, total });
      });
      if (run !== downloadRun) {
        update.close().catch(() => {});
        return;
      }
      setState({ kind: 'ready', version });
    } catch (e) {
      if (run !== downloadRun) return;
      hold(null); // trying again starts from a fresh check
      setState({ kind: 'failed', version, message: errorText(e) });
    }
  }, [setState, setBannerClosed]);

  /** Stop following the download; the updater has no way to stop it, so its bytes are dropped when it ends. */
  const cancel = useCallback(() => {
    const s = store.get(updateStateAtom);
    if (s.kind !== 'downloading') return;
    downloadRun++;
    pending = null; // closed by the abandoned download when it ends
    setState({ kind: 'available', version: s.version });
  }, [store, setState]);

  /** Stop the downloads cleanly, install, start again. */
  const restart = useCallback(async () => {
    const s = store.get(updateStateAtom);
    if (s.kind !== 'ready' || !pending) return;
    setState({ kind: 'restarting', version: s.version });
    try {
      await backend.app.prepareUpdate();
      await pending.install();
      await relaunch(); // Windows ends the app in install(); elsewhere restart here
    } catch (e) {
      setState({ kind: 'failed', version: s.version, message: errorText(e) });
    }
  }, [store, setState]);

  const skip = useCallback((version: string) => updateSettings({ updateSkipped: majorMinor(version) }), [updateSettings]);

  return { checkNow, install, cancel, restart, skip, later: () => setBannerClosed(true) };
}

/** Read the app's version once (main window). */
export function useAppVersion() {
  const version = useAtomValue(appVersionAtom);
  const setVersion = useSetAtom(appVersionAtom);
  useEffect(() => {
    if (!version) getVersion().then(setVersion).catch(() => {});
  }, [version, setVersion]);
  return version;
}

/** Run the automatic checks (main window only). */
export function useAutomaticUpdateChecks(ready: boolean) {
  const store = useStore();
  const enabled = useAtomValue(settingsAtom).checkAppUpdates;
  const setState = useSetAtom(updateStateAtom);
  const { checkNow } = useAppUpdate();
  useAppVersion();

  useEffect(() => {
    // Dev builds have no release to compare with.
    if (!ready || !enabled || import.meta.env.DEV) return;
    let timer: ReturnType<typeof setTimeout>;
    const run = async () => {
      await checkNow(false);
      timer = setTimeout(run, DAY_MS);
    };
    timer = setTimeout(async () => {
      const { updateLastCheck, updateLatest } = store.get(settingsAtom);
      const dueIn = checkDueIn(updateLastCheck, Date.now());
      if (dueIn === 0) return run();
      // Checked within the last day: use that answer, and check again when it is a day old.
      const current = await getVersion();
      if (store.get(updateStateAtom).kind === 'idle') {
        setState(newerBy(current, updateLatest) === 'none' ? { kind: 'upToDate' } : { kind: 'available', version: updateLatest! });
      }
      timer = setTimeout(run, dueIn);
    }, FIRST_CHECK_MS);
    return () => clearTimeout(timer);
  }, [ready, enabled, store, setState, checkNow]);
}
