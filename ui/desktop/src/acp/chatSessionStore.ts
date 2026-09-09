import { useEffect, useState } from 'react';
import type {
  GoslingSessionNotification_unstable,
  SessionArtifactDto,
} from '@repo-makeover/gosling-sdk';
import type { RequestPermissionRequest, SessionNotification } from '@agentclientprotocol/sdk';
import type { TokenState } from '../types/chat';
import { ChatState } from '../types/chatState';
import type { Message, NotificationEvent } from '../types/message';
import type { Session } from '../types/session';
import {
  createAcpSessionNotificationAdapter,
  type AcpChatStateChange,
  type AcpSessionNotificationAdapter,
} from './sessionNotificationAdapter';
import type { ElicitationStatus } from './adapter/elicitations';
import { cloneMessage } from './adapter/shared';
import type { AcpElicitationRequest } from './elicitationRequests';

export interface AcpChatSessionSnapshot {
  session: Session | undefined;
  connectionGeneration: number | null;
  messages: Message[];
  artifacts: SessionArtifactDto[];
  historyCursor: string | null;
  historyHasMore: boolean;
  historyLoading: boolean;
  historyTotalCount: number | null;
  tokenState: TokenState;
  notifications: NotificationEvent[];
  chatState: ChatState;
  sessionLoadError: string | undefined;
  promptError: AcpPromptError | undefined;
  interruptedPrompt: boolean;
  activePromptAttemptId: string | null;
  activeRunId: string | null;
  pendingCancelPromptAttemptId: string | null;
  /** Ids of local steer echoes not yet applied by the agent — see `addPendingLocalSteerMessage`. */
  pendingLocalSteerMessageIds: ReadonlySet<string>;
}

export interface AcpPromptError {
  message: string;
  connectionLost: boolean;
  /** The turn ended on a question to the user; nothing failed. */
  awaitingReply?: boolean;
}

type SnapshotListener = (snapshot: AcpChatSessionSnapshot) => void;

interface StoreEntry extends AcpChatSessionSnapshot {
  adapter: AcpSessionNotificationAdapter;
  promptCancellationRestoreState: {
    activeRunId: string | null;
    chatState: ChatState;
    pendingUserInputRequestIds: Set<string>;
  } | null;
  pendingUserInputRequestIds: Set<string>;
  pendingLocalSteerMessageIds: Set<string>;
}

const initialTokenState: TokenState = {
  inputTokens: 0,
  outputTokens: 0,
  totalTokens: 0,
  accumulatedInputTokens: 0,
  accumulatedOutputTokens: 0,
  accumulatedTotalTokens: 0,
};

export interface AcpChatSessionStore {
  getSnapshot(sessionId: string): AcpChatSessionSnapshot | undefined;
}

export interface AcpChatSessionActions {
  deleteSnapshot(sessionId: string): void;

  applyAcpSessionNotification(notification: SessionNotification): AcpChatSessionSnapshot;
  applyAcpGoslingSessionNotification(
    notification: GoslingSessionNotification_unstable
  ): AcpChatSessionSnapshot;
  applyPermissionRequest(request: RequestPermissionRequest): AcpChatSessionSnapshot;
  applyElicitationRequest(request: AcpElicitationRequest): AcpChatSessionSnapshot;
  setElicitationStatus(
    sessionId: string,
    elicitationId: string,
    status: ElicitationStatus
  ): AcpChatSessionSnapshot | undefined;

  setSessionMetadata(sessionId: string, session: Session | undefined): AcpChatSessionSnapshot;
  startSessionLoad(sessionId: string): AcpChatSessionSnapshot;
  finishSessionLoad(
    sessionId: string,
    session: Session,
    connectionGeneration: number
  ): AcpChatSessionSnapshot;
  failSessionLoad(sessionId: string, sessionLoadError: string): AcpChatSessionSnapshot;
  setSessionLoadError(
    sessionId: string,
    sessionLoadError: string | undefined
  ): AcpChatSessionSnapshot;

