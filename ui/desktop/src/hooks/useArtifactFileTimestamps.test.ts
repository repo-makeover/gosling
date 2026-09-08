import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useArtifactFileTimestamps } from './useArtifactFileTimestamps';
import type { ArtifactFileTimestampMap } from '../types/artifactFileTimestamps';

const original = { createdAt: '2026-09-01T10:00:00Z', modifiedAt: '2026-09-08T11:00:00Z' };
const updated = { ...original, modifiedAt: '2026-09-08T12:00:00Z' };

beforeEach(() => {
  vi.mocked(window.electron.getArtifactFileTimestamps).mockReset().mockResolvedValue({});
});

describe('useArtifactFileTimestamps', () => {
  it('bounds large requests and preserves successful batches when one fails', async () => {
    const files = Array.from({ length: 450 }, (_, index) => ({ path: `/outputs/${index}.md` }));
    const getTimes = vi.mocked(window.electron.getArtifactFileTimestamps);
    getTimes.mockImplementation(async (paths) =>
      Object.fromEntries(paths.map((path) => [path, original]))
    );
    getTimes.mockImplementationOnce(async (paths) =>
      Object.fromEntries(paths.map((path) => [path, original]))
    );
    getTimes.mockRejectedValueOnce(new Error('Unavailable'));
    const { result } = renderHook(() => useArtifactFileTimestamps(files));
    await waitFor(() => expect(Object.keys(result.current)).toHaveLength(450));
    expect(getTimes.mock.calls.map(([paths]) => paths.length)).toEqual([200, 200, 50]);
    expect(result.current[files[0].path]).toEqual(original);
    expect(result.current[files[200].path]).toBeNull();
    expect(result.current[files[449].path]).toEqual(original);
  });

  it('ignores a delayed result after the inventory version changes', async () => {
    let finish!: (result: ArtifactFileTimestampMap) => void;
    const getTimes = vi.mocked(window.electron.getArtifactFileTimestamps);
    getTimes.mockReturnValueOnce(
      new Promise((resolve) => {
        finish = resolve;
      })
    );
    getTimes.mockResolvedValueOnce({ '/outputs/report.md': updated });
    const { result, rerender } = renderHook(
      ({ version }) =>
        useArtifactFileTimestamps([{ path: '/outputs/report.md', timestampRevision: version }]),
      { initialProps: { version: 'first' } }
    );
    rerender({ version: 'second' });
    await waitFor(() => expect(result.current['/outputs/report.md']).toEqual(updated));
    await act(async () => finish({ '/outputs/report.md': original }));
    expect(result.current['/outputs/report.md']).toEqual(updated);
  });

  it('refreshes on focus, avoids repeat requests on equivalent renders and cleans up on unmount', async () => {
    const getTimes = vi.mocked(window.electron.getArtifactFileTimestamps);
    getTimes.mockResolvedValue({ '/outputs/report.md': original });
    const { result, rerender, unmount } = renderHook(() =>
      useArtifactFileTimestamps([{ path: '/outputs/report.md' }])
    );
    await waitFor(() => expect(result.current['/outputs/report.md']).toEqual(original));
    rerender();
    expect(getTimes).toHaveBeenCalledTimes(1);
    getTimes.mockResolvedValue({ '/outputs/report.md': updated });
    act(() => window.dispatchEvent(new Event('focus')));
    await waitFor(() => expect(result.current['/outputs/report.md']).toEqual(updated));
    unmount();
    window.dispatchEvent(new Event('focus'));
    expect(getTimes).toHaveBeenCalledTimes(2);
  });
});
