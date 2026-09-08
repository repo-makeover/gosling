import type { Message } from '../../types/message';
import type { AcpElicitationRequest } from '../elicitationRequests';
import {
  type AcpChatStateChange,
  type AdapterState,
  DEFAULT_VISIBLE_MESSAGE_METADATA,
  messageUpserted,
} from './shared';

export type ElicitationStatus = 'submitted' | 'cancelled';

export function applyElicitationRequest(
  state: AdapterState,
  request: AcpElicitationRequest
): AcpChatStateChange[] {
  // Reapplying an ID must preserve the existing form and its submitted/cancelled state.
  if (hasElicitationMessage(state, request.id)) {
    return [];
  }

  const message: Message = {
    id: request.id,
    role: 'assistant',
    created: Math.floor(Date.now() / 1000),
    content: [
      {
        type: 'actionRequired',
        data: {
          actionType: 'elicitation',
          id: request.id,
          message: request.request.message,
          requested_schema: request.request.requestedSchema,
        },
      },
    ],
    metadata: { ...DEFAULT_VISIBLE_MESSAGE_METADATA },
  };
  state.messages.push(message);

  return [messageUpserted(state, message)];
}

export function applyElicitationStatus(
  state: AdapterState,
  elicitationId: string,
  status: ElicitationStatus
): AcpChatStateChange[] {
  const statusFlags = {
    isSubmitted: status === 'submitted',
    isCancelled: status === 'cancelled',
  };
  const messageChanges: AcpChatStateChange[] = [];

  state.messages = state.messages.map((message, messageIndex) => {
    let hasMatchingElicitation = false;
    const updatedContent = message.content.map((contentBlock) => {
      if (
        contentBlock.type !== 'actionRequired' ||
        contentBlock.data.actionType !== 'elicitation' ||
        contentBlock.data.id !== elicitationId
      ) {
        return contentBlock;
      }

      hasMatchingElicitation = true;
      return {
        ...contentBlock,
        data: {
          ...contentBlock.data,
          ...statusFlags,
        },
      };
    });

    if (!hasMatchingElicitation) {
      return message;
    }

    const updatedMessage = { ...message, content: updatedContent };
    // The replacement is not in state.messages yet, so report its original position explicitly.
    messageChanges.push(messageUpserted(state, updatedMessage, messageIndex));
    return updatedMessage;
  });

  return messageChanges;
}

function hasElicitationMessage(state: AdapterState, elicitationId: string): boolean {
  return state.messages.some((message: Message) =>
    message.content.some(
      (content) =>
        content.type === 'actionRequired' &&
        content.data.actionType === 'elicitation' &&
        content.data.id === elicitationId
    )
  );
}
