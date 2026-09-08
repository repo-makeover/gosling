import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
import { dialog, shell } from 'electron';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { registerFileIpcHandlers } from './fileIpc';
import {
  assertArtifactFileAccess,
  resolveArtifactFileCapability,
} from '../utils/artifactFileAccess';
import { ArtifactRoutingRegistry } from '../utils/artifactRoutingRegistry';
import { RendererDirectoryGrantRegistry } from '../utils/rendererDirectoryGrants';
import { assertPathWithinRoots } from '../utils/rendererFileAccess';
import { expandTilde } from '../utils/pathUtils';

vi.mock('electron', () => ({
  clipboard: { write: vi.fn(), writeText: vi.fn() },
  dialog: { showMessageBox: vi.fn(), showOpenDialog: vi.fn(), showSaveDialog: vi.fn() },
  shell: { openPath: vi.fn().mockResolvedValue(''), showItemInFolder: vi.fn(), trashItem: vi.fn() },
}));

const temporaryDirectories: string[] = [];

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(shell.trashItem).mockReset().mockResolvedValue(undefined);
});
afterEach(async () => {
  await Promise.all(
    temporaryDirectories
      .splice(0)
      .map((directory) => fs.rm(directory, { recursive: true, force: true }))
  );
});

async function createMainFileIpc() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'gosling-main-artifacts-'));
  temporaryDirectories.push(root);
  const launchRoot = path.join(root, 'launch');
  const outputRoot = path.join(root, 'documents');
  await fs.mkdir(launchRoot);
  await fs.mkdir(outputRoot);
  const reportPath = path.join(outputRoot, 'repair-plan.md');
  await fs.writeFile(reportPath, '# Repair plan\n\nSession output.');
  const directoryGrants = new RendererDirectoryGrantRegistry(path.join(root, 'grants.json'));
  directoryGrants.grantSelectedPath(7, launchRoot, false);

  // Execute the live facade's authorization and dependency wiring, not the unused controller.
  // Exclude Electron startup so the regression cannot open windows or touch user settings.
  const source = ts.createSourceFile(
    'main.ts',
    await fs.readFile(path.join(process.cwd(), 'src/main.ts'), 'utf8'),
    ts.ScriptTarget.Latest,
    true
  );
  const functionNames = new Set([
    'resolveRendererPath',
    'rendererFileRoots',
    'assertRendererFileAccess',
    'assertRendererArtifactFileAccess',
    'assertArtifactOutputRootAccess',
    'validateArtifactRoutingConfig',
  ]);
  const functions = source.statements.filter(
    (statement) =>
      ts.isFunctionDeclaration(statement) && functionNames.has(statement.name?.text ?? '')
  );
  const productTypes = source.statements.find(
    (statement) =>
      ts.isVariableStatement(statement) &&
      statement.declarationList.declarations.some(
        (declaration) => declaration.name.getText(source) === 'ARTIFACT_PRODUCT_TYPES'
      )
  );
  const registration = source.statements.find(
    (statement) =>
      ts.isExpressionStatement(statement) &&
      ts.isCallExpression(statement.expression) &&
      statement.expression.expression.getText(source) === 'registerFileIpcHandlers'
  );
  expect(functions).toHaveLength(functionNames.size);
  expect(productTypes).toBeDefined();
  expect(registration).toBeDefined();
  const executable = [productTypes!, ...functions, registration!]
    .map((statement) => statement.getText(source))
    .join('\n');
  type Handler = (event: { sender: { id: number } }, ...args: unknown[]) => Promise<unknown>;
  const handlers = new Map<string, Handler>();
  vm.runInNewContext(
    ts.transpileModule(executable, { compilerOptions: { target: ts.ScriptTarget.ES2022 } })
      .outputText,
    {
      fs,
      path,
      expandTilde,
      assertPathWithinRoots,
      assertArtifactFileAccess,
      resolveArtifactFileCapability,
      rendererDirectoryGrants: directoryGrants,
      rendererArtifactFileGrants: new Map<number, Set<string>>(),
      artifactRoutingRegistry: new ArtifactRoutingRegistry(),
      registerFileIpcHandlers,
      ipcMain: { handle: (channel: string, handler: Handler) => handlers.set(channel, handler) },
      getAllowList: async () => [],
    }
  );
  const invoke = (channel: string, webContentsId: number, ...args: unknown[]) => {
    const handler = handlers.get(channel);
    if (!handler) throw new Error(`Missing file IPC handler: ${channel}`);
    return handler({ sender: { id: webContentsId } }, ...args);
  };
  const publish = (artifactFiles: string[]) =>
    invoke('set-artifact-routing-config', 7, { outputs: [], artifactFiles });
  return { invoke, publish, reportPath, outputRoot, launchRoot };
}

