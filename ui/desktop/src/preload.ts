import Electron, { contextBridge, ipcRenderer, webUtils } from 'electron';
import { desktopCommandChannels, rendererEventChannels } from './ipc/channels';
import type {
  RendererEventCallback,
  RendererEventChannel,
  ThemeChangePayload,
  UpdaterEvent,
} from './ipc/channels';
import type { Settings, SettingKey } from './utils/settings';
import { defaultSettings } from './utils/settings';
import type {
  ArtifactRoutingConfig,
  ArtifactSaveRequest,
  ArtifactSaveResponse,
} from './types/artifactRouter';
import type { ResearchLibraryListing } from './utils/researchLibrary';
import type { ArtifactTrashResult } from './types/artifactTrash';
import type { ArtifactRepositoryClassification } from './utils/artifactRepository';
import type { ArtifactFileTimestampMap } from './types/artifactFileTimestamps';

// Mapping from settings keys to their old localStorage keys for lazy migration
const localStorageKeyMap: Partial<Record<SettingKey, string>> = {
  theme: 'theme',
  useSystemTheme: 'use_system_theme',
  responseStyle: 'response_style',
  showPricing: 'show_pricing',
  seenAnnouncementIds: 'seenAnnouncementIds',
};

// Parse localStorage value based on the setting key
function parseLocalStorageValue<K extends SettingKey>(
  key: K,
  rawValue: string
): Settings[K] | null {
  try {
    switch (key) {
      case 'theme':
        return (rawValue === 'dark' || rawValue === 'light' ? rawValue : null) as Settings[K];
      case 'useSystemTheme':
        return (rawValue === 'true') as unknown as Settings[K];
      case 'responseStyle':
        return rawValue as Settings[K];
      case 'showPricing':
        return (rawValue === 'true') as unknown as Settings[K];
      case 'seenAnnouncementIds':
        return JSON.parse(rawValue) as Settings[K];
      default:
        return null;
    }
  } catch {
    return null;
  }
}

interface NotificationData {
  title: string;
  body: string;
}

interface MessageBoxOptions {
  type?: 'none' | 'info' | 'error' | 'question' | 'warning';
  buttons?: string[];
  defaultId?: number;
  title?: string;
  message: string;
  detail?: string;
}

interface MessageBoxResponse {
  response: number;
  checkboxChecked?: boolean;
}

interface FileResponse {
  file: string;
  filePath: string;
  error: string | null;
  found: boolean;
}

export interface ArtifactFileResponse {
  content: string;
  encoding: 'base64' | 'utf8';
  error: string | null;
  filePath: string;
  found: boolean;
  sizeBytes: number;
  truncated: boolean;
}

interface McpAppProxyCsp {
  connectDomains?: string[];
  resourceDomains?: string[];
  frameDomains?: string[];
  baseUriDomains?: string[];
}

/// The ACP WebSocket endpoint. The shared secret is carried in `subprotocol`
/// rather than the URL so it stays out of logs and crash reports
/// (SEC-GOS-001).
export interface AcpEndpoint {
  url: string;
  subprotocol: string;
}

const config = JSON.parse(process.argv.find((arg) => arg.startsWith('{')) || '{}');

export interface CreateChatWindowOptions {
  query?: string;
  dir?: string;
  version?: string;
  resumeSessionId?: string;
  viewType?: string;
}

