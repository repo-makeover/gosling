import { useEffect, useState } from 'react';
import type { OutputRevisionDto } from '@repo-makeover/gosling-sdk';
import { History } from 'lucide-react';
import {
  getLatestOutputRevision,
  getOutputHistory,
  getOutputRevision,
  restoreOutputRevision,
} from '../../acp/outputRevisions';
import { defineMessages, useIntl } from '../../i18n';
import { errorMessage } from '../../utils/conversionUtils';
import { ARTIFACT_TIMESTAMPS_REFRESH_EVENT } from '../../types/artifactFileTimestamps';
import { Button } from '../ui/button';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '../ui/dialog';

const i18n = defineMessages({
  history: { id: 'outputHistory.history', defaultMessage: 'History' },
  title: { id: 'outputHistory.title', defaultMessage: 'Output history' },
  loading: { id: 'outputHistory.loading', defaultMessage: 'Loading history…' },
  noRevisions: { id: 'outputHistory.noRevisions', defaultMessage: 'No saved revisions' },
  unknown: { id: 'outputHistory.unknown', defaultMessage: 'Unknown' },
  empty: {
    id: 'outputHistory.empty',
    defaultMessage:
      'No saved revisions. History begins with new changes observed by gosling; earlier authors are unknown.',
  },
  unavailable: { id: 'outputHistory.unavailable', defaultMessage: 'History unavailable' },
  more: { id: 'outputHistory.more', defaultMessage: 'Load older revisions' },
  refresh: { id: 'outputHistory.refresh', defaultMessage: 'Refresh' },
  export: { id: 'outputHistory.export', defaultMessage: 'Export revision' },
  restore: { id: 'outputHistory.restore', defaultMessage: 'Restore revision' },
  confirmRestore: {
    id: 'outputHistory.confirmRestore',
    defaultMessage:
      'Restore v{version} to this file? The current contents will be preserved and the restore will create a new revision.',
  },
  cancel: { id: 'outputHistory.cancel', defaultMessage: 'Cancel' },
  compare: { id: 'outputHistory.compare', defaultMessage: 'Compare with previous' },
  preview: { id: 'outputHistory.preview', defaultMessage: 'Saved content · v{version}' },
  previous: { id: 'outputHistory.previous', defaultMessage: 'Previous content · v{version}' },
  binary: {
    id: 'outputHistory.binary',
    defaultMessage:
      'Preview is available for text documents. Export this revision to open it in its application.',
  },
  truncated: {
    id: 'outputHistory.truncated',
    defaultMessage: 'Preview limited to 200,000 characters. Export preserves the complete file.',
  },
  tool: { id: 'outputHistory.tool', defaultMessage: 'Tool write' },
  observed: { id: 'outputHistory.observed', defaultMessage: 'Observed during tool execution' },
  user: { id: 'outputHistory.user', defaultMessage: 'User restore' },
  baseline: { id: 'outputHistory.baseline', defaultMessage: 'Saved pre-existing content' },
  model: {
    id: 'outputHistory.model',
    defaultMessage: 'Selected: {selected} · Actual: {actual} · Provider: {provider}',
  },
  note: {
    id: 'outputHistory.note',
    defaultMessage:
      'Observed changes identify the running agent, not exclusive authorship. Reading a file does not add authorship. Revisions belong to this file path and remain after Trash or chat deletion; later authorized chats can access them.',
  },
});

const DOCUMENT_EXTENSIONS =
  /\.(md|markdown|txt|csv|tsv|pdf|doc|docx|rtf|odt|xlsx|pptx|html|htm|png|jpg|jpeg|svg|webp)$/i;
const TEXT_EXTENSIONS = /\.(md|markdown|txt|csv|tsv|html|htm|svg)$/i;

type SavedRevision = Awaited<ReturnType<typeof getOutputRevision>>;

