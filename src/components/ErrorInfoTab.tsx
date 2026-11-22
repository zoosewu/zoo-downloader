// Error info tab component - displays error information for failed downloads
import React from 'react';
import { Box, Typography, Alert, Stack } from '@mui/material';
import { useTranslation } from 'react-i18next';
import type { Task } from '../backend';
import { errorText } from '../utils/errors';
import { formatAbsoluteTime } from '../utils/formatters';

interface ErrorInfoTabProps {
  task: Task;
}

const ErrorInfoTab: React.FC<ErrorInfoTabProps> = ({ task }) => {
  const { t } = useTranslation();

  if (!task.error) {
    return (
      <Box sx={{ padding: 1.5, display: 'flex', justifyContent: 'center', alignItems: 'center', minHeight: 150 }}>
        <Alert severity="success" variant="outlined">
          <Typography variant="body2">{t('downloads.details.noError')}</Typography>
        </Alert>
      </Box>
    );
  }

  // The site said to wait (a free download limit): not a failure of the download.
  const limited = task.error.kind === 'siteLimit';
  return (
    <Box sx={{ padding: 1.5 }}>
      <Stack spacing={2}>
        <Alert severity={limited ? 'warning' : 'error'} variant="outlined">
          <Typography variant="subtitle2" gutterBottom>
            {limited ? t('downloads.details.siteLimit') : t(`downloads.state.${task.state}`)}
          </Typography>
          <Typography variant="body2" sx={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
            {errorText(task.error, t)}
          </Typography>
        </Alert>

        {task.retryCount > 0 && (
          <Box>
            <Typography variant="subtitle2" color="text.secondary" gutterBottom>
              {t('downloads.details.retryCount')}
            </Typography>
            <Typography variant="body2">{task.retryCount}</Typography>
          </Box>
        )}

        {task.nextRetryAt && (
          <Box>
            <Typography variant="subtitle2" color="text.secondary" gutterBottom>
              {t(limited ? 'downloads.details.nextTryAt' : 'downloads.details.nextRetryAt')}
            </Typography>
            <Typography variant="body2">{formatAbsoluteTime(task.nextRetryAt)}</Typography>
          </Box>
        )}
      </Stack>
    </Box>
  );
};

export default ErrorInfoTab;
