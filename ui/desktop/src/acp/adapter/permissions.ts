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
  const existingIndex = state.messages.findIndex((message) =>
    message.content.some(
      (content) =>
        content.type === 'actionRequired' &&
        content.data.actionType === 'toolConfirmation' &&
        content.data.id === toolCallId
    )
  );

  const identity = toolIdentity(request.toolCall);
  const prompt = permissionPrompt(request);
  const meta = request.toolCall._meta;
  const gosling = isRecord(meta) && isRecord(meta.gosling) ? meta.gosling : undefined;
  const permission = gosling && isRecord(gosling.permission) ? gosling.permission : undefined;
  const domain =
    request.options.some((option) => option.optionId === 'allow_always_domain') &&
    typeof permission?.domain === 'string'
      ? permission.domain
      : undefined;

  const message: Message = {
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
  if (existingIndex >= 0) {
    state.messages[existingIndex] = message;
  } else {
    state.messages.push(message);
  }

  return [messageUpserted(state, message)];
}

function permissionPrompt(request: RequestPermissionRequest): string | undefined {
  for (const content of request.toolCall.content ?? []) {
    if (content.type === 'content' && content.content.type === 'text') {
      return content.content.text;
    }
  }

  return undefined;
}
