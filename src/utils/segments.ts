// Turns a backend SegmentMap into fixed-size cells for the segment grid.
import type { SegmentMap } from '../backend';

/**
 * `downloading` / `retrying`: a connection writes in this cell now (one cell per connection).
 * `claimed`: a connection will get to it; `pending`: no connection has it yet.
 */
export type CellStatus = 'pending' | 'claimed' | 'partial' | 'downloading' | 'retrying' | 'completed';

export interface SegmentCell {
  index: number;
  /** Byte range `[start, end)`. */
  start: number;
  end: number;
  status: CellStatus;
  /** Share of the cell's bytes already downloaded, 0-1. */
  done: number;
}

/** Bytes per cell, merged into larger cells once there would be more than `MAX_CELLS`. */
export const CELL_BYTES = 100 * 1024;
export const MAX_CELLS = 3000;

function overlap(aStart: number, aEnd: number, bStart: number, bEnd: number): number {
  return Math.max(0, Math.min(aEnd, bEnd) - Math.max(aStart, bStart));
}

/**
 * Empty when the size is unknown (nothing to divide). One pass over the cells and the done
 * ranges together, so thousands of each stay cheap.
 */
export function buildSegmentCells(map: SegmentMap, maxCells = MAX_CELLS, cellBytes = CELL_BYTES): SegmentCell[] {
  const total = map.total;
  if (!total || total <= 0) return [];
  const cellSize = Math.max(cellBytes, Math.ceil(total / maxCells), 1);
  const count = Math.ceil(total / cellSize);
  const done = [...map.done].sort((a, b) => a[0] - b[0]);
  let first = 0; // the first done range that may still reach this cell

  return Array.from({ length: count }, (_, index) => {
    const start = index * cellSize;
    const end = Math.min(total, start + cellSize);
    while (first < done.length && done[first][1] <= start) first++;
    let covered = 0;
    for (let i = first; i < done.length && done[i][0] < end; i++) covered += overlap(start, end, done[i][0], done[i][1]);
    // A connection's range runs from where it writes next (`start`) to the end of its claim.
    const writing = map.active.filter((r) => r.start >= start && r.start < end);
    const share = covered / (end - start);

    let status: CellStatus;
    if (share >= 1) status = 'completed';
    else if (writing.some((r) => r.retrying)) status = 'retrying';
    else if (writing.length > 0) status = 'downloading';
    else if (covered > 0) status = 'partial';
    else if (map.active.some((r) => overlap(start, end, r.start, r.end) > 0)) status = 'claimed';
    else status = 'pending';

    return { index, start, end, status, done: share };
  });
}
