// Owns native file, artifact, clipboard, process-probe, and allowlist IPC handlers.
// Extracted from ui/desktop/src/main.ts in a behavior-preserving modularization.
// The compatibility facade imports registerFileIpcHandlers; it re-exports none.

import type { IpcMain, OpenDialogOptions, OpenDialogReturnValue } from 'electron';
import { clipboard, dialog, shell } from 'electron';
import { Buffer } from 'node:buffer';
import { constants } from 'node:fs';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'child_process';
import type { ArtifactRoutingConfig, ArtifactSaveRequest } from '../types/artifactRouter';
import { saveArtifactWithDialog } from '../utils/artifactSave';
import { canonicalizePotentialPath } from '../utils/rendererFileAccess';
import { errorMessage } from '../utils/conversionUtils';
import { expandTilde } from '../utils/pathUtils';
import { readBoundedSessionImportFile } from '../utils/sessionImport';
import { documentTitleFromContent, supportsDocumentTitle } from '../utils/documentTitle';
import { ARTIFACT_TRASH_BATCH_LIMIT, type ArtifactTrashResult } from '../types/artifactTrash';
import {
  ARTIFACT_TIMESTAMPS_BATCH_LIMIT,
  type ArtifactFileTimestampMap,
} from '../types/artifactFileTimestamps';
import {
  ARTIFACT_REPOSITORY_BATCH_LIMIT,
  type ArtifactRepositoryClassification,
} from '../utils/artifactRepository';

type AssertRendererFileAccess = (webContentsId: number, filePath: string) => Promise<string>;
type AssertRendererArtifactFileAccess = (
  webContentsId: number,
  filePath: string,
  baseDirectory?: string
) => Promise<string>;

export interface FileIpcDependencies {
  assertRendererFileAccess: AssertRendererFileAccess;
  assertRendererArtifactFileAccess: AssertRendererArtifactFileAccess;
  resolveRendererPath: (filePath: string) => string;
  grantRendererDirectory: (webContentsId: number, filePath: string) => void;
  grantRendererArtifactFile: (webContentsId: number, filePath: string) => void;
  updateArtifactRoutingConfig: (
    webContentsId: number,
    config: ArtifactRoutingConfig | null
  ) => Promise<boolean>;
  getAllowList: () => Promise<string[]>;
}

/// Bounds the `ps`/`grep` probe below. (RES-GSL-001)
const CHECK_OLLAMA_TIMEOUT_MS = 5000;

/// Upper bound on a single `read-file` IPC response. Matches the text
/// preview limit used by `read-artifact-file`. (MEM-GSL-008)
const READ_FILE_MAX_BYTES = 2 * 1024 * 1024;
const COPY_ARTIFACT_MAX_BYTES = 20 * 1024 * 1024;

export const FILE_IPC_CHANNELS = [
  'select-file-or-directory',
  'select-artifact-file',
  'select-import-session-file',
  'check-ollama',
  'read-file',
  'read-artifact-file',
  'copy-artifact-contents',
  'read-artifact-titles',
  'get-artifact-file-timestamps',
  'classify-artifact-repositories',
  'open-artifact-file',
  'reveal-artifact-file',
  'write-file',
  'delete-file',
  'trash-artifact-files',
  'ensure-directory',
  'list-files',
  'show-message-box',
  'save-artifact',
  'set-artifact-routing-config',
  'write-clipboard-text',
  'write-clipboard-html',
  'get-allowed-extensions',
] as const;

async function isRepositoryDirectory(
  directory: string,
  cache: Map<string, boolean>
): Promise<boolean> {
  const cached = cache.get(directory);
  if (cached !== undefined) return cached;
  for (const marker of ['.git', '.hg', '.svn']) {
    try {
      const stats = await fs.stat(path.join(directory, marker));
      if (stats.isDirectory() || (marker === '.git' && stats.isFile())) {
        cache.set(directory, true);
        return true;
      }
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    }
  }
  const parent = path.dirname(directory);
  const result = parent !== directory && (await isRepositoryDirectory(parent, cache));
  cache.set(directory, result);
  return result;
}

