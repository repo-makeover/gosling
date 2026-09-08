import { useState } from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { toast } from 'react-toastify';
import { IntlTestWrapper } from '../../i18n/test-utils';
import {
  ArtifactFileList,
  trashArtifactFilesInBatches,
  type ArtifactFileListItem,
} from './ArtifactFileList';
import type { ArtifactTrashResult } from '../../types/artifactTrash';

vi.mock('react-toastify', () => ({ toast: { success: vi.fn(), info: vi.fn(), error: vi.fn() } }));

const files: ArtifactFileListItem[] = [
  { path: '/reports/one.md', name: 'One', detail: 'one.md', active: false },
  { path: '/reports/two.md', name: 'Two', detail: 'two.md', active: false },
];
const trashArtifactFiles = vi.fn();
const opened = vi.fn();
const deleted = vi.fn();

function Harness({
  scope = 'outputs:a',
  initialItems = files,
}: {
  scope?: string;
  initialItems?: ArtifactFileListItem[];
}) {
  const [items, setItems] = useState(initialItems);
  return (
    <IntlTestWrapper>
      <ArtifactFileList
        key={scope}
        label="Files"
        items={items}
        onOpen={opened}
        onDeleted={(paths) => {
          deleted(paths);
          setItems((previous) => previous.filter((item) => !paths.includes(item.path)));
        }}
      />
    </IntlTestWrapper>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  trashArtifactFiles
    .mockReset()
    .mockImplementation(async (paths: string[]) =>
      paths.map((path) => ({ path, status: 'trashed' }))
    );
  Object.assign(window.electron, { trashArtifactFiles });
});

