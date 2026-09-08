import { render, type RenderOptions, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  isAcpPermissionRequestPending,
  resolveAcpPermissionRequest,
} from '../acp/permissionRequests';
import { listTools, setToolPermissions } from '../acp/permissions';
import { IntlTestWrapper } from '../i18n/test-utils';
import ToolApprovalButtons from './ToolApprovalButtons';

vi.mock('../acp/permissionRequests', () => ({
  isAcpPermissionRequestPending: vi.fn(),
  resolveAcpPermissionRequest: vi.fn(),
  acpPermissionRequestIdentity: (session: string, id: string) => `${session}:${id}`,
  subscribeAcpPermissionRequests: () => () => {},
}));

vi.mock('../acp/permissions', () => ({
  listTools: vi.fn(),
  setToolPermissions: vi.fn(),
}));

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

const resolveAcpPermissionRequestMock = vi.mocked(resolveAcpPermissionRequest);
const isAcpPermissionRequestPendingMock = vi.mocked(isAcpPermissionRequestPending);
const listToolsMock = vi.mocked(listTools);
const setToolPermissionsMock = vi.mocked(setToolPermissions);

describe('ToolApprovalButtons', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    isAcpPermissionRequestPendingMock.mockReturnValue(true);
  });

  it('marks the approval accepted when the ACP request resolves', async () => {
    resolveAcpPermissionRequestMock.mockReturnValueOnce(true);

    renderWithIntl(
      <ToolApprovalButtons
        data={{
          id: 'tool-call-approved',
          toolName: 'developer__shell',
          sessionId: 'session-1',
        }}
      />
    );

    await userEvent.click(screen.getByRole('button', { name: 'Allow Once' }));

    expect(resolveAcpPermissionRequestMock).toHaveBeenCalledWith(
      'session-1',
      'tool-call-approved',
      'allow_once',
      expect.any(String)
    );
    expect(screen.getByText('developer__shell - Allowed once')).toBeInTheDocument();
  });

  it('shows a stale request error when ACP has no pending request', async () => {
    resolveAcpPermissionRequestMock.mockReturnValueOnce(false);

    renderWithIntl(
      <ToolApprovalButtons
        data={{
          id: 'tool-call-rerun',
          toolName: 'developer__shell',
          sessionId: 'session-1',
        }}
      />
    );

    await userEvent.click(screen.getByRole('button', { name: 'Allow Once' }));

    expect(resolveAcpPermissionRequestMock).toHaveBeenCalledWith(
      'session-1',
      'tool-call-rerun',
      'allow_once',
      expect.any(String)
    );
    expect(screen.getByText('This approval request is no longer active.')).toBeInTheDocument();
    expect(screen.queryByText('developer__shell - Allowed once')).not.toBeInTheDocument();
  });

  it('does not mutate extension permissions when the approval request is stale', async () => {
    isAcpPermissionRequestPendingMock.mockReturnValueOnce(false);

    renderWithIntl(
      <ToolApprovalButtons
        data={{
          id: 'tool-call-stale',
          toolName: 'developer__shell',
          sessionId: 'session-1',
        }}
      />
    );

    await userEvent.click(screen.getByRole('button', { name: 'Always Allow all developer tools' }));

    expect(resolveAcpPermissionRequestMock).not.toHaveBeenCalled();
    expect(listToolsMock).not.toHaveBeenCalled();
    expect(setToolPermissionsMock).not.toHaveBeenCalled();
    expect(screen.getByText('This approval request is no longer active.')).toBeInTheDocument();
    expect(
      screen.queryByText('developer__shell - Always allowed (developer tools)')
    ).not.toBeInTheDocument();
  });

  it('validates the approval request before mutating extension permissions', async () => {
    const callOrder: string[] = [];
    isAcpPermissionRequestPendingMock.mockImplementation(() => {
      callOrder.push('pending');
      return true;
    });
    resolveAcpPermissionRequestMock.mockImplementationOnce(() => {
      callOrder.push('resolve');
      return true;
    });
    listToolsMock.mockImplementationOnce(async () => {
      callOrder.push('listTools');
      return [];
    });
    setToolPermissionsMock.mockImplementationOnce(async () => {
      callOrder.push('setToolPermissions');
    });

    renderWithIntl(
      <ToolApprovalButtons
        data={{
          id: 'tool-call-live',
          toolName: 'developer__shell',
          sessionId: 'session-1',
        }}
      />
    );

    await userEvent.click(screen.getByRole('button', { name: 'Always Allow all developer tools' }));

    expect(callOrder).toEqual(['pending', 'listTools', 'pending', 'setToolPermissions', 'resolve']);
    expect(resolveAcpPermissionRequestMock).toHaveBeenCalledWith(
      'session-1',
      'tool-call-live',
      'always_allow',
      expect.any(String)
    );
    expect(setToolPermissionsMock).toHaveBeenCalledWith([
      { toolName: 'developer__shell', permission: 'always_allow' },
    ]);
    expect(
      screen.getByText('developer__shell - Always allowed (developer tools)')
    ).toBeInTheDocument();
  });

  it('leaves the live approval pending when bulk persistence fails', async () => {
    listToolsMock.mockResolvedValueOnce([]);
    setToolPermissionsMock.mockRejectedValueOnce(new Error('permission.yaml is read-only'));

    renderWithIntl(
      <ToolApprovalButtons
        data={{
          id: 'tool-call-persist-failure',
          toolName: 'developer__shell',
          sessionId: 'session-1',
        }}
      />
    );

    await userEvent.click(screen.getByRole('button', { name: 'Always Allow all developer tools' }));

    expect(setToolPermissionsMock).toHaveBeenCalled();
    expect(resolveAcpPermissionRequestMock).not.toHaveBeenCalled();
    expect(screen.getByText('Failed to update permissions for this extension')).toBeInTheDocument();
  });

  it('offers a domain-scoped always-allow button only for a security prompt with a flagged domain', () => {
    renderWithIntl(
      <ToolApprovalButtons
        data={{
          id: 'tool-call-egress',
          toolName: 'developer__shell',
          sessionId: 'session-1',
          prompt: 'Egress destinations detected: https://arxiv.org/e-print/1',
          domain: 'arxiv.org',
        }}
      />
    );

    expect(screen.getByRole('button', { name: 'Always allow arxiv.org' })).toBeInTheDocument();
    // The tool-wide grant stays withheld on any security prompt (WFG-GOS-006).
    expect(screen.queryByRole('button', { name: 'Always Allow' })).not.toBeInTheDocument();
  });

  it('omits the domain-scoped button when the security prompt has no single flagged domain', () => {
    renderWithIntl(
      <ToolApprovalButtons
        data={{
          id: 'tool-call-egress-multi',
          toolName: 'developer__shell',
          sessionId: 'session-1',
          prompt: 'Egress destinations detected: https://a.example, https://b.example',
        }}
      />
    );

    expect(screen.queryByRole('button', { name: /Always allow/ })).not.toBeInTheDocument();
  });

  it('omits the domain-scoped button for a non-security prompt even if a domain were supplied', () => {
    renderWithIntl(
      <ToolApprovalButtons
        data={{
          id: 'tool-call-plain',
          toolName: 'developer__shell',
          sessionId: 'session-1',
        }}
      />
    );

    expect(
      screen.queryByRole('button', { name: 'Always allow arxiv.org' })
    ).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Always Allow' })).toBeInTheDocument();
  });

  it('resolves a domain-scoped always-allow decision distinctly from a tool-wide one', async () => {
    resolveAcpPermissionRequestMock.mockReturnValueOnce(true);

    renderWithIntl(
      <ToolApprovalButtons
        data={{
          id: 'tool-call-egress-approve',
          toolName: 'developer__shell',
          sessionId: 'session-1',
          prompt: 'Egress destinations detected: https://arxiv.org/e-print/1',
          domain: 'arxiv.org',
        }}
      />
    );

    await userEvent.click(screen.getByRole('button', { name: 'Always allow arxiv.org' }));

    expect(resolveAcpPermissionRequestMock).toHaveBeenCalledWith(
      'session-1',
      'tool-call-egress-approve',
      'always_allow_domain',
      expect.any(String)
    );
    expect(
      screen.getByText('developer__shell - Always allow arxiv.org requested')
    ).toBeInTheDocument();
  });
});