  setMessages(sessionId: string, messages: Message[]): AcpChatSessionSnapshot;
  setArtifacts(sessionId: string, artifacts: SessionArtifactDto[]): AcpChatSessionSnapshot;
  setHistoryPageState(
    sessionId: string,
    state: {
      cursor?: string | null;
      hasMore?: boolean;
      loading?: boolean;
      totalCount?: number | null;
    }
  ): AcpChatSessionSnapshot;
  prependMessages(
    sessionId: string,
    messages: Message[],
    nextCursor: string | null,
    totalCount?: number | null
  ): AcpChatSessionSnapshot;
  addPendingLocalSteerMessage(sessionId: string, message: Message): AcpChatSessionSnapshot;
  setChatState(sessionId: string, chatState: ChatState): AcpChatSessionSnapshot;
  resolveUserInputRequest(
    sessionId: string,
    userInputRequestId: string
  ): AcpChatSessionSnapshot | undefined;

  startPromptAttempt(sessionId: string, promptAttemptId: string): AcpChatSessionSnapshot;
  startPromptCancellation(
    sessionId: string,
    promptAttemptId: string
  ): AcpChatSessionSnapshot | undefined;
  clearPromptCancellation(
    sessionId: string,
    promptAttemptId: string
  ): AcpChatSessionSnapshot | undefined;
  restorePromptCancellation(
    sessionId: string,
    promptAttemptId: string
  ): AcpChatSessionSnapshot | undefined;
  waitForPromptCancellation(sessionId: string, promptAttemptId: string): Promise<void>;
  finishPromptAttemptIfCurrent(
    sessionId: string,
    promptAttemptId: string,
    error?: AcpPromptError
  ): boolean;
  clearActivePromptAttempt(sessionId: string): AcpChatSessionSnapshot | undefined;
  isCurrentPromptAttempt(sessionId: string, promptAttemptId: string): boolean;
}

interface AcpChatSessionStoreInternal extends AcpChatSessionStore, AcpChatSessionActions {
  subscribe(sessionId: string, listener: (snapshot: AcpChatSessionSnapshot) => void): () => void;
}

/// Upper bound on retained per-session snapshots. Unobserved entries are
/// rebuilt on demand, so this only bounds memory, never correctness.
const MAX_RETAINED_SESSIONS = 50;

