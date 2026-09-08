import { useRef, useState } from 'react';
import { File, Trash2 } from 'lucide-react';
import { toast } from 'react-toastify';
import { defineMessages, useIntl } from '../../i18n';
import { cn } from '../../utils';
import { errorMessage } from '../../utils/conversionUtils';
import { ARTIFACT_TRASH_BATCH_LIMIT, type ArtifactTrashResult } from '../../types/artifactTrash';
import { Button } from '../ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';

const i18n = defineMessages({
  select: { id: 'artifactFiles.select', defaultMessage: 'Select {name}' },
  selectAll: { id: 'artifactFiles.selectAll', defaultMessage: 'Select all' },
  clear: { id: 'artifactFiles.clear', defaultMessage: 'Clear selection' },
  selected: { id: 'artifactFiles.selected', defaultMessage: '{count} selected' },
  delete: { id: 'artifactFiles.delete', defaultMessage: 'Delete {name}' },
  deleteSelected: { id: 'artifactFiles.deleteSelected', defaultMessage: 'Delete selected' },
  title: {
    id: 'artifactFiles.title',
    defaultMessage: 'Move {count, plural, one {# file} other {# files}} to Trash?',
  },
  description: {
    id: 'artifactFiles.description',
    defaultMessage:
      'These files will be removed from their current locations. You can restore them from Trash. Other copies are kept.',
  },
  cancel: { id: 'artifactFiles.cancel', defaultMessage: 'Cancel' },
  confirm: { id: 'artifactFiles.confirm', defaultMessage: 'Move to Trash' },
  deleting: { id: 'artifactFiles.deleting', defaultMessage: 'Moving to Trash…' },
  trashed: {
    id: 'artifactFiles.trashed',
    defaultMessage: '{count, plural, one {# file moved} other {# files moved}} to Trash.',
  },
  missing: {
    id: 'artifactFiles.missing',
    defaultMessage:
      '{count, plural, one {# file was} other {# files were}} already missing; removed from this list.',
  },
  failed: {
    id: 'artifactFiles.failed',
    defaultMessage:
      'Unable to delete {count, plural, one {# file} other {# files}}. See the errors beside each item.',
  },
});

export interface ArtifactFileListItem {
  path: string;
  name: string;
  detail: string;
  active: boolean;
  status?: string;
}

interface ArtifactFileListProps {
  items: ArtifactFileListItem[];
  label: string;
  onOpen: (path: string) => void;
  onDeleted: (paths: string[]) => void;
}

export async function trashArtifactFilesInBatches(
  requested: string[]
): Promise<ArtifactTrashResult[]> {
  const results: ArtifactTrashResult[] = [];
  for (let offset = 0; offset < requested.length; offset += ARTIFACT_TRASH_BATCH_LIMIT) {
    const batch = requested.slice(offset, offset + ARTIFACT_TRASH_BATCH_LIMIT);
    try {
      results.push(...(await window.electron.trashArtifactFiles(batch)));
    } catch (error) {
      const message = errorMessage(error, 'Unable to move files to Trash.');
      results.push(...batch.map((path) => ({ path, status: 'failed' as const, error: message })));
    }
  }
  return results;
}

