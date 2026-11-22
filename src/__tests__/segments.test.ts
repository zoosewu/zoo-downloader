import { describe, test, expect } from 'vitest';
import { buildSegmentCells, CELL_BYTES, MAX_CELLS } from '../utils/segments';
import type { SegmentMap } from '../backend';

const map = (overrides: Partial<SegmentMap>): SegmentMap => ({
  total: 400,
  done: [],
  active: [],
  ...overrides,
});
/** Cells of 100 bytes, as if `CELL_BYTES` were 100. */
const cells100 = (m: Partial<SegmentMap>) => buildSegmentCells(map(m), MAX_CELLS, 100);

describe('buildSegmentCells', () => {
  test('100 KB cells, the last trimmed to the file size', () => {
    const cells = buildSegmentCells(map({ total: 3.5 * CELL_BYTES }));
    expect(cells.map((c) => [c.start, c.end])).toEqual([
      [0, CELL_BYTES],
      [CELL_BYTES, 2 * CELL_BYTES],
      [2 * CELL_BYTES, 3 * CELL_BYTES],
      [3 * CELL_BYTES, 3.5 * CELL_BYTES],
    ]);
  });

  test('a connection writes in one cell; the rest of its claim is claimed', () => {
    const cells = cells100({
      total: 700,
      done: [[0, 150], [500, 550]],
      active: [
        { start: 150, end: 400, retrying: false },
        { start: 400, end: 500, retrying: true },
      ],
    });
    expect(cells.map((c) => c.status)).toEqual([
      'completed',
      'downloading', // it writes at 150
      'claimed',
      'claimed',
      'retrying', // the other waits to write at 400
      'partial', // stopped there, nobody has it now
      'pending',
    ]);
    expect(cells[1].done).toBeCloseTo(0.5);
  });

  test('done ranges spanning cells, out of order, counted once each', () => {
    const cells = cells100({ total: 1000, done: [[650, 1000], [50, 420]] });
    expect(cells.map((c) => c.done)).toEqual([0.5, 1, 1, 1, 0.2, 0, 0.5, 1, 1, 1]);
  });

  test('a fully downloaded cell is completed even if a connection still lists it', () => {
    const cells = cells100({ done: [[0, 100]], active: [{ start: 0, end: 100, retrying: false }] });
    expect(cells[0].status).toBe('completed');
  });

  test('large files merge cells so the grid stays at 3000', () => {
    const cells = buildSegmentCells(map({ total: 50 * 1024 ** 3 }));
    expect(cells).toHaveLength(MAX_CELLS);
    expect(cells[MAX_CELLS - 1].end).toBe(50 * 1024 ** 3);
  });

  test('thousands of done ranges stay fast', () => {
    const total = 10 * 1024 ** 3;
    const done: [number, number][] = Array.from({ length: 5000 }, (_, i) => [i * 2e6, i * 2e6 + 1e6]);
    const t0 = performance.now();
    for (let i = 0; i < 20; i++) buildSegmentCells(map({ total, done }));
    expect((performance.now() - t0) / 20).toBeLessThan(20);
  });

  test('unknown size yields no cells', () => {
    expect(buildSegmentCells(map({ total: null }))).toEqual([]);
  });
});
