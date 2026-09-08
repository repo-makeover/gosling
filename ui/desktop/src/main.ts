// Full Desktop compatibility facade. Extracted main-process responsibilities live under
// ./main; these imports and registration calls preserve the executable entrypoint and must not
// be pruned as unused compatibility wiring.

import type { Certificate } from 'electron';
import {
  app,
  App,
  BrowserWindow,
  dialog,
  globalShortcut,
  ipcMain,
  Menu,
  MenuItem,
  net,
  Notification,
  powerSaveBlocker,
  session,
  shell,
  webContents,
} from 'electron';
import { pathToFileURL, format as formatUrl, URLSearchParams } from 'node:url';
import fs from 'node:fs/promises';
import fsSync from 'node:fs';
import started from 'electron-squirrel-startup';
import path from 'node:path';
import os from 'node:os';
import { spawn } from 'child_process';
import 'dotenv/config';
import { checkBackendStatus } from './backendStatus';
import { startGoslingServe } from './goslingServe';
import { GoslingServeLeaseRegistry, type GoslingServeLease } from './goslingServeLeaseRegistry';
import { cleanupRecordedBackendProcesses } from './backendProcessRegistry';
import { getOverrideOriginForRequest } from './requestOrigin';
import { acpWebSocketUrlFromHttpBase, normalizeAcpHttpBaseUrl } from './acp/url';
import { expandTilde } from './utils/pathUtils';
import { assertPathWithinRoots } from './utils/rendererFileAccess';
import { writeJsonFileAtomicSync, readJsonFileWithRecoverySync } from './utils/atomicJsonStore';
import { RendererDirectoryGrantRegistry } from './utils/rendererDirectoryGrants';
import log from './utils/logger';
import { ensureWinShims } from './utils/winShims';
import { addRecentDir, loadRecentDirs } from './utils/recentDirs';
import { errorMessage, formatErrorForLogging } from './utils/conversionUtils';
import type { LegacySettings, Settings } from './utils/settings';
import { getKeyboardShortcuts, resolveStoredSettings } from './utils/settings';
import * as crypto from 'crypto';
import windowStateKeeper from 'electron-window-state';
import {
  registerUpdateIpcHandlers,
  setAutoDownloadDisabled,
  setupAutoUpdater,
} from './utils/autoUpdater';
import { UPDATES_ENABLED } from './updates';
import installExtension, { REACT_DEVELOPER_TOOLS } from 'electron-devtools-installer';
import { blockTopLevelNavigation, openExternalUrlIfSafe } from './utils/urlSecurity';
import { buildCSP } from './utils/csp';
import { rendererEventChannels } from './ipc/channels';
import type { ArtifactRoutingConfig } from './types/artifactRouter';
import { installArtifactDownloadRouter } from './utils/artifactDownloads';
import { ArtifactRoutingRegistry } from './utils/artifactRoutingRegistry';
import {
  assertArtifactFileAccess,
  resolveArtifactFileCapability,
} from './utils/artifactFileAccess';
import {
  dispatchFullGoslingProtocolUrl,
  findGoslingProtocolUrl,
  parseGoslingProtocolRoute,
} from './handoffProtocol';
import {
  translateMenuLabel,
  translateMenuLabels as translateNativeMenuLabels,
} from './main/menuLocalization';
import {
  installBackendCertificateVerifier,
  isTrustedHost,
  normalizeFingerprint,
  trustBackendCertificate,
  verifyBackendCertificate,
  type BackendCertificateTrustRegistration,
} from './main/backendCertificateTrust';
import { getAllowList } from './main/allowlist';
import { registerFileIpcHandlers } from './main/fileIpc';
import { registerSystemIpcHandlers } from './main/systemIpc';
import { registerRendererIpcHandlers } from './main/rendererIpc';
import { registerSettingsIpcHandlers } from './main/settingsIpc';
import { createWindowChrome } from './main/windowChrome';
import { installApplicationMenu } from './main/applicationMenu';
import { registerAppIpcHandlers } from './main/appIpc';

function shouldSetupUpdater(): boolean {
  // Setup updater if either the flag is enabled OR dev updates are enabled
  return UPDATES_ENABLED || process.env.ENABLE_DEV_UPDATES === 'true';
}

// =======================================================================
// Native menu localization
// -----------------------------------------------------------------------
// Electron's main process can't use react-intl (which runs in the renderer),
// so the native menu bar is translated here with a small hand-maintained
// dictionary. Only Simplified Chinese is filled in right now; other locales
// fall through to the original English labels. Keep the keys in sync with
// the raw label strings used below.
// =======================================================================

function detectMenuLocale(): string {
  return getConfiguredGoslingLocale() ?? 'en';
}

function menuT(label: string): string {
  return translateMenuLabel(detectMenuLocale(), label);
}

/**
 * Recursively translate `label` on every item in the given menu, including nested submenus.
 * Electron's default application menu comes with English labels that are not otherwise
 * configurable, so we post-process them here before calling `Menu.setApplicationMenu`.
 */
function translateMenuLabels(items: MenuItem[]): void {
  translateNativeMenuLabels(items, menuT);
}

// Settings management
if (process.env.ENABLE_PLAYWRIGHT && process.env.GOSLING_PLAYWRIGHT_USER_DATA_DIR) {
  app.setPath('userData', path.resolve(process.env.GOSLING_PLAYWRIGHT_USER_DATA_DIR));
}
const SETTINGS_FILE = path.join(app.getPath('userData'), 'settings.json');
const RENDERER_DIRECTORY_GRANTS_FILE = path.join(
  app.getPath('userData'),
  'renderer-directory-grants.json'
);
const STARTUP_LOGS_DIR = path.join(app.getPath('userData'), 'logs', 'startup');
const BACKEND_PROCESS_REGISTRY_PATH = path.join(app.getPath('userData'), 'backend-processes.json');
const validLanguageSettings = new Set<Settings['language']>([
  'system',
  'en',
  'es',
  'fr',
  'de',
  'it',
  'pt',
  'id',
  'ms',
  'vi',
  'hi',
  'ja',
  'ko',
  'ru',
  'tr',
  'zh-CN',
  'zh-TW',
]);

function isValidLanguageSetting(value: unknown): value is Settings['language'] {
  return typeof value === 'string' && validLanguageSettings.has(value as Settings['language']);
}

// Cached parsed settings: getSettings() is called from hot paths on the main
// process (notably the CSP rebuild in onHeadersReceived, which runs for every
// HTTP response), and synchronous disk reads there jank the whole UI. All
// writes go through updateSettings(), which invalidates the cache.
let settingsCache: Settings | null = null;
let externalBackendSecret = '';
let legacySecretRemovalNoticePending = false;
let externalSecretPersistenceNoticePending = false;
let settingsRecoveryNoticePending = false;

