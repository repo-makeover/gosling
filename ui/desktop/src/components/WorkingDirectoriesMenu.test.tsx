import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import type { Session } from '../types/session';
import WorkingDirectoriesMenu from './WorkingDirectoriesMenu';

const addSessionWorkingDir = vi.fn();

vi.mock('../acp/sessions', () => ({
  acpAddSessionWorkingDir: (...args: unknown[]) => addSessionWorkingDir(...args),
  acpRemoveSessionWorkingDir: vi.fn(),
  acpSetWorkingDirRestriction: vi.fn(),
}));

const workspaceSession: Session = {
  id: 'session-a',
  name: 'Private workshop',
  message_count: 0,
  created_at: '2026-08-26T00:00:00Z',
  updated_at: '2026-08-26T00:00:00Z',
  working_dir: '/workspace/project',
  additional_working_dirs: [],
  extension_data: { active: [], installed: [] },
  workspace_id: 'workspace-a',
  workspace_name: 'Math research',
};

describe('WorkingDirectoriesMenu workspace session grants', () => {
  afterEach(() => vi.unstubAllGlobals());

  beforeEach(() => {
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      }
    );
    addSessionWorkingDir.mockReset();
    addSessionWorkingDir.mockResolvedValue({
      workingDir: '/workspace/project',
      additionalWorkingDirs: ['/private/workshop'],
    });
    Object.assign(window.electron, {
      listRecentDirs: vi.fn().mockResolvedValue([]),
      addRecentDir: vi.fn(),
      sessionDirectoryChooser: vi.fn().mockResolvedValue({
        canceled: false,
        filePaths: ['/private/workshop'],
      }),
    });
  });

  it('adds a directory to the selected workspace session only', async () => {
    const user = userEvent.setup();
    const onSessionChange = vi.fn();
    render(
      <WorkingDirectoriesMenu session={workspaceSession} onSessionChange={onSessionChange} />,
      { wrapper: IntlTestWrapper }
    );

    await user.click(screen.getByRole('button', { name: /Add Dir/ }));
    expect(
      await screen.findByText(/Directories added here belong only to this session/)
    ).toBeInTheDocument();
    await user.click(screen.getByText('Add directory…'));

    await waitFor(() =>
      expect(addSessionWorkingDir).toHaveBeenCalledWith('session-a', '/private/workshop')
    );
    const update = onSessionChange.mock.calls[0][0] as (session: Session) => Session;
    expect(update(workspaceSession).additional_working_dirs).toEqual(['/private/workshop']);
  });

  it('shows pinned read-only folder access', async () => {
    const user = userEvent.setup();
    render(
      <WorkingDirectoriesMenu
        session={{
          ...workspaceSession,
          additional_working_dirs: ['/workspace/reference'],
          workspace_folder_roots: [
            { path: '/workspace/project', access: 'read_write' },
            { path: '/workspace/reference', access: 'read' },
          ],
        }}
        onSessionChange={vi.fn()}
      />,
      { wrapper: IntlTestWrapper }
    );
    await user.click(screen.getByRole('button', { name: /Dirs/ }));
    expect(await screen.findByText('Read-only')).toBeInTheDocument();
    expect(screen.getByText('Primary · Read/write/run')).toBeInTheDocument();
  });
});
