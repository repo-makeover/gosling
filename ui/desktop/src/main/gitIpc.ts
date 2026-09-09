// Owns authorized Git subprocess helpers and renderer IPC registration.
// Extracted from ui/desktop/src/main.ts in a behavior-preserving modularization.
// The compatibility facade imports registerGitIpcHandlers; it re-exports none.

import type { IpcMain } from 'electron';
import { execFile } from 'child_process';

type AssertRendererFileAccess = (webContentsId: number, filePath: string) => Promise<string>;

function listGitWorktreeDirs(dir: string): Promise<string[]> {
  return new Promise((resolve) => {
    if (!dir?.trim()) {
      resolve([]);
      return;
    }

    execFile(
      'git',
      ['-C', dir, 'worktree', 'list', '--porcelain'],
      { timeout: 3000 },
      (error, stdout) => {
        if (error) {
          resolve([]);
          return;
        }

        const dirs = stdout
          .split('\n')
          .filter((line) => line.startsWith('worktree '))
          .map((line) => line.slice('worktree '.length).trim())
          .filter(Boolean)
          .filter((worktreeDir, index, allDirs) => allDirs.indexOf(worktreeDir) === index);

        resolve(dirs);
      }
    );
  });
}

export function gitArgs(dir: string, args: string[]): string[] {
  return ['-c', 'safe.bareRepository=explicit', '-c', 'core.fsmonitor=false', '-C', dir, ...args];
}

function runGit(dir: string, args: string[], timeout = 3000): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile('git', gitArgs(dir, args), { timeout }, (error, stdout) => {
      if (error) {
        reject(error);
        return;
      }
      resolve(stdout.trim());
    });
  });
}

async function getGitBranchInfo(dir: string): Promise<{ branch: string } | null> {
  try {
    const branch = await runGit(dir, ['symbolic-ref', '--quiet', '--short', 'HEAD']);
    return branch ? { branch } : null;
  } catch {
    try {
      const branch = await runGit(dir, ['rev-parse', '--short', 'HEAD']);
      return branch ? { branch } : null;
    } catch {
      return null;
    }
  }
}

export function getGitRepoRoot(dir: string): Promise<string | null> {
  return new Promise((resolve) => {
    if (!dir?.trim()) {
      resolve(null);
      return;
    }

    execFile(
      'git',
      gitArgs(dir, ['rev-parse', '--show-toplevel']),
      { timeout: 3000 },
      (error, stdout) => {
        resolve(error ? null : stdout.trim() || null);
      }
    );
  });
}

export function isPathGitIgnored(dir: string, targetPath: string): Promise<boolean> {
  return new Promise((resolve) => {
    execFile(
      'git',
      gitArgs(dir, ['check-ignore', '--quiet', targetPath]),
      { timeout: 3000 },
      (error) => {
        resolve(!error);
      }
    );
  });
}

export function isValidGitBranch(branch: unknown): branch is string {
  return (
    typeof branch === 'string' &&
    branch.length > 0 &&
    branch.length <= 255 &&
    !branch.startsWith('-') &&
    !branch.includes('\0')
  );
}

export function registerGitIpcHandlers(
  targetIpcMain: Pick<IpcMain, 'handle'>,
  assertRendererFileAccess: AssertRendererFileAccess
): void {
  targetIpcMain.handle('list-git-worktree-dirs', async (event, dir: string) => {
    const authorizedDir = await assertRendererFileAccess(event.sender.id, dir);
    return await listGitWorktreeDirs(authorizedDir);
  });

  targetIpcMain.handle('get-git-branch-info', async (event, dir: string) => {
    const authorizedDir = await assertRendererFileAccess(event.sender.id, dir);
    return await getGitBranchInfo(authorizedDir);
  });

  targetIpcMain.handle('list-git-branches', async (event, dir: string) => {
    const authorizedDir = await assertRendererFileAccess(event.sender.id, dir);
    try {
      const output = await runGit(authorizedDir, [
        'for-each-ref',
        'refs/heads/',
        '--format=%(refname:lstrip=2)',
      ]);
      return output ? output.split('\n').filter(Boolean) : [];
    } catch {
      return [];
    }
  });

  targetIpcMain.handle('switch-git-branch', async (event, dir: string, branch: unknown) => {
    const authorizedDir = await assertRendererFileAccess(event.sender.id, dir);
    if (!isValidGitBranch(branch)) return { success: false };

    try {
      await runGit(authorizedDir, ['check-ref-format', '--branch', branch]);
      await runGit(authorizedDir, ['switch', '--', branch], 30000);
      return { success: true };
    } catch {
      const currentBranch = await getGitBranchInfo(authorizedDir);
      return { success: currentBranch?.branch === branch };
    }
  });
}
