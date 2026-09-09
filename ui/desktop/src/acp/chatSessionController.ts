import { v7 as uuidv7 } from 'uuid';
import type { GoslingExtension, SessionArtifactDto } from '@repo-makeover/gosling-sdk';
import { AppEvents } from '../constants/events';
import { ChatState } from '../types/chatState';
import type { Session } from '../types/session';
import { showExtensionLoadResults } from '../utils/extensionErrorUtils';
import {
  createUserMessage,
  getPendingToolConfirmationIds,
  getTextAndImageContent,
  type Message,
} from '../types/message';
import {
  acpChatSessionActions,
  acpChatSessionStore,
  type AcpChatSessionSnapshot,
} from './chatSessionStore';
import { cancelAcpElicitationRequestsForSession } from './elicitationRequests';
import {
  describeAcpError,
  isAcpConnectionClosedError,
  parseAcpCreditsExhaustedError,
  type AcpCreditsExhaustedError,
  isAcpAwaitingReplyError,
} from './errors';
import { cancelAcpPermissionRequestsForSession } from './permissionRequests';
import { acpCancelPrompt, acpPromptSession } from './prompt';
import { getAcpConnectionGeneration } from './acpConnection';
import { resolveSessionLibraryInputs } from './sessionLibraryInputs';
import { clearSelectedSessionInputs, getSelectedSessionInputs } from './sessionInputSelection';
import { viewableFilePathsFromMarkdown } from '../components/artifacts/artifactUtils';
import {
  acpForkSession,
  acpHandoffSession,
  acpLoadSession,
  acpListSessionArtifacts,
  acpNewSession,
  acpTruncateSessionConversation,
  isAcpSessionLoadInFlight,
  sessionInfoToSession,
  type AcpWorkspaceLaunchOptions,
} from './sessions';

export interface AcpLoadSessionOptions {
  onSessionLoaded?: () => void;
  force?: boolean;
}

export interface AcpSnapshotOptions {
  getCurrentSnapshot(): AcpChatSessionSnapshot | undefined;
}

export interface AcpSubmitMessageOptions extends AcpSnapshotOptions {
  onFinish(error?: string): void | Promise<void>;
}

export interface AcpChatSessionController {
  createSession(
    cwd: string,
    goslingExtensions: GoslingExtension[],
    workspaceId?: string,
    workspaceLaunchOptions?: AcpWorkspaceLaunchOptions
  ): Promise<Session>;
  loadSession(sessionId: string, options?: AcpLoadSessionOptions): Promise<boolean>;
  submitMessage(
    sessionId: string,
    userMessage: Message,
    options: AcpSubmitMessageOptions
  ): Promise<void>;
  stop(sessionId: string): void;
  updateMessage(
    sessionId: string,
    messageId: string,
    newContent: string,
    editType: 'fork' | 'edit' | undefined,
    options: AcpSubmitMessageOptions
  ): Promise<void>;
  handoffSession(sessionId: string): Promise<{ hadSummary: boolean }>;
}

function createAcpCreditsExhaustedMessage(error: AcpCreditsExhaustedError): Message {
  return {
    id: uuidv7(),
    role: 'assistant',
    created: Math.floor(Date.now() / 1000),
    content: [
      {
        type: 'systemNotification',
        notificationType: 'creditsExhausted',
        msg: error.message,
        ...(error.url ? { data: { top_up_url: error.url } } : {}),
      },
    ],
    metadata: { userVisible: true, agentVisible: false },
  };
}

function assertNoPendingPromptCancellation(sessionId: string): void {
  const snapshot = acpChatSessionStore.getSnapshot(sessionId);
  if (snapshot?.pendingCancelPromptAttemptId) {
    throw new Error('Cannot submit while prompt cancellation is pending');
  }
}

function finishPromptCancellation(sessionId: string, promptAttemptId: string): boolean {
  if (
    acpChatSessionStore.getSnapshot(sessionId)?.pendingCancelPromptAttemptId === promptAttemptId
  ) {
    cancelAcpPermissionRequestsForSession(sessionId);
    cancelAcpElicitationRequestsForSession(sessionId);
  }
  return acpChatSessionActions.clearPromptCancellation(sessionId, promptAttemptId) !== undefined;
}

async function forkSessionWithEditedMessage(
  sessionId: string,
  message: Message,
  editedMessage: string
): Promise<void> {
  const targetSessionId = await acpForkSession(sessionId, message.created);

  const event = new CustomEvent(AppEvents.SESSION_FORKED, {
    detail: {
      newSessionId: targetSessionId,
      shouldStartAgent: true,
      editedMessage,
    },
  });
  window.dispatchEvent(event);
}

