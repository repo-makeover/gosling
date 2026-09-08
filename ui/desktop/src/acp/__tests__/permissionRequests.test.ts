import type { RequestPermissionRequest, RequestPermissionResponse } from '@agentclientprotocol/sdk';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  cancelAcpPermissionRequestsForSession,
  acpPermissionRequestIdentity,
  isAcpPermissionRequestPending,
  requestAcpPermission,
  resolveAcpPermissionRequest,
} from '../permissionRequests';
import { acpChatSessionActions } from '../chatSessionStore';

vi.mock('../chatSessionStore', () => ({
  acpPermissionUserInputRequestId: (toolCallId: string) => `permission:${toolCallId}`,
  acpChatSessionActions: {
    applyPermissionRequest: vi.fn(),
    resolveUserInputRequest: vi.fn(),
  },
}));

function permissionRequest(sessionId: string, toolCallId: string): RequestPermissionRequest {
  return {
    sessionId,
    options: [
      { optionId: 'allow-once', name: 'Allow once', kind: 'allow_once' },
      { optionId: 'reject-once', name: 'Deny once', kind: 'reject_once' },
    ],
    toolCall: {
      toolCallId,
      title: 'Read file',
      rawInput: { path: 'README.md' },
      content: [
        {
          type: 'content',
          content: { type: 'text', text: 'Allow reading README.md?' },
        },
      ],
    },
  };
}

const TEST_SESSION_IDS = ['session-1', 'session-2'];

async function expectStillPending(promise: Promise<RequestPermissionResponse>): Promise<void> {
  let settled = false;
  promise.then(
    () => {
      settled = true;
    },
    () => {
      settled = true;
    }
  );

  await Promise.resolve();

  expect(settled).toBe(false);
}

