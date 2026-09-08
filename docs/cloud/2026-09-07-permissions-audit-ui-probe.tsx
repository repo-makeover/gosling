import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import ToolApprovalButtons from './ToolApprovalButtons';

vi.mock('../acp/permissionRequests', () => ({
  isAcpPermissionRequestPending: vi.fn(() => true),
  resolveAcpPermissionRequest: vi.fn(() => true),
}));
vi.mock('../acp/permissions', () => ({
  listTools: vi.fn(),
  setToolPermissions: vi.fn(),
}));

describe('2026-09-07 permission audit', () => {
  it('keeps a second session approval actionable when its tool id was used before', async () => {
    const first = render(
      <ToolApprovalButtons data={{
        id: 'audit-reused-id', toolName: 'developer__shell', sessionId: 'audit-first',
      }} />,
      { wrapper: IntlTestWrapper }
    );
    await userEvent.click(screen.getByRole('button', { name: 'Allow Once' }));
    expect(screen.getByText('developer__shell - Allowed once')).toBeInTheDocument();
    first.unmount();

    render(
      <ToolApprovalButtons data={{
        id: 'audit-reused-id', toolName: 'developer__shell', sessionId: 'audit-second',
      }} />,
      { wrapper: IntlTestWrapper }
    );
    expect(screen.getByRole('button', { name: 'Allow Once' })).toBeInTheDocument();
  });
});
