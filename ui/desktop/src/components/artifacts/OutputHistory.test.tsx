import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { OutputRevisionDto } from '@repo-makeover/gosling-sdk';
import { IntlTestWrapper } from '../../i18n/test-utils';
import {
  getLatestOutputRevision,
  getOutputHistory,
  getOutputRevision,
  restoreOutputRevision,
} from '../../acp/outputRevisions';
import { OutputHistory } from './OutputHistory';

vi.mock('../../acp/outputRevisions', () => ({
  getLatestOutputRevision: vi.fn(),
  getOutputHistory: vi.fn(),
  getOutputRevision: vi.fn(),
  restoreOutputRevision: vi.fn(),
}));

function revision(version: number): OutputRevisionDto {
  return {
    version,
    action: version === 1 ? 'created' : 'modified',
    attribution: 'tool',
    contentHash: `hash-${version}`,
    recordedAt: '2026-09-08T12:00:00Z',
    sizeBytes: 10,
    restoredFrom: null,
    contributor: {
      agent: version === 1 ? 'Researcher' : 'Reviewer',
      provider: 'test',
      selectedModel: `model-${version}`,
      resolvedModel: `actual-${version}`,
      sessionId: `chat-${version}`,
      sessionName: 'Report chat',
      sourceId: `tool-${version}`,
    },
  };
}

const path = '/workspace/Outputs/report.md';
const save = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(window.electron, { saveArtifact: save.mockResolvedValue({ canceled: false }) });
  vi.mocked(getLatestOutputRevision).mockResolvedValue(revision(2));
  vi.mocked(getOutputHistory).mockResolvedValue({
    revisions: [revision(2), revision(1)],
    nextBeforeVersion: null,
  });
  vi.mocked(getOutputRevision).mockImplementation(async (_session, _path, version) => ({
    revision: revision(version),
    contentBase64: window.btoa(`Report version ${version}`),
    currentHash: 'current-file-hash',
  }));
  vi.mocked(restoreOutputRevision).mockResolvedValue({
    revision: { ...revision(3), action: 'restored', attribution: 'user', restoredFrom: 1 },
  });
});

function show(onRestored = vi.fn()) {
  render(<OutputHistory sessionId="session" path={path} onRestored={onRestored} />, {
    wrapper: IntlTestWrapper,
  });
}

describe('OutputHistory', () => {
  it('shows the latest contributor and compares saved revisions', async () => {
    show();
    expect(await screen.findByText('v2 · Reviewer · model-2')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
    expect(await screen.findByText('Report version 2')).toBeInTheDocument();
    expect(
      screen.getByText('Selected: model-2 · Actual: actual-2 · Provider: test')
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Compare with previous' }));
    expect(await screen.findByText('Report version 1')).toBeInTheDocument();
    expect(screen.getByText('Report version 2')).toBeInTheDocument();
    expect(restoreOutputRevision).not.toHaveBeenCalled();
  });

  it.each(['\n', '\r\n'])(
    'hides a formatted managed footer using %j line endings',
    async (newline) => {
      const content = [
        'Report body',
        '',
        '<!-- gosling:output-history:start -->  ',
        'History table',
        '<!-- gosling:output-history:end -->',
        '  ',
        '',
      ].join(newline);
      vi.mocked(getOutputRevision).mockResolvedValue({
        revision: revision(2),
        contentBase64: window.btoa(content),
        currentHash: 'hash',
      });
      show();
      fireEvent.click(screen.getByRole('button', { name: 'History' }));
      expect(await screen.findByText('Report body')).toBeInTheDocument();
      expect(screen.queryByText(/History table/)).not.toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: 'Export revision' }));
      await waitFor(() =>
        expect(save).toHaveBeenCalledWith(
          expect.objectContaining({
            source: { type: 'content', encoding: 'base64', content: window.btoa(content) },
          })
        )
      );
    }
  );

  it('preserves an incomplete footer in the preview', async () => {
    vi.mocked(getOutputRevision).mockResolvedValue({
      revision: revision(2),
      contentBase64: window.btoa(
        'Report body\n\n<!-- gosling:output-history:start -->\nUnfinished history'
      ),
      currentHash: 'hash',
    });
    show();
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
    expect(await screen.findByText(/Unfinished history/)).toBeInTheDocument();
  });

  it('exports the exact selected revision bytes', async () => {
    show();
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
    await screen.findByText('Report version 2');
    fireEvent.click(screen.getByRole('button', { name: /v1 · Researcher/ }));
    await screen.findByText('Report version 1');
    fireEvent.click(screen.getByRole('button', { name: 'Export revision' }));
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        expect.objectContaining({
          defaultPath: 'report.v1.md',
          source: { type: 'content', encoding: 'base64', content: window.btoa('Report version 1') },
        })
      )
    );
  });

  it('confirms restore and passes the captured current hash', async () => {
    const onRestored = vi.fn();
    show(onRestored);
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
    await screen.findByText('Report version 2');
    fireEvent.click(screen.getByRole('button', { name: /v1 · Researcher/ }));
    await screen.findByText('Report version 1');
    fireEvent.click(screen.getByRole('button', { name: 'Restore revision' }));
    const confirmation = screen.getByRole('dialog', { name: 'Restore revision' });
    expect(restoreOutputRevision).not.toHaveBeenCalled();
    fireEvent.click(within(confirmation).getByRole('button', { name: 'Restore revision' }));
    await waitFor(() =>
      expect(restoreOutputRevision).toHaveBeenCalledWith('session', path, 1, 'current-file-hash')
    );
    await waitFor(() => expect(onRestored).toHaveBeenCalledOnce());
  });

  it('shows a restore conflict without claiming success', async () => {
    vi.mocked(restoreOutputRevision).mockRejectedValue(
      new Error('Output changed; refresh its history before restoring')
    );
    const onRestored = vi.fn();
    show(onRestored);
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
    await screen.findByText('Report version 2');
    fireEvent.click(screen.getByRole('button', { name: 'Restore revision' }));
    fireEvent.click(
      within(screen.getByRole('dialog', { name: 'Restore revision' })).getByRole('button', {
        name: 'Restore revision',
      })
    );
    expect(await screen.findByRole('alert')).toHaveTextContent('Output changed');
    expect(onRestored).not.toHaveBeenCalled();
  });

  it('leaves old outputs explicitly unattributed', async () => {
    vi.mocked(getLatestOutputRevision).mockResolvedValue(null);
    vi.mocked(getOutputHistory).mockResolvedValue({ revisions: [], nextBeforeVersion: null });
    show();
    fireEvent.click(screen.getByRole('button', { name: 'History' }));
    expect(await screen.findByText(/No saved revisions/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Restore revision' })).toBeDisabled();
  });

  it('does not request history for unsupported source files', () => {
    render(<OutputHistory sessionId="session" path="/workspace/src/main.rs" />, {
      wrapper: IntlTestWrapper,
    });
    expect(getLatestOutputRevision).not.toHaveBeenCalled();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });
});
