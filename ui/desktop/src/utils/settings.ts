export interface ExternalGoslingdConfig {
  enabled: boolean;
  url: string;
  secret: string;
  secretConfigured?: boolean;
  certFingerprint?: string;
}

export type RecentModel = {
  provider: string;
  model: string;
};

export const defaultOutputFileExtensions = [
  'pdf',
  'md',
  'txt',
  'doc',
  'docx',
  'jpg',
  'png',
  'yaml',
  'json',
  'markdown',
  'csv',
  'tsv',
  'rtf',
  'odt',
  'xlsx',
  'pptx',
  'html',
  'htm',
  'jpeg',
  'svg',
  'webp',
] as const;

export const OUTPUT_FILE_EXTENSIONS_CHANGED_EVENT = 'outputFileExtensionsChanged';

export interface KeyboardShortcuts {
  focusWindow: string | null;
  quickLauncher: string | null;
  newChat: string | null;
  newChatWindow: string | null;
  openDirectory: string | null;
  settings: string | null;
  find: string | null;
  findNext: string | null;
  findPrevious: string | null;
  alwaysOnTop: string | null;
  toggleNavigation: string | null;
}

export type DefaultKeyboardShortcuts = {
  [K in keyof KeyboardShortcuts]: string;
};

// prettier-ignore
export type LanguageSetting =
  | 'system' | 'en' | 'es' | 'fr' | 'de' | 'it' | 'pt' | 'id' | 'ms' | 'vi'
  | 'hi' | 'ja' | 'ko' | 'ru' | 'tr' | 'zh-CN' | 'zh-TW';

export interface Settings {
  showMenuBarIcon: boolean;
  disableAutoDownload: boolean;
  showDockIcon: boolean;
  enableWakelock: boolean;
  enableNotifications: boolean;
  spellcheckEnabled: boolean;
  archiveFolder: string | null;
  archivedSessionFiles: Record<string, string>;
  externalGoslingd: ExternalGoslingdConfig;
  globalShortcut?: string | null;
  keyboardShortcuts: KeyboardShortcuts;
  theme: 'dark' | 'light';
  useSystemTheme: boolean;
  language: LanguageSetting;
  responseStyle: string;
  showPricing: boolean;
  seenAnnouncementIds: string[];
  recentModels: RecentModel[];
  outputFileExtensions: string[];
  researchLibraryPath: string | null;
}

export const settingKeys = [
  'showMenuBarIcon',
  'disableAutoDownload',
  'showDockIcon',
  'enableWakelock',
  'enableNotifications',
  'spellcheckEnabled',
  'archiveFolder',
  'archivedSessionFiles',
  'externalGoslingd',
  'globalShortcut',
  'keyboardShortcuts',
  'theme',
  'useSystemTheme',
  'language',
  'responseStyle',
  'showPricing',
  'seenAnnouncementIds',
  'recentModels',
  'outputFileExtensions',
  'researchLibraryPath',
] as const satisfies readonly (keyof Settings)[];

export type SettingKey = (typeof settingKeys)[number];

export interface LegacySettings {
  [key: string]: unknown;
  externalGoosed?: unknown;
  externalGoslingd?: unknown;
  managedSecretProfiles?: unknown;
}

export const defaultKeyboardShortcuts: DefaultKeyboardShortcuts = {
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
  toggleNavigation: 'CommandOrControl+/',
};

export const defaultSettings: Settings = {
  showMenuBarIcon: true,
  disableAutoDownload: false,
  showDockIcon: true,
  enableWakelock: false,
  enableNotifications: true,
  spellcheckEnabled: true,
  archiveFolder: null,
  archivedSessionFiles: {},
  keyboardShortcuts: defaultKeyboardShortcuts,
  externalGoslingd: {
    enabled: false,
    url: '',
    secret: '',
    secretConfigured: false,
  },
  theme: 'light',
  useSystemTheme: true,
  language: 'system',
  responseStyle: 'concise',
  showPricing: true,
  seenAnnouncementIds: [],
  recentModels: [],
  outputFileExtensions: [...defaultOutputFileExtensions],
  researchLibraryPath: null,
};

