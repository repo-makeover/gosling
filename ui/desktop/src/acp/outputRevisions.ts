import { getAcpClient } from './acpConnection';
import type { OutputRevisionDto } from '@repo-makeover/gosling-sdk';

let activeSummaries = 0;
const waitingSummaries: Array<() => void> = [];

export async function getOutputHistory(sessionId: string, path: string, beforeVersion?: number) {
  const client = await getAcpClient();
  return client.gosling.sessionOutputsHistory_unstable({
    sessionId,
    path,
    beforeVersion,
    limit: 50,
  });
}

export async function getLatestOutputRevision(
  sessionId: string,
  path: string,
  signal: globalThis.AbortSignal
): Promise<OutputRevisionDto | null> {
  if (activeSummaries >= 4) await new Promise<void>((resolve) => waitingSummaries.push(resolve));
  activeSummaries += 1;
  try {
    signal.throwIfAborted();
    const client = await getAcpClient();
    signal.throwIfAborted();
    const response = await client.gosling.sessionOutputsHistory_unstable({
      sessionId,
      path,
      limit: 1,
    });
    return response.revisions[0] ?? null;
  } finally {
    activeSummaries -= 1;
    waitingSummaries.shift()?.();
  }
}

export async function getOutputRevision(sessionId: string, path: string, version: number) {
  const client = await getAcpClient();
  return client.gosling.sessionOutputsRevision_unstable({ sessionId, path, version });
}

export async function restoreOutputRevision(
  sessionId: string,
  path: string,
  version: number,
  expectedCurrentHash: string
) {
  const client = await getAcpClient();
  return client.gosling.sessionOutputsRestore_unstable({
    sessionId,
    path,
    version,
    expectedCurrentHash,
  });
}
