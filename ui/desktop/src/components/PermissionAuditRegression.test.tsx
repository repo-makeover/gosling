import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import {
  cancelAcpPermissionRequestsForSession,
  requestAcpPermission,
} from '../acp/permissionRequests';
import ToolApprovalButtons from './ToolApprovalButtons';

vi.mock('../acp/chatSessionStore', () => ({
  acpPermissionUserInputRequestId: (id: string) => `permission:${id}`,
  acpChatSessionActions: {
    applyPermissionRequest: vi.fn(),
    resolveUserInputRequest: vi.fn(),
  },
}));
vi.mock('../acp/permissions', () => ({ listTools: vi.fn(), setToolPermissions: vi.fn() }));

function request(sessionId: string, toolCallId: string) {
  return requestAcpPermission({
    sessionId,
    toolCall: { toolCallId, title: 'developer__shell' },
    options: [
      { optionId: 'allow_once', name: 'Allow Once', kind: 'allow_once' },
      { optionId: 'reject_once', name: 'Deny', kind: 'reject_once' },
    ],
  });
}
function buttons(sessionId: string, id: string) {
  return <ToolApprovalButtons data={{ id, toolName: 'developer__shell', sessionId }} />;
}

afterEach(() => {
  act(() => {
    cancelAcpPermissionRequestsForSession('audit-first');
    cancelAcpPermissionRequestsForSession('audit-second');
  });
});

describe('permission request identity regressions', () => {
  it('keeps a second session approval actionable when its tool id was used before', async () => {
    const firstResponse = request('audit-first', 'reused-id');
    const first = render(buttons('audit-first', 'reused-id'), { wrapper: IntlTestWrapper });
    await userEvent.click(screen.getByRole('button', { name: 'Allow Once' }));
    await expect(firstResponse).resolves.toEqual({
      outcome: { outcome: 'selected', optionId: 'allow_once' },
    });
    first.unmount();

    void request('audit-second', 'reused-id');
    render(buttons('audit-second', 'reused-id'), { wrapper: IntlTestWrapper });
    expect(screen.getByRole('button', { name: 'Allow Once' })).toBeInTheDocument();
  });

  it('resets state on an in-place session or request change and preserves same-request remounts', async () => {
    void request('audit-first', 'switch-id');
    const view = render(buttons('audit-first', 'switch-id'), { wrapper: IntlTestWrapper });
    await userEvent.click(screen.getByRole('button', { name: 'Allow Once' }));
    act(() => {
      void request('audit-second', 'switch-id');
    });
    view.rerender(buttons('audit-second', 'switch-id'));
    expect(screen.getByRole('button', { name: 'Allow Once' })).toBeInTheDocument();
    act(() => {
      void request('audit-second', 'different-id');
    });
    view.rerender(buttons('audit-second', 'different-id'));
    expect(screen.getByRole('button', { name: 'Allow Once' })).toBeInTheDocument();
    view.rerender(buttons('audit-first', 'switch-id'));
    expect(screen.getByText('developer__shell - Allowed once')).toBeInTheDocument();
    view.unmount();
    render(buttons('audit-first', 'switch-id'), { wrapper: IntlTestWrapper });
    expect(screen.getByText('developer__shell - Allowed once')).toBeInTheDocument();
  });

  it('reopens the buttons for a replacement without approving the new request', async () => {
    void request('audit-first', 'retry-id');
    render(buttons('audit-first', 'retry-id'), { wrapper: IntlTestWrapper });
    await userEvent.click(screen.getByRole('button', { name: 'Allow Once' }));
    let replacement: ReturnType<typeof request>;
    act(() => {
      replacement = request('audit-first', 'retry-id');
    });
    expect(screen.getByRole('button', { name: 'Allow Once' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Deny' }));
    await expect(replacement!).resolves.toEqual({
      outcome: { outcome: 'selected', optionId: 'reject_once' },
    });
  });
});