function createAcpChatSessionStoreInternal(): AcpChatSessionStoreInternal {
  const sessionsById = new Map<string, StoreEntry>();
  const listenersBySessionId = new Map<string, Set<SnapshotListener>>();

  const getSnapshot: AcpChatSessionStore['getSnapshot'] = (sessionId) => {
    const entry = sessionsById.get(sessionId);
    return entry ? snapshotFromEntry(entry) : undefined;
  };

  const subscribe: AcpChatSessionStoreInternal['subscribe'] = (sessionId, listener) => {
    const listeners = listenersBySessionId.get(sessionId) ?? new Set<SnapshotListener>();
    listeners.add(listener);
    listenersBySessionId.set(sessionId, listeners);

    let subscribed = true;
    return () => {
      if (!subscribed) {
        return;
      }

      subscribed = false;
      const currentListeners = listenersBySessionId.get(sessionId);
      if (!currentListeners) {
        return;
      }

      currentListeners.delete(listener);
      if (currentListeners.size === 0) {
        listenersBySessionId.delete(sessionId);
      }
    };
  };

  const deleteSnapshot: AcpChatSessionActions['deleteSnapshot'] = (sessionId) => {
    sessionsById.delete(sessionId);
  };

  const getOrCreateEntry = (sessionId: string): StoreEntry => {
    const existing = sessionsById.get(sessionId);
    if (existing) {
      return existing;
    }

    const entry: StoreEntry = {
      session: undefined,
      connectionGeneration: null,
      messages: [],
      artifacts: [],
      historyCursor: null,
      historyHasMore: false,
      historyLoading: false,
      historyTotalCount: null,
      tokenState: { ...initialTokenState },
      notifications: [],
      chatState: ChatState.Idle,
      sessionLoadError: undefined,
      promptError: undefined,
      interruptedPrompt: false,
      activePromptAttemptId: null,
      activeRunId: null,
      pendingCancelPromptAttemptId: null,
      promptCancellationRestoreState: null,
      pendingUserInputRequestIds: new Set(),
      pendingLocalSteerMessageIds: new Set(),
      adapter: createAcpSessionNotificationAdapter(),
    };
    sessionsById.set(sessionId, entry);
    evictUnobservedSessions();
    return entry;
  };

  /// `sessionsById` only ever shrank on explicit archive/delete, so switching
  /// between many sessions in one window grew it for the life of the process
  /// (MEM-GSL-003). Entries with no subscriber are not rendered by anything
  /// and are rebuilt on demand, so they are the safe ones to drop. A session
  /// currently on screen always has a listener and is never evicted.
  function evictUnobservedSessions(): void {
    if (sessionsById.size <= MAX_RETAINED_SESSIONS) {
      return;
    }
    for (const sessionId of sessionsById.keys()) {
      if (sessionsById.size <= MAX_RETAINED_SESSIONS) {
        break;
      }
      if ((listenersBySessionId.get(sessionId)?.size ?? 0) === 0) {
        sessionsById.delete(sessionId);
      }
    }
  }

  const notify = (sessionId: string, entry: StoreEntry): AcpChatSessionSnapshot => {
    const snapshot = snapshotFromEntry(entry);
    const listeners = listenersBySessionId.get(sessionId);
    if (listeners) {
      for (const listener of listeners) {
        listener(snapshot);
      }
    }
    return snapshot;
  };

  const setSessionMetadata: AcpChatSessionActions['setSessionMetadata'] = (sessionId, session) => {
    const entry = getOrCreateEntry(sessionId);
    entry.session = session;
    return notify(sessionId, entry);
  };

  const startSessionLoad: AcpChatSessionActions['startSessionLoad'] = (sessionId) => {
    const entry = getOrCreateEntry(sessionId);
    resetReplayState(entry);
    entry.connectionGeneration = null;
    entry.sessionLoadError = undefined;
    entry.promptError = undefined;
    entry.chatState = ChatState.LoadingConversation;
    return notify(sessionId, entry);
  };

  const finishSessionLoad: AcpChatSessionActions['finishSessionLoad'] = (
    sessionId,
    session,
    connectionGeneration
  ) => {
    const entry = getOrCreateEntry(sessionId);
    entry.session = session;
    entry.connectionGeneration = connectionGeneration;
    entry.sessionLoadError = undefined;
    entry.promptError = undefined;
    entry.chatState = entry.activePromptAttemptId ? ChatState.Streaming : ChatState.Idle;
    return notify(sessionId, entry);
  };

  const failSessionLoad: AcpChatSessionActions['failSessionLoad'] = (
    sessionId,
    sessionLoadError
  ) => {
    const entry = getOrCreateEntry(sessionId);
    entry.connectionGeneration = null;
    entry.sessionLoadError = sessionLoadError;
    entry.chatState = ChatState.Idle;
    return notify(sessionId, entry);
  };

  const setMessages: AcpChatSessionActions['setMessages'] = (sessionId, messages) => {
    const entry = getOrCreateEntry(sessionId);
    entry.messages = cloneMessages(messages);
    retainPendingLocalSteerMessageIds(entry);
    entry.adapter = createAdapterForEntry(entry);
    return notify(sessionId, entry);
  };

  const setArtifacts: AcpChatSessionActions['setArtifacts'] = (sessionId, artifacts) => {
    const entry = getOrCreateEntry(sessionId);
    entry.artifacts = deduplicateArtifacts(artifacts);
    return notify(sessionId, entry);
  };

  const setHistoryPageState: AcpChatSessionActions['setHistoryPageState'] = (sessionId, state) => {
    const entry = getOrCreateEntry(sessionId);
    if ('cursor' in state) {
      entry.historyCursor = state.cursor ?? null;
    }
    if (state.hasMore !== undefined) {
      entry.historyHasMore = state.hasMore;
    }
    if (state.loading !== undefined) {
      entry.historyLoading = state.loading;
    }
    if ('totalCount' in state) {
      entry.historyTotalCount = state.totalCount ?? null;
    }
    return notify(sessionId, entry);
  };

  const prependMessages: AcpChatSessionActions['prependMessages'] = (
    sessionId,
    messages,
    nextCursor,
    totalCount
  ) => {
    const entry = getOrCreateEntry(sessionId);
    const existingKeys = new Set(entry.messages.map(messageIdentity));
    const olderMessages = cloneMessages(messages).filter((message) => {
      const key = messageIdentity(message);
      if (existingKeys.has(key)) {
        return false;
      }
      existingKeys.add(key);
      return true;
    });
    entry.messages = [...olderMessages, ...entry.messages];
    entry.historyCursor = nextCursor;
    entry.historyHasMore = nextCursor !== null;
    entry.historyLoading = false;
    if (totalCount !== undefined) {
      entry.historyTotalCount = totalCount;
    }
    retainPendingLocalSteerMessageIds(entry);
    entry.adapter = createAdapterForEntry(entry);
    return notify(sessionId, entry);
  };

  const addPendingLocalSteerMessage: AcpChatSessionActions['addPendingLocalSteerMessage'] = (
    sessionId,
    message
  ) => {
    const entry = getOrCreateEntry(sessionId);
    if (!message.id || entry.messages.some((existing) => existing.id === message.id)) {
      return notify(sessionId, entry);
    }

    entry.messages = [...entry.messages, cloneMessage(message)];
    entry.pendingLocalSteerMessageIds.add(message.id);
    entry.adapter = createAdapterForEntry(entry);
    return notify(sessionId, entry);
  };

  const setChatState: AcpChatSessionActions['setChatState'] = (sessionId, chatState) => {
    const entry = getOrCreateEntry(sessionId);
    entry.chatState = chatState;
    return notify(sessionId, entry);
  };

  const resolveUserInputRequest: AcpChatSessionActions['resolveUserInputRequest'] = (
    sessionId,
    userInputRequestId
  ) => {
    const entry = sessionsById.get(sessionId);
    if (!entry) {
      return undefined;
    }

    entry.pendingUserInputRequestIds.delete(userInputRequestId);

    if (
      entry.activePromptAttemptId &&
      entry.chatState === ChatState.WaitingForUserInput &&
      entry.pendingUserInputRequestIds.size === 0
    ) {
      entry.chatState = ChatState.Streaming;
      return notify(sessionId, entry);
    }

    return snapshotFromEntry(entry);
  };

  const setSessionLoadError: AcpChatSessionActions['setSessionLoadError'] = (
    sessionId,
    sessionLoadError
  ) => {
    const entry = getOrCreateEntry(sessionId);
    entry.sessionLoadError = sessionLoadError;
    return notify(sessionId, entry);
  };

  const startPromptAttempt: AcpChatSessionActions['startPromptAttempt'] = (
    sessionId,
    promptAttemptId
  ) => {
    const entry = getOrCreateEntry(sessionId);
    discardPendingLocalSteerMessages(entry);
    entry.activePromptAttemptId = promptAttemptId;
    entry.activeRunId = null;
    entry.pendingCancelPromptAttemptId = null;
    entry.promptCancellationRestoreState = null;
    entry.pendingUserInputRequestIds.clear();
    entry.chatState = ChatState.Streaming;
    entry.sessionLoadError = undefined;
    entry.promptError = undefined;
    entry.interruptedPrompt = false;
    entry.notifications = [];
    return notify(sessionId, entry);
  };

  const startPromptCancellation: AcpChatSessionActions['startPromptCancellation'] = (
    sessionId,
    promptAttemptId
  ) => {
    const entry = sessionsById.get(sessionId);
    if (!entry || entry.activePromptAttemptId !== promptAttemptId) {
      return undefined;
    }

    entry.promptCancellationRestoreState = {
      activeRunId: entry.activeRunId,
      chatState: entry.chatState,
      pendingUserInputRequestIds: new Set(entry.pendingUserInputRequestIds),
    };
    entry.activePromptAttemptId = null;
    entry.activeRunId = null;
    entry.pendingCancelPromptAttemptId = promptAttemptId;
    entry.pendingUserInputRequestIds.clear();
    discardPendingLocalSteerMessages(entry);
    entry.chatState = ChatState.Idle;
    return notify(sessionId, entry);
  };

  const clearPromptCancellation: AcpChatSessionActions['clearPromptCancellation'] = (
    sessionId,
    promptAttemptId
  ) => {
    const entry = sessionsById.get(sessionId);
    if (!entry || entry.pendingCancelPromptAttemptId !== promptAttemptId) {
      return undefined;
    }

    entry.pendingCancelPromptAttemptId = null;
    entry.promptCancellationRestoreState = null;
    return notify(sessionId, entry);
  };

  const restorePromptCancellation: AcpChatSessionActions['restorePromptCancellation'] = (
    sessionId,
    promptAttemptId
  ) => {
    const entry = sessionsById.get(sessionId);
    if (
      !entry ||
      entry.pendingCancelPromptAttemptId !== promptAttemptId ||
      !entry.promptCancellationRestoreState
    ) {
      return undefined;
    }

    const restoreState = entry.promptCancellationRestoreState;
    entry.activePromptAttemptId = promptAttemptId;
    entry.activeRunId = restoreState.activeRunId;
    entry.pendingCancelPromptAttemptId = null;
    entry.promptCancellationRestoreState = null;
    entry.pendingUserInputRequestIds = new Set(restoreState.pendingUserInputRequestIds);
    entry.chatState = restoreState.chatState;
    return notify(sessionId, entry);
  };

  const waitForPromptCancellation: AcpChatSessionActions['waitForPromptCancellation'] = (
    sessionId,
    promptAttemptId
  ) => {
    const entry = sessionsById.get(sessionId);
    if (!entry || entry.pendingCancelPromptAttemptId !== promptAttemptId) {
      return Promise.resolve();
    }

    return new Promise((resolve) => {
      const unsubscribe = subscribe(sessionId, (snapshot) => {
        if (snapshot.pendingCancelPromptAttemptId !== promptAttemptId) {
          unsubscribe();
          resolve();
        }
      });
    });
  };

  const finishPromptAttemptIfCurrent: AcpChatSessionActions['finishPromptAttemptIfCurrent'] = (
    sessionId,
    promptAttemptId,
    error
  ) => {
    const entry = sessionsById.get(sessionId);
    if (!entry || entry.activePromptAttemptId !== promptAttemptId) {
      return false;
    }

    entry.activePromptAttemptId = null;
    entry.activeRunId = null;
    entry.pendingCancelPromptAttemptId = null;
    entry.promptCancellationRestoreState = null;
    entry.pendingUserInputRequestIds.clear();
    discardPendingLocalSteerMessages(entry);
    entry.chatState = ChatState.Idle;
    entry.promptError = error;
    if (error?.connectionLost) {
      entry.connectionGeneration = null;
      entry.interruptedPrompt = true;
    }
    notify(sessionId, entry);
    return true;
  };

  const clearActivePromptAttempt: AcpChatSessionActions['clearActivePromptAttempt'] = (
    sessionId
  ) => {
    const entry = sessionsById.get(sessionId);
    if (!entry) {
      return undefined;
    }

    entry.activePromptAttemptId = null;
    entry.activeRunId = null;
    entry.pendingUserInputRequestIds.clear();
    discardPendingLocalSteerMessages(entry);
    entry.chatState = ChatState.Idle;
    return notify(sessionId, entry);
  };

  const isCurrentPromptAttempt: AcpChatSessionActions['isCurrentPromptAttempt'] = (
    sessionId,
    promptAttemptId
  ) => sessionsById.get(sessionId)?.activePromptAttemptId === promptAttemptId;

  const applyAcpSessionNotification: AcpChatSessionActions['applyAcpSessionNotification'] = (
    notification
  ) => {
    const entry = getOrCreateEntry(notification.sessionId);
    const changes = entry.adapter.apply(notification);
    applyChatStateChanges(entry, changes);
    return notify(notification.sessionId, entry);
  };

  const applyAcpGoslingSessionNotification: AcpChatSessionActions['applyAcpGoslingSessionNotification'] =
    (notification) => {
      const entry = getOrCreateEntry(notification.sessionId);
      const changes = entry.adapter.applyGosling(notification);
      applyChatStateChanges(entry, changes);
      return notify(notification.sessionId, entry);
    };

  const applyPermissionRequest: AcpChatSessionActions['applyPermissionRequest'] = (request) => {
    const entry = getOrCreateEntry(request.sessionId);
    const changes = entry.adapter.applyPermissionRequest(request);
    applyChatStateChanges(entry, changes);
    entry.pendingUserInputRequestIds.add(
      acpPermissionUserInputRequestId(request.toolCall.toolCallId)
    );
    entry.chatState = ChatState.WaitingForUserInput;
    return notify(request.sessionId, entry);
  };

  const applyElicitationRequest: AcpChatSessionActions['applyElicitationRequest'] = (request) => {
    const entry = getOrCreateEntry(request.sessionId);
    const changes = entry.adapter.applyElicitationRequest(request);
    applyChatStateChanges(entry, changes);
    entry.pendingUserInputRequestIds.add(acpElicitationUserInputRequestId(request.id));
    entry.chatState = ChatState.WaitingForUserInput;
    return notify(request.sessionId, entry);
  };

  const setElicitationStatus: AcpChatSessionActions['setElicitationStatus'] = (
    sessionId,
    elicitationId,
    status
  ) => {
    const entry = sessionsById.get(sessionId);
    if (!entry) {
      return undefined;
    }

    const changes = entry.adapter.applyElicitationStatus(elicitationId, status);
    if (changes.length === 0) {
      return snapshotFromEntry(entry);
    }

    applyChatStateChanges(entry, changes);
    return notify(sessionId, entry);
  };

  return {
    getSnapshot,
    subscribe,
    deleteSnapshot,
    setSessionMetadata,
    startSessionLoad,
    finishSessionLoad,
    failSessionLoad,
    setSessionLoadError,
    setMessages,
    setArtifacts,
    setHistoryPageState,
    prependMessages,
    addPendingLocalSteerMessage,
    setChatState,
    resolveUserInputRequest,
    startPromptAttempt,
    startPromptCancellation,
    clearPromptCancellation,
    restorePromptCancellation,
    waitForPromptCancellation,
    finishPromptAttemptIfCurrent,
    clearActivePromptAttempt,
    isCurrentPromptAttempt,
    applyAcpSessionNotification,
    applyAcpGoslingSessionNotification,
    applyPermissionRequest,
    applyElicitationRequest,
    setElicitationStatus,
  };
}