// Define the API types in a single place
type ElectronAPI = {
  platform: string;
  arch: string;
  reactReady: () => void;
  getConfig: () => Record<string, unknown>;
  directoryChooser: () => Promise<Electron.OpenDialogReturnValue>;
  sessionDirectoryChooser: () => Promise<Electron.OpenDialogReturnValue>;
  getResearchLibraryPath: () => Promise<string>;
  chooseResearchLibraryPath: () => Promise<string | null>;
  listResearchLibraryFiles: () => Promise<ResearchLibraryListing>;
  createChatWindow: (options?: CreateChatWindowOptions) => void;
  logInfo: (txt: string) => void;
  showNotification: (data: NotificationData) => void;
  showMessageBox: (options: MessageBoxOptions) => Promise<MessageBoxResponse>;
  saveArtifact: (request: ArtifactSaveRequest) => Promise<ArtifactSaveResponse>;
  setArtifactRoutingConfig: (config: ArtifactRoutingConfig | null) => Promise<boolean>;
  openInChrome: (url: string) => void;
  reloadApp: () => void;
  checkForOllama: () => Promise<boolean>;
  selectFileOrDirectory: (defaultPath?: string) => Promise<string | null>;
  selectArtifactFile: (defaultPath?: string) => Promise<string | null>;
  selectImportSessionFile: () => Promise<{
    filePath: string;
    contents: string;
    error?: string;
  } | null>;
  readFile: (directory: string) => Promise<FileResponse>;
  readArtifactFile: (filePath: string, baseDirectory?: string) => Promise<ArtifactFileResponse>;
  copyArtifactContents: (filePath: string, baseDirectory?: string) => Promise<void>;
  readArtifactTitles: (
    requests: Array<{ filePath: string; baseDirectory?: string }>
  ) => Promise<Record<string, string>>;
  classifyArtifactRepositories: (filePaths: string[]) => Promise<ArtifactRepositoryClassification>;
  getArtifactFileTimestamps: (filePaths: string[]) => Promise<ArtifactFileTimestampMap>;
  openArtifactFile: (filePath: string, baseDirectory?: string) => Promise<boolean>;
  revealArtifactFile: (filePath: string, baseDirectory?: string) => Promise<void>;
  writeFile: (directory: string, content: string) => Promise<boolean>;
  deleteFile: (filePath: string) => Promise<boolean>;
  trashArtifactFiles: (paths: string[]) => Promise<ArtifactTrashResult[]>;
  ensureDirectory: (dirPath: string) => Promise<boolean>;
  listFiles: (dirPath: string, extension?: string) => Promise<string[]>;
  getAllowedExtensions: () => Promise<string[]>;
  getPathForFile: (file: File) => string;
  setMenuBarIcon: (show: boolean) => Promise<boolean>;
  getMenuBarIconState: () => Promise<boolean>;
  setDockIcon: (show: boolean) => Promise<boolean>;
  getDockIconState: () => Promise<boolean>;
  getSetting: <K extends SettingKey>(key: K) => Promise<Settings[K]>;
  getSettings: <K extends SettingKey>(keys: K[]) => Promise<Pick<Settings, K>>;
  setSetting: <K extends SettingKey>(key: K, value: Settings[K]) => Promise<void>;
  getAcpUrl: () => Promise<AcpEndpoint | null>;
  getMcpAppProxyUrl: (csp?: McpAppProxyCsp | null) => Promise<string | null>;
  setWakelock: (enable: boolean) => Promise<boolean>;
  getWakelockState: () => Promise<boolean>;
  setWakelockActive: (sessionId: string, active: boolean) => Promise<boolean>;
  setSpellcheck: (enable: boolean) => Promise<boolean>;
  getSpellcheckState: () => Promise<boolean>;
  openNotificationsSettings: () => Promise<boolean>;
  isAnyWindowFocused: () => Promise<boolean>;
  getIsFullScreen: () => Promise<boolean>;
  onMouseBackButtonClicked: (callback: () => void) => void;
  offMouseBackButtonClicked: (callback: () => void) => void;
  on: <T extends RendererEventChannel>(channel: T, callback: RendererEventCallback<T>) => void;
  off: <T extends RendererEventChannel>(channel: T, callback: RendererEventCallback<T>) => void;
  broadcastThemeChange: (themeData: ThemeChangePayload) => void;
  broadcastWorkspaceChange: () => void;
  openExternal: (url: string) => Promise<void>;
  // Update-related functions
  getVersion: () => string;
  checkForUpdates: () => Promise<{ updateInfo: unknown; error: string | null }>;
  downloadUpdate: () => Promise<{ success: boolean; error: string | null }>;
  installUpdate: () => void;
  restartApp: () => void;
  onUpdaterEvent: (callback: (event: UpdaterEvent) => void) => () => void;
  getUpdateState: () => Promise<{ updateAvailable: boolean; latestVersion?: string } | null>;
  isUsingGitHubFallback: () => Promise<boolean>;
  getAutoDownloadDisabled: () => Promise<boolean>;
  closeWindow: () => void;
  openDirectoryInExplorer: (directoryPath: string) => Promise<boolean>;
  addRecentDir: (dir: string) => Promise<boolean>;
  listRecentDirs: () => Promise<string[]>;
  listGitWorktreeDirs: (dir: string) => Promise<string[]>;
  getGitBranchInfo: (dir: string) => Promise<{ branch: string } | null>;
  listGitBranches: (dir: string) => Promise<string[]>;
  switchGitBranch: (dir: string, branch: string) => Promise<{ success: boolean }>;
  writeClipboardText: (text: string) => Promise<void>;
  writeClipboardHtml: (html: string, text: string) => Promise<void>;
};

