// Settings save as they change: every change goes to the backend (the only place they are
// written) and the app follows its answer. Changes are sent one after another, each on top of
// the last answer, so two quick changes never undo each other.
import { useCallback } from 'react';
import { atom, useSetAtom, useStore } from 'jotai';
import { backend, type AppSettings } from '../backend';
import { settingsAtom } from '../stores/settingsStore';
import type { ToastMessage } from '../components/Toast';

/** A failed save, shown by the settings page. */
export const settingsNoticeAtom = atom<ToastMessage | null>(null);

let queue: Promise<unknown> = Promise.resolve();

export function useUpdateSettings() {
  const store = useStore();
  const notify = useSetAtom(settingsNoticeAtom);

  return useCallback(
    (patch: Partial<AppSettings>): Promise<AppSettings | null> => {
      const run = queue.then(async () => {
        try {
          const next = await backend.settings.update({ ...store.get(settingsAtom), ...patch });
          store.set(settingsAtom, next);
          return next;
        } catch (e) {
          notify({ severity: 'error', text: e instanceof Error ? e.message : String(e) });
          return null;
        }
      });
      queue = run;
      return run;
    },
    [store, notify],
  );
}