function isLegacySettings(value: unknown): value is LegacySettings {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function getSettings(): Settings {
  if (settingsCache) {
    return settingsCache;
  }

  if (fsSync.existsSync(SETTINGS_FILE)) {
    try {
      const storedResult = readJsonFileWithRecoverySync(SETTINGS_FILE, isLegacySettings);
      if (!storedResult) throw new Error('Settings file disappeared while loading');
      const {
        settings,
        migratedLegacyExternalBackend,
        removedLegacyManagedSecretProfiles,
        removedPersistedExternalSecret,
        legacyExternalBackendSecret,
      } = resolveStoredSettings(storedResult.value);
      externalBackendSecret ||= legacyExternalBackendSecret;
      legacySecretRemovalNoticePending ||= removedLegacyManagedSecretProfiles;
      externalSecretPersistenceNoticePending ||= removedPersistedExternalSecret;
      settingsCache = settings;
      settingsRecoveryNoticePending ||= storedResult.recoveredFromPrevious;
      if (
        storedResult.recoveredFromPrevious ||
        migratedLegacyExternalBackend ||
        removedLegacyManagedSecretProfiles ||
        removedPersistedExternalSecret
      ) {
        writeJsonFileAtomicSync(SETTINGS_FILE, settings, { preservePrevious: false });
      }
      return settingsCache;
    } catch (err) {
      console.error('Failed to read settings.json, using defaults:', err);
      settingsRecoveryNoticePending = true;
      settingsCache = resolveStoredSettings({}).settings;
      return settingsCache;
    }
  }
  settingsCache = resolveStoredSettings({}).settings;
  return settingsCache;
}

function resolveRendererPath(filePath: string): string {
  return path.resolve(expandTilde(filePath));
}

const rendererDirectoryGrants = new RendererDirectoryGrantRegistry(RENDERER_DIRECTORY_GRANTS_FILE);
try {
  rendererDirectoryGrants.load();
} catch (error) {
  console.error('Failed to load renderer directory grants; starting with no grants:', error);
}

function rendererFileRoots(webContentsId: number): string[] {
  return rendererDirectoryGrants.rootsFor(webContentsId);
}

function firstGrantedRecentDirectory(webContentsId = 0): string | undefined {
  return loadRecentDirs().find((dir) =>
    rendererDirectoryGrants.isGrantedDirectory(webContentsId, dir)
  );
}

async function assertRendererFileAccess(webContentsId: number, filePath: string): Promise<string> {
  const resolvedPath = resolveRendererPath(filePath);
  return assertPathWithinRoots(resolvedPath, rendererFileRoots(webContentsId));
}

const rendererArtifactFileGrants = new Map<number, Set<string>>();
const artifactRoutingRegistry = new ArtifactRoutingRegistry();
const ARTIFACT_PRODUCT_TYPES = new Set([
  'code',
  'data',
  'document',
  'export',
  'image',
  'other',
  'presentation',
  'spreadsheet',
  'video',
]);

async function assertRendererArtifactFileAccess(
  webContentsId: number,
  filePath: string,
  baseDirectory?: string
): Promise<string> {
  const routingConfig = artifactRoutingRegistry.get(webContentsId);
  const routedOutputRoots = routingConfig?.outputs.map((output) => output.path) ?? [];
  const routedArtifactFiles = routingConfig?.artifactFiles ?? [];
  const expandedPath = expandTilde(filePath);
  const candidatePath = path.isAbsolute(expandedPath) ? resolveRendererPath(filePath) : filePath;
  return assertArtifactFileAccess(
    candidatePath,
    baseDirectory ? resolveRendererPath(baseDirectory) : undefined,
    rendererFileRoots(webContentsId),
    routedOutputRoots,
    // Session capabilities stay in the current routing config so switching sessions revokes them.
    new Set([...(rendererArtifactFileGrants.get(webContentsId) ?? []), ...routedArtifactFiles])
  );
}

async function assertArtifactOutputRootAccess(
  webContentsId: number,
  outputPath: string
): Promise<string> {
  return assertRendererFileAccess(webContentsId, outputPath);
}

async function validateArtifactRoutingConfig(
  webContentsId: number,
  config: ArtifactRoutingConfig
): Promise<ArtifactRoutingConfig | null> {
  if (
    (config.workspaceId !== undefined && typeof config.workspaceId !== 'string') ||
    (config.workspaceName !== undefined && typeof config.workspaceName !== 'string') ||
    (config.workspaceId === undefined) !== (config.workspaceName === undefined) ||
    !Array.isArray(config.outputs) ||
    config.outputs.length > 64 ||
    (config.artifactFiles !== undefined &&
      (!Array.isArray(config.artifactFiles) || config.artifactFiles.length > 256))
  ) {
    return null;
  }

  const outputs = [];
  for (const output of config.outputs) {
    if (
      typeof output.id !== 'string' ||
      typeof output.path !== 'string' ||
      typeof output.isDefault !== 'boolean' ||
      !Array.isArray(output.productTypes) ||
      output.productTypes.length === 0 ||
      !output.productTypes.every((productType) => ARTIFACT_PRODUCT_TYPES.has(productType))
    ) {
      continue;
    }
    try {
      const outputPath = await assertArtifactOutputRootAccess(webContentsId, output.path);
      const stats = await fs.stat(outputPath);
      if (stats.isDirectory()) outputs.push({ ...output, path: outputPath });
    } catch {
      continue;
    }
  }

  const artifactFiles = [];
  for (const artifactFile of config.artifactFiles ?? []) {
    if (
      typeof artifactFile !== 'string' ||
      artifactFile.length === 0 ||
      artifactFile.length > 4096
    ) {
      continue;
    }
    try {
      const artifactPath = await resolveArtifactFileCapability(resolveRendererPath(artifactFile));
      if (artifactPath) artifactFiles.push(artifactPath);
    } catch {
      continue;
    }
  }

  return outputs.length > 0 || artifactFiles.length > 0
    ? { ...config, artifactFiles: [...new Set(artifactFiles)], outputs }
    : null;
}

async function openExternalIfSafe(url: string): Promise<void> {
  if (!(await openExternalUrlIfSafe(url, (safeUrl) => shell.openExternal(safeUrl)))) {
    console.warn(`[Main] Blocked unsafe external URL: ${url}`);
  }
}

function updateSettings(modifier: (settings: Settings) => void): void {
  const settings = getSettings();
  modifier(settings);
  try {
    writeJsonFileAtomicSync(SETTINGS_FILE, settings);
  } finally {
    settingsCache = null;
  }
}

function getConfiguredGoslingLocale(): string | undefined {
  const language = getSettings().language;
  if (isValidLanguageSetting(language) && language !== 'system') {
    return language;
  }

  if (process.env.GOSLING_LOCALE) {
    return process.env.GOSLING_LOCALE;
  }

  try {
    return app.isReady() ? app.getSystemLocale() || undefined : undefined;
  } catch {
    return undefined;
  }
}

async function configureProxy() {
  const httpsProxy = process.env.HTTPS_PROXY || process.env.https_proxy;
  const httpProxy = process.env.HTTP_PROXY || process.env.http_proxy;
  const noProxy = process.env.NO_PROXY || process.env.no_proxy || '';

  const proxyUrl = httpsProxy || httpProxy;

  if (proxyUrl) {
    console.log('[Main] Configuring proxy');
    await session.defaultSession.setProxy({
      proxyRules: proxyUrl,
      proxyBypassRules: noProxy,
    });
    console.log('[Main] Proxy configured successfully');
  }
}

if (started) app.quit();

// Certificate trust for active backend leases. Renderer requests pin to the
// exact cert fingerprint. Each backend lease owns a trust record so old windows
// keep working after settings change.
const MAIN_WINDOW_SESSION_PARTITION = 'persist:gosling';

// Renderer requests: pin to the exact cert once known.
app.on('certificate-error', (event, _webContents, url, _error, certificate, callback) => {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    callback(false);
    return;
  }
  if (!isTrustedHost(parsed.hostname)) {
    callback(false);
    return;
  }

  event.preventDefault();
  const cert = certificate as Certificate & {
    fingerprint256?: string;
  };
  callback(verifyBackendCertificate(parsed.hostname, cert.fingerprint256 ?? cert.fingerprint));
});

app.whenReady().then(() => {
  appConfig.GOSLING_LOCALE = getConfiguredGoslingLocale();
});

// Main-process net.fetch: pin to the exact cert once known.
if (process.env.ENABLE_PLAYWRIGHT) {
  const debugPort = process.env.PLAYWRIGHT_DEBUG_PORT || '9222';
  console.log(`[Main] Enabling Playwright remote debugging on port ${debugPort}`);
  app.commandLine.appendSwitch('remote-debugging-port', debugPort);
}

