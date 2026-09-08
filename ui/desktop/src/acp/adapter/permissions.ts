import type { RequestPermissionRequest } from '@agentclientprotocol/sdk';
import type { Message } from '../../types/message';
import {
  type AcpChatStateChange,
  type AdapterState,
  DEFAULT_VISIBLE_MESSAGE_METADATA,
  messageUpserted,
  rawInputToArguments,
  toolIdentity,
  isRecord,
} from './shared';

export function applyPermissionRequest(
  state: AdapterState,
  request: RequestPermissionRequest
): AcpChatStateChange[] {
  const toolCallId = request.toolCall.toolCallId;
  const existingMessageIndex = state.messages.findIndex((message) =>
    message.content.some(
      (content) =>
        content.type === 'actionRequired' &&
        content.data.actionType === 'toolConfirmation' &&
        content.data.id === toolCallId
    )
  );

  const identity = toolIdentity(request.toolCall);
  const prompt = firstPermissionPromptText(request);
  const toolMetadata = request.toolCall._meta;
  const goslingMetadata =
    isRecord(toolMetadata) && isRecord(toolMetadata.gosling) ? toolMetadata.gosling : undefined;
  const permissionMetadata =
    goslingMetadata && isRecord(goslingMetadata.permission)
      ? goslingMetadata.permission
      : undefined;
  // Metadata alone cannot offer a domain grant; the request must include that choice.
  const offersDomainApproval = request.options.some(
    (option) => option.optionId === 'allow_always_domain'
  );
  const domain =
    offersDomainApproval && typeof permissionMetadata?.domain === 'string'
      ? permissionMetadata.domain
      : undefined;

  const permissionMessage: Message = {
    id: `acp_permission_${toolCallId}`,
    role: 'assistant',
    created: Math.floor(Date.now() / 1000),
    content: [
      {
        type: 'actionRequired',
        data: {
          actionType: 'toolConfirmation',
          id: toolCallId,
          toolName: identity.toolName ?? request.toolCall.title ?? toolCallId,
          arguments: rawInputToArguments(request.toolCall.rawInput),
          ...(prompt ? { prompt } : {}),
          ...(domain ? { domain } : {}),
        },
      },
    ],
    metadata: { ...DEFAULT_VISIBLE_MESSAGE_METADATA },
  };
  // A reused tool-call ID may carry a new prompt; refresh its existing message.
  if (existingMessageIndex >= 0) {
    state.messages[existingMessageIndex] = permissionMessage;
  } else {
    state.messages.push(permissionMessage);
  }

  return [messageUpserted(state, permissionMessage)];
}

function firstPermissionPromptText(request: RequestPermissionRequest): string | undefined {
  for (const content of request.toolCall.content ?? []) {
    if (content.type === 'content' && content.content.type === 'text') {
      return content.content.text;
    }
  }

  return undefined;
}