const acpChatSessionStoreInternal = createAcpChatSessionStoreInternal();

export const acpChatSessionStore: AcpChatSessionStore = storeFromInternal(
  acpChatSessionStoreInternal
);

export const acpChatSessionActions: AcpChatSessionActions = actionsFromStore(
  acpChatSessionStoreInternal
);

interface AcpChatSessionSnapshotState {
  sessionId: string;
  snapshot: AcpChatSessionSnapshot | undefined;
}

export function useAcpChatSessionSnapshot(sessionId: string): AcpChatSessionSnapshot | undefined {
  const [snapshotState, setSnapshotState] = useState<AcpChatSessionSnapshotState>(() => ({
    sessionId,
    snapshot: acpChatSessionStoreInternal.getSnapshot(sessionId),
  }));

  useEffect(() => {
    setSnapshotState({
      sessionId,
      snapshot: acpChatSessionStoreInternal.getSnapshot(sessionId),
    });

    return acpChatSessionStoreInternal.subscribe(sessionId, (snapshot) => {
      setSnapshotState({ sessionId, snapshot });
    });
  }, [sessionId]);

  if (snapshotState.sessionId !== sessionId) {
    return acpChatSessionStoreInternal.getSnapshot(sessionId);
  }

  return snapshotState.snapshot;
}