const languageSettings = new Set<LanguageSetting>([
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

const keyboardShortcutKeys = Object.keys(defaultKeyboardShortcuts) as (keyof KeyboardShortcuts)[];
const MAX_PATH_LENGTH = 4096;
const MAX_SHORTCUT_LENGTH = 256;
const MAX_RESPONSE_STYLE_LENGTH = 100;
const MAX_ANNOUNCEMENTS = 1_000;
const MAX_ARCHIVED_SESSIONS = 10_000;
const MAX_RECENT_MODELS = 5;
const MAX_RECENT_MODEL_FIELD_LENGTH = 512;
const MAX_OUTPUT_FILE_EXTENSIONS = 100;
const MAX_OUTPUT_FILE_EXTENSION_LENGTH = 32;

export function normalizeOutputFileExtension(value: string): string | null {
  const normalized = value.trim().toLowerCase().replace(/^\.+/, '');
  if (
    !normalized ||
    normalized.length > MAX_OUTPUT_FILE_EXTENSION_LENGTH ||
    !/^[a-z0-9][a-z0-9._+-]*$/.test(normalized)
  ) {
    return null;
  }
  return normalized;
}

function isOutputFileExtensions(value: unknown): value is string[] {
  return (
    Array.isArray(value) &&
    value.length <= MAX_OUTPUT_FILE_EXTENSIONS &&
    value.every(
      (extension) =>
        typeof extension === 'string' && normalizeOutputFileExtension(extension) === extension
    ) &&
    new Set(value).size === value.length
  );
}

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function isBoundedString(value: unknown, maxLength: number): value is string {
  return typeof value === 'string' && value.length <= maxLength;
}

function isNullableBoundedString(value: unknown, maxLength: number): value is string | null {
  return value === null || isBoundedString(value, maxLength);
}

function isKeyboardShortcuts(value: unknown): value is KeyboardShortcuts {
  if (!isPlainRecord(value)) return false;
  if (
    Object.keys(value).some((key) => !keyboardShortcutKeys.includes(key as keyof KeyboardShortcuts))
  ) {
    return false;
  }
  return keyboardShortcutKeys.every((key) =>
    isNullableBoundedString(value[key], MAX_SHORTCUT_LENGTH)
  );
}

function isArchivedSessionFiles(value: unknown): value is Record<string, string> {
  if (!isPlainRecord(value)) return false;
  const entries = Object.entries(value);
  return (
    entries.length <= MAX_ARCHIVED_SESSIONS &&
    entries.every(
      ([sessionId, filePath]) =>
        sessionId.length > 0 &&
        sessionId.length <= 256 &&
        isBoundedString(filePath, MAX_PATH_LENGTH)
    )
  );
}

function isExternalGoslingdConfig(value: unknown): value is ExternalGoslingdConfig {
  if (!isPlainRecord(value)) return false;
  const allowedKeys = new Set(['enabled', 'url', 'secret', 'secretConfigured', 'certFingerprint']);
  return (
    Object.keys(value).every((key) => allowedKeys.has(key)) &&
    typeof value.enabled === 'boolean' &&
    isBoundedString(value.url, MAX_PATH_LENGTH) &&
    isBoundedString(value.secret, MAX_PATH_LENGTH) &&
    (value.secretConfigured === undefined || typeof value.secretConfigured === 'boolean') &&
    (value.certFingerprint === undefined || isBoundedString(value.certFingerprint, 512))
  );
}

export function isSettingKey(value: unknown): value is SettingKey {
  return typeof value === 'string' && (settingKeys as readonly string[]).includes(value);
}

export function isSettingValue<K extends SettingKey>(key: K, value: unknown): value is Settings[K] {
  switch (key) {
    case 'showMenuBarIcon':
    case 'disableAutoDownload':
    case 'showDockIcon':
    case 'enableWakelock':
    case 'enableNotifications':
    case 'spellcheckEnabled':
    case 'useSystemTheme':
    case 'showPricing':
      return typeof value === 'boolean';
    case 'archiveFolder':
    case 'researchLibraryPath':
      return isNullableBoundedString(value, MAX_PATH_LENGTH);
    case 'archivedSessionFiles':
      return isArchivedSessionFiles(value);
    case 'externalGoslingd':
      return isExternalGoslingdConfig(value);
    case 'globalShortcut':
      return isNullableBoundedString(value, MAX_SHORTCUT_LENGTH);
    case 'keyboardShortcuts':
      return isKeyboardShortcuts(value);
    case 'theme':
      return value === 'dark' || value === 'light';
    case 'language':
      return typeof value === 'string' && languageSettings.has(value as LanguageSetting);
    case 'responseStyle':
      return isBoundedString(value, MAX_RESPONSE_STYLE_LENGTH);
    case 'seenAnnouncementIds':
      return (
        Array.isArray(value) &&
        value.length <= MAX_ANNOUNCEMENTS &&
        value.every((id) => isBoundedString(id, 256))
      );
    case 'recentModels':
      return (
        Array.isArray(value) &&
        value.length <= MAX_RECENT_MODELS &&
        value.every(
          (recent) =>
            isPlainRecord(recent) &&
            Object.keys(recent).every((key) => key === 'provider' || key === 'model') &&
            isBoundedString(recent.provider, MAX_RECENT_MODEL_FIELD_LENGTH) &&
            isBoundedString(recent.model, MAX_RECENT_MODEL_FIELD_LENGTH)
        )
      );
    case 'outputFileExtensions':
      return isOutputFileExtensions(value);
  }
}

export function setSettingValue<K extends SettingKey>(
  settings: Settings,
  key: K,
  value: Settings[K]
): void {
  switch (key) {
    case 'showMenuBarIcon':
    case 'disableAutoDownload':
    case 'showDockIcon':
    case 'enableWakelock':
    case 'enableNotifications':
    case 'spellcheckEnabled':
    case 'archiveFolder':
    case 'archivedSessionFiles':
    case 'externalGoslingd':
    case 'globalShortcut':
    case 'keyboardShortcuts':
    case 'theme':
    case 'useSystemTheme':
    case 'language':
    case 'responseStyle':
    case 'showPricing':
    case 'seenAnnouncementIds':
    case 'recentModels':
    case 'outputFileExtensions':
    case 'researchLibraryPath':
      Object.assign(settings, { [key]: value });
  }
}

function freshDefaultSettings(): Settings {
  return {
    ...defaultSettings,
    archivedSessionFiles: {},
    externalGoslingd: { ...defaultSettings.externalGoslingd },
    keyboardShortcuts: { ...defaultSettings.keyboardShortcuts },
    seenAnnouncementIds: [],
    recentModels: [],
    outputFileExtensions: [...defaultSettings.outputFileExtensions],
  };
}

export function resolveStoredSettings(stored: LegacySettings): {
  settings: Settings;
  migratedLegacyExternalBackend: boolean;
  removedLegacyManagedSecretProfiles: boolean;
  removedPersistedExternalSecret: boolean;
  legacyExternalBackendSecret: string;
} {
  const settings = freshDefaultSettings();
  for (const key of settingKeys) {
    if (key === 'externalGoslingd' || key === 'keyboardShortcuts') continue;
    const value = stored[key];
    if (value !== undefined && isSettingValue(key, value)) {
      setSettingValue(settings, key, value);
    }
  }

  const migratedLegacyExternalBackend =
    stored.externalGoslingd === undefined && stored.externalGoosed !== undefined;
  const externalValue = stored.externalGoslingd ?? stored.externalGoosed;
  let legacyExternalBackendSecret = '';
  if (isPlainRecord(externalValue)) {
    const candidate: ExternalGoslingdConfig = {
      enabled:
        typeof externalValue.enabled === 'boolean'
          ? externalValue.enabled
          : defaultSettings.externalGoslingd.enabled,
      url: typeof externalValue.url === 'string' ? externalValue.url : '',
      secret: typeof externalValue.secret === 'string' ? externalValue.secret : '',
      secretConfigured:
        typeof externalValue.secretConfigured === 'boolean'
          ? externalValue.secretConfigured
          : false,
      ...(typeof externalValue.certFingerprint === 'string'
        ? { certFingerprint: externalValue.certFingerprint }
        : {}),
    };
    if (isExternalGoslingdConfig(candidate)) {
      legacyExternalBackendSecret = candidate.secret;
      settings.externalGoslingd = {
        ...candidate,
        secret: '',
        secretConfigured: candidate.secretConfigured || candidate.secret.length > 0,
      };
    }
  }

  const storedKeyboardShortcuts = stored.keyboardShortcuts;
  if (isPlainRecord(storedKeyboardShortcuts)) {
    const mergedKeyboardShortcuts = {
      ...defaultSettings.keyboardShortcuts,
      ...storedKeyboardShortcuts,
    };
    if (isKeyboardShortcuts(mergedKeyboardShortcuts)) {
      settings.keyboardShortcuts = mergedKeyboardShortcuts;
    }
  }

  return {
    settings,
    migratedLegacyExternalBackend,
    removedLegacyManagedSecretProfiles: Object.prototype.hasOwnProperty.call(
      stored,
      'managedSecretProfiles'
    ),
    removedPersistedExternalSecret: legacyExternalBackendSecret.length > 0,
    legacyExternalBackendSecret,
  };
}

export function getKeyboardShortcuts(settings: Settings): KeyboardShortcuts {
  if (!settings.keyboardShortcuts && settings.globalShortcut !== undefined) {
    const focusShortcut = settings.globalShortcut;
    let launcherShortcut: string | null = null;

    if (focusShortcut) {
      launcherShortcut = focusShortcut.includes('Shift')
        ? focusShortcut
        : focusShortcut.replace(/\+([Gg])$/, '+Shift+$1');
    }

    return {
      ...defaultKeyboardShortcuts,
      focusWindow: focusShortcut,
      quickLauncher: launcherShortcut,
    };
  }
  return { ...defaultKeyboardShortcuts, ...settings.keyboardShortcuts };
}
