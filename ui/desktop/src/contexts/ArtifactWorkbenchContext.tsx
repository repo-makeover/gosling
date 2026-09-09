import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import type { SessionArtifactDto } from '@repo-makeover/gosling-sdk';
import {
  artifactKindFromMetadata,
  artifactKindFromMimeType,
  artifactKindFromPath,
  artifactTitleFromPath,
  isArtifactKindPreviewableWithoutExtension,
} from '../components/artifacts/artifactUtils';
import type { ArtifactTab } from '../components/artifacts/types';

const STORAGE_KEY = 'gosling-artifact-workbench-v1';
const DEFAULT_SESSION_ID = '__no_session__';
const DEFAULT_WIDTH = 480;
const EMPTY_ARTIFACTS: SessionArtifactDto[] = [];

interface SessionPreviewState {
  activeTabId: string | null;
  tabs: ArtifactTab[];
  deletedArtifacts: Record<string, string>;
}

interface PersistedWorkbench {
  hideRepositoryFiles: boolean;
  isOpen: boolean;
  sessions: Record<string, SessionPreviewState>;
  tabs?: ArtifactTab[];
  activeTabId?: string | null;
  width: number;
}

interface OpenContentInput {
  content: string;
  encoding?: 'base64' | 'utf8';
  mimeType?: string;
  title: string;
  workspaceId?: string;
}

interface ArtifactWorkbenchValue {
  activeTab: ArtifactTab | null;
  activeTabId: string | null;
  artifacts: SessionArtifactDto[];
  trashedArtifacts: SessionArtifactDto[];
  closeTab: (id: string) => void;
  forgetTrashedFiles: (paths: string[]) => void;
  hideRepositoryFiles: boolean;
  isOpen: boolean;
  openArtifact: (artifact: SessionArtifactDto) => void;
  openContent: (input: OpenContentInput) => void;
  openFile: (path: string, baseDirectory?: string, workspaceId?: string) => void;
  resolveFilePath: (id: string, path: string) => void;
  setActiveTabId: (id: string) => void;
  setIsOpen: (isOpen: boolean) => void;
  setHideRepositoryFiles: (hide: boolean) => void;
  setVisibleSession: (sessionId: string | null, artifacts: SessionArtifactDto[]) => void;
  setWidth: (width: number) => void;
  tabs: ArtifactTab[];
  toggle: () => void;
  visibleSessionId: string | null;
  width: number;
}

const ArtifactWorkbenchContext = createContext<ArtifactWorkbenchValue | null>(null);

function createId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `artifact-${Date.now()}-${Math.random()}`;
}

function emptySessionState(): SessionPreviewState {
  return { activeTabId: null, tabs: [], deletedArtifacts: {} };
}

function validSessionState(value: Partial<SessionPreviewState> | undefined): SessionPreviewState {
  const tabs = Array.isArray(value?.tabs)
    ? value.tabs.flatMap((tab) => {
        if (tab?.source?.type !== 'file') return [];
        const pathKind = artifactKindFromPath(tab.source.path);
        if (pathKind !== 'unknown') return [{ ...tab, kind: pathKind }];
        return isArtifactKindPreviewableWithoutExtension(tab.kind) ? [tab] : [];
      })
    : [];
  return {
    deletedArtifacts: Object.fromEntries(
      Object.entries(value?.deletedArtifacts ?? {}).filter(
        ([, version]) => typeof version === 'string'
      )
    ),
    activeTabId: tabs.some((tab) => tab.id === value?.activeTabId)
      ? (value?.activeTabId ?? null)
      : (tabs[0]?.id ?? null),
    tabs,
  };
}

function loadPersistedWorkbench(): PersistedWorkbench {
  try {
    const parsed = JSON.parse(
      localStorage.getItem(STORAGE_KEY) ?? '{}'
    ) as Partial<PersistedWorkbench>;
    const sessions = Object.fromEntries(
      Object.entries(parsed.sessions ?? {}).map(([sessionId, state]) => [
        sessionId,
        validSessionState(state),
      ])
    );
    if (Array.isArray(parsed.tabs)) {
      sessions[DEFAULT_SESSION_ID] = validSessionState({
        tabs: parsed.tabs,
        activeTabId: parsed.activeTabId,
      });
    }
    return {
      hideRepositoryFiles: parsed.hideRepositoryFiles === true,
      isOpen: parsed.isOpen === true,
      sessions,
      width:
        typeof parsed.width === 'number'
          ? Math.min(720, Math.max(320, parsed.width))
          : DEFAULT_WIDTH,
    };
  } catch {
    return { hideRepositoryFiles: false, isOpen: false, sessions: {}, width: DEFAULT_WIDTH };
  }
}