function storeFromInternal(store: AcpChatSessionStoreInternal): AcpChatSessionStore {
  return {
    getSnapshot: store.getSnapshot,
  };
}

function actionsFromStore(store: AcpChatSessionStoreInternal): AcpChatSessionActions {
  return {
    deleteSnapshot: store.deleteSnapshot,
    applyAcpSessionNotification: store.applyAcpSessionNotification,
    applyAcpGoslingSessionNotification: store.applyAcpGoslingSessionNotification,
    applyPermissionRequest: store.applyPermissionRequest,
    applyElicitationRequest: store.applyElicitationRequest,
    setElicitationStatus: store.setElicitationStatus,
    setSessionMetadata: store.setSessionMetadata,
    startSessionLoad: store.startSessionLoad,
    finishSessionLoad: store.finishSessionLoad,
    failSessionLoad: store.failSessionLoad,
    setSessionLoadError: store.setSessionLoadError,
    setMessages: store.setMessages,
    setArtifacts: store.setArtifacts,
    setHistoryPageState: store.setHistoryPageState,
    prependMessages: store.prependMessages,
    addPendingLocalSteerMessage: store.addPendingLocalSteerMessage,
    setChatState: store.setChatState,
    resolveUserInputRequest: store.resolveUserInputRequest,
    startPromptAttempt: store.startPromptAttempt,
    startPromptCancellation: store.startPromptCancellation,
    clearPromptCancellation: store.clearPromptCancellation,
    restorePromptCancellation: store.restorePromptCancellation,
    waitForPromptCancellation: store.waitForPromptCancellation,
    finishPromptAttemptIfCurrent: store.finishPromptAttemptIfCurrent,
    clearActivePromptAttempt: store.clearActivePromptAttempt,
    isCurrentPromptAttempt: store.isCurrentPromptAttempt,
  };
}