// In development mode, force registration as the default protocol client
// In production, register normally
if (MAIN_WINDOW_VITE_DEV_SERVER_URL) {
  // Development mode - force registration
  console.log('[Main] Development mode: Forcing protocol registration for gosling://');
  app.setAsDefaultProtocolClient('gosling');

  if (process.platform === 'darwin') {
    try {
      // Reset the default handler to ensure dev version takes precedence
      spawn('open', ['-a', process.execPath, '--args', '--reset-protocol-handler', 'gosling'], {
        detached: true,
        stdio: 'ignore',
      });
    } catch {
      console.warn('[Main] Could not reset protocol handler');
    }
  }
} else {
  // Production mode - normal registration
  app.setAsDefaultProtocolClient('gosling');
}

let openUrlHandledLaunch = false;
let shouldQuitForSingleInstance = false;

function focusExistingWindow(): boolean {
  const existingWindows = BrowserWindow.getAllWindows();
  if (existingWindows.length === 0) {
    return false;
  }

  const mainWindow = existingWindows[0];
  if (mainWindow.isMinimized()) {
    mainWindow.restore();
  }
  mainWindow.show();
  mainWindow.focus();
  return true;
}

function handleSecondInstanceCommandLine(commandLine: string[]): void {
  const protocolUrl = findGoslingProtocolUrl(commandLine);
  if (!protocolUrl) {
    void app.whenReady().then(() => {
      focusExistingWindow();
    });
    return;
  }

  let parsedUrl: URL;
  try {
    parsedUrl = new URL(protocolUrl);
  } catch (error) {
    log.warn('[Main] Ignoring invalid second-instance protocol URL:', errorMessage(error));
    return;
  }

  void app.whenReady().then(async () => {
    try {
      if (!(await handleProtocolUrl(protocolUrl, parsedUrl))) {
        log.warn('[Main] Ignoring unsupported second-instance protocol URL');
        focusExistingWindow();
      }
    } catch (error) {
      log.error('[Main] Failed to handle second-instance protocol URL:', errorMessage(error));
      focusExistingWindow();
    }
  });
}

if (!process.env.ENABLE_PLAYWRIGHT && !app.requestSingleInstanceLock()) {
  shouldQuitForSingleInstance = true;
  app.quit();
} else if (!process.env.ENABLE_PLAYWRIGHT) {
  app.on('second-instance', (_event, commandLine) => {
    handleSecondInstanceCommandLine(commandLine);
  });
}

if (process.platform !== 'darwin') {
  // Handle protocol URLs on Windows and Linux startup
  const protocolUrl = findGoslingProtocolUrl(process.argv);
  if (protocolUrl) {
    const startupRoute = parseGoslingProtocolRoute(protocolUrl);
    if (startupRoute) {
      openUrlHandledLaunch = true;
    }
    app.whenReady().then(async () => {
      let parsedUrl: URL;
      try {
        parsedUrl = new URL(protocolUrl);
      } catch (error) {
        log.warn('[Main] Ignoring invalid startup protocol URL:', errorMessage(error));
        return;
      }

      try {
        openUrlHandledLaunch = await handleProtocolUrl(protocolUrl, parsedUrl);
        if (!openUrlHandledLaunch) {
          log.warn('[Main] Ignoring unsupported startup protocol URL');
        }
      } catch (error) {
        log.error('[Main] Failed to handle startup protocol URL:', errorMessage(error));
        openUrlHandledLaunch = false;
        if (BrowserWindow.getAllWindows().length === 0) {
          const { dirPath } = parseArgs();
          await createNewWindow(app, dirPath);
          openUrlHandledLaunch = true;
        }
      }
    });
  }
}

const pendingDeepLinks = new Map<number, string>();

function queuePendingDeepLink(windowId: number, url: string): void {
  if (pendingDeepLinks.get(windowId) === url) {
    return;
  }
  pendingDeepLinks.set(windowId, url);
}

const reactReadyWindows = new Set<number>();

const DEEPLINK_BURST_DEDUP_MS = 2000;
const recentSessionDeepLinkSends = new Map<string, number>();

function pruneExpiredSessionDeepLinkSends(now: number): void {
  for (const [url, sentAt] of recentSessionDeepLinkSends) {
    if (now - sentAt >= DEEPLINK_BURST_DEDUP_MS) {
      recentSessionDeepLinkSends.delete(url);
    }
  }
}

function isBurstDuplicateSessionDeepLink(url: string): boolean {
  const now = Date.now();
  pruneExpiredSessionDeepLinkSends(now);
  const sentAt = recentSessionDeepLinkSends.get(url);
  return sentAt !== undefined && now - sentAt < DEEPLINK_BURST_DEDUP_MS;
}

function recordSessionDeepLinkSend(url: string): void {
  const now = Date.now();
  recentSessionDeepLinkSends.set(url, now);
  pruneExpiredSessionDeepLinkSends(now);
}

function sendOpenSharedSession(window: BrowserWindow, url: string): void {
  if (isBurstDuplicateSessionDeepLink(url)) {
    log.info('[Main] Ignoring burst duplicate session deep link');
    return;
  }
  recordSessionDeepLinkSend(url);
  window.webContents.send(rendererEventChannels.openSharedSession, url);
}

async function createResumeChatWindow(resumeSessionId: string, dir?: string): Promise<boolean> {
  await createChat(app, { dir, resumeSessionId });
  return true;
}

async function deliverRendererProtocolUrl(
  url: string,
  parsedUrl: URL,
  openDir: string | undefined
): Promise<void> {
  const existingWindows = BrowserWindow.getAllWindows();
  let targetWindow: BrowserWindow | undefined;
  if (existingWindows.length > 0) {
    targetWindow = existingWindows[0];
    if (targetWindow.isMinimized()) {
      targetWindow.restore();
    }
    targetWindow.focus();
  } else {
    targetWindow = await createChat(app, { dir: openDir });
  }
  if (!targetWindow) return;
  if (targetWindow.webContents.isLoadingMainFrame()) {
    queuePendingDeepLink(targetWindow.id, url);
  } else {
    await processProtocolUrl(url, parsedUrl, targetWindow);
  }
}

async function handleProtocolUrl(url: string, parsedUrl: URL): Promise<boolean> {
  if (!url) return false;
  const openDir = firstGrantedRecentDirectory();
  return dispatchFullGoslingProtocolUrl(url, {
    openChat: async (options) => {
      await createChat(app, { dir: openDir, ...options });
    },
    resume: async (sessionId) => {
      await createResumeChatWindow(sessionId, openDir);
    },
    renderer: async () => {
      await deliverRendererProtocolUrl(url, parsedUrl, openDir);
    },
  });
}

async function processProtocolUrl(url: string, parsedUrl: URL, window: BrowserWindow) {
  if (parsedUrl.hostname === 'extension') {
    window.webContents.send(rendererEventChannels.addExtension, url);
  } else if (parsedUrl.hostname === 'sessions') {
    sendOpenSharedSession(window, url);
  }
}

