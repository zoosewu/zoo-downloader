// The main screen's new-version strip, under the toolbar (zoo-release docs/UI.md §1): a new
// major or minor version that was not skipped, then its download, "restart now", or why it failed.
// Patch versions are only on the About page, which shows the same states (`useUpdateContent`).
import React from 'react';
import { Alert, Box, Button, IconButton, LinearProgress, Tooltip } from '@mui/material';
import { Close as CloseIcon } from '@mui/icons-material';
import { useTranslation } from 'react-i18next';
import { useAtomValue } from 'jotai';
import { appVersionAtom, bannerClosedAtom, updateStateAtom, useAppUpdate, type UpdateState } from '../hooks/useAppUpdate';
import { settingsAtom } from '../stores/settingsStore';
import { REPO_URL, failureReason, offerOnMainScreen } from '../utils/appUpdate';

/** The message and buttons for an update on its way; null for the states only the About page has. */
export function useUpdateContent(state: UpdateState, { withSkip }: { withSkip: boolean }) {
  const { t } = useTranslation();
  const { install, cancel, restart, skip } = useAppUpdate();
  if (!('version' in state)) return null;

  const version = state.version ? `v${state.version}` : '';
  const page = version ? `${REPO_URL}/releases/tag/${version}` : `${REPO_URL}/releases/latest`;
  const link = (label: string) => (
    <Button variant="outlined" color="inherit" size="small" href={page} target="_blank" rel="noreferrer">
      {label}
    </Button>
  );

  switch (state.kind) {
    case 'available':
      return {
        message: t('update.available', { version }),
        actions: (
          <>
            {link(t('update.notes'))}
            {withSkip && (
              <Button variant="outlined" color="inherit" size="small" onClick={() => skip(state.version)}>
                {t('update.skip')}
              </Button>
            )}
            <Button variant="contained" size="small" onClick={install}>
              {t('update.install')}
            </Button>
          </>
        ),
      };
    case 'downloading': {
      const percent = state.total ? Math.floor((state.done / state.total) * 100) : null;
      return {
        message: (
          <Box component="span" sx={{ display: 'inline-flex', alignItems: 'center', gap: 1.5 }}>
            {t('update.downloading', { version })}
            <LinearProgress variant={percent === null ? 'indeterminate' : 'determinate'} value={percent ?? 0} sx={{ width: 160 }} />
            {percent !== null && `${percent}%`}
          </Box>
        ),
        actions: (
          <Button variant="outlined" color="inherit" size="small" onClick={cancel}>
            {t('update.cancel')}
          </Button>
        ),
      };
    }
    case 'ready':
    case 'restarting':
      return {
        message: t('update.ready', { version }),
        actions: (
          <Button variant="contained" size="small" disabled={state.kind === 'restarting'} onClick={restart}>
            {t('update.restart')}
          </Button>
        ),
      };
    case 'failed': {
      const reason = failureReason(state.message);
      return {
        message: t('update.failed', { reason: reason ? t(`update.reason.${reason}`) : state.message }),
        actions: link(t('update.manual')),
      };
    }
  }
}

const UpdateBanner: React.FC = () => {
  const { t } = useTranslation();
  const state = useAtomValue(updateStateAtom);
  const current = useAtomValue(appVersionAtom);
  const closed = useAtomValue(bannerClosedAtom);
  const skipped = useAtomValue(settingsAtom).updateSkipped;
  const { later } = useAppUpdate();
  const content = useUpdateContent(state, { withSkip: true });

  const shown =
    state.kind === 'available'
      ? !closed && !!current && offerOnMainScreen(current, state.version, skipped)
      : state.kind === 'failed'
        ? !closed
        : state.kind === 'downloading' || state.kind === 'ready' || state.kind === 'restarting';
  if (!shown || !content) return null;

  const closable = state.kind === 'available' || state.kind === 'failed';
  return (
    <Alert
      severity={state.kind === 'failed' ? 'error' : 'info'}
      icon={false}
      role="status"
      action={
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
          {content.actions}
          {closable && (
            <Tooltip title={t('update.later')}>
              <IconButton color="inherit" size="small" aria-label={t('update.later')} onClick={later}>
                <CloseIcon fontSize="small" />
              </IconButton>
            </Tooltip>
          )}
        </Box>
      }
      sx={{
        borderRadius: 0,
        borderBottom: 1,
        borderColor: 'divider',
        py: 0,
        alignItems: 'center',
        '& .MuiAlert-message': { flex: 1, minWidth: 0 },
        '& .MuiAlert-action': { pt: 0, alignItems: 'center' },
      }}
    >
      {content.message}
    </Alert>
  );
};

export default UpdateBanner;
