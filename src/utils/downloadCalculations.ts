// Download calculations utility - Time estimation and data formatting
import type { Task } from '../backend';
import { formatDuration } from './formatters';

/** Percent complete (0-100), or null when the size is unknown. */
export function progressPercent(task: Task): number | null {
  if (task.state === 'completed') return 100;
  if (!task.total) return null;
  return Math.min(100, (task.downloaded / task.total) * 100);
}

/** Seconds left at the current speed, or null when it cannot be estimated. */
export function calculateRemainingTime(task: Task): number | null {
  if (task.state !== 'running' || task.speed <= 0 || task.total === null) return null;
  const seconds = (task.total - task.downloaded) / task.speed;
  return isFinite(seconds) && seconds >= 0 ? seconds : null;
}

/** e.g. "2m 30s" / "2分鐘 30秒" (the app's language); "--" when unknown. */
export const formatRemainingTime = (seconds: number | null): string => formatDuration(seconds);

/** e.g. "1.50 GB", "230.00 MB". */
export function formatBytes(bytes: number): string {
  if (bytes <= 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.min(sizes.length - 1, Math.floor(Math.log(bytes) / Math.log(k)));
  return `${(bytes / Math.pow(k, i)).toFixed(2)} ${sizes[i]}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

export interface TableDisplayData {
  id: string;
  filename: string;
  filesizeFormatted: string;
  /** 0-100; 0 when the size is unknown. */
  progress: number;
  progressFormatted: string;
  speedFormatted: string;
  statusLabel: string;
  remainingTimeFormatted: string;
}

/** The name on disk: the finished file may have been renamed to avoid a conflict (`a (1).bin`). */
export function displayName(task: Task): string {
  return task.finalPath?.split(/[\\/]/).pop() || task.name;
}

export function formatTaskForTable(task: Task, t: (key: string) => string, now = Date.now()): TableDisplayData {
  const percent = progressPercent(task);
  return {
    id: task.id,
    filename: displayName(task),
    filesizeFormatted: task.total !== null ? formatBytes(task.total) : '--',
    progress: percent ?? 0,
    progressFormatted: percent !== null ? `${percent.toFixed(1)}%` : formatBytes(task.downloaded),
    speedFormatted: task.state === 'running' && task.speed > 0 ? formatSpeed(task.speed) : '--',
    statusLabel: statusLabel(task, t, now),
    remainingTimeFormatted: formatRemainingTime(calculateRemainingTime(task)),
  };
}

/**
 * A row dragged onto another takes its place: the task it ends up in front of, or null for
 * the end of the list.
 */
export function droppedBefore(ids: string[], dragged: string, target: string): string | null {
  const [from, to] = [ids.indexOf(dragged), ids.indexOf(target)];
  return to < from ? target : (ids[to + 1] ?? null);
}

/** When a waiting task moves on by itself: a module's countdown or the next automatic retry. */
export function deadline(task: Task): string | null {
  if (task.state === 'running' && task.phase?.kind === 'waiting') return task.phase.until;
  return task.state === 'failed' ? task.nextRetryAt : null;
}

/** "0:05", "12:34", "1:02:03". */
function formatCountdown(seconds: number): string {
  const s = Math.max(0, Math.ceil(seconds));
  const pad = (n: number) => String(n).padStart(2, '0');
  const [h, m] = [Math.floor(s / 3600), Math.floor((s % 3600) / 60)];
  return h > 0 ? `${h}:${pad(m)}:${pad(s % 60)}` : `${m}:${pad(s % 60)}`;
}

/** A running task shows what it is doing while it is not yet transferring; waits show a countdown. */
export function statusLabel(task: Task, t: (key: string) => string, now = Date.now()): string {
  const phase = task.state === 'running' ? task.phase : null;
  const until = deadline(task);
  const label =
    task.state === 'failed' && until
      ? t(task.error?.kind === 'siteLimit' ? 'downloads.siteLimit' : 'downloads.retryPending')
      : phase && phase.kind !== 'transferring'
        ? t(`downloads.phase.${phase.kind}`)
        : t(`downloads.state.${task.state}`);
  return until ? `${label} ${formatCountdown((Date.parse(until) - now) / 1000)}` : label;
}