app.on('open-url', async (_event, url) => {
  if (process.platform !== 'win32') {
    let parsedUrl: URL;
    try {
      parsedUrl = new URL(url);
    } catch (error) {
      log.warn('[Main] Ignoring invalid open-url protocol URL:', errorMessage(error));
      return;
    }

    log.info('[Main] Received open-url protocol action:', parsedUrl.hostname);

    const route = parseGoslingProtocolRoute(url);
    if (route && BrowserWindow.getAllWindows().length === 0) {
      openUrlHandledLaunch = true;
    }
    await app.whenReady();
    try {
      const handled = await handleProtocolUrl(url, parsedUrl);
      if (!handled) {
        openUrlHandledLaunch = false;
        log.warn('[Main] Ignoring unsupported open-url protocol action');
      }
    } catch (error) {
      log.error('[Main] Failed to handle open-url protocol URL:', errorMessage(error));
      if (BrowserWindow.getAllWindows().length === 0) {
        const { dirPath } = parseArgs();
        await createNewWindow(app, dirPath);
        openUrlHandledLaunch = true;
      }
    }
  }
});

// Handle macOS drag-and-drop onto dock icon
app.on('will-finish-launching', () => {
  if (process.platform === 'darwin') {
    app.setAboutPanelOptions({
      applicationName: 'Gosling',
      applicationVersion: app.getVersion(),
      credits: `Gosling v${app.getVersion()} — a fork of goose v1.38, a lighter version of goose.`,
    });
  }
});

// Handle drag-and-drop onto dock icon
app.on('open-file', async (event, filePath) => {
  event.preventDefault();
  await handleFileOpen(filePath);
});

// Handle multiple files/folders (macOS only)
if (process.platform === 'darwin') {
  // Use type assertion for non-standard Electron event
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  app.on('open-files' as any, async (event: any, filePaths: string[]) => {
    event.preventDefault();
    for (const filePath of filePaths) {
      await handleFileOpen(filePath);
    }
  });
}

async function handleFileOpen(filePath: string) {
  try {
    if (!filePath || typeof filePath !== 'string') {
      return;
    }

    const stats = fsSync.lstatSync(filePath);
    let targetDir = filePath;

    // If it's a file, use its parent directory
    if (stats.isFile()) {
      targetDir = path.dirname(filePath);
    }

    // Add to recent directories
    addRecentDir(targetDir);
    rendererDirectoryGrants.grantSelectedPath(0, targetDir);

    // Create new window for the directory
    const newWindow = await createChat(app, { dir: targetDir });

    // Focus the new window
    if (newWindow) {
      newWindow.show();
      newWindow.focus();
      newWindow.moveTop();
    }
  } catch (error) {
    console.error('Failed to handle file open:', error);

    // Show user-friendly error notification
    new Notification({
      title: 'Gosling',
      body: `Could not open directory: ${path.basename(filePath)}`,
    }).show();
  }
}

declare var MAIN_WINDOW_VITE_DEV_SERVER_URL: string;
declare var MAIN_WINDOW_VITE_NAME: string;

function getAppUrl(): URL {
  return MAIN_WINDOW_VITE_DEV_SERVER_URL
    ? new URL(MAIN_WINDOW_VITE_DEV_SERVER_URL)
    : pathToFileURL(path.join(__dirname, `../renderer/${MAIN_WINDOW_VITE_NAME}/index.html`));
}

// Parse command line arguments
const parseArgs = () => {
  let dirPath = null;

  // Remove first two elements in dev mode (electron and script path)
  const args = !dirPath && app.isPackaged ? process.argv : process.argv.slice(2);
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--dir' && i + 1 < args.length) {
      dirPath = args[i + 1];
      break;
    }
  }

  return { dirPath };
};

interface BundledConfig {
  defaultProvider?: string;
  defaultModel?: string;
  predefinedModels?: string;
  version?: string;
}

const getBundledConfig = (): BundledConfig => {
  //{env-macro-start}//
  //needed when gosling is bundled for a specific provider
  //{env-macro-end}//
  return {
    defaultProvider: process.env.GOSLING_DEFAULT_PROVIDER,
    defaultModel: process.env.GOSLING_DEFAULT_MODEL,
    predefinedModels: process.env.GOSLING_PREDEFINED_MODELS,
    version: process.env.GOSLING_VERSION,
  };
};

const { defaultProvider, defaultModel, predefinedModels, version } = getBundledConfig();

const resolveGoslingPathRoot = (): string | undefined => {
  const pathRoot = process.env.GOSLING_PATH_ROOT?.trim();
  if (pathRoot) {
    return expandTilde(pathRoot);
  }
  return undefined;
};

const GENERATED_SECRET = crypto.randomBytes(32).toString('hex');

interface ExternalBackend {
  source: 'env' | 'settings';
  url: string;
  secret: string;
  certFingerprint?: string;
}

const getExternalBackendUrlFromEnv = (): string | null => {
  if (!process.env.GOSLING_EXTERNAL_BACKEND) {
    return null;
  }

  const configuredUrl = process.env.GOSLING_EXTERNAL_BACKEND_URL?.trim();
  if (configuredUrl) {
    return configuredUrl;
  }

  return `http://127.0.0.1:${process.env.GOSLING_PORT || '3000'}`;
};

const getExternalBackendFromEnv = (): ExternalBackend | null => {
  const url = getExternalBackendUrlFromEnv();
  if (!url) {
    return null;
  }

  const secret = process.env.GOSLING_SERVER__SECRET_KEY;
  if (!secret) {
    throw new Error(
      'GOSLING_SERVER__SECRET_KEY must be set when using GOSLING_EXTERNAL_BACKEND. ' +
        'Set it to the same value on both the server and the desktop client.'
    );
  }

  return {
    source: 'env',
    url,
    secret,
  };
};

const getActiveExternalBackend = (settings: Settings): ExternalBackend | null => {
  const envBackend = getExternalBackendFromEnv();
  if (envBackend) {
    return envBackend;
  }

  if (settings.externalGoslingd?.enabled && settings.externalGoslingd.url) {
    if (!externalBackendSecret) {
      throw new Error(
        'Enter the external backend secret in Settings for this Gosling launch. It is intentionally not persisted in desktop settings.'
      );
    }
    return {
      source: 'settings',
      url: settings.externalGoslingd.url,
      secret: externalBackendSecret,
      certFingerprint: settings.externalGoslingd.certFingerprint,
    };
  }

  return null;
};

const getExternalBackendForCsp = (settings: Settings) => {
  const envUrl = getExternalBackendUrlFromEnv();
  if (!envUrl) {
    return settings.externalGoslingd;
  }

  return {
    ...settings.externalGoslingd,
    enabled: true,
    url: envUrl,
  };
};

let appConfig = {
  GOSLING_PLAYWRIGHT: process.env.ENABLE_PLAYWRIGHT === 'true',
  GOSLING_DEFAULT_PROVIDER: defaultProvider,
  GOSLING_DEFAULT_MODEL: defaultModel,
  GOSLING_PREDEFINED_MODELS: predefinedModels,
  GOSLING_PATH_ROOT: resolveGoslingPathRoot(),
  GOSLING_HOME_DIR: os.homedir(),
  GOSLING_WORKING_DIR: '',
  // Start with the env-var override; the OS region locale is filled in after app.ready
  // (see updateLocaleFromSystem below) since getSystemLocale() cannot be called earlier.
  GOSLING_LOCALE: process.env.GOSLING_LOCALE || undefined,
  // If GOSLING_ALLOWLIST_WARNING env var is not set, defaults to false (strict blocking mode)
  GOSLING_ALLOWLIST_WARNING: process.env.GOSLING_ALLOWLIST_WARNING === 'true',
  GOSLING_DISABLE_NOSTR_SHARING: process.env.GOSLING_DISABLE_NOSTR_SHARING === 'true',
};

const windowMap = new Map<number, BrowserWindow>();

const goslingServeLeases = new GoslingServeLeaseRegistry(log);

const windowPowerSaveBlockers = new Map<number, number>(); // windowId -> blockerId
const activeWakelockSessionsByWindow = new Map<number, Set<string>>();