function textPreview(path: string, saved: SavedRevision | null): string | null {
  if (!saved || !TEXT_EXTENSIONS.test(path)) return null;
  try {
    const bytes = Uint8Array.from(window.atob(saved.contentBase64), (character) =>
      character.charCodeAt(0)
    );
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    if (!/\.(md|markdown)$/i.test(path)) return text;
    const starts = [
      ...text.matchAll(/\r?\n[ \t]*\r?\n[ \t]*<!-- gosling:output-history:start -->[ \t]*\r?\n/g),
    ];
    const footer = starts[starts.length - 1];
    return footer &&
      /\n[ \t]*<!-- gosling:output-history:end -->\s*$/.test(
        text.slice(footer.index + footer[0].length)
      )
      ? text.slice(0, footer.index)
      : text;
  } catch {
    return null;
  }
}

export function OutputHistory({
  sessionId,
  path,
  refreshKey,
  onRestored,
}: {
  sessionId: string;
  path: string;
  refreshKey?: string;
  onRestored?: () => void;
}) {
  const intl = useIntl();
  const [open, setOpen] = useState(false);
  const [latest, setLatest] = useState<OutputRevisionDto | null>(null);
  const [latestError, setLatestError] = useState(false);
  const [latestLoaded, setLatestLoaded] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [revisions, setRevisions] = useState<OutputRevisionDto[]>([]);
  const [next, setNext] = useState<number | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [saved, setSaved] = useState<SavedRevision | null>(null);
  const [previous, setPrevious] = useState<SavedRevision | null>(null);
  const [compare, setCompare] = useState(false);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmRestore, setConfirmRestore] = useState(false);
  const supported = DOCUMENT_EXTENSIONS.test(path);

  useEffect(() => {
    if (!supported) return;
    setLatest(null);
    setLatestError(false);
    setLatestLoaded(false);
    let controller = new AbortController();
    const load = () => {
      controller.abort();
      controller = new AbortController();
      const signal = controller.signal;
      void getLatestOutputRevision(sessionId, path, signal)
        .then((revision) => {
          if (!signal.aborted) {
            setLatest(revision);
            setLatestError(false);
            setLatestLoaded(true);
          }
        })
        .catch(() => {
          if (!signal.aborted) setLatestError(true);
        });
    };
    load();
    window.addEventListener('focus', load);
    return () => {
      controller.abort();
      window.removeEventListener('focus', load);
    };
  }, [sessionId, path, refreshKey, refresh, supported]);

  useEffect(() => {
    if (!open) return;
    let canceled = false;
    setLoading(true);
    setError(null);
    void getOutputHistory(sessionId, path)
      .then((page) => {
        if (canceled) return;
        setRevisions(page.revisions);
        setNext(page.nextBeforeVersion ?? null);
        setSelected(page.revisions[0]?.version ?? null);
      })
      .catch((reason) => {
        if (!canceled) setError(errorMessage(reason));
      })
      .finally(() => {
        if (!canceled) setLoading(false);
      });
    return () => {
      canceled = true;
    };
  }, [sessionId, path, open, refresh]);

  useEffect(() => {
    if (!open || selected === null) {
      setSaved(null);
      return;
    }
    let canceled = false;
    setSaved(null);
    setPrevious(null);
    setError(null);
    void getOutputRevision(sessionId, path, selected)
      .then((current) => {
        if (!canceled) setSaved(current);
      })
      .catch((reason) => {
        if (!canceled) setError(errorMessage(reason));
      });
    if (compare && selected > 1) {
      void getOutputRevision(sessionId, path, selected - 1)
        .then((older) => {
          if (!canceled) setPrevious(older);
        })
        .catch((reason) => {
          if (!canceled) setError(errorMessage(reason));
        });
    }
    return () => {
      canceled = true;
    };
  }, [sessionId, path, open, selected, compare, refresh]);

  if (!supported) return null;
  const unknown = intl.formatMessage(i18n.unknown);
  const preview = textPreview(path, saved);
  const olderPreview = textPreview(path, previous);

  const restore = async () => {
    if (!saved?.currentHash || busy) return;
    setBusy(true);
    setError(null);
    try {
      await restoreOutputRevision(sessionId, path, saved.revision.version, saved.currentHash);
      setRefresh((value) => value + 1);
      window.dispatchEvent(new Event(ARTIFACT_TIMESTAMPS_REFRESH_EVENT));
      onRestored?.();
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
      setConfirmRestore(false);
    }
  };

  const exportSaved = async () => {
    if (!saved || busy) return;
    setBusy(true);
    setError(null);
    try {
      const name = path.split(/[\\/]/).pop() ?? 'output';
      await window.electron.saveArtifact({
        source: { type: 'content', encoding: 'base64', content: saved.contentBase64 },
        defaultPath: name.replace(/(\.[^.]+)$/, `.v${saved.revision.version}$1`),
        title: intl.formatMessage(i18n.export),
      });
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="flex items-center gap-2 px-10 pb-2 text-[10px] text-text-secondary">
        <span
          className="min-w-0 flex-1 truncate"
          title={
            latest
              ? `${latest.contributor.agent} · ${latest.contributor.selectedModel ?? unknown}`
              : undefined
          }
        >
          {latest
            ? `v${latest.version} · ${latest.contributor.agent} · ${latest.contributor.selectedModel ?? unknown}`
            : intl.formatMessage(
                latestError ? i18n.unavailable : latestLoaded ? i18n.noRevisions : i18n.loading
              )}
        </span>
        <Button size="xs" variant="ghost" onClick={() => setOpen(true)}>
          <History className="h-3 w-3" />
          {intl.formatMessage(i18n.history)}
        </Button>
      </div>
      <Dialog
        open={open}
        onOpenChange={(value) => {
          if (!busy) setOpen(value);
        }}
      >
        <DialogContent className="sm:max-w-5xl max-h-[90vh] flex flex-col">
          <DialogHeader className="pr-6">
            <DialogTitle>{intl.formatMessage(i18n.title)}</DialogTitle>
            <DialogDescription className="break-all">{path}</DialogDescription>
          </DialogHeader>
          <p className="text-xs text-text-secondary">{intl.formatMessage(i18n.note)}</p>
          {error && (
            <p role="alert" className="text-sm text-text-danger break-words">
              {error}
            </p>
          )}
          <div className="min-h-0 overflow-y-auto">
            {loading ? (
              <p role="status">{intl.formatMessage(i18n.loading)}</p>
            ) : revisions.length === 0 ? (
              <p>{intl.formatMessage(i18n.empty)}</p>
            ) : (
              <div className="grid min-h-0 gap-4 md:grid-cols-[260px_minmax(0,1fr)]">
                <div className="max-h-60 overflow-y-auto md:max-h-[50vh]">
                  {revisions.map((revision) => (
                    <button
                      key={revision.version}
                      type="button"
                      aria-pressed={selected === revision.version}
                      disabled={busy}
                      className={`mb-2 w-full rounded border p-3 text-left text-xs ${selected === revision.version ? 'border-border-primary bg-background-secondary' : 'border-transparent hover:bg-background-secondary'}`}
                      onClick={() => setSelected(revision.version)}
                    >
                      <strong className="block">
                        v{revision.version} · {revision.contributor.agent}
                      </strong>
                      <span className="block break-words">
                        {revision.contributor.selectedModel ?? unknown}
                      </span>
                      <time className="block" dateTime={revision.recordedAt}>
                        {intl.formatDate(revision.recordedAt, {
                          dateStyle: 'medium',
                          timeStyle: 'medium',
                        })}
                      </time>
                      <span className="block">
                        {intl.formatMessage(
                          revision.action === 'baseline'
                            ? i18n.baseline
                            : i18n[revision.attribution]
                        )}
                      </span>
                    </button>
                  ))}
                  {next !== null && (
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={busy}
                      onClick={async () => {
                        setBusy(true);
                        try {
                          const page = await getOutputHistory(sessionId, path, next);
                          setRevisions((items) => [...items, ...page.revisions]);
                          setNext(page.nextBeforeVersion ?? null);
                        } catch (reason) {
                          setError(errorMessage(reason));
                        } finally {
                          setBusy(false);
                        }
                      }}
                    >
                      {intl.formatMessage(i18n.more)}
                    </Button>
                  )}
                </div>
                <div className="min-w-0 space-y-3">
                  {saved ? (
                    <>
                      <p className="text-xs break-words">
                        {intl.formatMessage(i18n.model, {
                          selected: saved.revision.contributor.selectedModel ?? unknown,
                          actual: saved.revision.contributor.resolvedModel ?? unknown,
                          provider: saved.revision.contributor.provider ?? unknown,
                        })}
                      </p>
                      <p className="text-xs break-words">
                        {saved.revision.contributor.sessionName} ·{' '}
                        {saved.revision.contributor.sessionId}
                      </p>
                      {preview !== null ? (
                        <>
                          <label className="flex gap-2 text-xs">
                            <input
                              type="checkbox"
                              checked={compare}
                              disabled={saved.revision.version <= 1 || busy}
                              onChange={(event) => setCompare(event.target.checked)}
                            />
                            {intl.formatMessage(i18n.compare)}
                          </label>
                          <div
                            className={`grid gap-3 ${olderPreview !== null ? 'lg:grid-cols-2' : ''}`}
                          >
                            {olderPreview !== null && (
                              <div>
                                <p className="text-xs">
                                  {intl.formatMessage(i18n.previous, {
                                    version: saved.revision.version - 1,
                                  })}
                                </p>
                                <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded bg-background-secondary p-3 text-xs">
                                  {olderPreview.slice(0, 200000)}
                                </pre>
                              </div>
                            )}
                            <div>
                              <p className="text-xs">
                                {intl.formatMessage(i18n.preview, {
                                  version: saved.revision.version,
                                })}
                              </p>
                              <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded bg-background-secondary p-3 text-xs">
                                {preview.slice(0, 200000)}
                              </pre>
                            </div>
                          </div>
                          {(preview.length > 200000 || (olderPreview?.length ?? 0) > 200000) && (
                            <p className="text-xs">{intl.formatMessage(i18n.truncated)}</p>
                          )}
                        </>
                      ) : (
                        <p className="text-sm">{intl.formatMessage(i18n.binary)}</p>
                      )}
                    </>
                  ) : !error ? (
                    <p role="status">{intl.formatMessage(i18n.loading)}</p>
                  ) : null}
                </div>
              </div>
            )}
          </div>
          <div className="flex shrink-0 flex-wrap justify-end gap-2">
            <Button
              variant="ghost"
              disabled={busy}
              onClick={() => setRefresh((value) => value + 1)}
            >
              {intl.formatMessage(i18n.refresh)}
            </Button>
            <Button variant="outline" disabled={!saved || busy} onClick={() => void exportSaved()}>
              {intl.formatMessage(i18n.export)}
            </Button>
            <Button disabled={!saved?.currentHash || busy} onClick={() => setConfirmRestore(true)}>
              {intl.formatMessage(i18n.restore)}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
      <ConfirmationModal
        isOpen={confirmRestore}
        title={intl.formatMessage(i18n.restore)}
        message={intl.formatMessage(i18n.confirmRestore, {
          version: saved?.revision.version ?? '',
        })}
        confirmLabel={intl.formatMessage(i18n.restore)}
        cancelLabel={intl.formatMessage(i18n.cancel)}
        isSubmitting={busy}
        onCancel={() => {
          if (!busy) setConfirmRestore(false);
        }}
        onConfirm={() => void restore()}
      />
    </>
  );
}
