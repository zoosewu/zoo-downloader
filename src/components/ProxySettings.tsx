// Proxy settings: the imported proxies, their health, and list actions. Colors come from the
// theme palette (success / warning / error), so the page follows light and dark mode.
import { useEffect, useState } from 'react';
import { useAtom, useAtomValue, useSetAtom } from 'jotai';
import { open } from '@tauri-apps/plugin-dialog';
import {
  Alert,
  Box,
  Button,
  Chip,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  IconButton,
  LinearProgress,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  Tooltip,
  Typography,
} from '@mui/material';
import Toast, { type ToastMessage } from './Toast';
import {
  Upload as ImportIcon,
  RestartAlt as ResetIcon,
  DeleteSweep as ClearIcon,
  DeleteOutlined as RemoveIcon,
} from '@mui/icons-material';
import {
  proxyListAtom,
  proxyStatsAtom,
  proxyLoadingAtom,
  proxyErrorAtom,
  loadProxyListAtom,
  importProxyListAtom,
  removeProxyAtom,
  clearAllProxiesAtom,
  resetFailedProxiesAtom,
} from '../stores/proxyStore';
import { backend, type ProxyStatus } from '../backend';
import { useTranslation } from 'react-i18next';
import { settingsAtom } from '../stores/settingsStore';
import { useUpdateSettings } from '../hooks/useUpdateSettings';
import { SettingRow, SettingsGroup } from './settings/layout';
import { ChoiceField } from './settings/fields';

type Mode = 'off' | 'fallback' | 'only';

/** Whether downloads go through the proxies, and what happens when none of them works. */
function ProxyMode() {
  const { t } = useTranslation();
  const settings = useAtomValue(settingsAtom);
  const update = useUpdateSettings();
  const mode: Mode = !settings.enableProxy ? 'off' : settings.proxyFallbackDirect ? 'fallback' : 'only';
  return (
    <SettingsGroup>
      <SettingRow
        label={t('proxy.mode.label')}
        control={
          <ChoiceField<Mode>
            value={mode}
            minWidth={280}
            onChange={(next) => update({ enableProxy: next !== 'off', proxyFallbackDirect: next !== 'only' })}
            options={[
              { value: 'off', label: t('proxy.mode.off') },
              { value: 'fallback', label: t('proxy.mode.fallback') },
              { value: 'only', label: t('proxy.mode.only') },
            ]}
          />
        }
      />
    </SettingsGroup>
  );
}

const STATUS_COLOR: Record<ProxyStatus, 'success' | 'warning' | 'error' | 'info' | 'default'> = {
  unknown: 'default',
  verifying: 'info',
  active: 'success',
  failed: 'warning',
  blacklisted: 'error',
};

const healthColor = (score: number) => (score >= 80 ? 'success' : score >= 60 ? 'warning' : 'error');