type AppConfigAPI = {
  get: (key: string) => unknown;
  getAll: () => Record<string, unknown>;
};

const mouseBackButtonListeners = new WeakMap<() => void, () => void>();

const electronAPI: ElectronAPI = {
  platform: process.platform,
  arch: process.arch,
  reactReady: () => ipcRenderer.send(desktopCommandChannels.reactReady),
  getConfig: () => {
    if (!config || Object.keys(config).length === 0) {
      console.warn(
        'No config provided by main process. This may indicate an initialization issue.'
      );
    }
    return config;
  },
  directoryChooser: () => ipcRenderer.invoke('directory-chooser'),
  sessionDirectoryChooser: () => ipcRenderer.invoke('session-directory-chooser'),
  getResearchLibraryPath: () => ipcRenderer.invoke('get-research-library-path'),
  chooseResearchLibraryPath: () => ipcRenderer.invoke('choose-research-library-path'),
  listResearchLibraryFiles: () => ipcRenderer.invoke('list-research-library-files'),
  createChatWindow: (options?: CreateChatWindowOptions) =>
    ipcRenderer.send(desktopCommandChannels.createChatWindow, options || {}),
  logInfo: (txt: string) => ipcRenderer.send('logInfo', txt),
  showNotification: (data: NotificationData) => ipcRenderer.send('notify', data),
  showMessageBox: (options: MessageBoxOptions) => ipcRenderer.invoke('show-message-box', options),
  saveArtifact: (request: ArtifactSaveRequest) => ipcRenderer.invoke('save-artifact', request),
  setArtifactRoutingConfig: (routingConfig: ArtifactRoutingConfig | null) =>
    ipcRenderer.invoke('set-artifact-routing-config', routingConfig),
  openInChrome: (url: string) => ipcRenderer.send('open-in-chrome', url),
  reloadApp: () => ipcRenderer.send('reload-app'),
  checkForOllama: () => ipcRenderer.invoke('check-ollama'),

  selectFileOrDirectory: (defaultPath?: string) =>
    ipcRenderer.invoke('select-file-or-directory', defaultPath),
  selectArtifactFile: (defaultPath?: string) =>
    ipcRenderer.invoke('select-artifact-file', defaultPath),
  selectImportSessionFile: () => ipcRenderer.invoke('select-import-session-file'),
  readFile: (filePath: string) => ipcRenderer.invoke('read-file', filePath),
  readArtifactFile: (filePath: string, baseDirectory?: string) =>
    ipcRenderer.invoke('read-artifact-file', filePath, baseDirectory),
  copyArtifactContents: (filePath: string, baseDirectory?: string) =>
    ipcRenderer.invoke('copy-artifact-contents', filePath, baseDirectory),
  readArtifactTitles: (requests: Array<{ filePath: string; baseDirectory?: string }>) =>
    ipcRenderer.invoke('read-artifact-titles', requests),
  classifyArtifactRepositories: (filePaths: string[]) =>
    ipcRenderer.invoke('classify-artifact-repositories', filePaths),
  getArtifactFileTimestamps: (filePaths: string[]) =>
    ipcRenderer.invoke('get-artifact-file-timestamps', filePaths),
  openArtifactFile: (filePath: string, baseDirectory?: string) =>
    ipcRenderer.invoke('open-artifact-file', filePath, baseDirectory),
  revealArtifactFile: (filePath: string, baseDirectory?: string) =>
    ipcRenderer.invoke('reveal-artifact-file', filePath, baseDirectory),
  writeFile: (filePath: string, content: string) =>
    ipcRenderer.invoke('write-file', filePath, content),
  deleteFile: (filePath: string) => ipcRenderer.invoke('delete-file', filePath),
  trashArtifactFiles: (paths: string[]) => ipcRenderer.invoke('trash-artifact-files', paths),
  ensureDirectory: (dirPath: string) => ipcRenderer.invoke('ensure-directory', dirPath),
  listFiles: (dirPath: string, extension?: string) =>
    ipcRenderer.invoke('list-files', dirPath, extension),
  getPathForFile: (file: File) => webUtils.getPathForFile(file),
  getAllowedExtensions: () => ipcRenderer.invoke('get-allowed-extensions'),
  setMenuBarIcon: (show: boolean) => ipcRenderer.invoke('set-menu-bar-icon', show),
  getMenuBarIconState: () => ipcRenderer.invoke('get-menu-bar-icon-state'),
  setDockIcon: (show: boolean) => ipcRenderer.invoke('set-dock-icon', show),
  getDockIconState: () => ipcRenderer.invoke('get-dock-icon-state'),
  getSetting: async <K extends SettingKey>(key: K): Promise<Settings[K]> => {
    try {
      // Check for localStorage value first (lazy migration)
      const localStorageKey = localStorageKeyMap[key];
      if (localStorageKey) {
        const rawValue = localStorage.getItem(localStorageKey);
        if (rawValue !== null) {
          const parsed = parseLocalStorageValue(key, rawValue);
          if (parsed !== null) {
            return parsed;
          }
        }
      }
      return await ipcRenderer.invoke('get-setting', key);
    } catch (error) {
      console.error(`Failed to get setting '${key}', using default`, error);
      return defaultSettings[key];
    }
  },
  getSettings: async <K extends SettingKey>(keys: K[]): Promise<Pick<Settings, K>> => {
    const values: Partial<Pick<Settings, K>> = {};
    const ipcKeys: K[] = [];

    for (const key of keys) {
      const localStorageKey = localStorageKeyMap[key];
      if (localStorageKey) {
        const rawValue = localStorage.getItem(localStorageKey);
        if (rawValue !== null) {
          const parsed = parseLocalStorageValue(key, rawValue);
          if (parsed !== null) {
            values[key] = parsed;
            continue;
          }
        }
      }
      ipcKeys.push(key);
    }

    if (ipcKeys.length > 0) {
      try {
        Object.assign(values, await ipcRenderer.invoke('get-settings', ipcKeys));
      } catch (error) {
        console.error(`Failed to get settings '${ipcKeys.join(', ')}', using defaults`, error);
        for (const key of ipcKeys) {
          values[key] = defaultSettings[key];
        }
      }
    }

    return values as Pick<Settings, K>;
  },
  setSetting: async <K extends SettingKey>(key: K, value: Settings[K]): Promise<void> => {
    // Clear any localStorage version when writing
    const localStorageKey = localStorageKeyMap[key];
    if (localStorageKey) {
      localStorage.removeItem(localStorageKey);
    }
    return ipcRenderer.invoke('set-setting', key, value);
  },
  getAcpUrl: () => ipcRenderer.invoke('get-acp-url'),
  getMcpAppProxyUrl: (csp?: McpAppProxyCsp | null) =>
    ipcRenderer.invoke('get-mcp-app-proxy-url', csp),
  setWakelock: (enable: boolean) => ipcRenderer.invoke('set-wakelock', enable),
  getWakelockState: () => ipcRenderer.invoke('get-wakelock-state'),
  setWakelockActive: (sessionId: string, active: boolean) =>
    ipcRenderer.invoke('set-wakelock-active', sessionId, active),
  setSpellcheck: (enable: boolean) => ipcRenderer.invoke('set-spellcheck', enable),
  getSpellcheckState: () => ipcRenderer.invoke('get-spellcheck-state'),
  openNotificationsSettings: () => ipcRenderer.invoke('open-notifications-settings'),
  isAnyWindowFocused: () => ipcRenderer.invoke('is-any-window-focused'),
  getIsFullScreen: () => ipcRenderer.invoke('get-is-fullscreen'),
  onMouseBackButtonClicked: (callback: () => void) => {
    const wrappedCallback = () => callback();
    mouseBackButtonListeners.set(callback, wrappedCallback);
    ipcRenderer.on(rendererEventChannels.mouseBackButtonClicked, wrappedCallback);
  },
  offMouseBackButtonClicked: (callback: () => void) => {
    const wrappedCallback = mouseBackButtonListeners.get(callback);
    if (wrappedCallback) {
      ipcRenderer.removeListener(rendererEventChannels.mouseBackButtonClicked, wrappedCallback);
      mouseBackButtonListeners.delete(callback);
    }
  },
  on: <T extends RendererEventChannel>(channel: T, callback: RendererEventCallback<T>) => {
    ipcRenderer.on(channel, callback);
  },
  off: <T extends RendererEventChannel>(channel: T, callback: RendererEventCallback<T>) => {
    ipcRenderer.off(channel, callback);
  },
  broadcastThemeChange: (themeData: ThemeChangePayload) => {
    ipcRenderer.send(desktopCommandChannels.broadcastThemeChange, themeData);
  },
  broadcastWorkspaceChange: () => {
    ipcRenderer.send(desktopCommandChannels.broadcastWorkspaceChange);
  },
  openExternal: (url: string): Promise<void> => {
    return ipcRenderer.invoke(desktopCommandChannels.openExternal, url);
  },
  getVersion: (): string => {
    return (
      config.GOSLING_VERSION || ipcRenderer.sendSync(desktopCommandChannels.getAppVersion) || ''
    );
  },
  checkForUpdates: (): Promise<{ updateInfo: unknown; error: string | null }> => {
    return ipcRenderer.invoke('check-for-updates');
  },
  downloadUpdate: (): Promise<{ success: boolean; error: string | null }> => {
    return ipcRenderer.invoke('download-update');
  },
  installUpdate: (): void => {
    ipcRenderer.invoke('install-update');
  },
  restartApp: (): void => {
    ipcRenderer.send('restart-app');
  },
  onUpdaterEvent: (callback: (event: UpdaterEvent) => void): (() => void) => {
    const handler = (_event: Electron.IpcRendererEvent, data: UpdaterEvent) => callback(data);
    ipcRenderer.on(rendererEventChannels.updaterEvent, handler);
    return () => ipcRenderer.removeListener(rendererEventChannels.updaterEvent, handler);
  },
  getUpdateState: (): Promise<{ updateAvailable: boolean; latestVersion?: string } | null> => {
    return ipcRenderer.invoke('get-update-state');
  },
  isUsingGitHubFallback: (): Promise<boolean> => {
    return ipcRenderer.invoke('is-using-github-fallback');
  },
  getAutoDownloadDisabled: (): Promise<boolean> => {
    return ipcRenderer.invoke('get-auto-download-disabled');
  },
  closeWindow: () => ipcRenderer.send(desktopCommandChannels.closeWindow),
  openDirectoryInExplorer: (directoryPath: string) =>
    ipcRenderer.invoke('open-directory-in-explorer', directoryPath),
  addRecentDir: (dir: string) => ipcRenderer.invoke('add-recent-dir', dir),
  listRecentDirs: () => ipcRenderer.invoke('list-recent-dirs'),
  listGitWorktreeDirs: (dir: string) => ipcRenderer.invoke('list-git-worktree-dirs', dir),
  getGitBranchInfo: (dir: string) => ipcRenderer.invoke('get-git-branch-info', dir),
  listGitBranches: (dir: string) => ipcRenderer.invoke('list-git-branches', dir),
  switchGitBranch: (dir: string, branch: string) =>
    ipcRenderer.invoke('switch-git-branch', dir, branch),
  writeClipboardText: async (text: string): Promise<void> => {
    await ipcRenderer.invoke('write-clipboard-text', text);
  },
  writeClipboardHtml: async (html: string, text: string): Promise<void> => {
    await ipcRenderer.invoke('write-clipboard-html', html, text);
  },
};

function getAppLocale(): unknown {
  try {
    return ipcRenderer.sendSync(desktopCommandChannels.getAppLocale) ?? config.GOSLING_LOCALE;
  } catch {
    return config.GOSLING_LOCALE;
  }
}

const appConfigAPI: AppConfigAPI = {
  get: (key: string) => (key === 'GOSLING_LOCALE' ? getAppLocale() : config[key]),
  getAll: () => ({ ...config, GOSLING_LOCALE: getAppLocale() }),
};

// Expose the APIs
contextBridge.exposeInMainWorld('electron', electronAPI);
contextBridge.exposeInMainWorld('appConfig', appConfigAPI);

// Type declaration for TypeScript
declare global {
  interface Window {
    electron: ElectronAPI;
    appConfig: AppConfigAPI;
  }
}
