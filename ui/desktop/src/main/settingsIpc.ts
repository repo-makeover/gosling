// Owns renderer settings and research-library IPC handlers.
// Extracted from ui/desktop/src/main.ts in a behavior-preserving modularization.
// The compatibility facade imports registerSettingsIpcHandlers; it re-exports none.

import type { App, IpcMain } from 'electron';
import { dialog } from 'electron';
import fs from 'node:fs/promises';
import path from 'node:path';
import { getGitRepoRoot, isPathGitIgnored } from './gitIpc';
import type { RendererDirectoryGrantRegistry } from '../utils/rendererDirectoryGrants';
import { defaultResearchLibraryPath, listResearchLibraryFiles } from '../utils/researchLibrary';
import type { Settings, SettingKey } from '../utils/settings';
import { isSettingKey, isSettingValue, setSettingValue } from '../utils/settings';

// A research library folder is gosling's own scratch space, not something the
// user's repo should track by default — mirrors ensure_gitignored_if_in_repo
// in crates/gosling/src/workspace/service.rs for workspace output folders.
export async function ensureGitignoredIfInRepo(directoryPath: string): Promise<void> {
  const repoRoot = await getGitRepoRoot(directoryPath);
  if (!repoRoot) return;
  if (await isPathGitIgnored(repoRoot, directoryPath)) return;

  const parent = path.dirname(directoryPath);
  const entry = `/${path.basename(directoryPath)}/`;
  const gitignorePath = path.join(parent, '.gitignore');
  let contents = '';
  try {
    contents = await fs.readFile(gitignorePath, 'utf8');
  } catch {
    contents = '';
  }
  if (contents.split('\n').some((line) => line.trim() === entry)) return;
  if (contents && !contents.endsWith('\n')) contents += '\n';
  try {
    await fs.writeFile(gitignorePath, `${contents}${entry}\n`, 'utf8');
  } catch (error) {
    console.warn('Failed to add research library folder to .gitignore:', error);
  }
}

export interface SettingsIpcDependencies {
  app: App;
  getSettings: () => Settings;
  updateSettings: (modifier: (settings: Settings) => void) => void;
  getExternalBackendSecret: () => string;
  setExternalBackendSecret: (secret: string) => void;
  updateConfiguredLocale: () => void;
  registerGlobalShortcuts: () => void;
  setAutoDownloadDisabled: (disabled: boolean) => void;
  rendererDirectoryGrants: RendererDirectoryGrantRegistry;
}

export const SETTINGS_IPC_CHANNELS = [
  'get-setting',
  'get-settings',
  'set-setting',
  'get-research-library-path',
  'choose-research-library-path',
  'list-research-library-files',
] as const;

export function registerSettingsIpcHandlers(
  targetIpcMain: Pick<IpcMain, 'handle'>,
  dependencies: SettingsIpcDependencies
): void {
  const {
    app,
    getSettings,
    updateSettings,
    getExternalBackendSecret,
    setExternalBackendSecret,
    updateConfiguredLocale,
    registerGlobalShortcuts,
    setAutoDownloadDisabled,
    rendererDirectoryGrants,
  } = dependencies;

  function rendererSettingValue(settings: Settings, key: SettingKey): Settings[SettingKey] {
    if (key === 'externalGoslingd') {
      return {
        ...settings.externalGoslingd,
        secret: '',
        secretConfigured: getExternalBackendSecret().length > 0,
      };
    }
    return settings[key];
  }

  function configuredResearchLibraryPath(): string {
    return path.resolve(
      getSettings().researchLibraryPath ?? defaultResearchLibraryPath(app.getPath('documents'))
    );
  }

  async function ensureResearchLibrary(rendererId: number): Promise<string> {
    const libraryPath = configuredResearchLibraryPath();
    await fs.mkdir(libraryPath, { recursive: true });
    await ensureGitignoredIfInRepo(libraryPath);
    rendererDirectoryGrants.grantSelectedPath(rendererId, libraryPath);
    return libraryPath;
  }

  targetIpcMain.handle('get-setting', (_event, key: unknown) => {
    if (!isSettingKey(key)) throw new Error('Invalid setting key');
    return rendererSettingValue(getSettings(), key);
  });
  targetIpcMain.handle('get-settings', (_event, keys: unknown) => {
    if (!Array.isArray(keys) || keys.length > 64 || !keys.every(isSettingKey)) {
      throw new Error('Invalid settings key list');
    }
    const settings = getSettings();
    const values: Record<string, unknown> = {};
    for (const key of keys) values[key] = rendererSettingValue(settings, key);
    return values;
  });
  targetIpcMain.handle('set-setting', (_event, key: unknown, value: unknown) => {
    if (!isSettingKey(key)) throw new Error('Invalid setting key');
    if (key === 'researchLibraryPath') {
      throw new Error('Research Library location must be changed with the native folder chooser');
    }
    if (key === 'externalGoslingd') {
      if (!isSettingValue('externalGoslingd', value)) throw new Error('Invalid setting value');
      if (value.secret) setExternalBackendSecret(value.secret);
      const persistedValue: Settings['externalGoslingd'] = {
        ...value,
        secret: '',
        secretConfigured: getExternalBackendSecret().length > 0,
      };
      updateSettings((settings) => setSettingValue(settings, 'externalGoslingd', persistedValue));
    } else {
      if (!isSettingValue(key, value)) throw new Error('Invalid setting value');
      updateSettings((settings) => setSettingValue(settings, key, value as Settings[typeof key]));
    }
    if (key === 'language') updateConfiguredLocale();
    // Re-register shortcuts if keyboard shortcuts changed
    if (key === 'keyboardShortcuts') registerGlobalShortcuts();
    if (key === 'disableAutoDownload') setAutoDownloadDisabled(value === true);
  });
  targetIpcMain.handle('get-research-library-path', async (event) =>
    ensureResearchLibrary(event.sender.id)
  );
  targetIpcMain.handle('choose-research-library-path', async (event) => {
    const currentPath = await ensureResearchLibrary(event.sender.id);
    const result = await dialog.showOpenDialog({
      properties: ['openDirectory', 'createDirectory'],
      defaultPath: currentPath,
      title: 'Choose Research Library',
    });
    const selectedPath = result.canceled ? undefined : result.filePaths[0];
    if (!selectedPath) return null;
    const resolvedPath = path.resolve(selectedPath);
    updateSettings((settings) => setSettingValue(settings, 'researchLibraryPath', resolvedPath));
    await fs.mkdir(resolvedPath, { recursive: true });
    await ensureGitignoredIfInRepo(resolvedPath);
    rendererDirectoryGrants.grantSelectedPath(event.sender.id, resolvedPath);
    return resolvedPath;
  });
  targetIpcMain.handle('list-research-library-files', async (event) => {
    const libraryPath = await ensureResearchLibrary(event.sender.id);
    return listResearchLibraryFiles(libraryPath, getSettings().outputFileExtensions);
  });
}