async function handoffSession(sessionId: string): Promise<{ hadSummary: boolean }> {
  const { sessionId: newSessionId, handoffSummary } = await acpHandoffSession(sessionId);

  const event = new CustomEvent(AppEvents.SESSION_HANDED_OFF, {
    detail: {
      newSessionId,
      shouldStartAgent: Boolean(handoffSummary),
      initialMessage: handoffSummary,
    },
  });
  window.dispatchEvent(event);
  return { hadSummary: Boolean(handoffSummary) };
}

async function createSession(
  cwd: string,
  goslingExtensions: GoslingExtension[],
  workspaceId?: string,
  workspaceLaunchOptions?: AcpWorkspaceLaunchOptions
): Promise<Session> {
  const { sessionId, sessionInfo, meta } = await acpNewSession(
    cwd,
    goslingExtensions,
    workspaceId,
    workspaceLaunchOptions
  );
  const session = sessionInfoToSession(sessionInfo, meta);
  const connectionGeneration = getAcpConnectionGeneration();
  if (connectionGeneration === null) {
    throw new Error('ACP connection closed while creating the session');
  }

  showExtensionLoadResults(meta.extensionResults);
  window.dispatchEvent(
    new CustomEvent(AppEvents.SESSION_EXTENSIONS_LOADED, { detail: { sessionId } })
  );
  acpChatSessionActions.finishSessionLoad(sessionId, session, connectionGeneration);
  acpChatSessionActions.setArtifacts(sessionId, []);

  return session;
}

const inFlightSessionLoads = new Map<string, Promise<boolean>>();
const preparingPromptAttempts = new Set<string>();

async function loadSession(
  sessionId: string,
  options: AcpLoadSessionOptions = {}
): Promise<boolean> {
  let load = inFlightSessionLoads.get(sessionId);
  if (!load) {
    load = loadSessionSnapshot(sessionId, options.force ?? false);
    inFlightSessionLoads.set(sessionId, load);
  }

  try {
    const loaded = await load;
    if (loaded) {
      options.onSessionLoaded?.();
    }
    return loaded;
  } finally {
    if (inFlightSessionLoads.get(sessionId) === load) {
      inFlightSessionLoads.delete(sessionId);
    }
  }
}

async function loadSessionSnapshot(sessionId: string, force: boolean): Promise<boolean> {
  const cached = acpChatSessionStore.getSnapshot(sessionId);
  const connectionGeneration = getAcpConnectionGeneration();
  if (
    !force &&
    cached?.session &&
    connectionGeneration !== null &&
    cached.connectionGeneration === connectionGeneration
  ) {
    window.dispatchEvent(
      new CustomEvent(AppEvents.SESSION_EXTENSIONS_LOADED, { detail: { sessionId } })
    );
    return true;
  }

  if (!isAcpSessionLoadInFlight(sessionId)) {
    acpChatSessionActions.startSessionLoad(sessionId);
  }

  try {
    const { sessionInfo, meta } = await acpLoadSession(sessionId);
    let artifacts: SessionArtifactDto[] = [];
    try {
      artifacts = await acpListSessionArtifacts(sessionId);
    } catch (error) {
      if (!/method not found/i.test(describeAcpError(error))) {
        throw error;
      }
      artifacts = reconstructArtifactsFromLoadedMessages(
        sessionId,
        sessionInfo.cwd,
        acpChatSessionStore.getSnapshot(sessionId)?.messages ?? []
      );
    }
    const loadedConnectionGeneration = getAcpConnectionGeneration();
    if (loadedConnectionGeneration === null) {
      throw new Error('ACP connection closed while loading the session');
    }

    showExtensionLoadResults(meta.extensionResults);
    window.dispatchEvent(
      new CustomEvent(AppEvents.SESSION_EXTENSIONS_LOADED, { detail: { sessionId } })
    );
    acpChatSessionActions.finishSessionLoad(
      sessionId,
      sessionInfoToSession(sessionInfo, meta),
      loadedConnectionGeneration
    );
    acpChatSessionActions.setArtifacts(sessionId, artifacts);
    if (meta.historyLoad?.mode === 'compacted') {
      acpChatSessionActions.setHistoryPageState(sessionId, {
        cursor: meta.historyLoad.nextBeforeCursor ?? null,
        hasMore: meta.historyLoad.nextBeforeCursor != null,
        loading: false,
        totalCount: meta.historyLoad.totalCount ?? null,
      });
    }
    return true;
  } catch (error) {
    console.error('Failed to load ACP session:', error);
    acpChatSessionActions.failSessionLoad(sessionId, describeAcpError(error));
    return false;
  }
}

