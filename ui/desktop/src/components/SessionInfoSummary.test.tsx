import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { Session } from '../types/session';
import SessionInfoSummary from './SessionInfoSummary';

vi.mock('./WorkingDirectoriesMenu', () => ({
  default: () => <button type="button">Manage working directories</button>,
}));
vi.mock('./bottom_menu/CredentialProfileSelector', () => ({
  CredentialProfileSelector: ({ credentialProfileName }: { credentialProfileName?: string }) => (
    <button type="button">{credentialProfileName ?? 'No credential'}</button>
  ),
}));

const session: Session = {
  id: 'session-1',
  name: 'Workspace filter repair',
  message_count: 12,
  created_at: '2026-07-21T00:00:00Z',
  updated_at: '2026-07-21T00:00:00Z',
  working_dir: '/projects/gosling',
  additional_working_dirs: ['/projects/shared'],
  extension_data: { active: [], installed: [] },
  workspace_id: 'workspace-1',
  workspace_name: 'Cephalopod-AI',
  credential_profile_id: 'profile-1',
  credential_profile_name: 'Team OpenAI',
  provider_name: 'openai',
  model_config: { model_name: 'gpt-5.6', toolshim: false },
  gosling_mode: 'auto',
  usage: { total_tokens: 12_500 },
  accumulated_cost: 1.23,
};

describe('SessionInfoSummary', () => {
  it('uses the workspace as its label and reveals concise chat metadata', () => {
    const onCollapse = vi.fn();
    render(
      <SessionInfoSummary session={session} onSessionChange={vi.fn()} onCollapse={onCollapse} />
    );

    expect(screen.getByRole('region', { name: 'Chat information' })).toBeInTheDocument();
    expect(screen.getByText('Team OpenAI')).toBeInTheDocument();
    expect(screen.getByText('gosling')).toBeInTheDocument();
    expect(screen.getByText('shared')).toBeInTheDocument();
    expect(screen.getByText('gpt-5.6')).toBeInTheDocument();
    expect(screen.getByText('Autonomous')).toBeInTheDocument();
    expect(screen.getByText(/Workspace folder policy is enforced/)).toBeInTheDocument();
    expect(
      screen.queryByText('Tools are not restricted to the listed directories.')
    ).not.toBeInTheDocument();
    expect(screen.getByText('12.5K')).toBeInTheDocument();
    expect(screen.getByText('$1.23')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Collapse chat information' }));
    expect(onCollapse).toHaveBeenCalledOnce();
  });
});
