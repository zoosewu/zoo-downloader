// Advanced info tab: retries, schedule and the task id.
import React from 'react';
import { Box } from '@mui/material';
import { useTranslation } from 'react-i18next';
import type { Task } from '../backend';
import { formatAbsoluteTime } from '../utils/formatters';
import PropertyList from './PropertyList';

const AdvancedInfoTab: React.FC<{ task: Task }> = ({ task }) => {
  const { t } = useTranslation();

  return (
    <Box sx={{ p: 1.5 }}>
      <PropertyList
        items={[
          { label: t('downloads.details.retryCount'), value: task.retryCount },
          task.nextRetryAt && {
            label: t(task.error?.kind === 'siteLimit' ? 'downloads.details.nextTryAt' : 'downloads.details.nextRetryAt'),
            value: formatAbsoluteTime(task.nextRetryAt),
          },
          {
            label: t('downloads.details.scheduledStart'),
            value: task.scheduledAt ? formatAbsoluteTime(task.scheduledAt) : t('downloads.details.notScheduled'),
          },
          { label: 'Task ID', value: task.id, mono: true },
        ]}
      />
    </Box>
  );
};

export default AdvancedInfoTab;