describe('ACP permission requests', () => {
  it('does not confuse identities containing the old separator', async () => {
    const first = requestAcpPermission(permissionRequest('session-1', 'part\u0000tool'));
    const second = requestAcpPermission(permissionRequest('session-1\u0000part', 'tool'));
    expect(resolveAcpPermissionRequest('session-1', 'part\u0000tool', 'allow_once')).toBe(true);
    await expect(first).resolves.toEqual({
      outcome: { outcome: 'selected', optionId: 'allow-once' },
    });
    await expectStillPending(second);
    expect(resolveAcpPermissionRequest('session-1\u0000part', 'tool', 'deny_once')).toBe(true);
    await expect(second).resolves.toEqual({
      outcome: { outcome: 'selected', optionId: 'reject-once' },
    });
  });

  beforeEach(() => {
    vi.clearAllMocks();
    for (const sessionId of TEST_SESSION_IDS) {
      cancelAcpPermissionRequestsForSession(sessionId);
    }
  });

  afterEach(() => {
    for (const sessionId of TEST_SESSION_IDS) {
      cancelAcpPermissionRequestsForSession(sessionId);
    }
  });

  it('keeps permission requests pending until explicit resolve', async () => {
    const response = requestAcpPermission(permissionRequest('session-1', 'tool-1'));

    await expectStillPending(response);

    expect(resolveAcpPermissionRequest('session-1', 'tool-1', 'allow_once')).toBe(true);
    expect(acpChatSessionActions.resolveUserInputRequest).toHaveBeenCalledWith(
      'session-1',
      'permission:tool-1'
    );
    await expect(response).resolves.toEqual({
      outcome: {
        outcome: 'selected',
        optionId: 'allow-once',
      },
    });
  });

  it('cancels only pending requests for the requested session', async () => {
    const sessionOneResponse = requestAcpPermission(permissionRequest('session-1', 'tool-1'));
    const sessionTwoResponse = requestAcpPermission(permissionRequest('session-2', 'tool-2'));

    cancelAcpPermissionRequestsForSession('session-1');

    await expect(sessionOneResponse).resolves.toEqual({
      outcome: {
        outcome: 'cancelled',
      },
    });
    await expectStillPending(sessionTwoResponse);

    expect(resolveAcpPermissionRequest('session-2', 'tool-2', 'deny_once')).toBe(true);
    await expect(sessionTwoResponse).resolves.toEqual({
      outcome: {
        outcome: 'selected',
        optionId: 'reject-once',
      },
    });
  });

  it('isAcpPermissionRequestPending reports liveness without consuming the request', async () => {
    const response = requestAcpPermission(permissionRequest('session-1', 'tool-1'));

    expect(isAcpPermissionRequestPending('session-1', 'tool-1')).toBe(true);
    // A non-consuming check must not resolve or remove the pending request.
    expect(isAcpPermissionRequestPending('session-1', 'tool-1')).toBe(true);
    await expectStillPending(response);

    expect(resolveAcpPermissionRequest('session-1', 'tool-1', 'allow_once')).toBe(true);
    expect(isAcpPermissionRequestPending('session-1', 'tool-1')).toBe(false);
  });

  it('isAcpPermissionRequestPending is false for an unknown or already-resolved request', () => {
    expect(isAcpPermissionRequestPending('session-1', 'never-requested')).toBe(false);
  });

  it('cancels an older duplicate request for the same session and tool call', async () => {
    const firstResponse = requestAcpPermission(permissionRequest('session-1', 'tool-1'));
    const secondResponse = requestAcpPermission(permissionRequest('session-1', 'tool-1'));

    await expect(firstResponse).resolves.toEqual({
      outcome: {
        outcome: 'cancelled',
      },
    });
    await expectStillPending(secondResponse);

    expect(resolveAcpPermissionRequest('session-1', 'tool-1', 'allow_once')).toBe(true);
    await expect(secondResponse).resolves.toEqual({
      outcome: {
        outcome: 'selected',
        optionId: 'allow-once',
      },
    });
  });

  it('resolves always_allow_domain to the domain-scoped option, not the tool-wide one', async () => {
    // Both options share `kind: 'allow_always'` (ACP has no domain-scoped
    // kind), so only the exact option id can tell them apart.
    const response = requestAcpPermission({
      sessionId: 'session-1',
      options: [
        { optionId: 'allow_always_domain', name: 'Always allow arxiv.org', kind: 'allow_always' },
        { optionId: 'reject-once', name: 'Deny once', kind: 'reject_once' },
      ],
      toolCall: {
        toolCallId: 'tool-1',
        title: 'Run shell command',
        rawInput: { command: 'curl https://arxiv.org/e-print/1' },
        content: [
          {
            type: 'content',
            content: {
              type: 'text',
              text: 'Egress destinations detected: https://arxiv.org/e-print/1',
            },
          },
        ],
      },
    });

    expect(resolveAcpPermissionRequest('session-1', 'tool-1', 'always_allow_domain')).toBe(true);
    await expect(response).resolves.toEqual({
      outcome: {
        outcome: 'selected',
        optionId: 'allow_always_domain',
      },
    });
  });

  it('keeps the request pending when an unavailable option is selected', async () => {
    const response = requestAcpPermission(permissionRequest('session-1', 'tool-1'));

    expect(resolveAcpPermissionRequest('session-1', 'tool-1', 'always_allow_domain')).toBe(false);
    await expectStillPending(response);
  });
  it('rejects a response from a replaced request with the same session and id', async () => {
    const first = requestAcpPermission(permissionRequest('session-1', 'tool-1'));
    const identity = acpPermissionRequestIdentity('session-1', 'tool-1');
    const replacement = requestAcpPermission(permissionRequest('session-1', 'tool-1'));
    expect(resolveAcpPermissionRequest('session-1', 'tool-1', 'allow_once', identity)).toBe(false);
    await expect(first).resolves.toEqual({ outcome: { outcome: 'cancelled' } });
    await expectStillPending(replacement);
    expect(
      resolveAcpPermissionRequest(
        'session-1',
        'tool-1',
        'deny_once',
        acpPermissionRequestIdentity('session-1', 'tool-1')
      )
    ).toBe(true);
    await expect(replacement).resolves.toEqual({
      outcome: { outcome: 'selected', optionId: 'reject-once' },
    });
  });
});
