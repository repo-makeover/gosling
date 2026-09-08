import '@testing-library/jest-dom';
import { vi, afterEach } from 'vitest';
import { cleanup } from '@testing-library/react';

// Mock Electron modules before any imports
vi.mock('electron', () => ({
  app: {
    getPath: vi.fn((name: string) => {
      if (name === 'userData') return '/tmp/test-user-data';
      if (name === 'temp') return '/tmp';
      if (name === 'home') return '/tmp/home';
      return '/tmp';
    }),
  },
  ipcRenderer: {
    invoke: vi.fn(),
    send: vi.fn(),
    on: vi.fn(),
    off: vi.fn(),
  },
}));

// This is the standard set up to ensure that React Testing Library's
// automatic cleanup runs after each test.
afterEach(() => {
  cleanup();
});

// Mock console methods to avoid noise in tests
// eslint-disable-next-line no-undef
global.console = {
  ...console,
  log: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
};

// Mock window.navigator.clipboard for copy functionality tests
Object.assign(navigator, {
  clipboard: {
    write: vi.fn(() => Promise.resolve()),
    writeText: vi.fn(() => Promise.resolve()),
  },
});

// Mock settings store for tests
const mockSettings: Record<string, unknown> = {
  showMenuBarIcon: true,
  showDockIcon: true,
  enableWakelock: false,
  spellcheckEnabled: true,
  keyboardShortcuts: {
    focusWindow: 'CommandOrControl+Alt+G',
    quickLauncher: 'CommandOrControl+Alt+Shift+G',
    newChat: 'CommandOrControl+T',
    newChatWindow: 'CommandOrControl+N',
    openDirectory: 'CommandOrControl+O',
    settings: 'CommandOrControl+,',
    find: 'CommandOrControl+F',
    findNext: 'CommandOrControl+G',
    findPrevious: 'CommandOrControl+Shift+G',
    alwaysOnTop: 'CommandOrControl+Shift+T',
  },
  externalGoslingd: {
    enabled: false,
    url: '',
    secret: '',
  },
  theme: 'light',
  useSystemTheme: true,
  language: 'system',
  responseStyle: 'concise',
  showPricing: true,
  seenAnnouncementIds: [],
  recentModels: [],
  outputFileExtensions: ['pdf', 'md', 'txt', 'doc', 'docx', 'jpg', 'png', 'yaml', 'json'],
  researchLibraryPath: null,
};

// Mock window.electron for renderer process
Object.defineProperty(window, 'electron', {
  writable: true,
  value: {
    platform: 'darwin',
    sessionDirectoryChooser: vi.fn(() => Promise.resolve({ canceled: true, filePaths: [] })),
    getResearchLibraryPath: vi.fn(() =>
      Promise.resolve('/Users/tester/Documents/Gosling Research Library')
    ),
    chooseResearchLibraryPath: vi.fn(() => Promise.resolve(null)),
    listResearchLibraryFiles: vi.fn(() => Promise.resolve({ files: [], truncated: false })),
    getArtifactFileTimestamps: vi.fn(() => Promise.resolve({})),
    getSetting: vi.fn((key: string) => Promise.resolve(mockSettings[key])),
    getSettings: vi.fn((keys: string[]) =>
      Promise.resolve(Object.fromEntries(keys.map((key) => [key, mockSettings[key]])))
    ),
    setSetting: vi.fn((key: string, value: unknown) => {
      mockSettings[key] = value;
      return Promise.resolve();
    }),
    setWakelockActive: vi.fn(() => Promise.resolve(true)),
    reloadApp: vi.fn(),
    showMessageBox: vi.fn(() => Promise.resolve({ response: 0 })),
    saveArtifact: vi.fn(() => Promise.resolve({ canceled: true })),
    setArtifactRoutingConfig: vi.fn(() => Promise.resolve(true)),
    getIsFullScreen: vi.fn(() => Promise.resolve(false)),
    on: vi.fn(),
    off: vi.fn(),
    writeClipboardText: vi.fn(() => Promise.resolve()),
    writeClipboardHtml: vi.fn(() => Promise.resolve()),
  },
});