function reconstructArtifactsFromLoadedMessages(
  sessionId: string,
  workingDir: string,
  messages: Message[]
): SessionArtifactDto[] {
  const seen = new Set<string>();
  const now = new Date().toISOString();
  const artifacts: SessionArtifactDto[] = [];

  for (const message of messages) {
    if (message.role !== 'assistant' || message.metadata.importedUntrusted) continue;
    for (const part of message.content) {
      if (part.type !== 'text') continue;
      for (const displayPath of viewableFilePathsFromMarkdown(part.text)) {
        const resolvedPath = resolveCompatibilityArtifactPath(workingDir, displayPath);
        if (seen.has(resolvedPath)) continue;
        seen.add(resolvedPath);
        artifacts.push({
          sessionId,
          displayPath,
          resolvedPath,
          baseWorkingDir: workingDir,
          relation: 'referenced',
          provenance: 'compatibility_inference',
          sourceId: message.id,
          firstSeenAt: now,
          lastSeenAt: now,
        });
      }
    }
  }
  return artifacts;
}

function resolveCompatibilityArtifactPath(workingDir: string, displayPath: string): string {
  if (/^(?:[a-z]:[\\/]|\/)/i.test(displayPath)) return displayPath;
  const separator = workingDir.includes('\\') ? '\\' : '/';
  return `${workingDir.replace(/[\\/]$/, '')}${separator}${displayPath}`;
}

async function submitMessage(
  sessionId: string,
  userMessage: Message,
  options: AcpSubmitMessageOptions
): Promise<void> {
  assertNoPendingPromptCancellation(sessionId);

  const snapshot = acpChatSessionStore.getSnapshot(sessionId);
  if (snapshot?.activePromptAttemptId) {
    return;
  }

  const promptAttemptId = uuidv7();
  const selectedInputIds =
    userMessage.role === 'user' &&
    !getTextAndImageContent(userMessage).textContent.trim().startsWith('/')
      ? getSelectedSessionInputs(sessionId)
      : [];
  preparingPromptAttempts.add(promptAttemptId);
  acpChatSessionActions.startPromptAttempt(sessionId, promptAttemptId);

  try {
    await window.electron.setWakelockActive(sessionId, true).catch(() => false);
    if (finishPromptCancellation(sessionId, promptAttemptId)) return;
    if (selectedInputIds.length > 0) {
      const inputs = await resolveSessionLibraryInputs(sessionId, selectedInputIds);
      if (finishPromptCancellation(sessionId, promptAttemptId)) return;
      const inputContent = createUserMessage('', inputs.images, inputs.assistantContext).content;
      userMessage = { ...userMessage, content: [...inputContent, ...userMessage.content] };
      const messages = acpChatSessionStore.getSnapshot(sessionId)?.messages ?? [];
      acpChatSessionActions.setMessages(
        sessionId,
        messages.map((message) => (message.id === userMessage.id ? userMessage : message))
      );
      clearSelectedSessionInputs(sessionId, selectedInputIds);
    }
    preparingPromptAttempts.delete(promptAttemptId);
    if (finishPromptCancellation(sessionId, promptAttemptId)) {
      return;
    }
    await acpPromptSession(sessionId, userMessage);
    if (finishPromptCancellation(sessionId, promptAttemptId)) {
      return;
    }
    if (acpChatSessionActions.finishPromptAttemptIfCurrent(sessionId, promptAttemptId)) {
      void options.onFinish();
    }
  } catch (error) {
    if (finishPromptCancellation(sessionId, promptAttemptId)) {
      return;
    }

    const creditsExhaustedError = parseAcpCreditsExhaustedError(error);
    if (creditsExhaustedError) {
      if (!acpChatSessionActions.isCurrentPromptAttempt(sessionId, promptAttemptId)) {
        return;
      }

      const messages = [
        ...(options.getCurrentSnapshot()?.messages ?? []),
        createAcpCreditsExhaustedMessage(creditsExhaustedError),
      ];
      acpChatSessionActions.setMessages(sessionId, messages);
      if (acpChatSessionActions.finishPromptAttemptIfCurrent(sessionId, promptAttemptId)) {
        void options.onFinish();
      }
      return;
    }

    const awaitingReply = isAcpAwaitingReplyError(error);
    if (!awaitingReply) {
      console.error('Failed to submit ACP prompt:', error);
    }
    const submitError = awaitingReply
      ? { message: '', connectionLost: false, awaitingReply: true }
      : {
          message: 'Submit error: ' + describeAcpError(error),
          connectionLost: isAcpConnectionClosedError(error),
        };
    if (
      acpChatSessionActions.finishPromptAttemptIfCurrent(sessionId, promptAttemptId, submitError)
    ) {
      void options.onFinish(submitError.message);
    }
  } finally {
    preparingPromptAttempts.delete(promptAttemptId);
    const current = acpChatSessionStore.getSnapshot(sessionId);
    if (!current?.activePromptAttemptId && !current?.pendingCancelPromptAttemptId) {
      await window.electron.setWakelockActive(sessionId, false).catch(() => false);
    }
  }
}