function applyChatStateChanges(entry: StoreEntry, changes: AcpChatStateChange[]): void {
  for (const change of changes) {
    switch (change.type) {
      case 'messages':
        entry.messages = cloneMessages(change.messages);
        retainPendingLocalSteerMessageIds(entry);
        break;
      case 'messageUpserted':
        upsertMessageAtIndex(entry, change.index, change.message);
        retainPendingLocalSteerMessageIds(entry);
        break;
      case 'tokenState':
        entry.tokenState = { ...entry.tokenState, ...change.tokenState };
        break;
      case 'sessionInfo':
        if (change.name && entry.session) {
          entry.session = { ...entry.session, name: change.name };
        }
        if (change.activeRunId !== undefined) {
          entry.activeRunId = change.activeRunId;
        }
        break;
      case 'localSteerConfirmed':
        entry.pendingLocalSteerMessageIds.delete(change.messageId);
        break;
      case 'artifactUpserted':
        entry.artifacts = deduplicateArtifacts([...entry.artifacts, change.artifact]);
        break;
      case 'notification':
        entry.notifications = [...entry.notifications, change.notification];
        break;
    }
  }
}

function resetReplayState(entry: StoreEntry): void {
  entry.messages = [];
  entry.artifacts = [];
  entry.historyCursor = null;
  entry.historyHasMore = false;
  entry.historyLoading = false;
  entry.historyTotalCount = null;
  entry.tokenState = { ...initialTokenState };
  entry.notifications = [];
  entry.activeRunId = null;
  entry.pendingCancelPromptAttemptId = null;
  entry.promptCancellationRestoreState = null;
  entry.pendingUserInputRequestIds.clear();
  entry.pendingLocalSteerMessageIds.clear();
  entry.adapter = createAcpSessionNotificationAdapter();
}