describe('ArtifactFileList deletion', () => {
  it('selects without opening files, supports select-all and clear, and opens only the row action', () => {
    render(<Harness />);
    expect(screen.getByRole('button', { name: 'Delete selected' })).toBeDisabled();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select One' }));
    expect(screen.getByText('1 selected')).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: 'Select all' })).toBePartiallyChecked();
    expect(opened).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select all' }));
    expect(screen.getByText('2 selected')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Clear selection' }));
    expect(screen.getByRole('checkbox', { name: 'Select One' })).not.toBeChecked();
    fireEvent.click(screen.getByTitle('/reports/one.md'));
    expect(opened).toHaveBeenCalledWith('/reports/one.md');
  });

  it('confirms only the single row requested and leaves everything intact on cancel', () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select all' }));
    fireEvent.click(screen.getByRole('button', { name: 'Delete One' }));
    expect(screen.getByRole('dialog')).toHaveTextContent('Move 1 file to Trash?');
    expect(screen.getByRole('dialog')).toHaveTextContent('/reports/one.md');
    expect(screen.getByRole('dialog')).not.toHaveTextContent('/reports/two.md');
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(trashArtifactFiles).not.toHaveBeenCalled();
    expect(screen.getByText('2 selected')).toBeInTheDocument();
    expect(screen.getByTitle('/reports/one.md')).toBeInTheDocument();
  });

  it('moves the selected batch in one request after one confirmation', async () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select all' }));
    fireEvent.click(screen.getByRole('button', { name: 'Delete selected' }));
    expect(screen.getByRole('dialog')).toHaveTextContent('Move 2 files to Trash?');
    fireEvent.click(screen.getByRole('button', { name: 'Move to Trash' }));
    await waitFor(() => expect(deleted).toHaveBeenCalledWith(files.map((file) => file.path)));
    expect(trashArtifactFiles).toHaveBeenCalledTimes(1);
    expect(trashArtifactFiles).toHaveBeenCalledWith(files.map((file) => file.path));
    expect(screen.queryByTitle('/reports/one.md')).not.toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(toast.success).toHaveBeenCalledWith('2 files moved to Trash.');
  });

  it('keeps failed files selected with their errors and does not count them as deleted', async () => {
    trashArtifactFiles.mockResolvedValue([
      { path: files[0].path, status: 'trashed' },
      { path: files[1].path, status: 'failed', error: 'Trash unavailable' },
    ]);
    render(<Harness />);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select all' }));
    fireEvent.click(screen.getByRole('button', { name: 'Delete selected' }));
    fireEvent.click(screen.getByRole('button', { name: 'Move to Trash' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Trash unavailable');
    expect(deleted).toHaveBeenCalledWith([files[0].path]);
    expect(screen.getByRole('checkbox', { name: 'Select Two' })).toBeChecked();
    expect(toast.success).toHaveBeenCalledWith('1 file moved to Trash.');
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Unable to delete 1 file'));
  });

  it('retains the entire selection when IPC fails', async () => {
    trashArtifactFiles.mockRejectedValue(new Error('Connection lost'));
    render(<Harness />);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select all' }));
    fireEvent.click(screen.getByRole('button', { name: 'Delete selected' }));
    fireEvent.click(screen.getByRole('button', { name: 'Move to Trash' }));
    await waitFor(() => expect(screen.getAllByRole('alert')).toHaveLength(2));
    expect(deleted).not.toHaveBeenCalled();
    expect(screen.getByText('2 selected')).toBeInTheDocument();
    expect(toast.success).not.toHaveBeenCalled();
  });

  it('separates already-missing results from files actually moved to Trash', async () => {
    trashArtifactFiles.mockResolvedValue([{ path: files[0].path, status: 'missing' }]);
    render(<Harness />);
    fireEvent.click(screen.getByRole('button', { name: 'Delete One' }));
    fireEvent.click(screen.getByRole('button', { name: 'Move to Trash' }));
    await waitFor(() => expect(deleted).toHaveBeenCalledWith([files[0].path]));
    expect(toast.success).not.toHaveBeenCalled();
    expect(toast.info).toHaveBeenCalledWith('1 file was already missing; removed from this list.');
  });

  it('prevents repeat submission and clears selection when the list scope changes', async () => {
    let finish!: (results: ArtifactTrashResult[]) => void;
    trashArtifactFiles.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      })
    );
    const view = render(<Harness />);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select One' }));
    fireEvent.click(screen.getByRole('button', { name: 'Delete selected' }));
    fireEvent.click(screen.getByRole('button', { name: 'Move to Trash' }));
    expect(screen.getByRole('button', { name: 'Moving to Trash…' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Moving to Trash…' }));
    expect(trashArtifactFiles).toHaveBeenCalledTimes(1);
    await act(async () => finish([{ path: files[0].path, status: 'failed', error: 'Try again' }]));
    view.rerender(<Harness scope="outputs:b" />);
    expect(screen.getByRole('checkbox', { name: 'Select One' })).not.toBeChecked();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('splits large selections into bounded requests and preserves earlier successes if a later request fails', async () => {
    const manyFiles = Array.from({ length: 501 }, (_, index) => ({
      path: `/reports/${index}.md`,
      name: String(index),
      detail: `${index}.md`,
      active: false,
    }));
    trashArtifactFiles.mockImplementationOnce(async (paths: string[]) =>
      paths.map((path) => ({ path, status: 'trashed' }))
    );
    trashArtifactFiles.mockRejectedValueOnce(new Error('Connection lost'));
    const results = await trashArtifactFilesInBatches(manyFiles.map((file) => file.path));
    expect(trashArtifactFiles).toHaveBeenCalledTimes(2);
    expect(trashArtifactFiles.mock.calls.map(([batch]) => batch.length)).toEqual([500, 1]);
    expect(
      results.filter((result) => result.status === 'trashed').map((result) => result.path)
    ).toEqual(manyFiles.slice(0, 500).map((file) => file.path));
    expect(results[500]).toEqual({
      path: '/reports/500.md',
      status: 'failed',
      error: 'Connection lost',
    });
  });
});
