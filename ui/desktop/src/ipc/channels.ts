import type Electron from 'electron';

export interface ThemeChangePayload {
  mode: string;
  useSystemTheme: boolean;
  theme: string;
  tokensUpdated?: boolean;
}

export interface InitialMessageOptions {
  noAutoSubmit?: boolean;
}

export interface UpdaterEvent {
  event: string;
  data?: unknown;
}

export const desktopCommandChannels = {
  copyArtifactContents: 'copy-artifact-contents',
  classifyArtifactRepositories: 'classify-artifact-repositories',
  getArtifactFileTimestamps: 'get-artifact-file-timestamps',
  trashArtifactFiles: 'trash-artifact-files',
  reactReady: 'react-ready',
  createChatWindow: 'create-chat-window',
  broadcastThemeChange: 'broadcast-theme-change',
  broadcastWorkspaceChange: 'broadcast-workspace-change',
  openExternal: 'open-external',
  closeWindow: 'close-window',
  getAppVersion: 'get-app-version',
  getAppLocale: 'get-app-locale',
} as const;

export const rendererEventChannels = {
  addExtension: 'add-extension',
  artifactDownloadUnrouted: 'artifact-download-unrouted',
  fatalError: 'fatal-error',
  findCommand: 'find-command',
  findNext: 'find-next',
  findPrevious: 'find-previous',
  focusInput: 'focus-input',
  fullscreenChange: 'fullscreen-change',
  mouseBackButtonClicked: 'mouse-back-button-clicked',
  newChat: 'new-chat',
  openSharedSession: 'open-shared-session',
  setInitialMessage: 'set-initial-message',
  setView: 'set-view',
  themeChanged: 'theme-changed',
  toggleNavigation: 'toggle-navigation',
  updaterEvent: 'updater-event',
  useSelectionFind: 'use-selection-find',
  workspacesChanged: 'workspaces-changed',
} as const;

export type RendererEventChannel =
  (typeof rendererEventChannels)[keyof typeof rendererEventChannels];

export interface RendererEventPayloads {
  [rendererEventChannels.addExtension]: [url: string];
  [rendererEventChannels.artifactDownloadUnrouted]: [fileName: string];
  [rendererEventChannels.fatalError]: [message: string];
  [rendererEventChannels.findCommand]: [];
  [rendererEventChannels.findNext]: [];
  [rendererEventChannels.findPrevious]: [];
  [rendererEventChannels.focusInput]: [];
  [rendererEventChannels.fullscreenChange]: [isFullScreen: boolean];
  [rendererEventChannels.mouseBackButtonClicked]: [];
  [rendererEventChannels.newChat]: [];
  [rendererEventChannels.openSharedSession]: [url: string];
  [rendererEventChannels.setInitialMessage]: [
    initialMessage: string,
    options?: InitialMessageOptions,
  ];
  [rendererEventChannels.setView]: [view: string, section?: string];
  [rendererEventChannels.themeChanged]: [themeData: ThemeChangePayload];
  [rendererEventChannels.toggleNavigation]: [];
  [rendererEventChannels.updaterEvent]: [event: UpdaterEvent];
  [rendererEventChannels.useSelectionFind]: [];
  [rendererEventChannels.workspacesChanged]: [];
}

export type RendererEventCallback<T extends RendererEventChannel> = (
  event: Electron.IpcRendererEvent,
  ...args: RendererEventPayloads[T]
) => void;