function upsertMessageAtIndex(entry: StoreEntry, index: number, message: Message): void {
  const clonedMessage = cloneMessage(message);
  const resolvedIndex = resolveMessageIndex(entry, index, clonedMessage);

  if (resolvedIndex >= 0 && resolvedIndex < entry.messages.length) {
    // Replace in place on a shallow copy instead of `.map()`-ing the whole
    // array: streamed replies upsert the same trailing message on every
    // chunk, so re-mapping every prior message here made a full-conversation
    // pass cost per token.
    const nextMessages = entry.messages.slice();
    nextMessages[resolvedIndex] = clonedMessage;
    entry.messages = nextMessages;
    return;
  }

  entry.messages = [...entry.messages, clonedMessage];
}

function resolveMessageIndex(entry: StoreEntry, index: number, message: Message): number {
  if (index >= 0 && index <= entry.messages.length) {
    return index;
  }

  if (message.id) {
    return entry.messages.findIndex(
      (existingMessage) =>
        existingMessage.id === message.id && existingMessage.role === message.role
    );
  }

  return -1;
}

export function acpPermissionUserInputRequestId(toolCallId: string): string {
  return `permission:${toolCallId}`;
}

export function acpElicitationUserInputRequestId(elicitationId: string): string {
  return `elicitation:${elicitationId}`;
}