function stop(sessionId: string): void {
  const storedPromptAttemptId = acpChatSessionStore.getSnapshot(sessionId)?.activePromptAttemptId;
  const hasStoredAcpPrompt = storedPromptAttemptId !== null && storedPromptAttemptId !== undefined;

  if (hasStoredAcpPrompt) {
    if (!acpChatSessionActions.startPromptCancellation(sessionId, storedPromptAttemptId)) {
      return;
    }
    if (preparingPromptAttempts.has(storedPromptAttemptId)) {
      return;
    }
    acpCancelPrompt(sessionId)
      .then(() => {
        if (
          acpChatSessionStore.getSnapshot(sessionId)?.pendingCancelPromptAttemptId ===
          storedPromptAttemptId
        ) {
          cancelAcpPermissionRequestsForSession(sessionId);
          cancelAcpElicitationRequestsForSession(sessionId);
        }
      })
      .catch((error) => {
        console.warn('Failed to cancel ACP prompt:', error);
        acpChatSessionActions.restorePromptCancellation(sessionId, storedPromptAttemptId);
      });
    return;
  }

  acpChatSessionActions.setChatState(sessionId, ChatState.Idle);
}

async function updateMessage(
  sessionId: string,
  messageId: string,
  newContent: string,
  editType: 'fork' | 'edit' | undefined,
  options: AcpSubmitMessageOptions
): Promise<void> {
  assertNoPendingPromptCancellation(sessionId);

  const resolvedEditType = editType ?? 'fork';
  const currentSnapshot = options.getCurrentSnapshot();
  const storedSnapshot = acpChatSessionStore.getSnapshot(sessionId);
  const activePromptAttemptId = storedSnapshot?.activePromptAttemptId;
  const currentMessages = currentSnapshot?.messages ?? [];
  const message = currentMessages.find((m) => m.id === messageId);

  if (!message) {
    throw new Error(`Message with id ${messageId} not found in current messages`);
  }

  if (resolvedEditType === 'fork') {
    await forkSessionWithEditedMessage(sessionId, message, newContent);
    return;
  }

  const editSnapshot = currentSnapshot ?? storedSnapshot;
  const isPendingToolPermission =
    editSnapshot?.chatState === ChatState.WaitingForUserInput &&
    getPendingToolConfirmationIds(editSnapshot?.messages ?? []).size > 0;
  const isIdle = editSnapshot?.chatState === ChatState.Idle;
  const pendingToolPermissionPromptAttemptId = isPendingToolPermission
    ? activePromptAttemptId
    : undefined;
  const canEditInPlace = isIdle || pendingToolPermissionPromptAttemptId != null;

  if (!canEditInPlace) {
    return;
  }

  if (pendingToolPermissionPromptAttemptId != null) {
    const cancellation = acpChatSessionActions.startPromptCancellation(
      sessionId,
      pendingToolPermissionPromptAttemptId
    );
    if (!cancellation) {
      throw new Error('Cannot update message while prompt is active');
    }

    const promptCancellationSettled = acpChatSessionActions.waitForPromptCancellation(
      sessionId,
      pendingToolPermissionPromptAttemptId
    );

    try {
      await acpCancelPrompt(sessionId);
    } catch {
      acpChatSessionActions.restorePromptCancellation(
        sessionId,
        pendingToolPermissionPromptAttemptId
      );
      throw new Error('Cannot update message because the active prompt could not be cancelled');
    }

    cancelAcpPermissionRequestsForSession(sessionId);
    cancelAcpElicitationRequestsForSession(sessionId);
    await promptCancellationSettled;
  }

  acpChatSessionActions.setChatState(sessionId, ChatState.Thinking);

  try {
    await acpTruncateSessionConversation(sessionId, message.created);

    const truncatedMessages = currentMessages.filter((m) => m.created < message.created);
    const updatedUserMessage = createUserMessage(newContent);

    for (const content of message.content) {
      if (content.type === 'image') {
        updatedUserMessage.content.push(content);
      }
    }

    const messagesForUI = [...truncatedMessages, updatedUserMessage];
    acpChatSessionActions.setMessages(sessionId, messagesForUI);

    await submitMessage(sessionId, updatedUserMessage, options);
  } catch (error) {
    acpChatSessionActions.setChatState(sessionId, ChatState.Idle);
    throw error;
  }
}

export const acpChatSessionController: AcpChatSessionController = {
  createSession,
  loadSession,
  submitMessage,
  stop,
  updateMessage,
  handoffSession,
};
