import { AppEvents } from '../constants/events';
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { defineMessages, useIntl } from '../i18n';
import { useLocation, useNavigate } from 'react-router-dom';
import { SearchView } from './conversation/SearchView';
import LoadingGosling from './LoadingGosling';
import ProgressiveMessageList from './ProgressiveMessageList';
import { MainPanelLayout } from './Layout/MainPanelLayout';
import ChatInput from './ChatInput';
import { ChatInputCard } from './ChatInputCard';
import { ScrollArea, ScrollAreaHandle } from './ui/scroll-area';
import { useFileDrop } from '../hooks/useFileDrop';
import { ChatState } from '../types/chatState';
import { ChatType } from '../types/chat';
import { useIsMobile } from '../hooks/use-mobile';
import { useNavigationContextSafe } from './Layout/NavigationContext';
import { cn } from '../utils';
import { getMotionAwareScrollBehavior } from '../utils/motion';
import { useChatSession } from '../hooks/useChatSession';
import { acpSetSessionMode, acpUpdateWorkingDir } from '../acp/sessions';
import type { GoslingMode } from '../types/session';
import { useNavigation } from '../hooks/useNavigation';
import {
  getThinkingMessage,
  getTextAndImageContent,
  type Message,
  type UserInput,
} from '../types/message';
import { useAutoSubmit } from '../hooks/useAutoSubmit';
import { Gosling } from './icons';
import EnvironmentBadge from './GoslingSidebar/EnvironmentBadge';
import SessionActionsHeader from './SessionActionsHeader';
import SessionInfoSummary from './SessionInfoSummary';
import WorkingDirectoriesMenu from './WorkingDirectoriesMenu';
import { CredentialProfileSelector } from './bottom_menu/CredentialProfileSelector';
import { useArtifactWorkbench } from '../contexts/ArtifactWorkbenchContext';
import { useAcpChatSessionSnapshot } from '../acp/chatSessionStore';
import { useArtifactRouter } from '../contexts/ArtifactRouterContext';
import ThreadNavigator, { THREAD_TURN_ATTRIBUTE } from './conversation/ThreadNavigator';
import type { SessionExperience } from '../types/sessionExperience';

const i18n = defineMessages({
  failedToLoadSession: {
    id: 'baseChat.failedToLoadSession',
    defaultMessage: 'Failed to Load Session',
  },
  goHome: {
    id: 'baseChat.goHome',
    defaultMessage: 'Go home',
  },
  retry: {
    id: 'baseChat.retry',
    defaultMessage: 'Retry',
  },
  connectionInterrupted: {
    id: 'baseChat.connectionInterrupted',
    defaultMessage: 'Connection interrupted',
  },
  connectionInterruptedBody: {
    id: 'baseChat.connectionInterruptedBody',
    defaultMessage:
      'The connection closed while the task was running. Work completed before the interruption was saved.',
  },
  reconnect: {
    id: 'baseChat.reconnect',
    defaultMessage: 'Reconnect',
  },
  awaitingReply: {
    id: 'baseChat.awaitingReply',
    defaultMessage: 'Waiting for your reply',
  },
  awaitingReplyBody: {
    id: 'baseChat.awaitingReplyBody',
    defaultMessage:
      'Deep Research asked a question above. Answer it and the research will continue and write the report.',
  },
  taskInterrupted: {
    id: 'baseChat.taskInterrupted',
    defaultMessage: 'Task interrupted',
  },
  taskInterruptedBody: {
    id: 'baseChat.taskInterruptedBody',
    defaultMessage: 'Review the saved messages before explicitly resuming the task.',
  },
  resumeTask: {
    id: 'baseChat.resumeTask',
    defaultMessage: 'Resume task',
  },
  taskFailed: {
    id: 'baseChat.taskFailed',
    defaultMessage: 'Task failed',
  },
  researchBadge: {
    id: 'baseChat.researchBadge',
    defaultMessage: 'Deep Research',
  },
});

interface BaseChatProps {
  setChat: (chat: ChatType) => void;
  onMessageSubmit?: (message: string) => void;
  renderHeader?: () => React.ReactNode;
  customChatInputProps?: Record<string, unknown>;
  customMainLayoutProps?: Record<string, unknown>;
  contentClassName?: string;
  disableSearch?: boolean;
  suppressEmptyState: boolean;
  sessionId: string;
  isActiveSession: boolean;
  initialMessage?: UserInput;
  noAutoSubmit?: boolean;
  sessionExperience?: SessionExperience;
}