function retainPendingLocalSteerMessageIds(entry: StoreEntry): void {
  if (entry.pendingLocalSteerMessageIds.size === 0) {
    return;
  }

  const messageIds = new Set(entry.messages.map((message) => message.id).filter(Boolean));
  entry.pendingLocalSteerMessageIds = new Set(
    [...entry.pendingLocalSteerMessageIds].filter((messageId) => messageIds.has(messageId))
  );
}

function discardPendingLocalSteerMessages(entry: StoreEntry): void {
  if (entry.pendingLocalSteerMessageIds.size === 0) {
    return;
  }

  entry.messages = entry.messages.filter(
    (message) => !message.id || !entry.pendingLocalSteerMessageIds.has(message.id)
  );
  entry.pendingLocalSteerMessageIds.clear();
  entry.adapter = createAdapterForEntry(entry);
}

function createAdapterForEntry(entry: StoreEntry): AcpSessionNotificationAdapter {
  return createAcpSessionNotificationAdapter(
    entry.messages,
    confirmedLocalSteerTextByMessageId(entry)
  );
}

function confirmedLocalSteerTextByMessageId(entry: StoreEntry): Map<string, string> {
  const textByMessageId = new Map<string, string>();

  for (const message of entry.messages) {
    if (
      !message.id ||
      !message.metadata.steer ||
      entry.pendingLocalSteerMessageIds.has(message.id)
    ) {
      continue;
    }

    const firstContent = message.content[0];
    if (firstContent?.type === 'text') {
      textByMessageId.set(message.id, firstContent.text);
    }
  }

  return textByMessageId;
}

function snapshotFromEntry(entry: StoreEntry): AcpChatSessionSnapshot {
  return {
    session: entry.session,
    connectionGeneration: entry.connectionGeneration,
    messages: entry.messages,
    artifacts: [...entry.artifacts],
    historyCursor: entry.historyCursor,
    historyHasMore: entry.historyHasMore,
    historyLoading: entry.historyLoading,
    historyTotalCount: entry.historyTotalCount,
    tokenState: { ...entry.tokenState },
    notifications: [...entry.notifications],
    chatState: entry.chatState,
    sessionLoadError: entry.sessionLoadError,
    promptError: entry.promptError,
    interruptedPrompt: entry.interruptedPrompt,
    activePromptAttemptId: entry.activePromptAttemptId,
    activeRunId: entry.activeRunId,
    pendingCancelPromptAttemptId: entry.pendingCancelPromptAttemptId,
    pendingLocalSteerMessageIds: new Set(entry.pendingLocalSteerMessageIds),
  };
}

function deduplicateArtifacts(artifacts: SessionArtifactDto[]): SessionArtifactDto[] {
  const byPath = new Map<string, SessionArtifactDto>();
  for (const artifact of artifacts) {
    byPath.set(artifact.resolvedPath, artifact);
  }
  return [...byPath.values()].sort((left, right) =>
    left.firstSeenAt.localeCompare(right.firstSeenAt)
  );
}

function cloneMessages(messages: Message[]): Message[] {
  return messages.map(cloneMessage);
}

function messageIdentity(message: Message): string {
  return message.id ? `${message.role}:${message.id}` : `${message.role}:${message.created}`;
}