export function registerFileIpcHandlers(
  targetIpcMain: Pick<IpcMain, 'handle'>,
  dependencies: FileIpcDependencies
): void {
  const {
    assertRendererFileAccess,
    assertRendererArtifactFileAccess,
    resolveRendererPath,
    grantRendererDirectory,
    grantRendererArtifactFile,
    updateArtifactRoutingConfig,
    getAllowList,
  } = dependencies;

  // Add file/directory selection handler
  targetIpcMain.handle('select-file-or-directory', async (event, defaultPath?: string) => {
    const dialogOptions: OpenDialogOptions = {
      properties: process.platform === 'darwin' ? ['openFile', 'openDirectory'] : ['openFile'],
    };

    // Set default path if provided
    if (defaultPath) {
      // Expand tilde to home directory
      const expandedPath = expandTilde(defaultPath);
      // Check if the path exists
      try {
        const stats = await fs.stat(expandedPath);
        dialogOptions.defaultPath = stats.isDirectory() ? expandedPath : path.dirname(expandedPath);
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
      } catch (error) {
        // If path doesn't exist, fall back to home directory and log error
        console.error(
          `Default path does not exist: ${expandedPath}, falling back to home directory`
        );
        dialogOptions.defaultPath = os.homedir();
      }
    }

    const result = (await dialog.showOpenDialog(dialogOptions)) as unknown as OpenDialogReturnValue;
    if (!result.canceled && result.filePaths.length > 0) {
      const selectedPath = result.filePaths[0];
      grantRendererDirectory(event.sender.id, selectedPath);
      return selectedPath;
    }
    return null;
  });

  targetIpcMain.handle('select-artifact-file', async (event, defaultPath?: string) => {
    const result = await dialog.showOpenDialog({
      properties: ['openFile'],
      defaultPath: defaultPath ? expandTilde(defaultPath) : undefined,
    });
    if (result.canceled || result.filePaths.length === 0) return null;
    const selectedPath = await canonicalizePotentialPath(resolveRendererPath(result.filePaths[0]));
    grantRendererArtifactFile(event.sender.id, selectedPath);
    return selectedPath;
  });

  // Native picker tailored for session imports: shows hidden files (so users can
  // reach `~/.claude/projects/...` or `~/.pi/agent/sessions/...`), filters for
  // .json/.jsonl, and returns the file's contents inline so the renderer doesn't
  // need a separate read step.
  targetIpcMain.handle('select-import-session-file', async () => {
    const result = (await dialog.showOpenDialog({
      title: 'Import session',
      defaultPath: os.homedir(),
      properties: ['openFile', 'showHiddenFiles'],
      filters: [
        { name: 'Session files', extensions: ['json', 'jsonl'] },
        { name: 'All files', extensions: ['*'] },
      ],
    })) as unknown as OpenDialogReturnValue;
    if (result.canceled || result.filePaths.length === 0) return null;
    const filePath = result.filePaths[0];
    try {
      const contents = await readBoundedSessionImportFile(filePath);
      return { filePath, contents };
    } catch (err) {
      return { filePath, contents: '', error: errorMessage(err) };
    }
  });

  targetIpcMain.handle('check-ollama', async () => {
    try {
      return new Promise((resolve) => {
        // Run `ps` and filter for "ollama"
        const ps = spawn('ps', ['aux']);
        const grep = spawn('grep', ['-iw', '[o]llama']);
        let output = '';
        let errorOutput = '';
        // Pipe ps output to grep
        ps.stdout.pipe(grep.stdin);
        grep.stdout.on('data', (data) => {
          output += data.toString();
        });
        grep.stderr.on('data', (data) => {
          errorOutput += data.toString();
        });
        grep.on('close', (code) => {
          if (code !== null && code !== 0 && code !== 1) {
            // grep returns 1 when no matches found
            console.error('Error executing grep command:', errorOutput);
            return resolve(false);
          }
          resolve(output.trim().length > 0);
        });
        ps.on('error', (error) => {
          console.error('Error executing ps command:', error);
          grep.kill();
          resolve(false);
        });
        grep.on('error', (error) => {
          console.error('Error executing grep command:', error);
          ps.kill();
          resolve(false);
        });
        // Close ps stdin when done
        ps.stdout.on('end', () => {
          grep.stdin.end();
        });
        // Neither child was bounded: if `ps` hung (a wedged filesystem is
        // enough), both processes and this promise stayed alive for the life of
        // the app, and each check leaked another pair (RES-GSL-001).
        const timeout = setTimeout(() => {
          ps.kill('SIGKILL');
          grep.kill('SIGKILL');
          console.warn('check-ollama timed out; assuming Ollama is not running');
          resolve(false);
        }, CHECK_OLLAMA_TIMEOUT_MS);
        timeout.unref?.();
        grep.on('close', () => clearTimeout(timeout));
      });
    } catch (err) {
      console.error('Error checking for Ollama:', err);
      return false;
    }
  });

  targetIpcMain.handle('read-file', async (event, filePath) => {
    try {
      const expandedPath = await assertRendererFileAccess(event.sender.id, filePath);
      // Read a bounded prefix rather than the whole file. The renderer chooses
      // the path, so an accidental multi-GB target used to be pulled entirely
      // into the main process (MEM-GSL-008). `read-artifact-file` beside this
      // handler already caps its read the same way.
      const stats = await fs.stat(expandedPath);
      const bytesToRead = Math.min(stats.size, READ_FILE_MAX_BYTES);
      const handle = await fs.open(expandedPath, 'r');
      let buffer: Buffer;
      try {
        buffer = Buffer.alloc(bytesToRead);
        await handle.read(buffer, 0, bytesToRead, 0);
      } finally {
        await handle.close();
      }
      return { file: buffer.toString('utf8'), filePath: expandedPath, error: null, found: true };
    } catch (error) {
      console.error('Error reading file:', error);
      return {
        file: '',
        filePath: resolveRendererPath(filePath),
        error: errorMessage(error),
        found: false,
      };
    }
  });

  targetIpcMain.handle(
    'read-artifact-file',
    async (event, filePath: string, baseDirectory?: string) => {
      try {
        const resolvedPath = await assertRendererArtifactFileAccess(
          event.sender.id,
          filePath,
          baseDirectory
        );
        const stats = await fs.stat(resolvedPath);
        if (!stats.isFile()) throw new Error('The selected output is not a file');
        const extension = path.extname(resolvedPath).toLowerCase();
        const binaryExtensions = new Set([
          '.gif',
          '.jpeg',
          '.jpg',
          '.pdf',
          '.png',
          '.svg',
          '.webp',
        ]);
        const previewLimit = binaryExtensions.has(extension) ? 20 * 1024 * 1024 : 2 * 1024 * 1024;
        const bytesToRead = Math.min(stats.size, previewLimit);
        const handle = await fs.open(resolvedPath, 'r');
        const buffer = Buffer.alloc(bytesToRead);
        try {
          await handle.read(buffer, 0, bytesToRead, 0);
        } finally {
          await handle.close();
        }
        const encoding = binaryExtensions.has(extension) ? 'base64' : 'utf8';
        return {
          content: buffer.toString(encoding),
          encoding,
          error: null,
          filePath: resolvedPath,
          found: true,
          sizeBytes: stats.size,
          truncated: stats.size > previewLimit,
        };
      } catch (error) {
        return {
          content: '',
          encoding: 'utf8',
          error: errorMessage(error),
          filePath: resolveRendererPath(filePath),
          found: false,
          sizeBytes: 0,
          truncated: false,
        };
      }
    }
  );

  targetIpcMain.handle(
    'copy-artifact-contents',
    async (event, filePath: string, baseDirectory?: string) => {
      const resolvedPath = await assertRendererArtifactFileAccess(
        event.sender.id,
        filePath,
        baseDirectory
      );
      const handle = await fs.open(
        resolvedPath,
        constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK
      );
      try {
        const stats = await handle.stat();
        if (!stats.isFile()) throw new Error('The selected output is not a file');
        if (stats.size > COPY_ARTIFACT_MAX_BYTES) {
          throw new Error(
            'Copy contents supports text files up to 20 MiB. Save a copy for larger files.'
          );
        }
        const buffer = Buffer.alloc(stats.size + 1);
        let total = 0;
        while (total < buffer.length) {
          const { bytesRead } = await handle.read(buffer, total, buffer.length - total, total);
          if (bytesRead === 0) break;
          total += bytesRead;
        }
        const current = await handle.stat();
        if (
          total !== stats.size ||
          current.size !== stats.size ||
          current.mtimeMs !== stats.mtimeMs
        ) {
          throw new Error('The file changed while copying. Try again.');
        }
        const contents = buffer.subarray(0, total);
        if (contents.includes(0)) throw new Error('Copy contents supports UTF-8 text files only.');
        let text: string;
        try {
          text = new TextDecoder('utf-8', { fatal: true }).decode(contents);
        } catch {
          throw new Error('Copy contents supports UTF-8 text files only.');
        }
        clipboard.writeText(text);
      } finally {
        await handle.close();
      }
    }
  );

  targetIpcMain.handle(
    'read-artifact-titles',
    async (event, requests: Array<{ filePath: string; baseDirectory?: string }>) => {
      // A list row only needs the document's own heading, so this reads a small
      // prefix per file instead of the preview reader's multi-megabyte window.
      const TITLE_PREFIX_BYTES = 16 * 1024;
      const MAX_FILES = 200;
      const titles: Record<string, string> = {};
      for (const request of (requests ?? []).slice(0, MAX_FILES)) {
        if (!request?.filePath || !supportsDocumentTitle(request.filePath)) continue;
        try {
          const resolvedPath = await assertRendererArtifactFileAccess(
            event.sender.id,
            request.filePath,
            request.baseDirectory
          );
          const stats = await fs.stat(resolvedPath);
          if (!stats.isFile()) continue;
          const bytesToRead = Math.min(stats.size, TITLE_PREFIX_BYTES);
          if (bytesToRead === 0) continue;
          const handle = await fs.open(resolvedPath, 'r');
          const buffer = Buffer.alloc(bytesToRead);
          try {
            await handle.read(buffer, 0, bytesToRead, 0);
          } finally {
            await handle.close();
          }
          const title = documentTitleFromContent(buffer.toString('utf8'));
          if (title) titles[request.filePath] = title;
        } catch {
          // A file the renderer may not read, or that vanished, simply has no
          // title; the row keeps showing its name.
        }
      }
      return titles;
    }
  );

  targetIpcMain.handle(
    'get-artifact-file-timestamps',
    async (event, filePaths: string[]): Promise<ArtifactFileTimestampMap> => {
      if (
        !Array.isArray(filePaths) ||
        filePaths.length > ARTIFACT_TIMESTAMPS_BATCH_LIMIT ||
        filePaths.some(
          (filePath) => typeof filePath !== 'string' || !filePath || filePath.length > 4096
        )
      ) {
        throw new Error('Invalid artifact timestamp batch');
      }
      const timestamps: ArtifactFileTimestampMap = {};
      for (const filePath of new Set(filePaths)) {
        timestamps[filePath] = null;
        try {
          const resolvedPath = await assertRendererArtifactFileAccess(event.sender.id, filePath);
          const stats = await fs.stat(resolvedPath);
          if (stats.isFile()) {
            timestamps[filePath] = {
              // Some filesystems report zero when birth time is unavailable; ctime is not creation.
              createdAt: stats.birthtimeMs === 0 ? null : stats.birthtime.toISOString(),
              modifiedAt: stats.mtime.toISOString(),
            };
          }
        } catch {
          // Missing and unauthorized files have no readable filesystem timestamps.
        }
      }
      return timestamps;
    }
  );

  targetIpcMain.handle(
    'classify-artifact-repositories',
    async (event, filePaths: string[]): Promise<ArtifactRepositoryClassification> => {
      if (
        !Array.isArray(filePaths) ||
        filePaths.length > ARTIFACT_REPOSITORY_BATCH_LIMIT ||
        filePaths.some(
          (filePath) => typeof filePath !== 'string' || !filePath || filePath.length > 4096
        )
      ) {
        throw new Error('Invalid artifact repository batch');
      }
      const result: ArtifactRepositoryClassification = {
        repositoryPaths: [],
        unavailablePaths: [],
      };
      const directories = new Map<string, boolean>();
      for (const filePath of new Set(filePaths)) {
        try {
          const resolvedPath = await assertRendererArtifactFileAccess(event.sender.id, filePath);
          if (await isRepositoryDirectory(path.dirname(resolvedPath), directories)) {
            result.repositoryPaths.push(filePath);
          }
        } catch {
          result.unavailablePaths.push(filePath);
        }
      }
      return result;
    }
  );

  targetIpcMain.handle(
    'open-artifact-file',
    async (event, filePath: string, baseDirectory?: string) => {
      const resolvedPath = await assertRendererArtifactFileAccess(
        event.sender.id,
        filePath,
        baseDirectory
      );
      return (await shell.openPath(resolvedPath)) === '';
    }
  );
  targetIpcMain.handle(
    'reveal-artifact-file',
    async (event, filePath: string, baseDirectory?: string) => {
      const resolvedPath = await assertRendererArtifactFileAccess(
        event.sender.id,
        filePath,
        baseDirectory
      );
      shell.showItemInFolder(resolvedPath);
    }
  );
  targetIpcMain.handle('write-file', async (event, filePath, content) => {
    try {
      await fs.writeFile(await assertRendererFileAccess(event.sender.id, filePath), content, {
        encoding: 'utf8',
      });
      return true;
    } catch (error) {
      console.error('Error writing to file:', error);
      return false;
    }
  });
  targetIpcMain.handle('delete-file', async (event, filePath) => {
    try {
      await fs.unlink(await assertRendererFileAccess(event.sender.id, filePath));
      return true;
    } catch (error) {
      console.error('Error deleting file:', error);
      return false;
    }
  });
  targetIpcMain.handle('trash-artifact-files', async (event, requestedPaths: unknown) => {
    if (
      !Array.isArray(requestedPaths) ||
      requestedPaths.length === 0 ||
      requestedPaths.length > ARTIFACT_TRASH_BATCH_LIMIT ||
      !requestedPaths.every(
        (filePath) =>
          typeof filePath === 'string' &&
          filePath.length <= 4096 &&
          !filePath.includes('\0') &&
          path.isAbsolute(filePath)
      )
    ) {
      throw new Error(`Select between 1 and ${ARTIFACT_TRASH_BATCH_LIMIT} local files.`);
    }

    const results: ArtifactTrashResult[] = [];
    for (const filePath of new Set<string>(requestedPaths)) {
      try {
        const authorizedPath = await assertRendererArtifactFileAccess(event.sender.id, filePath);
        const requestedFile = await fs.lstat(filePath).catch((error: NodeJS.ErrnoException) => {
          if (error.code === 'ENOENT') return null;
          throw error;
        });
        if (!requestedFile) {
          results.push({ path: filePath, status: 'missing' });
          continue;
        }
        if (!requestedFile.isFile()) {
          throw new Error('Only regular files can be moved to Trash from this pane.');
        }
        const authorizedFile = await fs.lstat(authorizedPath);
        if (
          !authorizedFile.isFile() ||
          requestedFile.dev !== authorizedFile.dev ||
          requestedFile.ino !== authorizedFile.ino
        ) {
          throw new Error('The file changed while preparing deletion. Try again.');
        }
        // Trash failures must never fall back to permanent deletion.
        await shell.trashItem(authorizedPath);
        results.push({ path: filePath, status: 'trashed' });
      } catch (error) {
        results.push({
          path: filePath,
          status: 'failed',
          error: errorMessage(error, 'Unable to move file to Trash.'),
        });
      }
    }
    return results;
  });
  // Enhanced file operations
  targetIpcMain.handle('ensure-directory', async (event, dirPath) => {
    try {
      await fs.mkdir(await assertRendererFileAccess(event.sender.id, dirPath), { recursive: true });
      return true;
    } catch (error) {
      console.error('Error creating directory:', error);
      return false;
    }
  });
  targetIpcMain.handle('list-files', async (event, dirPath, extension) => {
    try {
      const files = await fs.readdir(await assertRendererFileAccess(event.sender.id, dirPath));
      return extension ? files.filter((file) => file.endsWith(extension)) : files;
    } catch (error) {
      console.error('Error listing files:', error);
      return [];
    }
  });
  targetIpcMain.handle('show-message-box', async (_event, options) =>
    dialog.showMessageBox(options)
  );
  targetIpcMain.handle('save-artifact', async (event, request: ArtifactSaveRequest) =>
    saveArtifactWithDialog(request, {
      resolveSource: (filePath, baseDirectory) =>
        assertRendererArtifactFileAccess(event.sender.id, filePath, baseDirectory),
      showSaveDialog: (options) => dialog.showSaveDialog(options),
    })
  );
  targetIpcMain.handle(
    'set-artifact-routing-config',
    async (event, config: ArtifactRoutingConfig | null) =>
      updateArtifactRoutingConfig(event.sender.id, config)
  );
  targetIpcMain.handle('write-clipboard-text', async (_event, text: string) =>
    clipboard.writeText(text)
  );
  targetIpcMain.handle('write-clipboard-html', async (_event, html: string, text: string) =>
    clipboard.write({ html, text })
  );
  targetIpcMain.handle('get-allowed-extensions', async () => await getAllowList());
}
