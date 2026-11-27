/** The main screen's update strip follows zoo-release's UI.md: when it shows, and its wording. */
import { beforeAll, describe, expect, test, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { Provider, createStore } from 'jotai';
import UpdateBanner from '../components/UpdateBanner';
import { appVersionAtom, updateStateAtom, type UpdateState } from '../hooks/useAppUpdate';
import { settingsAtom } from '../stores/settingsStore';
import i18n from '../i18n';

vi.mock('@tauri-apps/plugin-updater', () => ({ check: vi.fn() }));
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: vi.fn() }));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(async () => '0.6.4') }));

function show(state: UpdateState, skipped: string | null = null) {
  const store = createStore();
  store.set(appVersionAtom, '0.6.4');
  store.set(settingsAtom, { ...store.get(settingsAtom), updateSkipped: skipped });
  store.set(updateStateAtom, state);
  render(
    <Provider store={store}>
      <UpdateBanner />
    </Provider>,
  );
}

beforeAll(() => i18n.changeLanguage('zh-TW'));

describe('update banner', () => {
  test('a new minor version: notes, skip, update, later', () => {
    show({ kind: 'available', version: '0.7.0' });
    expect(screen.getByText('有新版本 v0.7.0')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '版本說明' })).toHaveAttribute('href', 'https://github.com/zoosewu/zoo-downloader/releases/tag/v0.7.0');
    expect(screen.getByRole('button', { name: '略過此版本' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '更新' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '稍後' }));
    expect(screen.queryByText('有新版本 v0.7.0')).not.toBeInTheDocument();
  });

  test('a patch version is not shown on the main screen', () => {
    show({ kind: 'available', version: '0.6.5' });
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  test('a skipped major.minor is not shown', () => {
    show({ kind: 'available', version: '0.7.2' }, '0.7');
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  test('downloading shows progress and cancel', () => {
    show({ kind: 'downloading', version: '0.7.0', done: 68, total: 100 });
    expect(screen.getByText(/正在下載 v0.7.0/)).toHaveTextContent('68%');
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '68');
    expect(screen.getByRole('button', { name: '取消' })).toBeInTheDocument();
  });

  test('downloaded: restart now', () => {
    show({ kind: 'ready', version: '0.7.0' });
    expect(screen.getByText('v0.7.0 已下載，重新啟動後完成更新')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '立即重新啟動' })).toBeInTheDocument();
  });

  test('a failure names the reason and links the release', () => {
    show({ kind: 'failed', version: '0.7.0', message: 'signature verification failed' });
    expect(screen.getByText('更新失敗：簽章驗證失敗')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '手動下載' })).toHaveAttribute('href', 'https://github.com/zoosewu/zoo-downloader/releases/tag/v0.7.0');
  });
});
