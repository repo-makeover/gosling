import { act, render, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { ArtifactWorkbenchProvider, useArtifactWorkbench } from './ArtifactWorkbenchContext';

type Workbench = ReturnType<typeof useArtifactWorkbench>;

describe('ArtifactWorkbenchProvider', () => {
  let workbench: Workbench;

  function Harness() {
    workbench = useArtifactWorkbench();
    return null;
  }

  beforeEach(() => {
    localStorage.clear();
  });

  it('remembers the repository filter across sessions and remounts', () => {
    const view = render(
      <ArtifactWorkbenchProvider>
        <Harness />
      </ArtifactWorkbenchProvider>
    );
    expect(workbench.hideRepositoryFiles).toBe(false);
    act(() => workbench.setHideRepositoryFiles(true));
    act(() => workbench.setVisibleSession('another-session', []));
    expect(workbench.hideRepositoryFiles).toBe(true);
    view.unmount();
    render(
      <ArtifactWorkbenchProvider>
        <Harness />
      </ArtifactWorkbenchProvider>
    );
    expect(workbench.hideRepositoryFiles).toBe(true);
  });

  it('opens local files and transient tool outputs without a source-specific contract', () => {
    render(
      <ArtifactWorkbenchProvider>
        <Harness />
      </ArtifactWorkbenchProvider>
    );

    act(() => workbench.openFile('deliverables/brief.md', '/workspace'));
    expect(workbench.isOpen).toBe(true);
    expect(workbench.activeTab?.kind).toBe('markdown');

    act(() =>
      workbench.openContent({
        title: 'Tool output',
        content: '{"ok":true}',
        mimeType: 'application/json',
      })
    );
    expect(workbench.tabs).toHaveLength(2);
    expect(workbench.activeTab?.kind).toBe('json');

    act(() => workbench.openFile('deliverables/archive.bin', '/workspace'));
    expect(workbench.tabs).toHaveLength(2);
  });

  it('keeps the same relative output path distinct across unrelated working directories', () => {
    render(
      <ArtifactWorkbenchProvider>
        <Harness />
      </ArtifactWorkbenchProvider>
    );

    act(() => {
      workbench.openFile('output/report.md', '/projects/alpha');
      workbench.openFile('output/report.md', '/projects/beta');
    });

    expect(workbench.tabs).toHaveLength(2);
    expect(workbench.tabs.map((tab) => tab.source)).toEqual([
      { type: 'file', path: 'output/report.md', baseDirectory: '/projects/alpha' },
      { type: 'file', path: 'output/report.md', baseDirectory: '/projects/beta' },
    ]);
  });

  it('persists file tabs but not transient content', async () => {
    render(
      <ArtifactWorkbenchProvider>
        <Harness />
      </ArtifactWorkbenchProvider>
    );

    act(() => {
      workbench.openFile('/workspace/report.csv');
      workbench.openContent({ title: 'Log', content: 'done' });
    });

    await waitFor(() => {
      const stored = JSON.parse(localStorage.getItem('gosling-artifact-workbench-v1') ?? '{}');
      expect(stored.tabs).toHaveLength(1);
      expect(stored.tabs[0].source.path).toBe('/workspace/report.csv');
    });
  });

  it('keeps inventory and preview tabs session scoped without opening the pane', () => {
    render(
      <ArtifactWorkbenchProvider>
        <Harness />
      </ArtifactWorkbenchProvider>
    );
    const artifact = {
      sessionId: 'session-a',
      displayPath: 'src/main.rs',
      resolvedPath: '/workspace/src/main.rs',
      baseWorkingDir: '/workspace',
      relation: 'created' as const,
      provenance: 'built_in_tool' as const,
      firstSeenAt: '2026-01-01T00:00:00Z',
      lastSeenAt: '2026-01-01T00:00:00Z',
    };

    act(() => workbench.setVisibleSession('session-a', [artifact]));
    expect(workbench.artifacts).toEqual([artifact]);
    expect(workbench.tabs).toEqual([]);
    expect(workbench.isOpen).toBe(false);

    act(() => workbench.openArtifact(artifact));
    expect(workbench.activeTab?.kind).toBe('code');

    act(() =>
      workbench.openArtifact({
        ...artifact,
        displayPath: 'David.Casbeer@us.af.mil',
        resolvedPath: '/workspace/David.Casbeer@us.af.mil',
        relation: 'referenced',
        provenance: 'compatibility_inference',
      })
    );
    expect(workbench.tabs).toHaveLength(2);
    expect(workbench.activeTab?.kind).toBe('unknown');

    act(() => workbench.setVisibleSession('session-b', []));
    expect(workbench.artifacts).toEqual([]);
    expect(workbench.tabs).toEqual([]);

    act(() => workbench.setVisibleSession('session-a', [artifact]));
    expect(workbench.tabs).toHaveLength(2);
  });

  it('drops unsupported persisted tabs while retaining MIME-only text previews', () => {
    localStorage.setItem(
      'gosling-artifact-workbench-v1',
      JSON.stringify({
        isOpen: true,
        width: 480,
        tabs: [
          {
            id: 'unsupported',
            kind: 'unknown',
            source: { type: 'file', path: 'David.Casbeer@us.af.mil' },
            title: 'David.Casbeer@us.af.mil',
          },
          {
            id: 'supported',
            kind: 'markdown',
            source: { type: 'file', path: 'report.md' },
            title: 'report.md',
          },
          {
            id: 'mime-only-json',
            kind: 'json',
            source: { type: 'file', path: 'download' },
            title: 'download',
          },
          {
            id: 'mime-only-image',
            kind: 'image',
            source: { type: 'file', path: 'image-download' },
            title: 'image-download',
          },
          {
            id: 'stale-pdf-kind',
            kind: 'unknown',
            source: { type: 'file', path: 'report.pdf' },
            title: 'report.pdf',
          },
        ],
        activeTabId: 'unsupported',
      })
    );

    render(
      <ArtifactWorkbenchProvider>
        <Harness />
      </ArtifactWorkbenchProvider>
    );

    expect(workbench.tabs.map((tab) => tab.id)).toEqual([
      'supported',
      'mime-only-json',
      'stale-pdf-kind',
    ]);
    expect(workbench.tabs.find((tab) => tab.id === 'stale-pdf-kind')?.kind).toBe('pdf');
    expect(workbench.activeTabId).toBe('supported');
  });

  it('persists deleted output versions, closes previews, and shows later regenerated outputs', async () => {
    const artifact = {
      sessionId: 'session-a',
      displayPath: 'report.md',
      resolvedPath: '/workspace/report.md',
      baseWorkingDir: '/workspace',
      relation: 'created' as const,
      provenance: 'built_in_tool' as const,
      firstSeenAt: '2026-01-01T00:00:00Z',
      lastSeenAt: '2026-01-01T00:00:00Z',
    };
    const firstMount = render(
      <ArtifactWorkbenchProvider>
        <Harness />
      </ArtifactWorkbenchProvider>
    );
    act(() => workbench.setVisibleSession('session-a', [artifact]));
    act(() => workbench.openArtifact(artifact));
    act(() => workbench.forgetTrashedFiles([artifact.resolvedPath]));
    expect(workbench.artifacts).toEqual([]);
    expect(workbench.tabs).toEqual([]);
    act(() => workbench.openFile('/workspace/other.md'));
    act(() => workbench.closeTab(workbench.activeTabId!));
    await waitFor(() =>
      expect(
        JSON.parse(localStorage.getItem('gosling-artifact-workbench-v1')!).sessions['session-a']
          .deletedArtifacts
      ).toEqual({
        '/workspace/report.md': artifact.lastSeenAt,
      })
    );
    firstMount.unmount();
    render(
      <ArtifactWorkbenchProvider>
        <Harness />
      </ArtifactWorkbenchProvider>
    );
    act(() => workbench.setVisibleSession('session-a', [artifact]));
    expect(workbench.artifacts).toEqual([]);
    const regenerated = { ...artifact, lastSeenAt: '2026-01-02T00:00:00Z' };
    act(() => workbench.setVisibleSession('session-a', [regenerated]));
    expect(workbench.artifacts).toEqual([regenerated]);
  });

  it('applies a pending deletion to its captured session and version after navigation', () => {
    const artifact = {
      sessionId: 'session-a',
      displayPath: 'report.md',
      resolvedPath: '/alpha/report.md',
      baseWorkingDir: '/alpha',
      relation: 'created' as const,
      provenance: 'built_in_tool' as const,
      firstSeenAt: '2026-01-01T00:00:00Z',
      lastSeenAt: '2026-01-01T00:00:00Z',
    };
    render(
      <ArtifactWorkbenchProvider>
        <Harness />
      </ArtifactWorkbenchProvider>
    );
    act(() => workbench.setVisibleSession('session-a', [artifact]));
    act(() => workbench.openArtifact(artifact));
    const finishDeletion = workbench.forgetTrashedFiles;
    const other = {
      ...artifact,
      sessionId: 'session-b',
      baseWorkingDir: '/beta',
      resolvedPath: '/beta/report.md',
    };
    act(() => workbench.setVisibleSession('session-b', [other]));
    act(() => workbench.openArtifact(other));
    act(() => finishDeletion([artifact.resolvedPath]));
    expect(workbench.artifacts).toEqual([other]);
    expect(workbench.tabs).toHaveLength(1);
    act(() => workbench.setVisibleSession('session-a', [artifact]));
    expect(workbench.artifacts).toEqual([]);
    expect(workbench.tabs).toEqual([]);
  });

  it('does not dismiss a new artifact version published while Trash was pending', () => {
    const artifact = {
      sessionId: 'session-a',
      displayPath: 'report.md',
      resolvedPath: '/workspace/report.md',
      baseWorkingDir: '/workspace',
      relation: 'created' as const,
      provenance: 'built_in_tool' as const,
      firstSeenAt: '2026-01-01T00:00:00Z',
      lastSeenAt: '2026-01-01T00:00:00Z',
    };
    render(
      <ArtifactWorkbenchProvider>
        <Harness />
      </ArtifactWorkbenchProvider>
    );
    act(() => workbench.setVisibleSession('session-a', [artifact]));
    const finishDeletion = workbench.forgetTrashedFiles;
    const regenerated = { ...artifact, lastSeenAt: '2026-01-02T00:00:00Z' };
    act(() => workbench.setVisibleSession('session-a', [regenerated]));
    act(() => finishDeletion([artifact.resolvedPath]));
    expect(workbench.artifacts).toEqual([regenerated]);
  });
});
