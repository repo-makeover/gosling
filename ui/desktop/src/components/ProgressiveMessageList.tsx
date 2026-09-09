/**
 * ProgressiveMessageList Component
 *
 * A performance-optimized message list that renders messages progressively
 * to prevent UI blocking when loading long chat sessions. This component
 * renders messages in batches with a loading indicator, maintaining full
 * compatibility with the search functionality.
 *
 * Key Features:
 * - Progressive rendering in configurable batches
 * - Loading indicator during batch processing
 * - Maintains search functionality compatibility
 * - Smooth user experience with responsive UI
 * - Configurable batch size and delay
 */

import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { defineMessages, useIntl } from '../i18n';
import GoslingMessage from './GoslingMessage';
import UserMessage from './UserMessage';
import {
  SystemNotificationInline,
  getInlineSystemNotification,
} from './context_management/SystemNotificationInline';
import {
  CreditsExhaustedNotification,
  getCreditsExhaustedNotification,
} from './context_management/CreditsExhaustedNotification';
import {
  getAnyToolConfirmationData,
  getPendingToolConfirmationIds,
  getTextAndImageContent,
  getToolRequests,
  getToolResponses,
  type Message,
  type NotificationEvent,
  type SystemNotificationContent,
  type ToolConfirmationData,
  type ToolResponseMessageContent,
} from '../types/message';
import LoadingGosling from './LoadingGosling';
import { ChatType } from '../types/chat';
import {
  identifyCollapsibleToolActivityGroups,
  identifyConsecutiveToolCalls,
} from '../utils/toolCallChaining';
import { getModelDisplayName } from './settings/models/predefinedModelsUtils';
import ToolActivityGroup from './ToolActivityGroup';

const i18n = defineMessages({
  loadingMessages: {
    id: 'progressiveMessageList.loadingMessages',
    defaultMessage: 'Loading messages... ({renderedCount}/{totalCount})',
  },
  searchHint: {
    id: 'progressiveMessageList.searchHint',
    defaultMessage: 'Press Cmd/Ctrl+F to load all messages immediately for search',
  },
  modelChanged: {
    id: 'progressiveMessageList.modelChanged',
    defaultMessage: 'Model changed: {previousModel} → {currentModel}',
  },
});

interface ProgressiveMessageListProps {
  messages: Message[];
  chat: Pick<ChatType, 'sessionId'>;
  toolCallNotifications?: Map<string, NotificationEvent[]>; // Make optional
  append?: (value: string) => void; // Make optional
  isUserMessage: (message: Message) => boolean;
  batchSize?: number;
  batchDelay?: number;
  showLoadingThreshold?: number; // Only show loading if more than X messages
  // Custom render function for messages
  renderMessage?: (message: Message, index: number) => React.ReactNode | null;
  isStreamingMessage?: boolean; // Whether messages are currently being streamed
  onMessageUpdate?: (messageId: string, newContent: string, editType?: 'fork' | 'edit') => void;
  onRenderingComplete?: () => void; // Callback when all messages are rendered
  submitElicitationResponse?: (
    elicitationId: string,
    userData: Record<string, unknown>
  ) => Promise<boolean>;
  workingDirectory?: string;
  workspaceId?: string;
  threadTurnAttribute?: string;
  forceRenderAll?: boolean;
  onThreadTurnsRendered?: () => void;
  /** Ids of local steer echoes the agent hasn't applied yet — see `AcpChatSessionSnapshot.pendingSteerMessageIds`. */
  pendingSteerMessageIds?: ReadonlySet<string>;
}

interface MessageRenderIndex {
  collapsibleToolCallGroupIndexes: Set<number>;
  collapsibleToolCallGroupsByStart: Map<number, number[]>;
  confirmationByToolRequestId: Map<string, ToolConfirmationData>;
  hiddenTimestampIndexes: Set<number>;
  pendingConfirmationIds: Set<string>;
  previousResolvedModelByIndex: Array<string | null>;
  hasModelSwitchSincePreviousResolvedModelByIndex: boolean[];
  toolRequestIds: Set<string>;
  toolResponseByRequestId: Map<string, ToolResponseMessageContent>;
  toolCallChainIndexes: Set<number>;
}

function hasOnlyToolResponses(message: Message): boolean {
  return message.content.every((content) => content.type === 'toolResponse');
}