export default function BaseChat({
  setChat,
  renderHeader,
  customChatInputProps = {},
  customMainLayoutProps = {},
  sessionId,
  initialMessage,
  noAutoSubmit,
  isActiveSession,
  sessionExperience = 'chat',
}: BaseChatProps) {
  const intl = useIntl();
  const location = useLocation();
  const navigate = useNavigate();
  const scrollRef = useRef<ScrollAreaHandle>(null);
  const chatInputRef = useRef<HTMLTextAreaElement>(null);
  const disableAnimation = location.state?.disableAnimation || false;
  const isMobile = useIsMobile();
  const navContext = useNavigationContextSafe();
  const { isOpen: isArtifactWorkbenchOpen, setVisibleSession: setVisibleArtifactSession } =
    useArtifactWorkbench();
  const acpSessionSnapshot = useAcpChatSessionSnapshot(sessionId);
  const { setVisibleSessionArtifacts, setVisibleSessionWorkspaceId } = useArtifactRouter();
  const setView = useNavigation();
  const isNavCollapsed = !navContext?.isNavExpanded;
  const contentClassName = cn('pr-1 pb-10 pt-12', (isMobile || isNavCollapsed) && 'pt-16');
  const { droppedFiles, setDroppedFiles, handleDrop, handleDragOver } = useFileDrop();
  const onStreamFinish = useCallback(() => {}, []);

  const {
    session,
    messages,
    historyHasMore,
    historyLoading,
    chatState,
    updateSession,
    handleSubmit,
    loadOlderMessages,
    loadAllOlderMessages,
    onSteerQueuedMessage,
    submitElicitationResponse,
    stopStreaming,
    sessionLoadError,
    promptError,
    interruptedPrompt,
    retrySessionLoad,
    resumeInterruptedPrompt,
    tokenState,
    notifications: toolCallNotifications,
    pauseQueueOnStop,
    queueProcessingBlocked,
    onMessageUpdate,
  } = useChatSession({
    sessionId,
    onStreamFinish,
  });

  useEffect(() => {
    if (!isActiveSession) return;
    setVisibleSessionWorkspaceId(session?.workspace_id ?? null);
    setVisibleArtifactSession(sessionId, acpSessionSnapshot?.artifacts ?? []);
    setVisibleSessionArtifacts(acpSessionSnapshot?.artifacts ?? []);
    return () => {
      setVisibleSessionArtifacts([]);
      setVisibleSessionWorkspaceId(undefined);
      setVisibleArtifactSession(null, []);
    };
  }, [
    acpSessionSnapshot?.artifacts,
    isActiveSession,
    session?.workspace_id,
    sessionId,
    setVisibleArtifactSession,
    setVisibleSessionArtifacts,
    setVisibleSessionWorkspaceId,
  ]);

  const handleWorkingDirChange = useCallback(
    async (newDir: string) => {
      if (!session) {
        throw new Error('Cannot update working directory before ACP session is loaded');
      }
      await acpUpdateWorkingDir(session.id, newDir);
      updateSession((currentSession) => ({ ...currentSession, working_dir: newDir }));
    },
    [session, updateSession]
  );

  const handleGoslingModeChange = useCallback(
    async (newMode: GoslingMode) => {
      if (!session) {
        throw new Error('Cannot update session mode before ACP session is loaded');
      }
      await acpSetSessionMode(session.id, newMode);
      updateSession((currentSession) => ({ ...currentSession, gosling_mode: newMode }));
    },
    [session, updateSession]
  );

  // noAutoSubmit only suppresses auto-submitting the initial prompt of a fresh session
  // (gosling://new-session?prompt=...). Once the conversation has messages, later flows
  // such as forks or resumes should auto-submit normally.
  const canAutoSubmit = !(noAutoSubmit && messages.length === 0);

  useAutoSubmit({
    sessionId,
    session,
    messages,
    chatState,
    initialMessage,
    canAutoSubmit,
    handleSubmit,
  });

  useEffect(() => {
    let streamState: 'idle' | 'loading' | 'streaming' | 'error' = 'idle';
    if (chatState === ChatState.LoadingConversation) {
      streamState = 'loading';
    } else if (
      chatState === ChatState.Streaming ||
      chatState === ChatState.Thinking ||
      chatState === ChatState.Compacting
    ) {
      streamState = 'streaming';
    } else if (sessionLoadError || promptError) {
      streamState = 'error';
    }

    window.dispatchEvent(
      new CustomEvent(AppEvents.SESSION_STATUS_UPDATE, {
        detail: {
          sessionId,
          workspaceId: session?.workspace_id ?? null,
          streamState,
          messageCount: messages.length,
        },
      })
    );
  }, [sessionId, session?.workspace_id, chatState, messages.length, promptError, sessionLoadError]);

  // Generate command history from user messages (most recent first)
  const commandHistory = useMemo(() => {
    return messages
      .reduce<string[]>((history, message) => {
        if (message.role === 'user') {
          const text = getTextAndImageContent(message).textContent.trim();
          if (text) {
            history.push(text);
          }
        }
        return history;
      }, [])
      .reverse();
  }, [messages]);

  const chatInputSubmit = useCallback(
    (input: UserInput) => {
      scrollRef.current?.scrollToBottom('auto');
      void handleSubmit(input);
      window.requestAnimationFrame(() => scrollRef.current?.scrollToBottom('auto'));
    },
    [handleSubmit]
  );

  const sessionModel = session?.model_config?.model_name ?? null;
  const sessionProvider = session?.provider_name ?? null;
  const sessionLoaded = session !== undefined;
  const latestInference = useMemo(() => {
    for (let i = messages.length - 1; i >= 0; i--) {
      const message = messages[i];
      if (
        message.role === 'assistant' &&
        message.metadata.userVisible &&
        message.metadata.inference
      ) {
        return message.metadata.inference;
      }
    }
    return null;
  }, [messages]);

  // Track if this is the initial render for session resuming
  const initialRenderRef = useRef(true);
  const initialSessionScrollRef = useRef<string | null>(null);

  const requestScrollToBottom = useCallback(
    (delayMs = 0, behavior: 'auto' | 'smooth' = 'smooth') => {
      window.setTimeout(() => {
        scrollRef.current?.scrollToBottom?.(behavior);
      }, delayMs);
    },
    []
  );

  useEffect(() => {
    initialRenderRef.current = true;
    initialSessionScrollRef.current = null;
  }, [sessionId]);

  useEffect(() => {
    if (
      !sessionId ||
      messages.length === 0 ||
      chatState === ChatState.LoadingConversation ||
      initialSessionScrollRef.current === sessionId
    ) {
      return;
    }

    initialSessionScrollRef.current = sessionId;
    requestScrollToBottom(0, 'auto');
    requestScrollToBottom(150, 'auto');
    requestScrollToBottom(500, 'auto');
  }, [sessionId, messages.length, chatState, requestScrollToBottom]);

  // Auto-scroll when messages are loaded (for session resuming)
  const handleRenderingComplete = React.useCallback(() => {
    // Only force scroll on the very first render
    if (initialRenderRef.current && messages.length > 0) {
      initialRenderRef.current = false;
      requestScrollToBottom(0, 'auto');
    } else if (scrollRef.current?.isFollowing) {
      requestScrollToBottom(0, 'auto');
    }
  }, [messages.length, requestScrollToBottom]);

  const handleHistoryScroll = useCallback(
    (viewport: HTMLDivElement) => {
      if (viewport.scrollTop < 240 && historyHasMore && !historyLoading) {
        void loadOlderMessages();
      }
    },
    [historyHasMore, historyLoading, loadOlderMessages]
  );

  // Listen for global scroll-to-bottom requests (e.g., from MCP App message actions)
  useEffect(() => {
    const handleGlobalScrollRequest = () => {
      requestScrollToBottom(200);
    };

    window.addEventListener(AppEvents.SCROLL_CHAT_TO_BOTTOM, handleGlobalScrollRequest);
    return () =>
      window.removeEventListener(AppEvents.SCROLL_CHAT_TO_BOTTOM, handleGlobalScrollRequest);
  }, [requestScrollToBottom]);

  useEffect(() => {
    if (
      isActiveSession &&
      sessionId &&
      chatInputRef.current &&
      chatState !== ChatState.LoadingConversation
    ) {
      const timeoutId = setTimeout(() => {
        chatInputRef.current?.focus();
      }, 100);
      return () => clearTimeout(timeoutId);
    }
    return undefined;
  }, [isActiveSession, sessionId, chatState]);

  useEffect(() => {
    const handleSessionForked = (event: Event) => {
      const customEvent = event as CustomEvent<{
        newSessionId: string;
        shouldStartAgent?: boolean;
        editedMessage?: string;
      }>;
      window.dispatchEvent(new CustomEvent(AppEvents.SESSION_CREATED));
      const { newSessionId, shouldStartAgent, editedMessage } = customEvent.detail;

      const params = new URLSearchParams();
      params.set('resumeSessionId', newSessionId);
      if (shouldStartAgent) {
        params.set('shouldStartAgent', 'true');
      }

      navigate(`/pair?${params.toString()}`, {
        state: {
          disableAnimation: true,
          initialMessage: editedMessage ? { msg: editedMessage, images: [] } : undefined,
        },
      });
    };

    window.addEventListener(AppEvents.SESSION_FORKED, handleSessionForked);

    return () => {
      window.removeEventListener(AppEvents.SESSION_FORKED, handleSessionForked);
    };
  }, [location.pathname, navigate]);

  const lastSetNameRef = useRef<string>('');

  useEffect(() => {
    const currentSessionName = session?.name;
    if (currentSessionName && currentSessionName !== lastSetNameRef.current) {
      lastSetNameRef.current = currentSessionName;
      setChat({
        messages,
        sessionId,
        name: currentSessionName,
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session?.name, setChat]);

  const initialPrompt =
    noAutoSubmit && messages.length === 0 && initialMessage?.msg ? initialMessage.msg : '';

  const [threadViewport, setThreadViewport] = useState<HTMLDivElement | null>(null);
  const [threadNavigationRenderSessionId, setThreadNavigationRenderSessionId] = useState<
    string | null
  >(null);
  const pendingThreadNavigationRef = useRef<'latest' | number | null>(null);

  useEffect(() => {
    setThreadViewport(scrollRef.current?.viewportRef.current ?? null);
  }, [sessionId, messages.length]);

  const handleJumpToThreadStart = useCallback(async () => {
    const reachedStart = await loadAllOlderMessages();
    if (!reachedStart) {
      return;
    }
    window.requestAnimationFrame(() => {
      scrollRef.current?.scrollToPosition({
        top: 0,
        behavior: getMotionAwareScrollBehavior(),
      });
    });
  }, [loadAllOlderMessages]);

  const handleJumpToThreadLatest = useCallback(() => {
    if (threadNavigationRenderSessionId === sessionId) {
      scrollRef.current?.scrollToBottom(getMotionAwareScrollBehavior());
      return;
    }
    setThreadNavigationRenderSessionId(sessionId);
    pendingThreadNavigationRef.current = 'latest';
  }, [sessionId, threadNavigationRenderSessionId]);

  const handleThreadTurnsRendered = useCallback(() => {
    if (pendingThreadNavigationRef.current === 'latest') {
      pendingThreadNavigationRef.current = null;
      scrollRef.current?.scrollToBottom(getMotionAwareScrollBehavior());
      return;
    }

    if (typeof pendingThreadNavigationRef.current === 'number') {
      const messageIndex = pendingThreadNavigationRef.current;
      pendingThreadNavigationRef.current = null;
      const viewport = scrollRef.current?.viewportRef.current;
      viewport
        ?.querySelector<HTMLElement>(`[${THREAD_TURN_ATTRIBUTE}="${messageIndex}"]`)
        ?.scrollIntoView({ behavior: getMotionAwareScrollBehavior(), block: 'center' });
    }
  }, []);

  if (sessionLoadError) {
    return (
      <div className="h-full flex flex-col min-h-0">
        <MainPanelLayout
          backgroundColor={'bg-background-primary'}
          removeTopPadding={true}
          {...customMainLayoutProps}
        >
          {renderHeader && renderHeader()}
          <div className="flex flex-col flex-1 min-h-0 relative">
            <div className="flex-1 flex items-center justify-center">
              <div className="flex flex-col items-center justify-center p-8">
                <div className="text-red-700 dark:text-red-300 bg-red-400/50 p-4 rounded-lg mb-4 max-w-md">
                  <h3 className="font-semibold mb-2">
                    {intl.formatMessage(i18n.failedToLoadSession)}
                  </h3>
                  <p className="text-sm">{sessionLoadError}</p>
                </div>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => void retrySessionLoad()}
                    className="px-4 py-2 text-center cursor-pointer text-text-primary border border-border-primary hover:bg-background-secondary rounded-lg transition-all duration-150"
                  >
                    {intl.formatMessage(i18n.retry)}
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setView('chat');
                    }}
                    className="px-4 py-2 text-center cursor-pointer text-text-primary border border-border-primary hover:bg-background-secondary rounded-lg transition-all duration-150"
                  >
                    {intl.formatMessage(i18n.goHome)}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </MainPanelLayout>
      </div>
    );
  }

  return (
    <div
      className="h-full flex flex-col min-h-0"
      data-session-experience={session?.research_library_path ? 'research' : sessionExperience}
    >
      <MainPanelLayout
        backgroundColor={'bg-background-primary'}
        removeTopPadding={true}
        {...customMainLayoutProps}
      >
        {/* Custom header */}
        {renderHeader && renderHeader()}

        <div className="flex flex-col flex-1 min-h-0 relative">
          {/* Gosling watermark - top right */}
          <div className="pointer-events-none absolute top-[14px] right-4 z-[60] flex flex-col items-end gap-2">
            <div
              className={cn(
                'pointer-events-auto flex flex-row items-center gap-2',
                !isArtifactWorkbenchOpen && 'mr-10'
              )}
            >
              <a
                href="https://github.com/cephalopod-ai/gosling"
                target="_blank"
                rel="noopener noreferrer"
                className="no-drag flex flex-row items-center gap-1 hover:opacity-80 transition-opacity"
              >
                <Gosling className="size-5 gosling-icon-animation" />
                <span className="text-sm leading-none text-text-secondary -translate-y-px">
                  gosling
                </span>
              </a>
              <EnvironmentBadge className="translate-y-px" />
              {(sessionExperience === 'research' || session?.research_library_path) && (
                <span
                  className="rounded-full border border-border-primary bg-background-secondary px-2 py-1 text-[10px] font-medium uppercase tracking-wide text-text-secondary"
                  data-session-tag="research"
                >
                  {intl.formatMessage(i18n.researchBadge)}
                </span>
              )}
              <WorkingDirectoriesMenu session={session} onSessionChange={updateSession} compact />
              <CredentialProfileSelector
                credentialProfileId={session?.credential_profile_id}
                credentialProfileName={session?.credential_profile_name}
                surface="header"
              />
            </div>
          </div>

          <SessionActionsHeader session={session} onSessionChange={updateSession} />

          <ScrollArea
            ref={scrollRef}
            className={`flex-1 min-h-0 relative ${contentClassName}`}
            autoScroll
            onDrop={handleDrop}
            onDragOver={handleDragOver}
            handleScroll={handleHistoryScroll}
            data-drop-zone="true"
            paddingX={6}
            paddingY={0}
          >
            {messages.length > 0 ? (
              <>
                <SearchView className="pr-9">
                  {historyHasMore ? (
                    <div className="flex justify-center py-3">
                      <button
                        type="button"
                        className="rounded border border-border-primary px-3 py-1 text-xs text-text-secondary hover:bg-background-secondary disabled:opacity-60"
                        disabled={historyLoading}
                        onClick={() => void loadOlderMessages()}
                      >
                        {historyLoading ? 'Loading older history' : 'Older history available'}
                      </button>
                    </div>
                  ) : null}
                  <ProgressiveMessageList
                    key={sessionId}
                    messages={messages}
                    chat={{ sessionId }}
                    toolCallNotifications={toolCallNotifications}
                    append={(text: string) => handleSubmit({ msg: text, images: [] })}
                    isUserMessage={(m: Message) => m.role === 'user'}
                    isStreamingMessage={chatState !== ChatState.Idle}
                    onRenderingComplete={handleRenderingComplete}
                    onMessageUpdate={onMessageUpdate}
                    submitElicitationResponse={submitElicitationResponse}
                    workingDirectory={session?.working_dir}
                    workspaceId={session?.workspace_id ?? undefined}
                    threadTurnAttribute={THREAD_TURN_ATTRIBUTE}
                    forceRenderAll={threadNavigationRenderSessionId === sessionId}
                    onThreadTurnsRendered={handleThreadTurnsRendered}
                  />
                </SearchView>

                <div className="block h-8" />
              </>
            ) : null}
          </ScrollArea>

          <ThreadNavigator
            messages={messages}
            viewport={threadViewport}
            historyHasMore={historyHasMore}
            historyLoading={historyLoading}
            onPrepareNavigation={(messageIndex) => {
              pendingThreadNavigationRef.current = messageIndex;
              setThreadNavigationRenderSessionId(sessionId);
            }}
            onJumpToStart={handleJumpToThreadStart}
            onJumpToLatest={handleJumpToThreadLatest}
          />

          {chatState !== ChatState.Idle && (
            <div className="absolute bottom-1 left-4 z-20 pointer-events-none">
              <LoadingGosling
                chatState={chatState}
                message={
                  messages.length > 0
                    ? getThinkingMessage(messages[messages.length - 1])
                    : undefined
                }
              />
            </div>
          )}
        </div>

        {(promptError || interruptedPrompt) && (
          <div
            role={promptError?.awaitingReply ? 'status' : 'alert'}
            data-prompt-state={promptError?.awaitingReply ? 'awaiting-reply' : undefined}
            className={`mx-4 mb-2 flex items-center justify-between gap-4 rounded-lg border px-4 py-3 text-text-primary ${
              promptError?.awaitingReply
                ? 'border-border-primary bg-background-secondary'
                : 'border-border-warning bg-background-warning/20'
            }`}
          >
            <div className="min-w-0">
              <p className="text-sm font-semibold">
                {intl.formatMessage(
                  promptError?.awaitingReply
                    ? i18n.awaitingReply
                    : promptError?.connectionLost
                      ? i18n.connectionInterrupted
                      : promptError
                        ? i18n.taskFailed
                        : i18n.taskInterrupted
                )}
              </p>
              <p className="mt-1 text-xs text-text-secondary">
                {promptError?.awaitingReply
                  ? intl.formatMessage(i18n.awaitingReplyBody)
                  : promptError?.connectionLost
                    ? intl.formatMessage(i18n.connectionInterruptedBody)
                    : promptError?.message || intl.formatMessage(i18n.taskInterruptedBody)}
              </p>
            </div>
            {promptError?.connectionLost ? (
              <button
                type="button"
                onClick={() => void retrySessionLoad()}
                className="shrink-0 rounded-md border border-border-primary px-3 py-1.5 text-sm hover:bg-background-secondary"
              >
                {intl.formatMessage(i18n.reconnect)}
              </button>
            ) : interruptedPrompt && !promptError && chatState === ChatState.Idle ? (
              <button
                type="button"
                onClick={() => void resumeInterruptedPrompt()}
                className="shrink-0 rounded-md border border-border-primary px-3 py-1.5 text-sm hover:bg-background-secondary"
              >
                {intl.formatMessage(i18n.resumeTask)}
              </button>
            ) : null}
          </div>
        )}

        <ChatInputCard
          className={cn(
            'relative z-10 mx-4 mb-4',
            !disableAnimation && 'animate-[fadein_400ms_ease-in_forwards]'
          )}
        >
          <ChatInput
            inputRef={chatInputRef}
            sessionId={sessionId}
            handleSubmit={chatInputSubmit}
            chatState={chatState}
            onStop={stopStreaming}
            onSteerQueuedMessage={onSteerQueuedMessage}
            pauseQueueOnStop={pauseQueueOnStop}
            queueProcessingBlocked={queueProcessingBlocked}
            commandHistory={commandHistory}
            initialValue={initialPrompt}
            setView={setView}
            totalTokens={tokenState?.totalTokens ?? session?.usage?.total_tokens ?? undefined}
            accumulatedInputTokens={
              tokenState?.accumulatedInputTokens ??
              session?.accumulated_usage?.input_tokens ??
              undefined
            }
            accumulatedOutputTokens={
              tokenState?.accumulatedOutputTokens ??
              session?.accumulated_usage?.output_tokens ??
              undefined
            }
            accumulatedCost={tokenState?.accumulatedCost ?? session?.accumulated_cost ?? undefined}
            droppedFiles={droppedFiles}
            onFilesProcessed={() => setDroppedFiles([])} // Clear dropped files after processing
            messages={messages}
            disableAnimation={disableAnimation}
            initialPrompt={initialPrompt}
            sessionModel={sessionModel}
            sessionProvider={sessionProvider}
            sessionLoaded={sessionLoaded}
            workingDir={session?.working_dir}
            onWorkingDirChange={handleWorkingDirChange}
            renderChatInfo={(close) => (
              <SessionInfoSummary
                session={session}
                onSessionChange={updateSession}
                liveTotalTokens={tokenState?.totalTokens}
                liveAccumulatedCost={tokenState?.accumulatedCost}
                onCollapse={close}
              />
            )}
            goslingMode={session?.gosling_mode}
            onGoslingModeChange={handleGoslingModeChange}
            latestInference={latestInference}
            {...customChatInputProps}
          />
        </ChatInputCard>
      </MainPanelLayout>
    </div>
  );
}
