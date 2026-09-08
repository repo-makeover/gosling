import { beforeEach, expect, it, vi } from 'vitest';
import { getAcpClient } from './acpConnection';
import { getLatestOutputRevision } from './outputRevisions';

vi.mock('./acpConnection', () => ({ getAcpClient: vi.fn() }));

const history = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(getAcpClient).mockResolvedValue({
    gosling: { sessionOutputsHistory_unstable: history },
  } as never);
});

it('limits simultaneous row summary requests and drains canceled rows without sending them', async () => {
  const pending: Array<() => void> = [];
  history.mockImplementation(
    () => new Promise((resolve) => pending.push(() => resolve({ revisions: [] })))
  );
  const controllers = Array.from({ length: 6 }, () => new AbortController());
  const requests = controllers.map((controller, index) =>
    getLatestOutputRevision('chat', `/Outputs/${index}.md`, controller.signal).catch(
      () => 'canceled'
    )
  );
  await vi.waitFor(() => expect(history).toHaveBeenCalledTimes(4));
  controllers[4].abort();
  pending.shift()!();
  await vi.waitFor(() => expect(history).toHaveBeenCalledTimes(5));
  expect(history.mock.calls.some(([request]) => request.path === '/Outputs/4.md')).toBe(false);
  pending.splice(0).forEach((resolve) => resolve());
  expect(await Promise.all(requests)).toEqual([null, null, null, null, 'canceled', null]);
});

it('releases a summary slot after a backend error', async () => {
  history.mockRejectedValueOnce(new Error('unavailable')).mockResolvedValue({ revisions: [] });
  await expect(
    getLatestOutputRevision('chat', '/Outputs/a.md', new AbortController().signal)
  ).rejects.toThrow('unavailable');
  await expect(
    getLatestOutputRevision('chat', '/Outputs/a.md', new AbortController().signal)
  ).resolves.toBeNull();
});