export function ArtifactWorkbenchProvider({ children }: { children: React.ReactNode }) {
  const [initial] = useState(loadPersistedWorkbench);
  const [visibleSessionId, setVisibleSessionId] = useState(DEFAULT_SESSION_ID);
  const [artifactsBySession, setArtifactsBySession] = useState<
    Record<string, SessionArtifactDto[]>
  >({});
  const [sessions, setSessions] = useState(initial.sessions);
  const [isOpen, setIsOpen] = useState(initial.isOpen);
  const [hideRepositoryFiles, setHideRepositoryFiles] = useState(initial.hideRepositoryFiles);
  const [width, setWidthState] = useState(initial.width);
  const current = sessions[visibleSessionId] ?? emptySessionState();
  const deletedArtifacts = sessions[visibleSessionId]?.deletedArtifacts;
  const artifacts = useMemo(
    () =>
      (artifactsBySession[visibleSessionId] ?? EMPTY_ARTIFACTS).filter(
        (artifact) => deletedArtifacts?.[artifact.resolvedPath] !== artifact.lastSeenAt
      ),
    [artifactsBySession, visibleSessionId, deletedArtifacts]
  );

  const trashedArtifacts = useMemo(
    () =>
      (artifactsBySession[visibleSessionId] ?? EMPTY_ARTIFACTS).filter(
        (artifact) => deletedArtifacts?.[artifact.resolvedPath] === artifact.lastSeenAt
      ),
    [artifactsBySession, visibleSessionId, deletedArtifacts]
  );

  useEffect(() => {
    const fallback = sessions[DEFAULT_SESSION_ID] ?? emptySessionState();
    const persisted: PersistedWorkbench = {
      hideRepositoryFiles,
      isOpen,
      sessions: Object.fromEntries(
        Object.entries(sessions).map(([sessionId, state]) => [
          sessionId,
          { ...state, tabs: state.tabs.filter((tab) => tab.source.type === 'file') },
        ])
      ),
      tabs: fallback.tabs.filter((tab) => tab.source.type === 'file'),
      activeTabId: fallback.activeTabId,
      width,
    };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(persisted));
  }, [hideRepositoryFiles, isOpen, sessions, width]);

  const updateCurrent = useCallback(
    (update: (state: SessionPreviewState) => SessionPreviewState) => {
      setSessions((all) => ({
        ...all,
        [visibleSessionId]: update(all[visibleSessionId] ?? emptySessionState()),
      }));
    },
    [visibleSessionId]
  );

  const openFile = useCallback(
    (path: string, baseDirectory?: string, workspaceId?: string) => {
      const kind = artifactKindFromPath(path);
      if (kind === 'unknown') return;
      updateCurrent((state) => {
        const existing = state.tabs.find(
          (tab) =>
            tab.source.type === 'file' &&
            tab.source.path === path &&
            tab.source.baseDirectory === baseDirectory &&
            tab.workspaceId === workspaceId
        );
        if (existing) return { ...state, activeTabId: existing.id };
        const tab: ArtifactTab = {
          id: createId(),
          kind,
          source: { type: 'file', path, baseDirectory },
          title: artifactTitleFromPath(path),
          workspaceId,
        };
        return { ...state, activeTabId: tab.id, tabs: [...state.tabs, tab] };
      });
      setIsOpen(true);
    },
    [updateCurrent]
  );

  const openArtifact = useCallback(
    (artifact: SessionArtifactDto) => {
      const kind = artifactKindFromMetadata(artifact.displayPath, artifact.mimeType);
      updateCurrent((state) => {
        const existing = state.tabs.find(
          (tab) =>
            tab.source.type === 'file' &&
            (tab.source.path === artifact.resolvedPath ||
              (tab.source.path === artifact.displayPath &&
                tab.source.baseDirectory === artifact.baseWorkingDir))
        );
        if (existing) return { ...state, activeTabId: existing.id };
        const tab: ArtifactTab = {
          id: createId(),
          kind,
          source: {
            type: 'file',
            path: artifact.displayPath,
            baseDirectory: artifact.baseWorkingDir,
          },
          title: artifactTitleFromPath(artifact.displayPath),
          workspaceId: artifact.workspaceId ?? undefined,
        };
        return { ...state, activeTabId: tab.id, tabs: [...state.tabs, tab] };
      });
      setIsOpen(true);
    },
    [updateCurrent]
  );

  const openContent = useCallback(
    (input: OpenContentInput) => {
      const mimeType = input.mimeType ?? 'text/plain';
      const tab: ArtifactTab = {
        id: createId(),
        kind: artifactKindFromMimeType(mimeType),
        source: {
          type: 'content',
          content: input.content,
          encoding: input.encoding ?? 'utf8',
          mimeType,
        },
        title: input.title,
        workspaceId: input.workspaceId,
      };
      updateCurrent((state) => ({ ...state, activeTabId: tab.id, tabs: [...state.tabs, tab] }));
      setIsOpen(true);
    },
    [updateCurrent]
  );

  const closeTab = useCallback(
    (id: string) => {
      updateCurrent((state) => {
        const index = state.tabs.findIndex((tab) => tab.id === id);
        const tabs = state.tabs.filter((tab) => tab.id !== id);
        return {
          ...state,
          tabs,
          activeTabId:
            state.activeTabId === id
              ? (tabs[Math.min(index, tabs.length - 1)]?.id ?? null)
              : state.activeTabId,
        };
      });
    },
    [updateCurrent]
  );

  const forgetTrashedFiles = useCallback(
    (paths: string[]) => {
      const removedPaths = new Set(paths);
      // Preserve discovery history; dismiss only the version actually selected for deletion.
      // This callback captures that inventory even if a different chat is active when Trash finishes.
      setSessions((all) =>
        Object.fromEntries(
          [...new Set([...Object.keys(all), ...Object.keys(artifactsBySession)])].map(
            (sessionId) => {
              const state = all[sessionId] ?? emptySessionState();
              const removedArtifacts = (artifactsBySession[sessionId] ?? []).filter((artifact) =>
                removedPaths.has(artifact.resolvedPath)
              );
              const tabs = state.tabs.filter(
                (tab) =>
                  tab.source.type !== 'file' ||
                  (!removedPaths.has(tab.source.path) &&
                    !removedArtifacts.some(
                      (artifact) =>
                        tab.source.type === 'file' &&
                        tab.source.path === artifact.displayPath &&
                        tab.source.baseDirectory === artifact.baseWorkingDir
                    ))
              );
              return [
                sessionId,
                {
                  ...state,
                  tabs,
                  activeTabId: tabs.some((tab) => tab.id === state.activeTabId)
                    ? state.activeTabId
                    : (tabs[0]?.id ?? null),
                  deletedArtifacts: {
                    ...state.deletedArtifacts,
                    ...Object.fromEntries(
                      removedArtifacts.map((artifact) => [
                        artifact.resolvedPath,
                        artifact.lastSeenAt,
                      ])
                    ),
                  },
                },
              ];
            }
          )
        )
      );
    },
    [artifactsBySession]
  );

  const resolveFilePath = useCallback(
    (id: string, path: string) => {
      updateCurrent((state) => ({
        ...state,
        tabs: state.tabs.map((tab) =>
          tab.id === id && tab.source.type === 'file'
            ? {
                ...tab,
                kind: artifactKindFromPath(path),
                source: { type: 'file', path },
                title: artifactTitleFromPath(path),
              }
            : tab
        ),
      }));
    },
    [updateCurrent]
  );

  const setVisibleSession = useCallback(
    (sessionId: string | null, nextArtifacts: SessionArtifactDto[]) => {
      const key = sessionId ?? DEFAULT_SESSION_ID;
      setVisibleSessionId(key);
      setArtifactsBySession((currentArtifacts) => ({
        ...currentArtifacts,
        [key]: nextArtifacts,
      }));
    },
    []
  );

  const setWidth = useCallback((nextWidth: number) => {
    setWidthState(Math.min(720, Math.max(320, nextWidth)));
  }, []);

  const setActiveTabId = useCallback(
    (id: string) => updateCurrent((state) => ({ ...state, activeTabId: id })),
    [updateCurrent]
  );
  const activeTab = current.tabs.find((tab) => tab.id === current.activeTabId) ?? null;
  const value = useMemo<ArtifactWorkbenchValue>(
    () => ({
      activeTab,
      activeTabId: current.activeTabId,
      artifacts,
      trashedArtifacts,
      closeTab,
      forgetTrashedFiles,
      hideRepositoryFiles,
      isOpen,
      openArtifact,
      openContent,
      openFile,
      resolveFilePath,
      setActiveTabId,
      setIsOpen,
      setHideRepositoryFiles,
      setVisibleSession,
      setWidth,
      tabs: current.tabs,
      toggle: () => setIsOpen((open) => !open),
      visibleSessionId: visibleSessionId === DEFAULT_SESSION_ID ? null : visibleSessionId,
      width,
    }),
    [
      activeTab,
      artifacts,
      trashedArtifacts,
      closeTab,
      forgetTrashedFiles,
      hideRepositoryFiles,
      current.activeTabId,
      current.tabs,
      isOpen,
      openArtifact,
      openContent,
      openFile,
      resolveFilePath,
      setActiveTabId,
      setVisibleSession,
      setWidth,
      visibleSessionId,
      width,
    ]
  );

  return (
    <ArtifactWorkbenchContext.Provider value={value}>{children}</ArtifactWorkbenchContext.Provider>
  );
}

export function useArtifactWorkbench(): ArtifactWorkbenchValue {
  const context = useContext(ArtifactWorkbenchContext);
  if (!context)
    throw new Error('useArtifactWorkbench must be used within ArtifactWorkbenchProvider');
  return context;
}
