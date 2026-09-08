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
  shell: { openPath: vi.fn().mockResolvedValue(''), showItemInFolder: vi.fn() },
}));

const temporaryDirectories: string[] = [];

beforeEach(() => vi.clearAllMocks());
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