function syncWindowPowerSaveBlocker(windowId: number): void {
  const activeSessions = activeWakelockSessionsByWindow.get(windowId);
  const shouldBlockSleep = getSettings().enableWakelock && (activeSessions?.size ?? 0) > 0;
  const blockerId = windowPowerSaveBlockers.get(windowId);

  if (shouldBlockSleep && blockerId === undefined) {
    try {
      windowPowerSaveBlockers.set(windowId, powerSaveBlocker.start('prevent-app-suspension'));
    } catch (error) {
      log.error('Failed to start task power save blocker', { windowId, error });
    }
    return;
  }

  if (!shouldBlockSleep && blockerId !== undefined) {
    try {
      powerSaveBlocker.stop(blockerId);
    } catch (error) {
      log.error('Failed to stop task power save blocker', { windowId, blockerId, error });
    }
    windowPowerSaveBlockers.delete(windowId);
  }
}

function clearWindowWakelock(windowId: number): void {
  activeWakelockSessionsByWindow.delete(windowId);
  syncWindowPowerSaveBlocker(windowId);
}

function clearAllWakelocks(): void {
  for (const windowId of new Set([
    ...activeWakelockSessionsByWindow.keys(),
    ...windowPowerSaveBlockers.keys(),
  ])) {
    clearWindowWakelock(windowId);
  }
}

// Track pending initial messages per window
const pendingInitialMessages = new Map<number, string>(); // windowId -> initialMessage
const pendingInitialMessageNoAutoSubmit = new Set<number>(); // windowIds whose initialMessage should NOT auto-submit

interface CreateChatOptions {
  initialMessage?: string;
  initialMessageNoAutoSubmit?: boolean;
  dir?: string;
  resumeSessionId?: string;
  viewType?: string;
}

