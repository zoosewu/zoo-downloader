// Settings → Downloads: where and how fast, what happens to new links, the browser extension,
// and retrying.
import { Box, Button, Switch, Typography } from '@mui/material';
import { useTranslation } from 'react-i18next';
import { useAtomValue } from 'jotai';
import { open } from '@tauri-apps/plugin-dialog';
import { settingsAtom } from '../stores/settingsStore';
import BrowserExtensionSection from './BrowserExtensionSection';
import { SectionedPanel, SettingRow, SettingsGroup } from './settings/layout';
import { ChoiceField, NumberField } from './settings/fields';
import { useUpdateSettings } from '../hooks/useUpdateSettings';

type Clipboard = 'off' | 'ask' | 'start';
type LinkAction = 'ask' | 'start';

export default function DownloadSettings() {
  const { t } = useTranslation();
  return (
    <SectionedPanel
      id="downloads"
      sections={[
        { id: 'download', label: t('settings.section.download'), content: <Download /> },
        { id: 'links', label: t('settings.section.links'), content: <Links /> },
        { id: 'extension', label: t('settings.extension.title'), content: <BrowserExtensionSection /> },
        { id: 'retry', label: t('settings.autoRetry'), content: <Retry /> },
      ]}
    />
  );
}

function Download() {
  const { t } = useTranslation();
  const settings = useAtomValue(settingsAtom);
  const update = useUpdateSettings();

  const pickFolder = async () => {
    const selected = await open({ directory: true, multiple: false, defaultPath: settings.defaultDownloadDir });
    if (typeof selected === 'string') update({ defaultDownloadDir: selected });
  };

  return (
    <SettingsGroup>
      <SettingRow
        label={t('settings.downloadPath')}
        description={
          <Box component="span" sx={{ fontFamily: 'monospace', wordBreak: 'break-all', userSelect: 'text' }}>
            {settings.defaultDownloadDir}
          </Box>
        }
        control={
          <Button variant="outlined" onClick={pickFolder}>
            {t('settings.selectPath')}
          </Button>
        }
      />
      <SettingRow
        label={t('settings.maxConcurrent')}
        control={
          <NumberField
            value={settings.maxConcurrentDownloads}
            min={1}
            allowEmpty
            placeholder={t('settings.unlimited')}
            onCommit={(maxConcurrentDownloads) => update({ maxConcurrentDownloads })}
          />
        }
      />
      <SettingRow
        label={t('settings.maxThreads')}
        control={<NumberField value={settings.threadCount} min={1} max={32} onCommit={(n) => update({ threadCount: n ?? 8 })} />}
      />
      <SettingRow
        label={t('settings.speedLimit')}
        control={
          <>
            {settings.speedLimitMode === 'fixed' && (
              <NumberField
                value={settings.speedLimitKbps}
                min={1}
                unit="KB/s"
                width={140}
                onCommit={(speedLimitKbps) => update({ speedLimitKbps })}
              />
            )}
            <ChoiceField
              value={settings.speedLimitMode}
              minWidth={120}
              onChange={(speedLimitMode) => update({ speedLimitMode })}
              options={[
                { value: 'unlimited', label: t('settings.unlimited') },
                { value: 'fixed', label: t('settings.fixed') },
              ]}
            />
          </>
        }
      />
    </SettingsGroup>
  );
}

function Links() {
  const { t } = useTranslation();
  const settings = useAtomValue(settingsAtom);
  const update = useUpdateSettings();
  const clipboard: Clipboard = !settings.enableClipboardMonitoring ? 'off' : settings.autoStartCopied ? 'start' : 'ask';
  const actions = [
    { value: 'ask' as const, label: t('settings.links.ask') },
    { value: 'start' as const, label: t('settings.links.start') },
  ];

  return (
    <>
      <SettingsGroup>
        <SettingRow
          label={t('settings.links.copied')}
          control={
            <ChoiceField<Clipboard>
              value={clipboard}
              onChange={(next) => update({ enableClipboardMonitoring: next !== 'off', autoStartCopied: next === 'start' })}
              options={[{ value: 'off', label: t('settings.links.ignore') }, ...actions]}
            />
          }
        />
        <SettingRow
          label={t('settings.links.browser')}
          control={
            <ChoiceField<LinkAction>
              value={settings.autoStartBrowser ? 'start' : 'ask'}
              onChange={(next) => update({ autoStartBrowser: next === 'start' })}
              options={actions}
            />
          }
        />
      </SettingsGroup>
      <Typography variant="caption" color="text.secondary">
        {t('settings.links.help')}
      </Typography>
    </>
  );
}

function Retry() {
  const { t } = useTranslation();
  const settings = useAtomValue(settingsAtom);
  const update = useUpdateSettings();
  const off = !settings.autoRetryEnabled;

  return (
    <SettingsGroup>
      <SettingRow
        label={t('settings.autoRetry')}
        control={<Switch checked={settings.autoRetryEnabled} onChange={(e) => update({ autoRetryEnabled: e.target.checked })} />}
      />
      <SettingRow
        label={t('settings.maxRetryCount')}
        disabled={off}
        control={<NumberField value={settings.maxRetryCount} min={1} max={10} disabled={off} onCommit={(n) => update({ maxRetryCount: n ?? 3 })} />}
      />
      <SettingRow
        label={t('settings.retryInterval')}
        disabled={off}
        control={
          <NumberField
            value={settings.retryIntervalSecs}
            min={1}
            max={60}
            unit={t('settings.seconds')}
            width={140}
            disabled={off}
            onCommit={(n) => update({ retryIntervalSecs: n ?? 5 })}
          />
        }
      />
    </SettingsGroup>
  );
}
