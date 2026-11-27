// Settings → Advanced: the default User-Agent, and the settings file (export, import, defaults).
import { Button, Dialog, DialogActions, DialogContent, DialogContentText } from '@mui/material';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useAtomValue, useSetAtom } from 'jotai';
import { open, save } from '@tauri-apps/plugin-dialog';
import { backend } from '../backend';
import { settingsAtom } from '../stores/settingsStore';
import UserAgentField from './UserAgentField';
import { SectionedPanel, SettingRow, SettingsGroup } from './settings/layout';
import { settingsNoticeAtom, useUpdateSettings } from '../hooks/useUpdateSettings';
import { applyLanguage } from '../utils/languages';

export default function AdvancedSettings() {
  const { t } = useTranslation();
  return (
    <SectionedPanel
      id="advanced"
      sections={[
        { id: 'userAgent', label: t('headers.userAgent'), content: <UserAgent /> },
        { id: 'file', label: t('settings.section.file'), content: <SettingsFile /> },
      ]}
    />
  );
}

function UserAgent() {
  const { t } = useTranslation();
  const saved = useAtomValue(settingsAtom).userAgent ?? '';
  const update = useUpdateSettings();
  const [value, setValue] = useState(saved);
  useEffect(() => setValue(saved), [saved]);

  const commit = () => {
    const next = value.trim() ? value : null;
    if (next !== (saved || null)) update({ userAgent: next });
  };

  return (
    <SettingsGroup>
      <SettingRow label={t('headers.userAgent')} description={t('settings.userAgentHelp')}>
        {/* Saved when the field is left: typing a long one sends nothing until then. */}
        <div onBlur={commit}>
          <UserAgentField bare value={value} onChange={setValue} placeholder={t('headers.userAgentDefault')} />
        </div>
      </SettingRow>
    </SettingsGroup>
  );
}

function SettingsFile() {
  const { t } = useTranslation();
  const setSettings = useSetAtom(settingsAtom);
  const notify = useSetAtom(settingsNoticeAtom);
  const update = useUpdateSettings();
  const [confirmReset, setConfirmReset] = useState(false);

  const run = (action: () => Promise<string | null>, failure: string) => async () => {
    try {
      const done = await action();
      if (done) notify({ severity: 'success', text: done });
    } catch (e) {
      notify({ severity: 'error', text: `${failure}: ${e instanceof Error ? e.message : String(e)}` });
    }
  };
  /** Replaced settings: the language may have changed with them. */
  const replaced = async (next: Awaited<ReturnType<typeof backend.settings.reset>>) => {
    setSettings(next);
    await applyLanguage(next.language);
  };

  const exportFile = run(async () => {
    const path = await save({ filters: [{ name: 'JSON', extensions: ['json'] }], defaultPath: 'zoo-downloader-settings.json' });
    if (!path) return null;
    await backend.settings.export(path);
    return t('settings.exportSuccess');
  }, t('settings.errors.exportFailed'));

  const importFile = run(async () => {
    const path = await open({ filters: [{ name: 'JSON', extensions: ['json'] }], multiple: false });
    if (typeof path !== 'string') return null;
    await update({}); // let a change still on its way land first
    await replaced(await backend.settings.import(path));
    return t('settings.importSuccess');
  }, t('settings.errors.importFailed'));

  const reset = run(async () => {
    await replaced(await backend.settings.reset());
    return t('settings.resetSuccess');
  }, t('settings.errors.resetFailed'));

  return (
    <>
      <SettingsGroup>
        <SettingRow
          label={t('settings.section.file')}
          description={t('settings.fileHelp')}
          control={
            <>
              <Button variant="outlined" onClick={exportFile}>
                {t('settings.export')}
              </Button>
              <Button variant="outlined" onClick={importFile}>
                {t('settings.import')}
              </Button>
            </>
          }
        />
        <SettingRow
          label={t('settings.resetToDefaults')}
          description={t('settings.resetHelp')}
          control={
            <Button variant="outlined" color="warning" onClick={() => setConfirmReset(true)}>
              {t('settings.reset')}
            </Button>
          }
        />
      </SettingsGroup>
      <Dialog open={confirmReset} onClose={() => setConfirmReset(false)}>
        <DialogContent>
          <DialogContentText>{t('settings.confirmReset')}</DialogContentText>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setConfirmReset(false)}>{t('common.cancel')}</Button>
          <Button
            color="warning"
            onClick={() => {
              setConfirmReset(false);
              reset();
            }}
          >
            {t('settings.reset')}
          </Button>
        </DialogActions>
      </Dialog>
    </>
  );
}
