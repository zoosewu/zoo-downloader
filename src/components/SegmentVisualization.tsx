// Segment grid: which parts of the file are done, being fetched, or waiting to retry.
import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Box, Typography, Stack, useTheme, alpha } from '@mui/material';
import SegmentCanvas from './SegmentCanvas';
import { useTranslation } from 'react-i18next';
import { backend, type SegmentMap, type Task } from '../backend';
import { buildSegmentCells, type CellStatus } from '../utils/segments';
import { formatBytes } from '../utils/downloadCalculations';

const MIN_GAP_MS = 500;

/** Block colors from the theme palette, the same ones the list's states use. */
function useCellColors(): Record<CellStatus, string> {
  const { palette } = useTheme();
  return useMemo(
    () => ({
      pending: alpha(palette.text.primary, 0.07),
      claimed: alpha(palette.text.primary, 0.22),
      partial: alpha(palette.success.main, 0.45),
      downloading: palette.info.main,
      retrying: palette.error.main,
      completed: palette.success.main,
    }),
    [palette],
  );
}

const LEGEND: CellStatus[] = ['completed', 'partial', 'downloading', 'retrying', 'claimed', 'pending'];

interface SegmentVisualizationProps {
  task: Task;
}

const SegmentVisualization: React.FC<SegmentVisualizationProps> = ({ task }) => {
  const { t } = useTranslation();
  const colors = useCellColors();
  const [map, setMap] = useState<SegmentMap | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Event-driven: refetch when the task changes (its progress events included), at most every
  // MIN_GAP_MS. A stalled or finished download sends no events, so nothing is fetched.
  const idRef = useRef(task.id);
  idRef.current = task.id;
  const scheduled = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const lastFetch = useRef(0);
  useEffect(
    () => () => {
      clearTimeout(scheduled.current);
      scheduled.current = undefined; // StrictMode remounts: the next mount must schedule again
    },
    [],
  );
  useEffect(() => {
    // Throttle, not debounce: a fetch already scheduled runs with the latest id; rescheduling
    // on every tick would postpone it forever while progress keeps coming.
    if (scheduled.current) return;
    const wait = Math.max(0, lastFetch.current + MIN_GAP_MS - Date.now());
    scheduled.current = setTimeout(async () => {
      scheduled.current = undefined;
      lastFetch.current = Date.now();
      const id = idRef.current;
      try {
        const next = await backend.downloads.segments(id);
        if (id !== idRef.current) return; // another task was selected meanwhile
        setMap(next);
        setError(null);
      } catch (e) {
        if (id === idRef.current) setError(e instanceof Error ? e.message : String(e));
      }
    }, wait);
  }, [task.id, task.state, task.rev, task.downloaded]);

  const cells = useMemo(() => (map ? buildSegmentCells(map) : []), [map]);

  if (error) {
    return (
      <Typography sx={{ p: 1.5 }} color="error">
        {error}
      </Typography>
    );
  }
  if (!map) return null;
  if (cells.length === 0) {
    return (
      <Typography sx={{ p: 1.5 }} color="text.secondary">
        {t('downloads.segments.unknownSize')}
      </Typography>
    );
  }

  return (
    <Box sx={{ p: 1.5 }}>
      <Stack direction="row" spacing={2} sx={{ mb: 1.5, flexWrap: 'wrap' }}>
        {LEGEND.map((status) => (
          <Stack key={status} direction="row" spacing={0.5} sx={{ alignItems: 'center' }}>
            <Box sx={{ width: 12, height: 12, borderRadius: '2px', backgroundColor: colors[status] }} />
            <Typography variant="caption">{t(`downloads.segments.${status}`)}</Typography>
          </Stack>
        ))}
      </Stack>

      <SegmentCanvas
        cells={cells}
        colors={colors}
        describe={(cell) => `#${cell.index + 1} · ${formatBytes(cell.end - cell.start)}`}
      />
    </Box>
  );
};

export default SegmentVisualization;
