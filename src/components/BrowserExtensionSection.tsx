// Settings: installing the browser extension. The store (Edge Add-ons, in either browser) is the
// way; loading the folder the app ships is a guide in a dialog, each step with the button that
// does it.
import React, { useEffect, useState } from 'react';
import { Alert, Box, Button, Dialog, DialogActions, DialogContent, DialogTitle, Stack, Typography } from '@mui/material';
import { FolderOpen as FolderIcon, OpenInNew as OpenIcon, Storefront as StoreIcon } from '@mui/icons-material';
import { useTranslation } from 'react-i18next';
import { backend, type ExtensionSetup } from '../backend';
import Toast, { type ToastMessage } from './Toast';
import { SettingRow, SettingsGroup } from './settings/layout';

const BrowserExtensionSection: React.FC = () => {
  const { t } = useTranslation();
  const [setup, setSetup] = useState<ExtensionSetup | null>(null);
  const [error, setError] = useState<ToastMessage | null>(null);
  const [guide, setGuide] = useState(false);
  const fail = (e: unknown) => setError({ severity: 'error', text: e instanceof Error ? e.message : String(e) });

  useEffect(() => {
    backend.extension.setup().then(setSetup).catch(fail);
  }, []);

  const openStore = (browser?: string) => backend.extension.openStore(browser).catch(fail);

  return (
    <>
      <Toast message={error} onClose={() => setError(null)} />
      <SettingsGroup>
        <SettingRow
          label={t('settings.extension.store')}
          description={t('settings.extension.storeHelp')}
          control={
            setup?.browsers.length
              ? setup.browsers.map((b) => (
                  <Button key={b.id} variant="contained" startIcon={<StoreIcon fontSize="small" />} onClick={() => openStore(b.id)}>
                    {b.name}
                  </Button>
                ))
              : setup && (
                  <Button variant="contained" startIcon={<StoreIcon fontSize="small" />} onClick={() => openStore()}>
                    {t('settings.extension.openStore')}
                  </Button>
                )
          }
        />
        <SettingRow
          label={t('settings.extension.manual')}
          description={t('settings.extension.manualHelp')}
          control={
            <Button variant="outlined" onClick={() => setGuide(true)}>
              {t('settings.extension.howTo')}
            </Button>
          }
        />
      </SettingsGroup>
      {guide && <ManualGuide setup={setup} onClose={() => setGuide(false)} fail={fail} />}
    </>
  );
};

function ManualGuide({ setup, onClose, fail }: { setup: ExtensionSetup | null; onClose: () => void; fail: (e: unknown) => void }) {
  const { t } = useTranslation();
  /** The extensions page address just put on the clipboard. */
  const [copied, setCopied] = useState<string | null>(null);

  const step = (n: number, text: string, actions: React.ReactNode, after?: React.ReactNode) => (
    <Box sx={{ display: 'flex', gap: 1.5 }}>
      <Typography variant="body2" color="primary" sx={{ fontWeight: 600, width: 16, flexShrink: 0, pt: 0.75 }}>
        {n}
      </Typography>
      <Box sx={{ flex: 1, minWidth: 0 }}>
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, flexWrap: 'wrap', minHeight: 36 }}>
          <Typography variant="body2" sx={{ flex: 1, minWidth: 160 }}>
            {text}
          </Typography>
          {actions}
        </Box>
        {after}
      </Box>
    </Box>
  );

  return (
    <Dialog open onClose={onClose} maxWidth="sm" fullWidth>
      <DialogTitle>{t('settings.extension.manualTitle')}</DialogTitle>
      <DialogContent>
        <Stack spacing={2}>
          {setup && setup.browsers.length === 0 && <Alert severity="info">{t('settings.extension.noBrowser')}</Alert>}
          {step(
            1,
            t('settings.extension.step1'),
            setup?.browsers.map((b) => (
              <Button key={b.id} variant="outlined" startIcon={<OpenIcon fontSize="small" />} onClick={() => backend.extension.openPage(b.id).then(setCopied, fail)}>
                {b.name}
              </Button>
            )),
            // Its line is kept from the start, so step 2 does not move when the address is copied.
            <Typography variant="caption" color="info.main" component="div" sx={{ minHeight: '1.66em', visibility: copied ? 'visible' : 'hidden' }}>
              {copied && t('settings.extension.pasteAddress', { address: copied })}
            </Typography>,
          )}
          {step(2, t('settings.extension.step2'), null)}
          {step(
            3,
            t('settings.extension.step3'),
            <Button variant="outlined" startIcon={<FolderIcon fontSize="small" />} onClick={() => backend.extension.showFolder().catch(fail)}>
              {t('settings.extension.showFolder')}
            </Button>,
            setup && (
              <Typography
                variant="caption"
                color="text.secondary"
                component="div"
                sx={{ fontFamily: 'monospace', userSelect: 'text', wordBreak: 'break-all', mt: 0.5 }}
              >
                {setup.folder}
              </Typography>
            ),
          )}
          <Typography variant="caption" color="text.secondary">
            {t('settings.extension.afterUpdate')}
          </Typography>
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>{t('common.close')}</Button>
      </DialogActions>
    </Dialog>
  );
}

export default BrowserExtensionSection;