export function ArtifactFileList({ items, label, onOpen, onDeleted }: ArtifactFileListProps) {
  const intl = useIntl();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [pending, setPending] = useState<ArtifactFileListItem[]>([]);
  const [busy, setBusy] = useState(false);
  const deleting = useRef(false);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const selectedItems = items.filter((item) => selected.has(item.path));
  const allSelected = items.length > 0 && selectedItems.length === items.length;

  const deletePending = async () => {
    if (deleting.current || pending.length === 0) return;
    deleting.current = true;
    setBusy(true);
    const requested = pending.map((item) => item.path);
    try {
      const results = await trashArtifactFilesInBatches(requested);
      const removed = results
        .filter((result) => result.status !== 'failed')
        .map((result) => result.path);
      const failures = results.filter((result) => result.status === 'failed');
      if (removed.length > 0) onDeleted(removed);
      setSelected((previous) => new Set([...previous].filter((path) => !removed.includes(path))));
      setErrors((previous) => ({
        ...Object.fromEntries(
          Object.entries(previous).filter(([path]) => !requested.includes(path))
        ),
        ...Object.fromEntries(
          failures.map((result) => [result.path, result.error ?? 'Unable to move file to Trash.'])
        ),
      }));
      const trashedCount = results.filter((result) => result.status === 'trashed').length;
      const missingCount = results.filter((result) => result.status === 'missing').length;
      if (trashedCount) toast.success(intl.formatMessage(i18n.trashed, { count: trashedCount }));
      if (missingCount) toast.info(intl.formatMessage(i18n.missing, { count: missingCount }));
      if (failures.length) toast.error(intl.formatMessage(i18n.failed, { count: failures.length }));
    } catch (error) {
      const message = errorMessage(error, 'Unable to move files to Trash.');
      setErrors((previous) => ({
        ...previous,
        ...Object.fromEntries(requested.map((path) => [path, message])),
      }));
      toast.error(message);
    } finally {
      deleting.current = false;
      setBusy(false);
      setPending([]);
    }
  };

  return (
    <>
      <div className="sticky top-0 z-10 flex flex-wrap items-center gap-2 border-b border-border-primary bg-background-primary px-3 py-1.5">
        <label className="flex items-center gap-2 text-xs">
          <input
            type="checkbox"
            aria-label={intl.formatMessage(i18n.selectAll)}
            checked={allSelected}
            ref={(node) => {
              if (node) node.indeterminate = selectedItems.length > 0 && !allSelected;
            }}
            disabled={busy || items.length === 0}
            onChange={(event) =>
              setSelected(
                event.target.checked ? new Set(items.map((item) => item.path)) : new Set()
              )
            }
          />
          {intl.formatMessage(i18n.selectAll)}
        </label>
        {selectedItems.length > 0 && (
          <>
            <span className="text-xs text-text-secondary">
              {intl.formatMessage(i18n.selected, { count: selectedItems.length })}
            </span>
            <Button
              type="button"
              variant="ghost"
              size="xs"
              disabled={busy}
              onClick={() => setSelected(new Set())}
            >
              {intl.formatMessage(i18n.clear)}
            </Button>
          </>
        )}
        <Button
          type="button"
          variant="ghost"
          size="xs"
          className="ml-auto"
          disabled={busy || selectedItems.length === 0}
          onClick={() => setPending(selectedItems)}
        >
          <Trash2 className="h-3.5 w-3.5" />
          {intl.formatMessage(i18n.deleteSelected)}
        </Button>
      </div>
      <ul className="py-1" aria-label={label} aria-busy={busy}>
        {items.map((item) => (
          <li
            key={item.path}
            className={cn(
              'border-b border-border-primary last:border-b-0',
              item.active && 'bg-background-secondary'
            )}
          >
            <div className="flex items-center gap-2 px-3">
              <input
                type="checkbox"
                checked={selected.has(item.path)}
                disabled={busy}
                aria-label={intl.formatMessage(i18n.select, { name: item.name })}
                onChange={(event) =>
                  setSelected((previous) => {
                    const next = new Set(previous);
                    if (event.target.checked) next.add(item.path);
                    else next.delete(item.path);
                    return next;
                  })
                }
              />
              <button
                type="button"
                className="flex min-w-0 flex-1 items-center gap-2 py-2.5 text-left hover:bg-background-secondary/60"
                title={item.path}
                onClick={() => onOpen(item.path)}
              >
                <File className="h-4 w-4 shrink-0 text-text-secondary" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-xs text-text-primary">{item.name}</span>
                  <span className="block truncate text-[10px] text-text-secondary">
                    {item.detail}
                  </span>
                </span>
                {item.status && (
                  <span className="text-[10px] text-text-secondary">{item.status}</span>
                )}
              </button>
              <Button
                type="button"
                variant="ghost"
                size="xs"
                disabled={busy}
                aria-label={intl.formatMessage(i18n.delete, { name: item.name })}
                title={intl.formatMessage(i18n.delete, { name: item.name })}
                onClick={() => setPending([item])}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </div>
            {errors[item.path] && (
              <p role="alert" className="break-words px-3 pb-2 text-xs text-text-secondary">
                {errors[item.path]}
              </p>
            )}
          </li>
        ))}
      </ul>
      <Dialog
        open={pending.length > 0}
        onOpenChange={(open) => {
          if (!open && !deleting.current) setPending([]);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{intl.formatMessage(i18n.title, { count: pending.length })}</DialogTitle>
            <DialogDescription>{intl.formatMessage(i18n.description)}</DialogDescription>
          </DialogHeader>
          <ul className="max-h-60 overflow-y-auto text-xs">
            {pending.map((item) => (
              <li key={item.path} className="break-all py-1">
                {item.path}
              </li>
            ))}
          </ul>
          <DialogFooter>
            <Button type="button" variant="outline" disabled={busy} onClick={() => setPending([])}>
              {intl.formatMessage(i18n.cancel)}
            </Button>
            <Button
              type="button"
              variant="destructive"
              disabled={busy}
              onClick={() => void deletePending()}
            >
              {intl.formatMessage(busy ? i18n.deleting : i18n.confirm)}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