const createChat = async (
  app: App,
  options: CreateChatOptions = {}
): Promise<BrowserWindow | undefined> => {
  const { initialMessage, initialMessageNoAutoSubmit, dir, resumeSessionId, viewType } = options;
  const settings = getSettings();

  let externalBackend: ExternalBackend | null;
  try {
    externalBackend = getActiveExternalBackend(settings);
  } catch (error) {
    dialog.showMessageBoxSync({
      type: 'error',
      title: 'External Backend Misconfigured',
      message: 'The external backend environment is invalid.',
      detail: errorMessage(error),
      buttons: ['Quit'],
    });
    app.quit();
    return;
  }

  if (externalBackend?.certFingerprint) {
    const url = externalBackend.url;
    const usesHttps = (() => {
      try {
        return new URL(url).protocol === 'https:';
      } catch {
        return false;
      }
    })();

    if (!usesHttps) {
      const response = dialog.showMessageBoxSync({
        type: 'error',
        title: 'External Backend Misconfigured',
        message: 'Certificate fingerprint requires an HTTPS external backend URL.',
        detail: 'Use an https:// URL or remove the configured certificate fingerprint.',
        buttons: ['Disable External Backend & Retry', 'Quit'],
        defaultId: 0,
        cancelId: 1,
      });

      if (response === 0) {
        updateSettings((s) => {
          if (s.externalGoslingd) {
            s.externalGoslingd.enabled = false;
          }
        });
        return createChat(app, options);
      }

      app.quit();
      return;
    }
  }

  const serverSecret = externalBackend ? externalBackend.secret : GENERATED_SECRET;
  let workingDir = dir || os.homedir();
  let goslingServeLease: GoslingServeLease | null = null;

  if (externalBackend) {
    let externalCertificateTrust: BackendCertificateTrustRegistration | null = null;

    try {
      const externalBaseUrl = normalizeAcpHttpBaseUrl(externalBackend.url);
      const externalBase = new URL(externalBaseUrl);
      if (externalBase.protocol === 'https:') {
        externalCertificateTrust = trustBackendCertificate(
          externalBase.hostname,
          externalBackend.certFingerprint ?? null
        );
      }

      const externalBackendReady = await checkBackendStatus({
        baseUrl: externalBaseUrl,
        serverSecret,
        fetch: net.fetch as unknown as typeof globalThis.fetch,
      });
      if (!externalBackendReady) {
        externalCertificateTrust?.release();
        const canDisableExternalBackend = externalBackend.source === 'settings';
        const response = dialog.showMessageBoxSync({
          type: 'error',
          title: 'External Backend Unreachable',
          message: `Could not connect to external backend at ${externalBaseUrl}`,
          detail:
            'The external backend must be running and the configured secret must match GOSLING_SERVER__SECRET_KEY on the server.',
          buttons: canDisableExternalBackend
            ? ['Disable External Backend & Retry', 'Quit']
            : ['Quit'],
          defaultId: 0,
          cancelId: canDisableExternalBackend ? 1 : 0,
        });

        if (canDisableExternalBackend && response === 0) {
          updateSettings((s) => {
            if (s.externalGoslingd) {
              s.externalGoslingd.enabled = false;
            }
          });
          return createChat(app, options);
        }

        app.quit();
        return;
      }

      const leaseCertificateTrust = externalCertificateTrust;
      externalCertificateTrust = null;
      goslingServeLease = goslingServeLeases.createExternal(
        acpWebSocketUrlFromHttpBase(externalBaseUrl, serverSecret),
        serverSecret,
        leaseCertificateTrust ? async () => leaseCertificateTrust.release() : undefined
      );
    } catch (error) {
      externalCertificateTrust?.release();
      log.error('External ACP backend is misconfigured', error);
      const canDisableExternalBackend = externalBackend.source === 'settings';
      const response = dialog.showMessageBoxSync({
        type: 'error',
        title: 'External Backend Misconfigured',
        message: 'The external backend URL is invalid.',
        detail: errorMessage(error),
        buttons: canDisableExternalBackend
          ? ['Disable External Backend & Retry', 'Quit']
          : ['Quit'],
        defaultId: 0,
        cancelId: canDisableExternalBackend ? 1 : 0,
      });

      if (canDisableExternalBackend && response === 0) {
        updateSettings((s) => {
          if (s.externalGoslingd) {
            s.externalGoslingd.enabled = false;
          }
        });
        return createChat(app, options);
      }

      app.quit();
      return;
    }
  } else {
    const useLocalBackendTls = !app.isPackaged;
    const localCertificateTrust = useLocalBackendTls
      ? trustBackendCertificate('127.0.0.1', null)
      : null;

    let goslingServeResult: Awaited<ReturnType<typeof startGoslingServe>>;
    try {
      goslingServeResult = await startGoslingServe({
        serverSecret,
        dir: workingDir,
        tls: useLocalBackendTls,
        env: {
          GOSLING_PATH_ROOT: appConfig.GOSLING_PATH_ROOT as string | undefined,
        },
        isPackaged: app.isPackaged,
        resourcesPath: app.isPackaged ? process.resourcesPath : undefined,
        logger: log,
        diagnosticsDir: STARTUP_LOGS_DIR,
        processRegistryPath: BACKEND_PROCESS_REGISTRY_PATH,
        readinessFetch: globalThis.fetch,
        usePinnedTlsReadiness: useLocalBackendTls,
      });
      if (useLocalBackendTls && !goslingServeResult.certFingerprint) {
        await goslingServeResult.cleanup();
        throw new Error(
          'gosling serve started with TLS but did not return a certificate fingerprint'
        );
      }

      if (useLocalBackendTls && goslingServeResult.certFingerprint && localCertificateTrust) {
        const localCertFingerprint = normalizeFingerprint(goslingServeResult.certFingerprint);
        if (
          localCertificateTrust.trust.fingerprint &&
          localCertificateTrust.trust.fingerprint !== localCertFingerprint
        ) {
          await goslingServeResult.cleanup();
          throw new Error(
            'gosling serve TLS certificate fingerprint did not match readiness probe'
          );
        }
        localCertificateTrust.trust.fingerprint = localCertFingerprint;
        installBackendCertificateVerifier(session.fromPartition(MAIN_WINDOW_SESSION_PARTITION));
      }
    } catch (error) {
      localCertificateTrust?.release();
      log.error('gosling serve failed to start', error);
      dialog.showMessageBoxSync({
        type: 'error',
        title: 'Gosling Failed to Start',
        message: 'The backend server failed to start.',
        detail: [
          'Backend: gosling serve',
          'Readiness check: HTTPS GET /status',
          `Startup error:\n${errorMessage(error)}`,
        ].join('\n\n'),
        buttons: ['OK'],
      });
      app.quit();
      return;
    }

    workingDir = goslingServeResult.workingDir;
    const cleanupGoslingServe = goslingServeResult.cleanup;
    goslingServeResult.cleanup = async () => {
      try {
        await cleanupGoslingServe();
      } finally {
        localCertificateTrust?.release();
      }
    };
    goslingServeLease = goslingServeLeases.create(goslingServeResult, serverSecret);
  }

  const cleanupUnregisteredGoslingServeLease = async () => {
    if (!goslingServeLease) {
      return;
    }

    const lease = goslingServeLease;
    goslingServeLease = null;
    await goslingServeLeases.cleanupLease(lease);
  };

  let mainWindowState: ReturnType<typeof windowStateKeeper>;
  let mainWindow: BrowserWindow;
  try {
    mainWindowState = windowStateKeeper({
      defaultWidth: 940,
      defaultHeight: 800,
    });

    mainWindow = new BrowserWindow({
      show: false,
      titleBarStyle: process.platform === 'darwin' ? 'hidden' : 'default',
      trafficLightPosition: process.platform === 'darwin' ? { x: 20, y: 16 } : undefined,
      vibrancy: process.platform === 'darwin' ? 'window' : undefined,
      frame: process.platform !== 'darwin',
      // windowStateKeeper persists the outer window bounds (getBounds), so the
      // window must be restored by outer bounds too. With useContentSize the saved
      // outer height is reapplied as the content height, growing the window by the
      // frame height on every launch on framed platforms (#9363).
      x: mainWindowState.x,
      y: mainWindowState.y,
      width: mainWindowState.width,
      height: mainWindowState.height,
      minWidth: 480,
      minHeight: 400,
      resizable: true,
      icon: path.join(
        __dirname,
        process.platform === 'win32' ? '../images/icon.ico' : '../images/icon.png'
      ),
      webPreferences: {
        spellcheck: settings.spellcheckEnabled ?? true,
        preload: path.join(__dirname, 'preload.js'),
        webSecurity: true,
        nodeIntegration: false,
        contextIsolation: true,
        // Electron's default, pinned explicitly so the posture is declared
        // rather than inherited -- shellHost.ts already pins it, and this
        // window's preload uses no Node APIs. (SECN-GSL-003)
        sandbox: true,
        additionalArguments: [
          JSON.stringify({
            ...appConfig,
            GOSLING_LOCALE: getConfiguredGoslingLocale(),
            GOSLING_WORKING_DIR: workingDir,
            REQUEST_DIR: dir,
            GOSLING_VERSION: version,
            SECURITY_ML_MODEL_MAPPING: process.env.SECURITY_ML_MODEL_MAPPING,
            SECURITY_PROMPT_ENABLED_OVERRIDE: process.env.SECURITY_PROMPT_ENABLED_OVERRIDE,
            SECURITY_COMMAND_CLASSIFIER_ENABLED_OVERRIDE:
              process.env.SECURITY_COMMAND_CLASSIFIER_ENABLED_OVERRIDE,
          }),
        ],
        partition: MAIN_WINDOW_SESSION_PARTITION,
      },
    });
    rendererDirectoryGrants.grantSelectedPath(mainWindow.webContents.id, workingDir, false);
    if (settings.archiveFolder) {
      try {
        rendererDirectoryGrants.grantSelectedPath(
          mainWindow.webContents.id,
          settings.archiveFolder,
          false
        );
      } catch (error) {
        // The configured folder may have been moved or deleted since it was picked; the user is
        // re-prompted to choose an archive folder rather than the window failing to open.
        console.error('Failed to re-grant the configured archive folder:', error);
      }
    }
    installBackendCertificateVerifier(mainWindow.webContents.session);
    installArtifactDownloadRouter(
      mainWindow.webContents.session,
      (webContentsId) => artifactRoutingRegistry.get(webContentsId),
      (webContentsId, fileName) => {
        const target = BrowserWindow.getAllWindows().find(
          (window) => window.webContents.id === webContentsId
        );
        target?.webContents.send(rendererEventChannels.artifactDownloadUnrouted, fileName);
      }
    );
  } catch (error) {
    await cleanupUnregisteredGoslingServeLease();
    throw error;
  }

  if (goslingServeLease) {
    const lease = goslingServeLease;
    mainWindow.once('closed', () => {
      void goslingServeLeases.releaseWindow(mainWindow.id);
    });
    goslingServeLeases.attachWindow(mainWindow.id, lease);
    goslingServeLease = null;
  }

  if (!app.isPackaged) {
    installExtension(REACT_DEVELOPER_TOOLS, {
      loadExtensionOptions: { allowFileAccess: true },
      session: mainWindow.webContents.session,
    })
      .then(() => log.info('added react dev tools'))
      .catch((err) => log.info('failed to install react dev tools:', err));
  }

  // Let windowStateKeeper manage the window
  mainWindowState.manage(mainWindow);

  mainWindow.webContents.session.setSpellCheckerLanguages(['en-US', 'en-GB']);
  mainWindow.webContents.on('context-menu', (_event, params) => {
    const menu = new Menu();
    const hasSpellingSuggestions = params.dictionarySuggestions.length > 0 || params.misspelledWord;

    if (hasSpellingSuggestions) {
      for (const suggestion of params.dictionarySuggestions) {
        menu.append(
          new MenuItem({
            label: suggestion,
            click: () => mainWindow.webContents.replaceMisspelling(suggestion),
          })
        );
      }

      if (params.misspelledWord) {
        menu.append(
          new MenuItem({
            label: menuT('Add to dictionary'),
            click: () =>
              mainWindow.webContents.session.addWordToSpellCheckerDictionary(params.misspelledWord),
          })
        );
      }

      if (params.selectionText) {
        menu.append(new MenuItem({ type: 'separator' }));
      }
    }
    if (params.selectionText) {
      menu.append(
        new MenuItem({
          label: menuT('Cut'),
          accelerator: 'CmdOrCtrl+X',
          role: 'cut',
        })
      );
      menu.append(
        new MenuItem({
          label: menuT('Copy'),
          accelerator: 'CmdOrCtrl+C',
          role: 'copy',
        })
      );
    }

    // Only show paste in editable fields (text inputs)
    if (params.isEditable) {
      menu.append(
        new MenuItem({
          label: menuT('Paste'),
          accelerator: 'CmdOrCtrl+V',
          role: 'paste',
        })
      );
    }

    if (menu.items.length > 0) {
      menu.popup();
    }
  });

  // Handle new window creation for links (fallback for any links not handled by onClick)
  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    void openExternalIfSafe(url);
    return { action: 'deny' };
  });
  mainWindow.webContents.on('will-navigate', (event) => {
    blockTopLevelNavigation(event);
  });

  // Handle new-window events (alternative approach for external links)
  // Use type assertion for non-standard Electron event
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  mainWindow.webContents.on('new-window' as any, function (event: any, url: string) {
    event.preventDefault();
    void openExternalIfSafe(url);
  });

  const windowId = mainWindow.id;
  const webContentsId = mainWindow.webContents.id;
  mainWindow.webContents.once('destroyed', () => {
    artifactRoutingRegistry.clear(webContentsId);
    rendererArtifactFileGrants.delete(webContentsId);
    rendererDirectoryGrants.clearTransient(webContentsId);
  });
  const url = getAppUrl();

  let appPath = '/';
  const routeMap: Record<string, string> = {
    chat: '/',
    research: '/research',
    pair: '/pair',
    settings: '/settings',
    sessions: '/sessions',
    skills: '/skills',
    permission: '/permission',
    ConfigureProviders: '/configure-providers',
  };

  if (viewType) {
    appPath = routeMap[viewType] || '/';
  }
  if (appPath === '/' && initialMessage) {
    appPath = '/pair';
  }

  let searchParams = new URLSearchParams();
  if (resumeSessionId) {
    searchParams.set('resumeSessionId', resumeSessionId);
    if (appPath === '/') {
      appPath = '/pair';
    }
  }

  // Gosling's react app uses HashRouter, so the path + search params follow a #/
  url.hash = `${appPath}?${searchParams.toString()}`;
  let formattedUrl = formatUrl(url);
  log.info('Opening URL: ', formattedUrl);
  mainWindow.once('ready-to-show', () => {
    if (!mainWindow.isDestroyed()) {
      mainWindow.show();
    }
  });
  mainWindow.loadURL(formattedUrl);

  // If we have an initial message, store it to send after React is ready
  if (initialMessage) {
    pendingInitialMessages.set(mainWindow.id, initialMessage);
    if (initialMessageNoAutoSubmit) {
      pendingInitialMessageNoAutoSubmit.add(mainWindow.id);
    }
  }

  // Set up local keyboard shortcuts that only work when the window is focused
  mainWindow.webContents.on('before-input-event', (event, input) => {
    if (input.key === 'r' && input.meta) {
      mainWindow.reload();
      event.preventDefault();
    }

    if (input.key === 'i' && input.alt && input.meta) {
      mainWindow.webContents.openDevTools();
      event.preventDefault();
    }
  });

  mainWindow.on('app-command', (e, cmd) => {
    if (cmd === 'browser-backward') {
      mainWindow.webContents.send(rendererEventChannels.mouseBackButtonClicked);
      e.preventDefault();
    }
  });

  const broadcastFullScreenState = () => {
    if (!mainWindow.isDestroyed()) {
      mainWindow.webContents.send(
        rendererEventChannels.fullscreenChange,
        mainWindow.isFullScreen()
      );
    }
  };
  mainWindow.on('enter-full-screen', broadcastFullScreenState);
  mainWindow.on('leave-full-screen', broadcastFullScreenState);

  // Handle mouse back button (button 3)
  // Use type assertion for non-standard Electron event
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  mainWindow.webContents.on('mouse-up' as any, function (_event: any, mouseButton: number) {
    // MouseButton 3 is the back button.
    if (mouseButton === 3) {
      mainWindow.webContents.send(rendererEventChannels.mouseBackButtonClicked);
    }
  });

  windowMap.set(windowId, mainWindow);

  // Handle window closure
  mainWindow.on('closed', () => {
    windowMap.delete(windowId);

    pendingInitialMessages.delete(windowId);
    pendingInitialMessageNoAutoSubmit.delete(windowId);
    pendingDeepLinks.delete(windowId);
    reactReadyWindows.delete(windowId);

    clearWindowWakelock(windowId);
  });
  return mainWindow;
};

