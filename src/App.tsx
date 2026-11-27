// Zoo Downloader - Main Application Component
import { useMemo, useEffect, useState } from 'react';
import {
  ThemeProvider,
  CssBaseline,
  Box,
  Typography,
  CircularProgress,
  Alert,
} from '@mui/material';
import { useAtom } from 'jotai';
import { useTranslation } from 'react-i18next';

import { themeAtom, settingsAtom } from './stores/settingsStore';
import { lightTheme, darkTheme } from './styles/theme';
import MainPage from './pages/MainPage';
import SettingsPage from './pages/SettingsPage';
import Footer from './components/Footer';
import { useNavigation } from './hooks/useNavigation';
import { useClipboard } from './hooks/useClipboard';
import { usePageGuards } from './hooks/usePageGuards';
import { useDownloadSync } from './hooks/useDownloadSync';
import { useModulesSync } from './hooks/useSiteModules';
import CaptchaDialog from './components/CaptchaDialog';
import ErrorBoundary from './components/ErrorBoundary';
import AddDownloadsDialog from './components/AddDownloads';
import SourceChangedPrompt from './components/SourceChangedPrompt';
import { useAutomaticUpdateChecks } from './hooks/useAppUpdate';
import { waitForTauri } from './utils/tauri';
import { backend } from './backend';
import './i18n';
import { applyLanguage, loadUserLocales } from './utils/languages';

type Startup = 'waiting' | 'ready' | 'failed';

function App() {
  const [startup, setStartup] = useState<Startup>('waiting');
  const tauriReady = startup === 'ready';
  useTranslation(); // re-render in the new language

  useEffect(() => {
    waitForTauri().then((ready) => setStartup(ready ? 'ready' : 'failed'));
  }, []);

  usePageGuards();
  useClipboard(tauriReady);
  useDownloadSync(tauriReady);
  useModulesSync(tauriReady);
  useAutomaticUpdateChecks(tauriReady);
  const { currentPage } = useNavigation();
  const [currentTheme] = useAtom(themeAtom);
  const [, setSettings] = useAtom(settingsAtom);

  const theme = useMemo(() => (currentTheme === 'light' ? lightTheme : darkTheme), [currentTheme]);

  // Load settings once the backend answers, and apply the saved language (the user's language
  // files first, since it may be one of them).
  useEffect(() => {
    if (!tauriReady) return;
    backend.settings
      .get()
      .then(async (settings) => {
        setSettings(settings);
        await loadUserLocales().catch((error) => console.error('Failed to load language files:', error.message));
        await applyLanguage(settings.language);
      })
      .catch((error) => console.error('Failed to load settings:', error.message));
  }, [tauriReady, setSettings]);

  if (!tauriReady) {
    return (
      <ThemeProvider theme={theme}>
        <CssBaseline />
        <Box
          sx={{
            width: '100vw',
            height: '100vh',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexDirection: 'column',
            gap: 2,
            p: 4,
          }}
        >
          {startup === 'waiting' ? (
            <>
              <CircularProgress size={60} />
              <Typography variant="h6" color="text.secondary">
                正在初始化 Zoo Downloader...
              </Typography>
            </>
          ) : (
            <Alert severity="error" sx={{ maxWidth: 560 }}>
              無法連線到 Zoo Downloader 的後端。請重新啟動程式；若問題持續，請查看
              %APPDATA%\zoo-downloader\logs 中的記錄檔。
            </Alert>
          )}
        </Box>
      </ThemeProvider>
    );
  }

  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <Box
        sx={{
          width: '100vw',
          height: '100vh',
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        {/* Page Content */}
        <Box sx={{ flex: 1, overflow: 'hidden' }}>
          {/* Keyed by page: going to the other page starts it afresh after an error. */}
          <ErrorBoundary key={currentPage}>
            {currentPage === 'main' && <MainPage />}
            {currentPage === 'settings' && <SettingsPage />}
          </ErrorBoundary>
        </Box>

        {/* Global Footer */}
        <Footer />

        {/* A broken dialog leaves the list usable. */}
        <ErrorBoundary>
          <AddDownloadsDialog />
          <CaptchaDialog />
          <SourceChangedPrompt />
        </ErrorBoundary>
      </Box>
    </ThemeProvider>
  );
}

export default App;
