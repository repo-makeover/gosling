import type { Message } from '../types/message';

// Agent-only compaction summaries and tool/status messages are not replies to read.
export function lastAssistantReplyId(messages: Message[]): string | null {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (
      message.role === 'assistant' &&
      message.metadata.userVisible &&
      message.id &&
      message.content.some(
        (content) =>
          content.type === 'image' || (content.type === 'text' && content.text.trim().length > 0)
      )
    ) {
      return message.id;
    }
  }
  return null;
}
