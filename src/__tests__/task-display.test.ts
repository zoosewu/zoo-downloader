import { describe, expect, test } from 'vitest';
import type { Task } from '../backend/bindings/Task';
import type { TFunction } from 'i18next';
import { errorText } from '../utils/errors';
import { displayName, droppedBefore, statusLabel } from '../utils/downloadCalculations';

// Echoes the key and its options, so tests see what would be translated.
const t = ((key: string, options?: Record<string, unknown>) => (options ? `${key} ${JSON.stringify(options)}` : key)) as unknown as TFunction;

function task(overrides: Partial<Task>): Task {
  return {
    id: '00000000-0000-0000-0000-000000000000',
    url: 'https://share.example/f/abc',
    name: 'f.bin',
    customName: false,
    dir: '/tmp',
    headers: [],
    finalPath: null,
    state: 'running',
    phase: null,
    error: null,
    downloaded: 0,
    total: null,
    speed: 0,
    createdAt: '2026-11-04T00:00:00Z',
    startedAt: null,
    finishedAt: null,
    scheduledAt: null,
    retryCount: 0,
    nextRetryAt: null,
    rev: 1,
    order: 1,
    tags: [],
    customTags: false,
    module: null,
    ...overrides,
  };
}

describe('task name', () => {
  test('a finished file renamed on conflict shows its name on disk', () => {
    expect(displayName(task({ name: 'a.bin', finalPath: 'C:\\dl\\a (1).bin' }))).toBe('a (1).bin');
    expect(displayName(task({ name: 'a.bin', finalPath: '/dl/a (1).bin' }))).toBe('a (1).bin');
    expect(displayName(task({ name: 'a.bin' }))).toBe('a.bin');
  });
});

describe('drag to reorder', () => {
  test('a dropped row takes the place of the row it lands on', () => {
    const ids = ['a', 'b', 'c', 'd'];
    expect(droppedBefore(ids, 'd', 'b')).toBe('b'); // up: a d b c
    expect(droppedBefore(ids, 'a', 'c')).toBe('d'); // down: b c a d
    expect(droppedBefore(ids, 'a', 'd')).toBeNull(); // to the end
    expect(droppedBefore(ids, 'c', 'a')).toBe('a'); // to the front
  });

  test('the table shows the queue reversed; a drop there is worked out in queue order', () => {
    const queue = ['a', 'b', 'c', 'd']; // shown as d c b a
    // Shown d c b a, drag a onto c: d a c b on screen, so the queue is b c a d.
    expect(droppedBefore([...queue], 'a', 'c')).toBe('d');
    // Shown d c b a, drag d onto b: c b d a on screen, so the queue is a d b c.
    expect(droppedBefore([...queue], 'd', 'b')).toBe('b');
  });
});

describe('task status', () => {
  test('a running task shows its phase until it transfers', () => {
    expect(statusLabel(task({ phase: { kind: 'resolving', module: 'k2s@official' } }), t)).toBe('downloads.phase.resolving');
    expect(statusLabel(task({ phase: { kind: 'awaitingUser' } }), t)).toBe('downloads.phase.awaitingUser');
    expect(statusLabel(task({ phase: { kind: 'transferring' } }), t)).toBe('downloads.state.running');
    expect(statusLabel(task({ phase: null }), t)).toBe('downloads.state.running');
  });

  test('waiting tasks count down to when they move on by themselves', () => {
    const now = Date.parse('2026-11-04T00:00:00Z');
    expect(statusLabel(task({ phase: { kind: 'waiting', until: '2026-11-04T00:01:05Z', reason: null } }), t, now)).toBe(
      'downloads.phase.waiting 1:05'
    );
    expect(statusLabel(task({ state: 'failed', nextRetryAt: '2026-11-04T00:00:12Z' }), t, now)).toBe('downloads.retryPending 0:12');
    expect(statusLabel(task({ state: 'failed', nextRetryAt: '2026-11-04T01:02:03Z' }), t, now)).toBe('downloads.retryPending 1:02:03');
    // Due but not yet picked up.
    expect(statusLabel(task({ state: 'failed', nextRetryAt: '2026-11-03T23:59:00Z' }), t, now)).toBe('downloads.retryPending 0:00');
    expect(statusLabel(task({ state: 'failed' }), t, now)).toBe('downloads.state.failed');
    // The site said when (a free download limit): not a retry of a failure.
    const limited = task({
      state: 'failed',
      nextRetryAt: '2026-11-04T01:50:00Z',
      error: { kind: 'siteLimit', detail: { message: 'katfile: wait', hint: null, retryAfter: 6600 } },
    });
    expect(statusLabel(limited, t, now)).toBe('downloads.siteLimit 1:50:00');
  });

  test('a stale phase on a stopped task is ignored', () => {
    expect(statusLabel(task({ state: 'paused', phase: { kind: 'awaitingUser' } }), t)).toBe('downloads.state.paused');
  });

  test('module errors show their message and hint', () => {
    const failed = task({
      state: 'failed',
      error: { kind: 'resolve', detail: { message: 'k2s@official: file deleted', hint: 'ask for a new link', retryable: false } },
    });
    expect(errorText(failed.error, t)).toBe('k2s@official: file deleted (ask for a new link)');
  });

  test('core errors are translated, keeping the detail', () => {
    expect(errorText({ kind: 'http', detail: 404 }, t)).toBe('errors.core.http {"status":404}');
    expect(errorText({ kind: 'sourceChanged', detail: 'bytes 0-65535 differ' }, t)).toBe('errors.core.sourceChanged: bytes 0-65535 differ');
    expect(errorText({ kind: 'cancelled' }, t)).toBe('errors.core.cancelled');
    expect(errorText({ kind: 'insufficientSpace', detail: { needed: 3 * 1024 ** 3, available: 512 * 1024 ** 2 } }, t)).toBe(
      'errors.core.insufficientSpace {"needed":"3.00 GB","available":"512.00 MB"}'
    );
    expect(errorText(null, t)).toBe('');
  });
});