const {
  createLauncher,
  destroyTray,
  createTray,
  buildRecentFilesMenu,
  openDirectoryDialog,
  hasTray,
} = createWindowChrome({
  app,
  appConfig,
  getConfiguredGoslingLocale,
  getAppUrl,
  reactReadyWindows,
  updateSettings,
  createChat,
  firstGrantedRecentDirectory,
  rendererDirectoryGrants,
  log,
});

// Global error handler. Must never throw itself: it runs from
// uncaughtException/unhandledRejection, and a window mid-teardown would turn
// one error into a crash loop.
const handleFatalError = (error: Error) => {
  const windows = BrowserWindow.getAllWindows();
  windows.forEach((win) => {
    try {
      if (!win.isDestroyed() && !win.webContents.isDestroyed()) {
        win.webContents.send(
          rendererEventChannels.fatalError,
          error.message || 'An unexpected error occurred'
        );
      }
    } catch (sendError) {
      console.error('Failed to notify window of fatal error:', sendError);
    }
  });
};

process.on('uncaughtException', (error) => {
  console.error('Uncaught Exception:', formatErrorForLogging(error));
  handleFatalError(error);
});

process.on('unhandledRejection', (error) => {
  console.error('Unhandled Rejection:', formatErrorForLogging(error));
  handleFatalError(error instanceof Error ? error : new Error(String(error)));
});

registerRendererIpcHandlers(ipcMain, {
  log,
  pendingInitialMessages,
  pendingInitialMessageNoAutoSubmit,
  pendingDeepLinks,
  reactReadyWindows,
  sendOpenSharedSession,
  openExternalIfSafe,
  rendererDirectoryGrants,
  assertRendererFileAccess,
  goslingServeLeases,
});

registerSettingsIpcHandlers(ipcMain, {
  app,
  getSettings,
  updateSettings,
  getExternalBackendSecret: () => externalBackendSecret,
  setExternalBackendSecret: (secret) => {
    externalBackendSecret = secret;
  },
  updateConfiguredLocale: () => {
    appConfig.GOSLING_LOCALE = getConfiguredGoslingLocale();
  },
  registerGlobalShortcuts,
  setAutoDownloadDisabled,
  rendererDirectoryGrants,
});

registerSystemIpcHandlers(ipcMain, {
  app,
  getSettings,
  updateSettings,
  createTray,
  destroyTray,
  focusWindow,
  activeWakelockSessionsByWindow,
  syncWindowPowerSaveBlocker,
});

registerFileIpcHandlers(ipcMain, {
  assertRendererFileAccess,
  assertRendererArtifactFileAccess,
  resolveRendererPath,
  grantRendererDirectory: (webContentsId, filePath) => {
    rendererDirectoryGrants.grantSelectedPath(webContentsId, filePath);
  },
  grantRendererArtifactFile: (webContentsId, filePath) => {
    const grants = rendererArtifactFileGrants.get(webContentsId) ?? new Set<string>();
    grants.add(filePath);
    rendererArtifactFileGrants.set(webContentsId, grants);
  },
  updateArtifactRoutingConfig: (webContentsId, config) =>
    artifactRoutingRegistry.update(webContentsId, config, (candidate) =>
      validateArtifactRoutingConfig(webContentsId, candidate)
    ),
  getAllowList,
});

const createNewWindow = async (app: App, dir?: string | null) => {
  const openDir = dir || firstGrantedRecentDirectory();
  return await createChat(app, { dir: openDir });
};

function focusWindow(): void {
  const windows = BrowserWindow.getAllWindows();
  if (windows.length > 0) {
    windows.forEach((win) => {
      win.show();
    });
    windows[windows.length - 1].webContents.send(rendererEventChannels.focusInput);
  } else {
    createNewWindow(app);
  }
}

function registerGlobalShortcuts(): void {
  globalShortcut.unregisterAll();

  const settings = getSettings();
  const shortcuts = getKeyboardShortcuts(settings);

  if (shortcuts.focusWindow) {
    try {
      globalShortcut.register(shortcuts.focusWindow, () => {
        focusWindow();
      });
    } catch (e) {
      console.error('Error registering focus window hotkey:', e);
    }
  }

  if (shortcuts.quickLauncher) {
    try {
      globalShortcut.register(shortcuts.quickLauncher, () => {
        createLauncher();
      });
    } catch (e) {
      console.error('Error registering launcher hotkey:', e);
    }
  }
}

