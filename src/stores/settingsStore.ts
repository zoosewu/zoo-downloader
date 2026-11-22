// Jotai atoms for global settings state
import { atom } from 'jotai';
import type { AppSettings } from '../backend';

// Default settings
const defaultSettings: AppSettings = {
  language: '',
  theme: 'dark',
  defaultDownloadDir: '',  // Will be set from backend
  maxConcurrentDownloads: null,  // unlimited
  threadCount: 8,
  speedLimitMode: 'unlimited',
  speedLimitKbps: null,
  enableProxy: false,
  proxyFallbackDirect: true,
  proxyVerificationIntervalHours: 24,
  enableClipboardMonitoring: true,
  autoStartBrowser: false,
  autoStartCopied: false,
  enableNotifications: true,
  enableSystemTray: true,
  launchAtStartup: false,
  startInTray: true,
  autoRetryEnabled: true,
  maxRetryCount: 3,
  retryIntervalSecs: 5,
  userAgent: null,
  checkModuleUpdates: true,
  checkAppUpdates: true,
  categories: [],
};

// Settings atom
export const settingsAtom = atom<AppSettings>(defaultSettings);

// Theme atom (derived from settings)
export const themeAtom = atom(
  (get) => get(settingsAtom).theme
);

// Language atom (derived from settings)
export const languageAtom = atom(
  (get) => get(settingsAtom).language
);
