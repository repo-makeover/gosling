import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { SessionArtifactDto } from '@repo-makeover/gosling-sdk';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ArtifactWorkbenchProvider,
  useArtifactWorkbench,
} from '../../contexts/ArtifactWorkbenchContext';
import { useArtifactRouter } from '../../contexts/ArtifactRouterContext';
import { IntlTestWrapper } from '../../i18n/test-utils';
import {
  addSessionLibraryText,
  linkSessionLibraryFile,
  listSessionLibraryInputs,
} from '../../acp/sessionLibraryInputs';
import { acpChatSessionController } from '../../acp/chatSessionController';
import {
  clearSelectedSessionInputs,
  getSelectedSessionInputs,
} from '../../acp/sessionInputSelection';
import { ArtifactPane } from './ArtifactPane';

vi.mock('../../contexts/ArtifactRouterContext', () => ({ useArtifactRouter: vi.fn() }));
vi.mock('../../acp/sessionLibraryInputs', () => ({
  listSessionLibraryInputs: vi.fn(),
  addSessionLibraryText: vi.fn(),
  linkSessionLibraryFile: vi.fn(),
}));
vi.mock('../../acp/chatSessionController', () => ({
  acpChatSessionController: { loadSession: vi.fn() },
}));

describe('ArtifactPane', () => {
  const saveArtifact = vi.fn();
  const readArtifactFile = vi.fn();
  const readArtifactTitles = vi.fn();
  const trashArtifactFiles = vi.fn();

  function Harness() {
    const { openContent, openFile, setVisibleSession } = useArtifactWorkbench();
    return (
      <>
        <button
          type="button"
          onClick={() =>
            openContent({
              title: 'hero.png',
              content: 'AAEC',
              encoding: 'base64',
              mimeType: 'image/png',
              workspaceId: 'workspace-1',
            })
          }
        >
          Open image
        </button>
        <button
          type="button"
          onClick={() => openFile('/outputs/report.md', '/outputs', 'workspace-1')}
        >
          Open file
        </button>
        <button
          type="button"
          onClick={() => {
            const artifacts: SessionArtifactDto[] = [
              'report.md',
              'brief.docx',
              'analysis.py',
              'engine.rs',
              'build.sh',
            ].map((displayPath, index) => ({
              sessionId: 'session-four-files',
              displayPath,
              resolvedPath: `/outputs/${displayPath}`,
              baseWorkingDir: '/outputs',
              relation: 'created' as const,
              provenance: 'built_in_tool' as const,
              sourceId: `tool-${index}`,
              firstSeenAt: `2026-01-01T00:00:0${index}Z`,
              lastSeenAt: `2026-01-01T00:00:0${index}Z`,
            }));
            setVisibleSession(
              'session-four-files',
              artifacts.concat({
                sessionId: 'session-four-files',
                displayPath: 'David.Casbeer@us.af.mil',
                resolvedPath: '/outputs/David.Casbeer@us.af.mil',
                baseWorkingDir: '/outputs',
                relation: 'referenced',
                provenance: 'compatibility_inference',
                sourceId: 'assistant-message',
                firstSeenAt: '2026-01-01T00:00:04Z',
                lastSeenAt: '2026-01-01T00:00:04Z',
              })
            );
          }}
        >
          Load mixed outputs
        </button>
        <button
          type="button"
          onClick={() =>
            setVisibleSession('session-pdf', [
              {
                sessionId: 'session-pdf',
                displayPath: 'report.pdf',
                resolvedPath: '/outputs/report.pdf',
                baseWorkingDir: '/outputs',
                mimeType: 'application/pdf; version=1.7',
                relation: 'created',
                provenance: 'built_in_tool',
                firstSeenAt: '2026-01-01T00:00:00Z',
                lastSeenAt: '2026-01-01T00:00:00Z',
              },
            ])
          }
        >
          Load MIME PDF
        </button>
        <button type="button" onClick={() => setVisibleSession('session-inputs', [])}>
          Load inputs
        </button>
        <ArtifactPane />
      </>
    );
  }

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(acpChatSessionController.loadSession).mockReset().mockResolvedValue(true);
    clearSelectedSessionInputs('session-inputs', getSelectedSessionInputs('session-inputs'));
    localStorage.clear();
    saveArtifact.mockResolvedValue({ canceled: false, filePath: '/outputs/images/hero.png' });
    readArtifactFile.mockReset();
    readArtifactFile.mockResolvedValue({
      content: '',
      encoding: 'utf8',
      error: 'Renderer file access denied for path outside approved roots',
      filePath: '/outputs/report.md',
      found: false,
      sizeBytes: 0,
      truncated: false,
    });
    readArtifactTitles.mockReset();
    readArtifactTitles.mockResolvedValue({});
    trashArtifactFiles
      .mockReset()
      .mockImplementation(async (paths: string[]) =>
        paths.map((path) => ({ path, status: 'trashed' }))
      );
    Object.assign(window.electron, { readArtifactFile, readArtifactTitles, trashArtifactFiles });
    vi.mocked(window.electron.getResearchLibraryPath).mockResolvedValue(
      '/Users/tester/Documents/Gosling Research Library'
    );
    vi.mocked(window.electron.listResearchLibraryFiles).mockResolvedValue({
      files: [],
      truncated: false,
    });
    vi.mocked(listSessionLibraryInputs).mockResolvedValue([]);
    vi.mocked(useArtifactRouter).mockReturnValue({
      saveArtifact,
      setVisibleSessionArtifacts: vi.fn(),
      setVisibleSessionWorkspaceId: vi.fn(),
    });
  });

  it('saves a full transient artifact through its originating workspace', async () => {
    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(screen.getByRole('button', { name: 'Open image' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Save a copy' }));

    await waitFor(() =>
      expect(saveArtifact).toHaveBeenCalledWith({
        workspaceId: 'workspace-1',
        mimeType: 'image/png',
        suggestedName: 'hero.png',
        title: 'Save a copy',
        source: { type: 'content', content: 'AAEC', encoding: 'base64' },
      })
    );
  });

  it('keeps pane controls outside the native titlebar drag region', () => {
    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );

    expect(screen.getByTitle('Open file')).toHaveClass('no-drag');
    expect(screen.getByTitle('Close inputs and outputs pane')).toHaveClass('no-drag');
  });

  it('retries a transient route authorization failure before showing an error', async () => {
    readArtifactFile
      .mockResolvedValueOnce({
        content: '',
        encoding: 'utf8',
        error: 'Renderer file access denied for path outside approved roots',
        filePath: '/outputs/report.md',
        found: false,
        sizeBytes: 0,
        truncated: false,
      })
      .mockResolvedValueOnce({
        content: '# Report',
        encoding: 'utf8',
        error: null,
        filePath: '/outputs/report.md',
        found: true,
        sizeBytes: 9,
        truncated: false,
      });

    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(screen.getAllByRole('button', { name: 'Open file' })[0]);

    await waitFor(() => expect(readArtifactFile).toHaveBeenCalledTimes(2), { timeout: 1000 });
    expect(await screen.findByRole('heading', { name: 'Report' })).toBeInTheDocument();
  });

  it('offers to grant access when a preview stays blocked after retrying', async () => {
    const selectArtifactFile = vi.fn().mockResolvedValue('/outputs/report.md');
    Object.assign(window.electron, { selectArtifactFile });

    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(screen.getByRole('button', { name: 'Load mixed outputs' }));
    fireEvent.click(screen.getByTitle('/outputs/report.md'));

    const grantButton = await screen.findByRole(
      'button',
      {
        name: 'Select this file to grant access and preview it',
      },
      { timeout: 2_000 }
    );
    fireEvent.click(grantButton);

    await waitFor(() => expect(selectArtifactFile).toHaveBeenCalledWith('report.md'));
  });

  it('shows only the default configured output extensions', () => {
    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(screen.getByRole('button', { name: 'Load mixed outputs' }));

    const outputsTab = screen.getByRole('tab', { name: 'Outputs 2' });
    expect(outputsTab).toBeInTheDocument();
    expect(outputsTab).toHaveTextContent('2');
    expect(screen.getByTestId('outputs-count')).toHaveClass('rounded-md', 'border');
    expect(screen.getByText('report.md')).toBeInTheDocument();
    expect(screen.getByText('brief.docx')).toBeInTheDocument();
    expect(screen.queryByText('analysis.py')).not.toBeInTheDocument();
    expect(screen.queryByText('engine.rs')).not.toBeInTheDocument();
    expect(screen.queryByText('build.sh')).not.toBeInTheDocument();
    expect(screen.queryByText('David.Casbeer@us.af.mil')).not.toBeInTheDocument();
    expect(
      screen.queryByText('This file type does not have an in-app preview yet.')
    ).not.toBeInTheDocument();
    expect(readArtifactFile).not.toHaveBeenCalled();
  });

  it('keeps configured files without an in-app preview available for external opening', async () => {
    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(screen.getByRole('button', { name: 'Load mixed outputs' }));
    fireEvent.click(screen.getByTitle('/outputs/brief.docx'));

    expect(
      await screen.findByText('This file type does not have an in-app preview yet.')
    ).toBeInTheDocument();
    expect(screen.getByTitle('Open externally')).toBeInTheDocument();
    expect(readArtifactFile).not.toHaveBeenCalled();
  });

  it('updates the displayed inventory when an extension is added in settings', async () => {
    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(screen.getByRole('button', { name: 'Load mixed outputs' }));
    window.dispatchEvent(
      new CustomEvent('outputFileExtensionsChanged', {
        detail: ['pdf', 'md', 'txt', 'doc', 'docx', 'jpg', 'png', 'yaml', 'json', 'py'],
      })
    );

    await waitFor(() => expect(screen.getByRole('tab', { name: 'Outputs 3' })).toBeInTheDocument());
    expect(screen.getByText('analysis.py')).toBeInTheDocument();
  });

  it('keeps a supported PDF visible when its metadata includes a parameterized MIME type', () => {
    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(screen.getByRole('button', { name: 'Load MIME PDF' }));

    expect(screen.getByRole('tab', { name: 'Outputs 1' })).toBeInTheDocument();
    expect(screen.getByText('report.pdf')).toBeInTheDocument();
    expect(readArtifactFile).not.toHaveBeenCalled();
  });

  it('shows uploaded files and stored pasted text in the Inputs tab', async () => {
    vi.mocked(listSessionLibraryInputs).mockResolvedValue([
      {
        id: 'pasted-text',
        name: 'Initial research notes',
        kind: 'text',
        scope: 'session',
        status: 'available',
        mimeType: 'text/plain',
        sizeBytes: 182,
      },
      {
        id: 'uploaded-report',
        name: 'market-report.pdf',
        kind: 'file',
        scope: 'session',
        status: 'available',
        mimeType: 'application/pdf',
        sizeBytes: 2_400,
      },
    ]);

    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(screen.getByRole('button', { name: 'Load inputs' }));
    const inputsTab = await screen.findByRole('tab', { name: 'Inputs 2' });
    fireEvent.click(inputsTab);

    expect(inputsTab).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByTestId('inputs-count')).toHaveClass('rounded-md', 'border');
    expect(await screen.findByText('Initial research notes')).toBeInTheDocument();
    expect(screen.getByText('market-report.pdf')).toBeInTheDocument();
    expect(screen.getByText(/text\/plain · Session · 182 B/)).toBeInTheDocument();
    expect(screen.getByText(/application\/pdf · Session · 3 KB/)).toBeInTheDocument();
    expect(listSessionLibraryInputs).toHaveBeenCalledWith('session-inputs');
  });

  function renderInputs() {
    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );
    fireEvent.click(screen.getByRole('button', { name: 'Load inputs' }));
    fireEvent.click(screen.getByRole('tab', { name: /^Inputs/ }));
  }

  const addedInput = {
    id: 'new-input',
    name: 'Notes',
    kind: 'text' as const,
    scope: 'session' as const,
    status: 'available' as const,
    mimeType: 'text/plain',
    sizeBytes: 12,
  };

  it('waits for the session to load before listing inputs and offers retry on failure', async () => {
    let finishLoad!: (loaded: boolean) => void;
    vi.mocked(acpChatSessionController.loadSession).mockReturnValue(
      new Promise((resolve) => {
        finishLoad = resolve;
      })
    );
    renderInputs();
    expect(listSessionLibraryInputs).not.toHaveBeenCalled();
    await act(async () => finishLoad(false));
    expect(await screen.findByText('Unable to load session inputs.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add file' })).toBeEnabled();
    vi.mocked(acpChatSessionController.loadSession).mockResolvedValue(true);
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(await screen.findByText('No inputs for this session')).toBeInTheDocument();
    expect(listSessionLibraryInputs).toHaveBeenCalledWith('session-inputs');
  });

  it('stores named pasted text without losing whitespace and selects it for the next message', async () => {
    vi.mocked(addSessionLibraryText).mockImplementation(async () => {
      vi.mocked(listSessionLibraryInputs).mockResolvedValue([addedInput]);
      return addedInput;
    });
    renderInputs();
    fireEvent.click(screen.getByRole('button', { name: 'Paste text' }));
    fireEvent.change(screen.getByLabelText('Name (optional)'), { target: { value: 'Notes' } });
    fireEvent.change(screen.getByLabelText('Text content'), { target: { value: '  source\n\n' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add text' }));
    const selected = await screen.findByRole('checkbox', {
      name: 'Include Notes with the next message',
    });
    expect(addSessionLibraryText).toHaveBeenCalledWith('session-inputs', 'Notes', '  source\n\n');
    expect(selected).toBeChecked();
    expect(screen.getByRole('tab', { name: 'Inputs 1' })).toBeInTheDocument();
    fireEvent.click(selected);
    expect(getSelectedSessionInputs('session-inputs')).toEqual([]);
  });

  it('retains pasted text after a failed save and rejects blank or oversized content', async () => {
    vi.mocked(addSessionLibraryText).mockRejectedValue(new Error('Storage unavailable'));
    renderInputs();
    fireEvent.click(screen.getByRole('button', { name: 'Paste text' }));
    const content = screen.getByLabelText('Text content');
    fireEvent.change(content, { target: { value: '  ' } });
    expect(screen.getByRole('button', { name: 'Add text' })).toBeDisabled();
    fireEvent.change(content, { target: { value: '😀'.repeat(70_000) } });
    expect(screen.getByRole('button', { name: 'Add text' })).toBeDisabled();
    fireEvent.change(content, { target: { value: 'Keep these notes' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add text' }));
    expect(await screen.findByText('Unable to add input: Storage unavailable')).toBeInTheDocument();
    expect(content).toHaveValue('Keep these notes');
    expect(getSelectedSessionInputs('session-inputs')).toEqual([]);
  });

  it('adds an individual file and clears the chooser so it can be used again', async () => {
    const fileItem = {
      ...addedInput,
      name: 'report.pdf',
      kind: 'file' as const,
      mimeType: 'application/pdf',
    };
    vi.mocked(linkSessionLibraryFile).mockImplementation(async () => {
      vi.mocked(listSessionLibraryInputs).mockResolvedValue([fileItem]);
      return fileItem;
    });
    renderInputs();
    const file = new File(['pdf'], 'report.pdf', { type: 'application/pdf' });
    const chooser = screen.getByLabelText('Add file');
    fireEvent.change(chooser, { target: { files: [file] } });
    expect(await screen.findByText('report.pdf')).toBeInTheDocument();
    expect(linkSessionLibraryFile).toHaveBeenCalledWith('session-inputs', file);
    expect(chooser).toHaveValue('');
    expect(getSelectedSessionInputs('session-inputs')).toEqual(['new-input']);
  });

  it('keeps a delayed file addition scoped to its original chat after switching sessions', async () => {
    let finishAdd!: (item: typeof addedInput) => void;
    vi.mocked(linkSessionLibraryFile).mockReturnValue(
      new Promise((resolve) => {
        finishAdd = resolve;
      })
    );
    renderInputs();
    fireEvent.change(screen.getByLabelText('Add file'), {
      target: { files: [new File(['text'], 'notes.txt')] },
    });
    await waitFor(() => expect(linkSessionLibraryFile).toHaveBeenCalled());
    fireEvent.click(screen.getByRole('button', { name: 'Load MIME PDF' }));
    await act(async () => finishAdd(addedInput));
    expect(screen.queryByText('Notes')).not.toBeInTheDocument();
    expect(getSelectedSessionInputs('session-inputs')).toEqual(['new-input']);
    expect(getSelectedSessionInputs('session-pdf')).toEqual([]);
  });

  it('browses durable research documents from the Library tab', async () => {
    vi.mocked(window.electron.listResearchLibraryFiles).mockResolvedValue({
      files: [
        {
          name: 'bayesian-neural-networks.md',
          path: '/Users/tester/Documents/Gosling Research Library/bnn/bayesian-neural-networks.md',
          relativePath: 'bnn/bayesian-neural-networks.md',
          sizeBytes: 4_096,
          modifiedAt: '2026-08-26T12:00:00.000Z',
        },
      ],
      truncated: false,
    });

    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );

    const libraryTab = await screen.findByRole('tab', { name: 'Library 1' });
    fireEvent.click(libraryTab);

    expect(screen.getByTestId('library-count')).toHaveClass('rounded-md', 'border');
    expect(await screen.findByText('bayesian-neural-networks.md')).toBeInTheDocument();
    expect(screen.getByText(/bnn\/bayesian-neural-networks.md · 4 KB/)).toBeInTheDocument();

    fireEvent.click(screen.getByText('bayesian-neural-networks.md'));
    expect(screen.getByRole('tab', { name: 'Outputs 0' })).toHaveAttribute('aria-selected', 'true');
  });

  it('shows a document title above its file name in the Library list', async () => {
    vi.mocked(window.electron.listResearchLibraryFiles).mockResolvedValue({
      files: [
        {
          name: 'NUMERICAL_VALIDATION.md',
          path: '/library/bounded/NUMERICAL_VALIDATION.md',
          relativePath: 'bounded/NUMERICAL_VALIDATION.md',
          sizeBytes: 4_096,
          modifiedAt: '2026-09-06T12:00:00.000Z',
        },
      ],
      truncated: false,
    });
    readArtifactTitles.mockResolvedValue({
      '/library/bounded/NUMERICAL_VALIDATION.md': 'Numerical source-consistency sweep',
    });

    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(await screen.findByRole('tab', { name: 'Library 1' }));

    expect(await screen.findByText('Numerical source-consistency sweep')).toBeInTheDocument();
    expect(screen.getByText(/bounded\/NUMERICAL_VALIDATION.md · 4 KB/)).toBeInTheDocument();
    expect(screen.queryByText('NUMERICAL_VALIDATION.md')).not.toBeInTheDocument();
  });

  it('keeps the file name when a document carries no title', async () => {
    vi.mocked(window.electron.listResearchLibraryFiles).mockResolvedValue({
      files: [
        {
          name: 'untitled.md',
          path: '/library/untitled.md',
          relativePath: 'untitled.md',
          sizeBytes: 12,
          modifiedAt: '2026-09-06T12:00:00.000Z',
        },
      ],
      truncated: false,
    });
    readArtifactTitles.mockResolvedValue({});

    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(await screen.findByRole('tab', { name: 'Library 1' }));
    expect(await screen.findByText('untitled.md')).toBeInTheDocument();
  });

  it('surfaces a truncated Research Library listing', async () => {
    vi.mocked(window.electron.listResearchLibraryFiles).mockResolvedValue({
      files: [
        {
          name: 'first.md',
          path: '/library/first.md',
          relativePath: 'first.md',
          sizeBytes: 1,
          modifiedAt: '2026-08-26T12:00:00.000Z',
        },
      ],
      truncated: true,
    });

    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );

    fireEvent.click(await screen.findByRole('tab', { name: 'Library 1+' }));
    expect(await screen.findByRole('status')).toHaveTextContent(
      'Showing the first 500 files. Open the Research Library folder'
    );
  });

  it('deletes an output from its row and closes its preview only after Trash succeeds', async () => {
    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );
    fireEvent.click(screen.getByRole('button', { name: 'Load mixed outputs' }));
    fireEvent.click(screen.getByTitle('/outputs/report.md'));
    fireEvent.click(screen.getByRole('button', { name: 'Delete report.md' }));
    expect(screen.getByTitle('/outputs/report.md')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Move to Trash' }));
    await waitFor(() => expect(screen.getByRole('tab', { name: 'Outputs 1' })).toBeInTheDocument());
    expect(trashArtifactFiles).toHaveBeenCalledWith(['/outputs/report.md']);
    expect(screen.queryByTitle('/outputs/report.md')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Close report.md')).not.toBeInTheDocument();
    expect(screen.getByTitle('/outputs/brief.docx')).toBeInTheDocument();
  });

  it('keeps failed Library batch selections and errors through the post-delete refresh', async () => {
    const files = ['one.md', 'two.md'].map((name) => ({
      name,
      path: `/library/${name}`,
      relativePath: name,
      sizeBytes: 12,
      modifiedAt: '2026-09-08T12:00:00Z',
    }));
    vi.mocked(window.electron.listResearchLibraryFiles).mockResolvedValue({
      files,
      truncated: false,
    });
    trashArtifactFiles.mockResolvedValue([
      { path: '/library/one.md', status: 'trashed' },
      { path: '/library/two.md', status: 'failed', error: 'File is locked' },
    ]);
    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );
    fireEvent.click(await screen.findByRole('tab', { name: 'Library 2' }));
    await screen.findByTitle('/library/one.md');
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select all' }));
    fireEvent.click(screen.getByRole('button', { name: 'Delete selected' }));
    vi.mocked(window.electron.listResearchLibraryFiles).mockResolvedValue({
      files: [files[1]],
      truncated: false,
    });
    fireEvent.click(screen.getByRole('button', { name: 'Move to Trash' }));
    await waitFor(() => expect(screen.getByRole('tab', { name: 'Library 1' })).toBeInTheDocument());
    expect(screen.queryByTitle('/library/one.md')).not.toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: 'Select two.md' })).toBeChecked();
    expect(screen.getByRole('alert')).toHaveTextContent('File is locked');
    expect(trashArtifactFiles).toHaveBeenCalledWith(['/library/one.md', '/library/two.md']);
  });

  it('clears output selection when the visible session changes', () => {
    render(
      <IntlTestWrapper>
        <ArtifactWorkbenchProvider>
          <Harness />
        </ArtifactWorkbenchProvider>
      </IntlTestWrapper>
    );
    fireEvent.click(screen.getByRole('button', { name: 'Load mixed outputs' }));
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select all' }));
    expect(screen.getByText('2 selected')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Load MIME PDF' }));
    expect(screen.queryByText('2 selected')).not.toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: 'Select report.pdf' })).not.toBeChecked();
    expect(screen.getByRole('button', { name: 'Delete selected' })).toBeDisabled();
  });
});