function isModelSwitchNotification(message: Message): boolean {
  return message.content.some((content) => {
    if (content.type !== 'systemNotification' || content.notificationType !== 'inlineMessage') {
      return false;
    }
    return (
      (typeof content.data === 'object' &&
        content.data !== null &&
        'kind' in content.data &&
        content.data.kind === 'modelSwitch') ||
      content.msg.startsWith('Model changed:')
    );
  });
}

export default function ProgressiveMessageList({
  messages,
  chat,
  toolCallNotifications = new Map(),
  append = () => {},
  isUserMessage,
  batchSize = 20,
  batchDelay = 20,
  showLoadingThreshold = 50,
  renderMessage, // Custom render function
  isStreamingMessage = false, // Whether messages are currently being streamed
  onMessageUpdate,
  onRenderingComplete,
  submitElicitationResponse,
  workingDirectory,
  workspaceId,
  threadTurnAttribute,
  forceRenderAll = false,
  onThreadTurnsRendered,
  pendingSteerMessageIds,
}: ProgressiveMessageListProps) {
  const intl = useIntl();
  const [renderedCount, setRenderedCount] = useState(() => {
    // Initialize with either all messages (if small) or first batch (if large)
    return forceRenderAll || messages.length <= showLoadingThreshold
      ? messages.length
      : Math.min(batchSize, messages.length);
  });
  const [isLoading, setIsLoading] = useState(
    () => !forceRenderAll && messages.length > showLoadingThreshold
  );
  const timeoutRef = useRef<number | null>(null);
  const mountedRef = useRef(true);
  const onThreadTurnsRenderedRef = useRef(onThreadTurnsRendered);
  onThreadTurnsRenderedRef.current = onThreadTurnsRendered;
  const getResolvedModel = useCallback((message: Message): string | null => {
    if (message.role !== 'assistant' || !message.metadata.userVisible) return null;
    return message.metadata.inference?.resolvedModel ?? null;
  }, []);

  const renderModelChangeDisclosure = useCallback(
    (previousModel: string, currentModel: string) => (
      <SystemNotificationInline
        notification={{
          msg: intl.formatMessage(i18n.modelChanged, {
            previousModel: getModelDisplayName(previousModel),
            currentModel: getModelDisplayName(currentModel),
          }),
          notificationType: 'inlineMessage',
        }}
      />
    ),
    [intl]
  );

  const getSystemNotification = (message: Message): SystemNotificationContent | undefined => {
    return getCreditsExhaustedNotification(message) ?? getInlineSystemNotification(message);
  };

  const renderSystemNotification = (notification: SystemNotificationContent) => {
    switch (notification.notificationType) {
      case 'creditsExhausted':
        return <CreditsExhaustedNotification notification={notification} />;
      case 'inlineMessage':
        return <SystemNotificationInline notification={notification} />;
      default:
        return null;
    }
  };

  useEffect(() => {
    if (!forceRenderAll) {
      return;
    }
    if (timeoutRef.current) {
      window.clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
    }
    setRenderedCount(messages.length);
    setIsLoading(false);
    window.requestAnimationFrame(() => onThreadTurnsRenderedRef.current?.());
  }, [forceRenderAll, messages.length]);

  // Simple progressive loading - start immediately when component mounts if needed
  useEffect(() => {
    if (forceRenderAll || messages.length <= showLoadingThreshold) {
      setRenderedCount(messages.length);
      setIsLoading(false);
      // For small lists, call completion callback immediately
      if (onRenderingComplete) {
        setTimeout(() => onRenderingComplete(), 50);
      }
      return;
    }

    // Large list - start progressive loading
    const loadNextBatch = () => {
      setRenderedCount((current) => {
        const nextCount = Math.min(current + batchSize, messages.length);

        if (nextCount >= messages.length) {
          setIsLoading(false);
          // Call the completion callback after a brief delay to ensure DOM is updated
          if (onRenderingComplete) {
            setTimeout(() => onRenderingComplete(), 50);
          }
        } else {
          // Schedule next batch
          timeoutRef.current = window.setTimeout(loadNextBatch, batchDelay);
        }

        return nextCount;
      });
    };

    // Start loading after a short delay
    timeoutRef.current = window.setTimeout(loadNextBatch, batchDelay);

    return () => {
      if (timeoutRef.current) {
        window.clearTimeout(timeoutRef.current);
        timeoutRef.current = null;
      }
    };
  }, [
    messages.length,
    batchSize,
    batchDelay,
    showLoadingThreshold,
    forceRenderAll,
    renderedCount,
    onRenderingComplete,
  ]);

  // Cleanup on unmount
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (timeoutRef.current) {
        window.clearTimeout(timeoutRef.current);
      }
    };
  }, []);

  // Force complete rendering when search is active
  useEffect(() => {
    // Only add listener if we're actually loading
    if (!isLoading) {
      return;
    }

    const handleKeyDown = (e: KeyboardEvent) => {
      const isMac = window.electron.platform === 'darwin';
      const isSearchShortcut = (isMac ? e.metaKey : e.ctrlKey) && e.key === 'f';

      if (isSearchShortcut) {
        // Immediately render all messages when search is triggered
        setRenderedCount(messages.length);
        setIsLoading(false);
        if (timeoutRef.current) {
          window.clearTimeout(timeoutRef.current);
          timeoutRef.current = null;
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isLoading, messages.length]);

  const lastUserPromptIndex = useMemo(() => {
    for (let index = messages.length - 1; index >= 0; index -= 1) {
      const message = messages[index];
      if (isUserMessage(message) && !hasOnlyToolResponses(message)) {
        return index;
      }
    }
    return -1;
  }, [messages, isUserMessage]);

  // A steer appends its local echo (a user message) to the end of the array
  // while the assistant message it interrupted is still receiving updates in
  // its own slot — so "last array index" no longer identifies the message
  // that is actually streaming. Track the last assistant message instead.
  const lastAssistantMessageIndex = useMemo(() => {
    for (let index = messages.length - 1; index >= 0; index -= 1) {
      if (messages[index].role === 'assistant') {
        return index;
      }
    }
    return -1;
  }, [messages]);

  const messageRenderIndex = useMemo<MessageRenderIndex>(() => {
    const toolResponseByRequestId = new Map<string, ToolResponseMessageContent>();
    const confirmationByToolRequestId = new Map<string, ToolConfirmationData>();
    const toolRequestIds = new Set<string>();
    const previousResolvedModelByIndex = new Array<string | null>(messages.length).fill(null);
    const hasModelSwitchSincePreviousResolvedModelByIndex = new Array<boolean>(
      messages.length
    ).fill(false);
    let previousResolvedModel: string | null = null;
    let hasModelSwitchSincePreviousResolvedModel = false;

    for (const [index, message] of messages.entries()) {
      previousResolvedModelByIndex[index] = previousResolvedModel;
      hasModelSwitchSincePreviousResolvedModelByIndex[index] =
        hasModelSwitchSincePreviousResolvedModel;

      const resolvedModel = getResolvedModel(message);
      const isModelSwitch = isModelSwitchNotification(message);
      if (resolvedModel) {
        previousResolvedModel = resolvedModel;
      }
      if (isModelSwitch) {
        hasModelSwitchSincePreviousResolvedModel = true;
      } else if (resolvedModel) {
        hasModelSwitchSincePreviousResolvedModel = false;
      }

      for (const request of getToolRequests(message)) {
        toolRequestIds.add(request.id);
      }

      for (const response of getToolResponses(message)) {
        if (!toolResponseByRequestId.has(response.id)) {
          toolResponseByRequestId.set(response.id, response);
        }
      }

      const confirmationData = getAnyToolConfirmationData(message);
      if (confirmationData && !confirmationByToolRequestId.has(confirmationData.id)) {
        confirmationByToolRequestId.set(confirmationData.id, confirmationData);
      }
    }

    const toolCallChains = identifyConsecutiveToolCalls(messages);
    const toolCallChainIndexes = new Set<number>();
    const hiddenTimestampIndexes = new Set<number>();
    const collapsibleToolCallGroups = identifyCollapsibleToolActivityGroups(messages);
    const collapsibleToolCallGroupIndexes = new Set<number>();
    const collapsibleToolCallGroupsByStart = new Map<number, number[]>();

    for (const chain of toolCallChains) {
      for (const index of chain) {
        toolCallChainIndexes.add(index);
      }

      for (const index of chain.slice(0, -1)) {
        hiddenTimestampIndexes.add(index);
      }
    }

    for (const group of collapsibleToolCallGroups) {
      collapsibleToolCallGroupsByStart.set(group[0], group);
      for (const index of group) {
        collapsibleToolCallGroupIndexes.add(index);
      }
    }

    return {
      collapsibleToolCallGroupIndexes,
      collapsibleToolCallGroupsByStart,
      confirmationByToolRequestId,
      hiddenTimestampIndexes,
      hasModelSwitchSincePreviousResolvedModelByIndex,
      pendingConfirmationIds: getPendingToolConfirmationIds(messages),
      previousResolvedModelByIndex,
      toolRequestIds,
      toolResponseByRequestId,
      toolCallChainIndexes,
    };
  }, [getResolvedModel, messages]);

  // Render messages up to the current rendered count
  const renderMessages = useCallback(() => {
    const messagesToRender = messages.slice(0, renderedCount);
    const renderModelDisclosure = (message: Message, index: number) => {
      const currentResolvedModel = getResolvedModel(message);
      const previousResolvedModel = currentResolvedModel
        ? messageRenderIndex.previousResolvedModelByIndex[index]
        : null;
      const showModelChangeDisclosure = Boolean(
        currentResolvedModel &&
        previousResolvedModel &&
        currentResolvedModel !== previousResolvedModel &&
        !messageRenderIndex.hasModelSwitchSincePreviousResolvedModelByIndex[index]
      );

      return showModelChangeDisclosure && currentResolvedModel && previousResolvedModel
        ? renderModelChangeDisclosure(previousResolvedModel, currentResolvedModel)
        : null;
    };

    const renderDefaultMessage = (
      message: Message,
      index: number,
      showDisclosure = true,
      suppressTopMargin = false
    ) => {
      const notification = getSystemNotification(message);
      if (notification) {
        return (
          <div
            key={`notification-${message.id ?? `msg-${index}-${message.created}`}`}
            className={`relative ${index === 0 ? 'mt-0' : 'mt-4'} assistant`}
            data-testid="message-container"
          >
            {renderSystemNotification(notification)}
          </div>
        );
      }

      const isUser = isUserMessage(message);
      const messageIsInChain = messageRenderIndex.toolCallChainIndexes.has(index);
      const messageKey = message.id ?? `msg-${index}-${message.created}`;

      return (
        <Fragment key={messageKey}>
          {showDisclosure && renderModelDisclosure(message, index)}
          <div
            className={`relative ${index === 0 || suppressTopMargin ? 'mt-0' : 'mt-4'} ${isUser ? 'user' : 'assistant'} ${messageIsInChain ? 'in-chain' : ''}`}
            data-testid="message-container"
            {...(isUser && threadTurnAttribute ? { [threadTurnAttribute]: String(index) } : {})}
          >
            {isUser ? (
              !hasOnlyToolResponses(message) && (
                <UserMessage
                  message={message}
                  canRetry={!isStreamingMessage && index === lastUserPromptIndex}
                  isQueuedSteer={Boolean(message.id && pendingSteerMessageIds?.has(message.id))}
                  onMessageUpdate={onMessageUpdate}
                />
              )
            ) : (
              <GoslingMessage
                sessionId={chat.sessionId}
                message={message}
                hideTimestamp={messageRenderIndex.hiddenTimestampIndexes.has(index)}
                toolResponsesById={messageRenderIndex.toolResponseByRequestId}
                confirmationByToolRequestId={messageRenderIndex.confirmationByToolRequestId}
                pendingConfirmationIds={messageRenderIndex.pendingConfirmationIds}
                toolRequestIds={messageRenderIndex.toolRequestIds}
                append={append}
                toolCallNotifications={toolCallNotifications}
                isStreaming={
                  isStreamingMessage &&
                  !isUser &&
                  index === lastAssistantMessageIndex &&
                  message.role === 'assistant'
                }
                submitElicitationResponse={submitElicitationResponse}
                workingDirectory={workingDirectory}
                workspaceId={workspaceId}
              />
            )}
          </div>
        </Fragment>
      );
    };

    return messagesToRender
      .map((message, index) => {
        if (!message.metadata.userVisible) {
          return null;
        }
        if (renderMessage) {
          return renderMessage(message, index);
        }

        // Default rendering logic (for BaseChat)
        if (!chat) {
          console.warn(
            'ProgressiveMessageList: chat prop is required when not using custom renderMessage'
          );
          return null;
        }

        const activityGroup = messageRenderIndex.collapsibleToolCallGroupsByStart.get(index);
        if (activityGroup) {
          const visibleIndexes = activityGroup.filter(
            (messageIndex) => messageIndex < messagesToRender.length
          );
          const toolCount = activityGroup.reduce(
            (count, messageIndex) => count + getToolRequests(messages[messageIndex]).length,
            0
          );
          const hasPendingApproval = activityGroup.some((messageIndex) =>
            getToolRequests(messages[messageIndex]).some((request) =>
              messageRenderIndex.pendingConfirmationIds.has(request.id)
            )
          );
          const lastActivityIndex = activityGroup[activityGroup.length - 1];
          const isStreamingActivity =
            isStreamingMessage &&
            // A steer's local echo (a user message) can land right after this
            // group while the tools it's waiting on are still running — that
            // doesn't mean the assistant has moved past this activity, so it's
            // excluded rather than treated as evidence the group has closed.
            messages
              .slice(lastActivityIndex + 1)
              .filter((candidate) => !isUserMessage(candidate))
              .every((candidate) => {
                const { imagePaths, textContent } = getTextAndImageContent(candidate);
                return (
                  candidate.metadata.userVisible &&
                  getToolResponses(candidate).length > 0 &&
                  getToolRequests(candidate).length === 0 &&
                  !textContent.trim() &&
                  imagePaths.length === 0
                );
              });
          const activityRequests = activityGroup.flatMap((messageIndex) =>
            getToolRequests(messages[messageIndex])
          );
          const hasError = activityRequests.some((request) => {
            const response = messageRenderIndex.toolResponseByRequestId.get(request.id);
            return (
              (response?.toolResult as Record<string, unknown> | undefined)?.status === 'error'
            );
          });
          const hasMissingResponse = activityRequests.some(
            (request) => !messageRenderIndex.toolResponseByRequestId.has(request.id)
          );
          const activityStatus = hasError
            ? 'error'
            : isStreamingActivity && hasMissingResponse
              ? 'loading'
              : hasMissingResponse
                ? 'pending'
                : 'success';

          return (
            <Fragment key={`activity-${message.id ?? index}`}>
              {visibleIndexes.map((messageIndex) => (
                <Fragment key={`disclosure-${messages[messageIndex].id ?? messageIndex}`}>
                  {renderModelDisclosure(messages[messageIndex], messageIndex)}
                </Fragment>
              ))}
              <ToolActivityGroup
                count={toolCount}
                hasPendingApproval={hasPendingApproval}
                status={activityStatus}
                className={index === 0 ? undefined : 'mt-4'}
              >
                {visibleIndexes.map((messageIndex, activityIndex) =>
                  renderDefaultMessage(
                    messages[messageIndex],
                    messageIndex,
                    false,
                    activityIndex === 0
                  )
                )}
              </ToolActivityGroup>
            </Fragment>
          );
        }

        if (messageRenderIndex.collapsibleToolCallGroupIndexes.has(index)) {
          return null;
        }

        return renderDefaultMessage(message, index);
      })
      .filter(Boolean);
  }, [
    messages,
    renderedCount,
    renderMessage,
    isUserMessage,
    chat,
    append,
    toolCallNotifications,
    isStreamingMessage,
    onMessageUpdate,
    lastUserPromptIndex,
    lastAssistantMessageIndex,
    pendingSteerMessageIds,
    messageRenderIndex,
    submitElicitationResponse,
    workingDirectory,
    workspaceId,
    threadTurnAttribute,
    getResolvedModel,
    renderModelChangeDisclosure,
  ]);

  return (
    <>
      {renderMessages()}

      {/* Loading indicator when progressively rendering */}
      {isLoading && (
        <div className="flex flex-col items-center justify-center py-8">
          <LoadingGosling
            message={intl.formatMessage(i18n.loadingMessages, {
              renderedCount,
              totalCount: messages.length,
            })}
          />
          <div className="text-xs text-text-secondary mt-2">
            {intl.formatMessage(i18n.searchHint)}
          </div>
        </div>
      )}
    </>
  );
}
