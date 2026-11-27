// Settings → General: language and theme, how the app runs on the computer, and app updates.
import { IconButton, MenuItem, Select, Switch, Tooltip, Typography } from '@mui/material';
import FolderOpenIcon from '@mui/icons-material/FolderOpen';
import { useTranslation } from 'react-i18next';
import { useAtomValue, useSetAtom } from 'jotai';
import { settingsAtom } from '../stores/settingsStore';
import { backend, type Theme } from '../backend';
import AppUpdateSection from './AppUpdateSection';
import { SectionedPanel, SettingRow, SettingsGroup } from './settings/layout';
import { ChoiceField } from './settings/fields';
import { settingsNoticeAtom, useUpdateSettings } from '../hooks/useUpdateSettings';
import { applyLanguage, languagesAtom, loadUserLocales, localeProblemsAtom } from '../utils/languages';

type Startup = 'off' | 'window' | 'tray';

export default function GeneralSettings() {
  const { t } = useTranslation();
  return (
    <SectionedPanel
      id="general"
      sections={[
        { id: 'appearance', label: t('settings.section.appearance'), content: <Appearance /> },
        { id: 'system', label: t('settings.section.system'), content: <System /> },
        { id: 'updates', label: t('update.title'), content: <Updates /> },
      ]}
    />
  );
}

function Appearance() {
  const { t } = useTranslation();
  const settings = useAtomValue(settingsAtom);
  const update = useUpdateSettings();
  const notify = useSetAtom(settingsNoticeAtom);
  const languages = useAtomValue(languagesAtom);
  const localeProblems = useAtomValue(localeProblemsAtom);

  const setLanguage = async (language: string) => {
    const next = await update({ language });
    if (next) await applyLanguage(next.language);
  };

  return (
    <SettingsGroup>
      <SettingRow
        label={t('settings.language')}
        description={
          localeProblems.length > 0 &&
          localeProblems.map((p) => (
            <Typography key={p.file} variant="caption" color="error" component="div">
              {t('settings.languageFileError', { file: p.file, message: p.message })}
            </Typography>
          ))
        }
        control={
          <>
            <Tooltip title={t('settings.languageFolder')}>
              <IconButton size="small" onClick={() => backend.locales.openFolder().catch((e) => notify({ severity: 'error', text: e.message }))}>
                <FolderOpenIcon fontSize="small" />
              </IconButton>
            </Tooltip>
            <Select
              size="small"
              value={settings.language}
              displayEmpty
              sx={{ minWidth: 200 }}
              onOpen={() => loadUserLocales().catch((e) => console.error('Failed to load language files:', e.message))}
              onChange={(e) => setLanguage(e.target.value)}
            >
              <MenuItem value="">{t('settings.languageSystem')}</MenuItem>
              {languages.map((l) => (
                <MenuItem key={l.code} value={l.code}>
                  {l.name}
                </MenuItem>
              ))}
              {/* A saved language whose file is gone: keep it selectable as it is. */}
              {settings.language && !languages.some((l) => l.code === settings.language) && (
                <MenuItem value={settings.language}>{settings.language}</MenuItem>
              )}
            </Select>
          </>
        }
      />
      <SettingRow
        label={t('settings.theme')}
        control={
          <ChoiceField<Theme>
            value={settings.theme}
            onChange={(theme) => update({ theme })}
            options={[
              { value: 'light', label: t('settings.themeLight') },
              { value: 'dark', label: t('settings.themeDark') },
            ]}
          />
        }
      />
    </SettingsGroup>
  );
}

function System() {
  const { t } = useTranslation();
  const settings = useAtomValue(settingsAtom);
  const update = useUpdateSettings();
  const startup: Startup = !settings.launchAtStartup ? 'off' : settings.startInTray && settings.enableSystemTray ? 'tray' : 'window';

  return (
    <SettingsGroup>
      <SettingRow
        label={t('settings.notifications')}
        control={<Switch checked={settings.enableNotifications} onChange={(e) => update({ enableNotifications: e.target.checked })} />}
      />
      <SettingRow
        label={t('settings.systemTray')}
        control={<Switch checked={settings.enableSystemTray} onChange={(e) => update({ enableSystemTray: e.target.checked })} />}
      />
      <SettingRow
        label={t('settings.startup.label')}
        control={
          <ChoiceField<Startup>
            value={startup}
            onChange={(next) => update({ launchAtStartup: next !== 'off', startInTray: next === 'tray' })}
            options={[
              { value: 'off', label: t('settings.startup.off') },
              { value: 'window', label: t('settings.startup.window') },
              { value: 'tray', label: t('settings.startup.tray'), disabled: !settings.enableSystemTray },
            ]}
          />
        }
      />
    </SettingsGroup>
  );
}

function Updates() {
  const { t } = useTranslation();
  const settings = useAtomValue(settingsAtom);
  const update = useUpdateSettings();
  return (
    <SettingsGroup>
      <SettingRow label={<AppUpdateSection />} />
      <SettingRow
        label={t('update.automatic')}
        control={<Switch checked={settings.checkAppUpdates} onChange={(e) => update({ checkAppUpdates: e.target.checked })} />}
      />
    </SettingsGroup>
  );
}