async function appMain() {
  if (shouldQuitForSingleInstance) {
    return;
  }

  await configureProxy();

  // Ensure Windows shims are available before any MCP processes are spawned
  await ensureWinShims();

  try {
    await cleanupRecordedBackendProcesses(BACKEND_PROCESS_REGISTRY_PATH, log);
  } catch (error) {
    log.error('Failed to clean up stale gosling serve processes:', error);
  }

  registerUpdateIpcHandlers();

  // Handle microphone permission requests
  session
    .fromPartition(MAIN_WINDOW_SESSION_PARTITION)
    .setPermissionRequestHandler((_webContents, permission, callback) => {
      console.log('Permission requested:', permission);
      callback(permission === 'media');
    });

  // Add CSP headers to all sessions, recomputed on every response so external
  // backend settings take effect without restarting the app.
  session
    .fromPartition(MAIN_WINDOW_SESSION_PARTITION)
    .webRequest.onHeadersReceived((details, callback) => {
      const currentSettings = getSettings();
      const webContentsId = (details as { webContentsId?: number }).webContentsId;
      let localAcpUrl: string | null = null;
      try {
        // Leases are keyed by `BrowserWindow.id`, but this handler only has a
        // *webContents* id — a different id space. Passing it straight through
        // meant the lookup essentially never matched, so the CSP was built
        // without the local ACP origin and the renderer's own backend was
        // blocked by policy rather than allowed. Resolve the window first, the
        // same way every other lease lookup in this file does.
        // (ARCN-GSL-001)
        const windowId =
          typeof webContentsId === 'number'
            ? (BrowserWindow.fromWebContents(webContents.fromId(webContentsId)!)?.id ?? null)
            : null;
        localAcpUrl = windowId !== null ? goslingServeLeases.getAcpUrl(windowId) : null;
      } catch {
        localAcpUrl = null;
      }
      callback({
        responseHeaders: {
          ...details.responseHeaders,
          'Content-Security-Policy': buildCSP(
            getExternalBackendForCsp(currentSettings),
            localAcpUrl
          ),
        },
      });
    });

  // Migrate old settings format if needed (one-time migration)
  const settings = getSettings();
  if (!settings.keyboardShortcuts && settings.globalShortcut !== undefined) {
    updateSettings((s) => {
      s.keyboardShortcuts = getKeyboardShortcuts(s);
      delete s.globalShortcut;
    });
  }

  // Register global shortcuts based on settings
  registerGlobalShortcuts();

  session
    .fromPartition(MAIN_WINDOW_SESSION_PARTITION)
    .webRequest.onBeforeSendHeaders((details, callback) => {
      const overrideOrigin = getOverrideOriginForRequest(
        details.url,
        MAIN_WINDOW_VITE_DEV_SERVER_URL
      );
      if (overrideOrigin) {
        details.requestHeaders.Origin = overrideOrigin;
      }
      callback({ cancel: false, requestHeaders: details.requestHeaders });
    });

  if (settings.showMenuBarIcon) {
    createTray();
  }

  if (process.platform === 'darwin' && !settings.showDockIcon && settings.showMenuBarIcon) {
    app.dock?.hide();
  }

  const { dirPath } = parseArgs();

  if (!openUrlHandledLaunch) {
    await createNewWindow(app, dirPath);
  } else {
    log.info('[Main] Skipping window creation in appMain - open-url already handled launch');
  }

  // Setup auto-updater AFTER window is created and displayed (with delay to avoid blocking)
  setTimeout(() => {
    if (shouldSetupUpdater()) {
      log.info('Setting up auto-updater after window creation...');
      try {
        const settings = getSettings();
        if (settings.disableAutoDownload) {
          setAutoDownloadDisabled(true);
        }
        setupAutoUpdater();
      } catch (error) {
        log.error('Error setting up auto-updater:', error);
      }
    }
  }, 2000);

  if (process.platform === 'darwin') {
    const dockMenu = Menu.buildFromTemplate([
      {
        label: menuT('New Window'),
        click: () => {
          createNewWindow(app);
        },
      },
    ]);
    app.dock?.setMenu(dockMenu);
  }

  installApplicationMenu({
    app,
    settings,
    bundledVersion: version,
    menuT,
    translateMenuLabels,
    createNewWindow: () => createNewWindow(app),
    openDirectoryDialog,
    buildRecentFilesMenu,
    focusWindow,
    createLauncher,
  });

  registerAppIpcHandlers(ipcMain, {
    app,
    createNewWindow: () => createNewWindow(app),
    createChat: (options) => createChat(app, options),
    assertRendererFileAccess,
    firstGrantedRecentDirectory,
    getConfiguredGoslingLocale,
    log,
  });
}

app.whenReady().then(async () => {
  try {
    await appMain();
    if (
      legacySecretRemovalNoticePending ||
      externalSecretPersistenceNoticePending ||
      settingsRecoveryNoticePending
    ) {
      const details = [];
      if (settingsRecoveryNoticePending) {
        details.push(
          'Desktop settings were malformed or interrupted. Gosling recovered the previous-good snapshot when available and otherwise loaded safe defaults.'
        );
      }
      if (legacySecretRemovalNoticePending) {
        details.push(
          'Legacy Local Secret Profiles were removed because they stored values in plaintext and inserted them into model prompts. Configure provider or workspace credential profiles instead.'
        );
      }
      if (externalSecretPersistenceNoticePending) {
        details.push(
          'The external backend secret was removed from settings.json. It remains available only for this launch and must be re-entered after restarting Gosling.'
        );
      }
      await dialog.showMessageBox({
        type: 'warning',
        title: 'Credential security update',
        message: 'Gosling removed insecure persisted credential data.',
        detail: details.join('\n\n'),
        buttons: ['OK'],
      });
      legacySecretRemovalNoticePending = false;
      externalSecretPersistenceNoticePending = false;
      settingsRecoveryNoticePending = false;
    }
  } catch (error) {
    dialog.showErrorBox('Gosling Error', `Failed to create main window: ${error}`);
    app.quit();
  }
});

let shutdownCleanupPromise: Promise<void> | null = null;
let shutdownCleanupComplete = false;

async function runShutdownCleanup(): Promise<void> {
  const goslingServeLeaseCount = goslingServeLeases.activeLeaseCount();
  if (goslingServeLeaseCount > 0) {
    log.info(`App quitting, cleaning up ${goslingServeLeaseCount} backend lease(s)`);
    await goslingServeLeases.cleanupAll();
  }

  try {
    await cleanupRecordedBackendProcesses(BACKEND_PROCESS_REGISTRY_PATH, log);
  } catch (error) {
    log.error('Failed to clean up recorded gosling serve processes during quit:', error);
  }

  clearAllWakelocks();

  globalShortcut.unregisterAll();
}

function scheduleShutdownCleanup(event: { preventDefault: () => void }): void {
  if (shutdownCleanupComplete) {
    return;
  }

  event.preventDefault();
  if (!shutdownCleanupPromise) {
    shutdownCleanupPromise = runShutdownCleanup();
  }

  void shutdownCleanupPromise.finally(() => {
    shutdownCleanupComplete = true;
    app.exit(0);
  });
}

app.on('before-quit', scheduleShutdownCleanup);
app.on('will-quit', scheduleShutdownCleanup);

app.on('window-all-closed', () => {
  // Only quit if we're not on macOS or don't have a tray icon
  if (process.platform !== 'darwin' || !hasTray()) {
    app.quit();
  }
});