describe('live main artifact authorization', () => {
  it('classifies repository documents, worktrees and missing files without guessing from directory names', async () => {
    const { invoke, launchRoot } = await createMainFileIpc();
    const repo = path.join(launchRoot, 'project');
    const worktree = path.join(launchRoot, 'worktree');
    const ordinary = path.join(launchRoot, 'src');
    await fs.mkdir(path.join(repo, '.git'), { recursive: true });
    await fs.mkdir(path.join(repo, 'docs'));
    await fs.mkdir(worktree);
    await fs.writeFile(path.join(worktree, '.git'), 'gitdir: /elsewhere/worktrees/topic');
    await fs.mkdir(ordinary);
    const repoDoc = path.join(repo, 'docs', 'README.md');
    const missingDoc = path.join(repo, 'docs', 'removed.pdf');
    const worktreeData = path.join(worktree, 'data.json');
    const ordinaryDoc = path.join(ordinary, 'report.md');
    await fs.writeFile(repoDoc, '# Repository documentation');
    await fs.writeFile(worktreeData, '{}');
    await fs.writeFile(ordinaryDoc, '# Deliverable');
    expect(
      await invoke('classify-artifact-repositories', 7, [
        repoDoc,
        missingDoc,
        worktreeData,
        ordinaryDoc,
        repoDoc,
      ])
    ).toEqual({ repositoryPaths: [repoDoc, missingDoc, worktreeData], unavailablePaths: [] });
    expect(dialog.showOpenDialog).not.toHaveBeenCalled();
    expect(shell.openPath).not.toHaveBeenCalled();
  });

  it('guards repository classification with the exact per-window file capability', async () => {
    const { invoke, publish, reportPath, outputRoot, launchRoot } = await createMainFileIpc();
    await fs.mkdir(path.join(outputRoot, '.git'));
    const sibling = path.join(outputRoot, 'private.md');
    await fs.writeFile(sibling, 'private');
    await publish([reportPath]);
    expect(await invoke('classify-artifact-repositories', 7, [reportPath, sibling])).toEqual({
      repositoryPaths: [reportPath],
      unavailablePaths: [sibling],
    });
    expect(await invoke('classify-artifact-repositories', 8, [reportPath])).toEqual({
      repositoryPaths: [],
      unavailablePaths: [reportPath],
    });
    const escaped = path.join(launchRoot, 'escape.md');
    await fs.symlink(sibling, escaped);
    expect(await invoke('classify-artifact-repositories', 7, [escaped])).toEqual({
      repositoryPaths: [],
      unavailablePaths: [escaped],
    });
    expect(await invoke('read-file', 7, reportPath)).toMatchObject({
      error: expect.stringContaining('outside approved roots'),
    });
  });

  it('rejects invalid or oversized repository classification batches', async () => {
    const { invoke, reportPath } = await createMainFileIpc();
    for (const request of [null, [12], [''], Array(201).fill(reportPath)]) {
      await expect(invoke('classify-artifact-repositories', 7, request)).rejects.toThrow(
        'Invalid artifact repository batch'
      );
    }
  });

  it('previews a session document outside launch roots without a picker', async () => {
    const { invoke, publish, reportPath } = await createMainFileIpc();
    expect(await publish([reportPath])).toBe(true);
    expect(await invoke('read-artifact-file', 7, reportPath)).toMatchObject({
      content: '# Repair plan\n\nSession output.',
      error: null,
      found: true,
    });
    expect(dialog.showOpenDialog).not.toHaveBeenCalled();
  });

  it('supports relative previews, document titles, open and reveal through the same capability', async () => {
    const { invoke, publish, reportPath, outputRoot } = await createMainFileIpc();
    await publish([reportPath]);
    expect(
      await invoke('read-artifact-file', 7, path.basename(reportPath), outputRoot)
    ).toMatchObject({ error: null });
    expect(await invoke('read-artifact-titles', 7, [{ filePath: reportPath }])).toEqual({
      [reportPath]: 'Repair plan',
    });
    expect(await invoke('open-artifact-file', 7, reportPath)).toBe(true);
    await invoke('reveal-artifact-file', 7, reportPath);
    expect(shell.openPath).toHaveBeenCalledWith(await fs.realpath(reportPath));
    expect(shell.showItemInFolder).toHaveBeenCalledWith(await fs.realpath(reportPath));
    expect(dialog.showOpenDialog).not.toHaveBeenCalled();
  });

  it('does not grant neighboring files, another window or generic file IPC', async () => {
    const { invoke, publish, reportPath, outputRoot } = await createMainFileIpc();
    const sibling = path.join(outputRoot, 'private.md');
    await fs.writeFile(sibling, 'private');
    await publish([reportPath]);
    for (const [windowId, filePath] of [
      [7, sibling],
      [8, reportPath],
    ] as const) {
      expect(await invoke('read-artifact-file', windowId, filePath)).toMatchObject({
        error: expect.stringContaining('outside approved roots'),
      });
    }
    expect(await invoke('read-file', 7, reportPath)).toMatchObject({
      error: expect.stringContaining('outside approved roots'),
    });
  });

  it('revokes the previous session capability when routing changes or clears', async () => {
    const { invoke, publish, reportPath, outputRoot } = await createMainFileIpc();
    const nextReport = path.join(outputRoot, 'next.md');
    await fs.writeFile(nextReport, '# Next');
    await publish([reportPath]);
    await publish([nextReport]);
    expect(await invoke('read-artifact-file', 7, reportPath)).toMatchObject({
      error: expect.stringContaining('outside approved roots'),
    });
    expect(await invoke('read-artifact-file', 7, nextReport)).toMatchObject({ error: null });
    await invoke('set-artifact-routing-config', 7, null);
    expect(await invoke('read-artifact-file', 7, nextReport)).toMatchObject({
      error: expect.stringContaining('outside approved roots'),
    });
  });

  it('trashes an authorized output once and reports each denied or failed file honestly', async () => {
    const { invoke, publish, reportPath, outputRoot, launchRoot } = await createMainFileIpc();
    const failedFile = path.join(launchRoot, 'failed.md');
    const deniedFile = path.join(outputRoot, 'private.md');
    await fs.writeFile(failedFile, 'keep');
    await fs.writeFile(deniedFile, 'private');
    await publish([reportPath]);
    vi.mocked(shell.trashItem).mockImplementation(async (filePath) => {
      if (filePath === (await fs.realpath(failedFile))) {
        throw Object.assign(new Error('Trash is unavailable'), { code: 'ENOENT' });
      }
    });
    const result = await invoke('trash-artifact-files', 7, [
      reportPath,
      reportPath,
      deniedFile,
      failedFile,
    ]);
    expect(result).toEqual([
      { path: reportPath, status: 'trashed' },
      {
        path: deniedFile,
        status: 'failed',
        error: expect.stringContaining('outside approved roots'),
      },
      { path: failedFile, status: 'failed', error: 'Trash is unavailable' },
    ]);
    expect(shell.trashItem).toHaveBeenCalledTimes(2);
    expect(shell.trashItem).toHaveBeenCalledWith(await fs.realpath(reportPath));
    expect(await fs.readFile(failedFile, 'utf8')).toBe('keep');
    expect(await fs.readFile(deniedFile, 'utf8')).toBe('private');
  });

  it('does not trash folders, symlinks or another window’s session files', async () => {
    const { invoke, publish, reportPath, launchRoot } = await createMainFileIpc();
    await publish([reportPath]);
    const link = path.join(launchRoot, 'link.md');
    await fs.symlink(reportPath, link);
    expect(await invoke('trash-artifact-files', 7, [launchRoot, link])).toEqual([
      { path: launchRoot, status: 'failed', error: expect.stringContaining('Only regular files') },
      { path: link, status: 'failed', error: expect.stringContaining('Only regular files') },
    ]);
    expect(await invoke('trash-artifact-files', 8, [reportPath])).toEqual([
      {
        path: reportPath,
        status: 'failed',
        error: expect.stringContaining('outside approved roots'),
      },
    ]);
    expect(shell.trashItem).not.toHaveBeenCalled();
  });

  it('reports already-missing authorized files without claiming to have trashed them', async () => {
    const { invoke, launchRoot } = await createMainFileIpc();
    const missing = path.join(launchRoot, 'missing.md');
    expect(await invoke('trash-artifact-files', 7, [missing])).toEqual([
      { path: missing, status: 'missing' },
    ]);
    expect(shell.trashItem).not.toHaveBeenCalled();
  });

  it('rejects malformed and oversized Trash batches before touching any file', async () => {
    const { invoke, reportPath } = await createMainFileIpc();
    for (const requested of [
      null,
      [],
      ['relative.md'],
      [123],
      [reportPath, '../escape.md'],
      Array(501).fill(reportPath),
    ]) {
      await expect(invoke('trash-artifact-files', 7, requested)).rejects.toThrow('Select between');
    }
    expect(shell.trashItem).not.toHaveBeenCalled();
  });

  it('still rejects source files and directories as session document capabilities', async () => {
    const { publish, outputRoot } = await createMainFileIpc();
    const source = path.join(outputRoot, 'source.ts');
    const directory = path.join(outputRoot, 'directory.md');
    await fs.writeFile(source, 'export {};');
    await fs.mkdir(directory);
    expect(await publish([source, directory])).toBe(false);
  });

  it('rejects a symlink retargeted after validation and preserves ordinary directory access', async () => {
    const { invoke, publish, reportPath, outputRoot, launchRoot } = await createMainFileIpc();
    await publish([reportPath]);
    const otherFile = path.join(outputRoot, 'other.txt');
    await fs.writeFile(otherFile, 'private');
    await fs.unlink(reportPath);
    await fs.symlink(otherFile, reportPath);
    expect(await invoke('read-artifact-file', 7, reportPath)).toMatchObject({
      error: expect.stringContaining('outside approved roots'),
    });
    const launchFile = path.join(launchRoot, 'allowed.txt');
    await fs.writeFile(launchFile, 'allowed');
    expect(await invoke('read-artifact-file', 7, launchFile)).toMatchObject({
      content: 'allowed',
      error: null,
    });
  });
});
