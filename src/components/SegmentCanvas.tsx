// The segment grid, drawn on a canvas in device pixels. A CSS grid of small cells with gaps
// lands on half pixels at 125% display scaling, so the gaps alternate in width; here every cell
// and gap is a whole number of device pixels. One tooltip follows the pointer.
//
// Up to 3000 cells: only the cells whose status changed since the last frame are painted (in
// a download, the few under its connections); size, colors or cell count changing repaint all.
import React, { useEffect, useRef, useState } from 'react';
import { Box, Tooltip } from '@mui/material';
import type { SegmentCell, CellStatus } from '../utils/segments';

const CELL = 8; // CSS px
const GAP = 2;

interface Props {
  cells: SegmentCell[];
  colors: Record<CellStatus, string>;
  describe: (cell: SegmentCell) => string;
}

const SegmentCanvas: React.FC<Props> = ({ cells, colors, describe }) => {
  const box = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  /** What is on the canvas: the layout it was painted for and each cell's status. */
  const painted = useRef<{ layout: string; colors: Props['colors']; statuses: CellStatus[] } | null>(null);
  const [width, setWidth] = useState(0);
  const [hover, setHover] = useState<SegmentCell | null>(null);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const dpr = window.devicePixelRatio || 1;
  const cell = Math.round(CELL * dpr);
  const gap = Math.max(1, Math.round(GAP * dpr));
  const pitch = cell + gap;
  const columns = Math.max(1, Math.floor((Math.floor(width * dpr) + gap) / pitch));
  const rows = Math.ceil(cells.length / columns);

  useEffect(() => {
    const c = canvas.current;
    if (!c || width === 0) return;
    const ctx = c.getContext('2d');
    if (!ctx) return;
    const layout = `${columns}x${rows}/${cells.length}@${pitch}`;
    const last = painted.current;
    const all = !last || last.layout !== layout || last.colors !== colors;
    if (all) {
      // Resizing the canvas clears it.
      c.width = columns * pitch - gap;
      c.height = Math.max(0, rows * pitch - gap);
    }
    cells.forEach((s, i) => {
      if (!all && last.statuses[i] === s.status) return;
      ctx.fillStyle = colors[s.status];
      ctx.fillRect((i % columns) * pitch, Math.floor(i / columns) * pitch, cell, cell);
    });
    painted.current = { layout, colors, statuses: cells.map((s) => s.status) };
  }, [cells, colors, width, columns, rows, pitch, cell, gap]);

  const at = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const x = Math.floor(((e.clientX - rect.left) * dpr) / pitch);
    const y = Math.floor(((e.clientY - rect.top) * dpr) / pitch);
    const index = y * columns + x;
    return x < columns && index < cells.length ? cells[index] : null;
  };

  return (
    <Box ref={box} sx={{ maxHeight: 300, overflowY: 'auto', lineHeight: 0 }}>
      <Tooltip title={hover ? describe(hover) : ''} followCursor placement="top" enterDelay={0}>
        <canvas
          ref={canvas}
          style={{ width: (columns * pitch - gap) / dpr, height: Math.max(0, rows * pitch - gap) / dpr }}
          onMouseMove={(e) => setHover(at(e))}
          onMouseLeave={() => setHover(null)}
        />
      </Tooltip>
    </Box>
  );
};

export default SegmentCanvas;