const formatTime = (timestamp: string | null) =>
  timestamp ? new Date(timestamp).toLocaleString('zh-TW', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : '-';

interface Confirm {
  text: string;
  action: () => Promise<unknown>;
}

export default function ProxySettings() {
  const [proxies] = useAtom(proxyListAtom);
  const stats = useAtomValue(proxyStatsAtom);
  const loading = useAtomValue(proxyLoadingAtom);
  const error = useAtomValue(proxyErrorAtom);

  const loadProxyList = useSetAtom(loadProxyListAtom);
  const importProxyList = useSetAtom(importProxyListAtom);
  const removeProxy = useSetAtom(removeProxyAtom);
  const clearAllProxies = useSetAtom(clearAllProxiesAtom);
  const resetFailedProxies = useSetAtom(resetFailedProxiesAtom);

  const [notice, setNotice] = useState<ToastMessage | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);

  // The list follows the backend while the page is open: health and counters change with
  // every request through a proxy.
  useEffect(() => {
    loadProxyList();
    let stop: (() => void) | undefined;
    let disposed = false;
    backend.events.onProxiesChanged(() => loadProxyList()).then((u) => (disposed ? u() : (stop = u)));
    return () => {
      disposed = true;
      stop?.();
    };
  }, [loadProxyList]);

  const run = async (action: () => Promise<unknown>, failure: string) => {
    try {
      await action();
    } catch (e) {
      setNotice({ severity: 'error', text: `${failure}: ${e instanceof Error ? e.message : String(e)}` });
    }
  };

  const handleImport = () =>
    run(async () => {
      const file = await open({ filters: [{ name: 'Text Files', extensions: ['txt'] }] });
      if (!file) return;
      const count = await importProxyList(file);
      setNotice({ severity: 'success', text: `成功匯入 ${count} 個 Proxy` });
    }, '匯入失敗');

  const ask = (text: string, action: () => Promise<unknown>, failure: string) =>
    setConfirm({ text, action: () => run(action, failure) });

  const statItems: Array<[string, number | string, string?]> = [
    ['總數', stats.total],
    ['活躍', stats.active, 'success.main'],
    ['失敗', stats.failed, 'warning.main'],
    ['黑名單', stats.blacklisted, 'error.main'],
    ['平均健康度', stats.avgHealth.toFixed(1)],
  ];

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', gap: 1.5 }}>
      <ProxyMode />
      <Box sx={{ display: 'flex', gap: 3, flexWrap: 'wrap' }}>
        {statItems.map(([label, value, color]) => (
          <Box key={label}>
            <Typography variant="caption" color="text.secondary" component="div">
              {label}
            </Typography>
            <Typography variant="h6" sx={{ color, fontVariantNumeric: 'tabular-nums', lineHeight: 1.3 }}>
              {value}
            </Typography>
          </Box>
        ))}
      </Box>

      <Box sx={{ display: 'flex', gap: 0.5, flexWrap: 'wrap' }}>
        <Button variant="contained" startIcon={<ImportIcon fontSize="small" />} onClick={handleImport} disabled={loading}>
          匯入 Proxy
        </Button>
        <Button
          color="inherit"
          startIcon={<ResetIcon fontSize="small" />}
          disabled={loading || stats.failed === 0}
          onClick={() => ask('確定要重置所有失敗的 Proxy？', () => resetFailedProxies(), '重置失敗')}
        >
          重置失敗
        </Button>
        <Button
          color="error"
          startIcon={<ClearIcon fontSize="small" />}
          disabled={loading || proxies.length === 0}
          onClick={() => ask(`確定要清空所有 ${proxies.length} 個 Proxy？此操作無法撤銷。`, () => clearAllProxies(), '清空失敗')}
        >
          清空全部
        </Button>
      </Box>

      {error && <Alert severity="error">{error}</Alert>}
      {/* Always there, shown only while loading: the table below never moves. */}
      <LinearProgress sx={{ visibility: loading ? 'visible' : 'hidden' }} />

      {!loading && proxies.length === 0 ? (
        <Typography variant="body2" color="text.secondary" sx={{ py: 4, textAlign: 'center' }}>
          尚無 Proxy。按「匯入 Proxy」從檔案載入。
        </Typography>
      ) : (
        <Box sx={{ border: 1, borderColor: 'divider', borderRadius: 1, overflow: 'auto' }}>
          <Table stickyHeader>
            <TableHead>
              <TableRow>
                <TableCell>地址</TableCell>
                <TableCell>狀態</TableCell>
                <TableCell sx={{ width: 140 }}>健康度</TableCell>
                <TableCell align="right">成功 / 失敗</TableCell>
                <TableCell align="right">平均延遲</TableCell>
                <TableCell>最後使用</TableCell>
                <TableCell sx={{ width: 36 }} />
              </TableRow>
            </TableHead>
            <TableBody>
              {proxies.map((proxy) => {
                const health = proxy.healthScore * 100;
                return (
                  <TableRow key={proxy.id} hover>
                    <TableCell sx={{ fontFamily: 'Consolas, monospace', fontSize: '0.75rem' }}>
                      {proxy.host}:{proxy.port}
                    </TableCell>
                    <TableCell>
                      <Chip label={proxy.status} color={STATUS_COLOR[proxy.status]} variant="outlined" />
                    </TableCell>
                    <TableCell>
                      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                        <LinearProgress variant="determinate" value={health} color={healthColor(health)} sx={{ flex: 1, height: 4 }} />
                        <Typography variant="caption" sx={{ minWidth: 24, textAlign: 'right' }}>
                          {health.toFixed(0)}
                        </Typography>
                      </Box>
                    </TableCell>
                    <TableCell align="right" sx={{ fontVariantNumeric: 'tabular-nums' }}>
                      <Box component="span" sx={{ color: 'success.main' }}>
                        {proxy.successfulRequests}
                      </Box>
                      {' / '}
                      <Box component="span" sx={{ color: 'error.main' }}>
                        {proxy.failedRequests}
                      </Box>
                    </TableCell>
                    <TableCell align="right">{proxy.avgLatencyMs.toFixed(0)} ms</TableCell>
                    <TableCell sx={{ color: 'text.secondary' }}>{formatTime(proxy.lastUsed)}</TableCell>
                    <TableCell padding="none">
                      <Tooltip title="移除">
                        <IconButton
                          disabled={loading}
                          onClick={() => ask('確定要移除此 Proxy？', () => removeProxy(proxy.id), '移除失敗')}
                        >
                          <RemoveIcon fontSize="small" />
                        </IconButton>
                      </Tooltip>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </Box>
      )}

      <Dialog open={!!confirm} onClose={() => setConfirm(null)}>
        <DialogContent>
          <DialogContentText>{confirm?.text}</DialogContentText>
        </DialogContent>
        <DialogActions>
          <Button color="inherit" onClick={() => setConfirm(null)}>
            取消
          </Button>
          <Button
            variant="contained"
            color="error"
            onClick={() => {
              const action = confirm!.action;
              setConfirm(null);
              action();
            }}
          >
            確定
          </Button>
        </DialogActions>
      </Dialog>
      <Toast message={notice} onClose={() => setNotice(null)} />
    </Box>
  );
}
