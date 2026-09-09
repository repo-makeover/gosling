import { describe, expect, it } from 'vitest';
import type { Message } from '../types/message';
import { lastAssistantReplyId } from './sessionActivity';

const reply: Message = {
  id: 'reply',
  role: 'assistant',
  created: 1,
  metadata: { userVisible: true, agentVisible: true },
  content: [{ type: 'text', text: 'Answer' }],
};

describe('lastAssistantReplyId', () => {
  it('ignores user insertion, hidden compaction summaries, and synthetic status messages', () => {
    expect(
      lastAssistantReplyId([
        reply,
        { ...reply, id: 'user', role: 'user' },
        { ...reply, id: 'summary', metadata: { userVisible: false, agentVisible: true } },
        {
          ...reply,
          id: 'status',
          content: [
            { type: 'systemNotification', msg: 'Compacted', notificationType: 'inlineMessage' },
          ],
        },
      ])
    ).toBe('reply');
  });

  it('requires a real id and visible nonempty response text', () => {
    expect(lastAssistantReplyId([{ ...reply, id: undefined }])).toBeNull();
    expect(lastAssistantReplyId([{ ...reply, content: [{ type: 'text', text: ' ' }] }])).toBeNull();
    expect(lastAssistantReplyId([reply, { ...reply, id: 'new-reply' }])).toBe('new-reply');
    expect(
      lastAssistantReplyId([
        {
          ...reply,
          id: 'image-reply',
          content: [{ type: 'image', data: 'AA==', mimeType: 'image/png' }],
        },
      ])
    ).toBe('image-reply');
  });
});
