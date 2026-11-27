// Download list component - displays all download tasks in table view with resizable detail panel
import React, { useState, useRef, useEffect } from 'react';
import { Box, Divider, Button, IconButton, Tooltip } from '@mui/material';
import AddIcon from '@mui/icons-material/Add';
import SettingsIcon from '@mui/icons-material/SettingsOutlined';
import { useTranslation } from 'react-i18next';
import { useNavigation } from '../hooks/useNavigation';
import DownloadTable from './DownloadTable';
import DownloadDetailPanel from './DownloadDetailPanel';
import QueueControls from './QueueControls';
import Sidebar from './Sidebar';
import ImportListButton from './ImportListButton';
import { useSelectedDownload, useSelectedTask } from '../hooks/useSelectedDownload';
import { backend } from '../backend';
import UpdateBanner from './UpdateBanner';

/** The panel is the only part of the list that follows the selected task's progress. */
const SelectedTaskPanel: React.FC = () => <DownloadDetailPanel task={useSelectedTask()} />;

interface DownloadListProps {
  className?: string;
  onControlError: (error: string) => void;
}

const MIN_PANEL_HEIGHT = 200; // Minimum height for detail panel
const DEFAULT_PANEL_HEIGHT = 300; // Default height for detail panel

const DownloadList: React.FC<DownloadListProps> = ({
  className,
  onControlError,
}) => {
  const { t } = useTranslation();
  const { navigateTo } = useNavigation();
  const { selectedTaskId } = useSelectedDownload();
  const [panelHeight, setPanelHeight] = useState(DEFAULT_PANEL_HEIGHT);
  const [isDragging, setIsDragging] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Handle resize drag
  const handleMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    setIsDragging(true);
  };

  // Handle mouse move during drag
  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isDragging || !containerRef.current) return;

      const containerRect = containerRef.current.getBoundingClientRect();
      const newHeight = containerRect.bottom - e.clientY;

      // Constrain panel height
      const maxHeight = containerRect.height - 200; // Leave at least 200px for table
      const constrainedHeight = Math.max(MIN_PANEL_HEIGHT, Math.min(newHeight, maxHeight));

      setPanelHeight(constrainedHeight);
    };

    const handleMouseUp = () => {
      setIsDragging(false);
    };

    if (isDragging) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
    }

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isDragging]);

  return (
    <Box className={className} sx={{ width: '100%', height: '100%', display: 'flex', flexDirection: 'column' }}>
      {/* Toolbar */}
      <Box
        sx={{
          display: 'flex',
          alignItems: 'center',
          gap: 0.5,
          px: 1,
          minHeight: 40,
          borderBottom: 1,
          borderColor: 'divider',
          backgroundColor: 'background.paper',
        }}
      >
        <Button
          variant="contained"
          startIcon={<AddIcon />}
          onClick={() => backend.addWindow.open().catch((e) => onControlError(e.message))}
        >
          {t('downloads.addDownload')}
        </Button>
        <ImportListButton onError={onControlError} />
        <Divider orientation="vertical" flexItem sx={{ mx: 0.5, my: 1 }} />
        <QueueControls onControlError={onControlError} />
        <Box sx={{ flex: 1 }} />
        <Tooltip title={t('settings.title')}>
          <IconButton onClick={() => navigateTo('settings')}>
            <SettingsIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      </Box>
      <UpdateBanner />

      <Box sx={{ flex: 1, display: 'flex', minHeight: 0 }}>
      <Sidebar />
      <Box ref={containerRef} sx={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', position: 'relative' }}>
      <Box sx={{ flex: 1, overflow: 'hidden', minHeight: 120 }}>
        <DownloadTable onActionError={onControlError} />
      </Box>

      {/* Resize Handle and Detail Panel - fixed at bottom */}
      {selectedTaskId && (
        <>
          <Divider
            onMouseDown={handleMouseDown}
            sx={{
              height: 4,
              cursor: 'ns-resize',
              border: 0,
              backgroundColor: isDragging ? 'primary.main' : 'divider',
              '&:hover': { backgroundColor: 'primary.main' },
              transition: 'background-color 0.15s',
            }}
          />

          {/* Download Detail Panel - fixed height */}
          <Box
            sx={{
              height: `${panelHeight}px`,
              overflow: 'hidden',
              backgroundColor: 'background.paper',
            }}
          >
            <SelectedTaskPanel />
          </Box>
        </>
      )}
      </Box>
      </Box>
    </Box>
  );
};

export default DownloadList;
