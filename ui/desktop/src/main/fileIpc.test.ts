import { describe, expect, it, vi } from 'vitest';
import { FILE_IPC_CHANNELS, registerFileIpcHandlers } from './fileIpc';

const bridge = vi.hoisted(() => ({ exposeInMainWorld: vi.fn(), invoke: vi.fn() }));

vi.mock('electron', () => ({
  contextBridge: { exposeInMainWorld: bridge.exposeInMainWorld },
  ipcRenderer: { invoke: bridge.invoke },
  webUtils: {},
  clipboard: { write: vi.fn(), writeText: vi.fn() },
  dialog: { showMessageBox: vi.fn(), showOpenDialog: vi.fn(), showSaveDialog: vi.fn() },
  shell: { openPath: vi.fn(), showItemInFolder: vi.fn() },
}));

describe('file IPC registration', () => {
  it('registers every original file and artifact channel once', () => {
    const handle = vi.fn();
    registerFileIpcHandlers(
      { handle },
      {
        assertRendererFileAccess: vi.fn(),
        assertRendererArtifactFileAccess: vi.fn(),
        resolveRendererPath: vi.fn(),
        grantRendererDirectory: vi.fn(),
        grantRendererArtifactFile: vi.fn(),
        updateArtifactRoutingConfig: vi.fn(),
        getAllowList: vi.fn(),
      }
    );

    expect(handle.mock.calls.map(([channel]) => channel)).toEqual(FILE_IPC_CHANNELS);
  });

  it('routes the artifact preload operations to registered native handlers', async () => {
    const handle = vi.fn();
    registerFileIpcHandlers(
      { handle },
      {
        assertRendererFileAccess: vi.fn(),
        assertRendererArtifactFileAccess: vi.fn(),
        resolveRendererPath: vi.fn(),
        grantRendererDirectory: vi.fn(),
        grantRendererArtifactFile: vi.fn(),
        updateArtifactRoutingConfig: vi.fn(),
        getAllowList: vi.fn(),
      }
    );
    const handlers = new Map(handle.mock.calls.map(([channel, handler]) => [channel, handler]));
    await import('../preload');
    const api = bridge.exposeInMainWorld.mock.calls.find(
      ([name]) => name === 'electron'
    )?.[1] as typeof window.electron;
    bridge.invoke.mockImplementation(async (channel: string) => {
      expect(handlers.has(channel)).toBe(true);
    });

    await api.copyArtifactContents('/output/report.md', '/output');
    await api.classifyArtifactRepositories(['/output/report.md']);
    await api.getArtifactFileTimestamps(['/output/report.md']);
    await api.trashArtifactFiles(['/output/report.md']);

    expect(bridge.invoke.mock.calls).toEqual([
      ['copy-artifact-contents', '/output/report.md', '/output'],
      ['classify-artifact-repositories', ['/output/report.md']],
      ['get-artifact-file-timestamps', ['/output/report.md']],
      ['trash-artifact-files', ['/output/report.md']],
    ]);
  });
});
