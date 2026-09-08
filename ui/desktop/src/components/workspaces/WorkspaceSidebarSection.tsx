import { useCallback, useMemo, useState } from 'react';
import {
  ChevronDown,
  ChevronRight,
  FolderOpen,
  MoreHorizontal,
  Plus,
  TriangleAlert,
} from 'lucide-react';
import type { Workspace, WorkspaceWithValidation } from '@repo-makeover/gosling-sdk';
import { toast } from 'react-toastify';
import { acpExportWorkspace } from '../../acp/workspaces';
import { useArtifactRouter } from '../../contexts/ArtifactRouterContext';
import { useWorkspace } from '../../contexts/WorkspaceContext';
import { defineMessages, useIntl } from '../../i18n';
import { cn } from '../../utils';
import { workspaceErrorMessage } from '../../utils/workspaceError';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import { WorkspaceEditorDialog } from './WorkspaceEditorDialog';

const COLLAPSED_KEY = 'workspaces_sidebar_collapsed';

const i18n = defineMessages({
  chatReady: {
    id: 'workspaceSidebar.chatReady',
    defaultMessage: 'A chat in this workspace has a new reply',
  },
});

interface WorkspaceSidebarSectionProps {
  onNewChat(workspaceId: string): void;
  readyWorkspaceIds: ReadonlySet<string>;
}

export function WorkspaceSidebarSection({
  onNewChat,
  readyWorkspaceIds,
}: WorkspaceSidebarSectionProps) {
  const { saveArtifact } = useArtifactRouter();
  const {
    workspaces,
    defaultWorkspaceId,
    sessionWorkspaceFilterId,
    setSessionWorkspaceFilterId,
    loading,
    error,
    duplicateWorkspace,
    deleteWorkspace,
  } = useWorkspace();
  const [expanded, setExpanded] = useState(
    () => window.localStorage.getItem(COLLAPSED_KEY) !== 'true'
  );
  const [editor, setEditor] = useState<{ open: boolean; workspace?: Workspace | null }>({
    open: false,
  });
  const [warningsExpanded, setWarningsExpanded] = useState(false);
  const workspaceWarnings = useMemo(
    () =>
      workspaces.flatMap((item) =>
        (item.validation.issues ?? []).map((issue) => ({
          workspace: item.workspace,
          message: issue.message,
        }))
      ),
    [workspaces]
  );

  const toggleExpanded = useCallback(() => {
    setExpanded((current) => {
      window.localStorage.setItem(COLLAPSED_KEY, String(current));
      return !current;
    });
  }, []);

  const reveal = useCallback(async (workspace: Workspace) => {
    try {
      await window.electron.addRecentDir(workspace.workingFolder);
      const opened = await window.electron.openDirectoryInExplorer(workspace.workingFolder);
      if (!opened) {
        throw new Error('The workspace folder is unavailable. Relink it in Edit workspace.');
      }
    } catch (cause) {
      toast.error(workspaceErrorMessage(cause, 'Unable to reveal the workspace folder'));
    }
  }, []);

  const exportMetadata = useCallback(
    async (workspace: Workspace) => {
      try {
        const document = await acpExportWorkspace(workspace.id);
        const result = await saveArtifact({
          workspaceId: workspace.id,
          productType: 'export',
          suggestedName: `${workspace.name}.gosling-workspace.json`,
          title: 'Export workspace metadata',
          filters: [{ name: 'Gosling workspace', extensions: ['json'] }],
          source: { type: 'content', content: document, encoding: 'utf8' },
        });
        if (!result.canceled) toast.success('Workspace metadata exported');
      } catch (cause) {
        toast.error(workspaceErrorMessage(cause, 'Unable to export workspace'));
      }
    },
    [saveArtifact]
  );

  const remove = useCallback(
    async (workspace: Workspace) => {
      const response = await window.electron.showMessageBox({
        type: 'warning',
        buttons: ['Cancel', 'Delete workspace'],
        defaultId: 0,
        title: 'Delete workspace',
        message: `Delete “${workspace.name}”? Sessions and files will not be deleted.`,
      });
      if (response.response !== 1) return;
      try {
        await deleteWorkspace(workspace.id);
        toast.success('Workspace deleted; its sessions and files were preserved.');
      } catch (cause) {
        toast.error(workspaceErrorMessage(cause, 'Unable to delete workspace'));
      }
    },
    [deleteWorkspace]
  );

  return (
    <section aria-labelledby="workspaces-heading" className="pb-2">
      <div className="flex items-center px-3">
        <button
          type="button"
          onClick={toggleExpanded}
          className="flex min-w-0 flex-1 items-center gap-1 py-1 text-xs font-semibold uppercase tracking-wider text-text-secondary transition-colors hover:text-text-primary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-border-active"
          aria-expanded={expanded}
          aria-controls="workspace-list"
        >
          {expanded ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
          <span id="workspaces-heading">Workspaces</span>
        </button>
        <button
          type="button"
          onClick={() => setEditor({ open: true, workspace: null })}
          className="rounded-full p-1 text-text-secondary hover:bg-background-tertiary hover:text-text-primary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-border-active"
          aria-label="Add workspace"
        >
          <Plus className="size-4" />
        </button>
      </div>

      {expanded && (
        <>
          <div id="workspace-list" role="list" className="mt-1 space-y-0.5 px-2">
            <button
              type="button"
              onClick={() => setSessionWorkspaceFilterId(null)}
              className={workspaceRowClass(sessionWorkspaceFilterId === null)}
              aria-pressed={sessionWorkspaceFilterId === null}
            >
              <span className="flex size-5 items-center justify-center rounded bg-background-tertiary text-[10px]">
                ∞
              </span>
              <span className="flex-1 truncate text-left">All workspaces</span>
            </button>

            {loading ? (
              <div className="px-3 py-2 text-xs text-text-secondary">Loading workspaces…</div>
            ) : error ? (
              <div role="alert" className="px-3 py-2 text-xs text-red-600">
                {error}
              </div>
            ) : workspaces.length === 0 ? (
              <div className="px-3 py-2 text-xs text-text-secondary">No workspaces available</div>
            ) : (
              workspaces.map((item) => (
                <WorkspaceRow
                  key={item.workspace.id}
                  item={item}
                  filtered={item.workspace.id === sessionWorkspaceFilterId}
                  isDefault={item.workspace.id === defaultWorkspaceId}
                  hasReadyChat={readyWorkspaceIds.has(item.workspace.id)}
                  onFilter={() => setSessionWorkspaceFilterId(item.workspace.id)}
                  onNewChat={() => onNewChat(item.workspace.id)}
                  onEdit={() => setEditor({ open: true, workspace: item.workspace })}
                  onDuplicate={() => {
                    void duplicateWorkspace(item.workspace.id)
                      .then((duplicate) => toast.success(`Created “${duplicate.name}”.`))
                      .catch((cause) =>
                        toast.error(workspaceErrorMessage(cause, 'Unable to duplicate workspace'))
                      );
                  }}
                  onReveal={() => void reveal(item.workspace)}
                  onExport={() => void exportMetadata(item.workspace)}
                  onDelete={() => void remove(item.workspace)}
                />
              ))
            )}
          </div>

          {workspaceWarnings.length > 0 && (
            <div className="mx-2 mt-2 overflow-hidden rounded-lg border border-amber-500/30 bg-amber-500/5">
              <button
                type="button"
                className="flex w-full items-center gap-1.5 px-2 py-1.5 text-left text-xs text-amber-700 transition-colors hover:bg-amber-500/10 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-amber-500"
                onClick={() => setWarningsExpanded((current) => !current)}
                aria-expanded={warningsExpanded}
                aria-controls="workspace-warnings"
              >
                {warningsExpanded ? (
                  <ChevronDown className="size-3" aria-hidden="true" />
                ) : (
                  <ChevronRight className="size-3" aria-hidden="true" />
                )}
                <TriangleAlert className="size-3.5" aria-hidden="true" />
                <span className="font-medium">Warnings</span>
                <span className="ml-auto rounded-full bg-amber-500/15 px-1.5 py-0.5 text-[10px] tabular-nums">
                  {workspaceWarnings.length}
                </span>
              </button>
              {warningsExpanded && (
                <div id="workspace-warnings" role="list" className="border-t border-amber-500/20">
                  {workspaceWarnings.map((warning, index) => (
                    <button
                      key={`${warning.workspace.id}-${index}`}
                      type="button"
                      role="listitem"
                      className="block w-full border-b border-amber-500/15 px-2 py-2 text-left last:border-b-0 hover:bg-amber-500/10 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-amber-500"
                      onClick={() => setEditor({ open: true, workspace: warning.workspace })}
                      title={`Edit ${warning.workspace.name}`}
                    >
                      <span className="block truncate text-xs font-medium text-text-primary">
                        {warning.workspace.name}
                      </span>
                      <span className="mt-0.5 block text-[10px] leading-snug text-text-secondary">
                        {warning.message}
                      </span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </>
      )}

      <WorkspaceEditorDialog
        open={editor.open}
        workspace={editor.workspace}
        onOpenChange={(open) => setEditor((current) => ({ ...current, open }))}
      />
    </section>
  );
}

function WorkspaceRow({
  item,
  filtered,
  isDefault,
  hasReadyChat,
  onFilter,
  onNewChat,
  onEdit,
  onDuplicate,
  onReveal,
  onExport,
  onDelete,
}: {
  item: WorkspaceWithValidation;
  filtered: boolean;
  isDefault: boolean;
  hasReadyChat: boolean;
  onFilter(): void;
  onNewChat(): void;
  onEdit(): void;
  onDuplicate(): void;
  onReveal(): void;
  onExport(): void;
  onDelete(): void;
}) {
  const intl = useIntl();
  const { workspace, validation } = item;
  const hasWarnings = (validation.issues ?? []).length > 0;
  const warningSummary = (validation.issues ?? []).map((issue) => issue.message).join('; ');
  return (
    <div role="listitem" className="group flex items-center">
      <button
        type="button"
        onClick={onFilter}
        className={workspaceRowClass(filtered)}
        aria-pressed={filtered}
        aria-label={`${workspace.name}${filtered ? ', chat filter active' : ''}`}
      >
        <span className="flex size-5 items-center justify-center rounded bg-background-tertiary text-[10px] font-semibold uppercase">
          {(workspace.icon || workspace.name).slice(0, 2)}
        </span>
        {hasReadyChat && (
          <span
            role="img"
            className="size-2 shrink-0 rounded-full bg-green-500"
            aria-label={intl.formatMessage(i18n.chatReady)}
            title={intl.formatMessage(i18n.chatReady)}
          />
        )}
        <span className="min-w-0 flex-1 text-left">
          <span className="block truncate">{workspace.name}</span>
          <span className="block truncate text-[10px] text-text-secondary">
            {workspace.folders?.length ?? 0} refs · {workspace.productOutputFolders.length} outputs
          </span>
        </span>
        {hasWarnings && (
          <span
            role="img"
            aria-label={`Workspace needs attention: ${warningSummary}`}
            title={warningSummary}
          >
            <TriangleAlert className="size-3.5 text-amber-600" aria-hidden="true" />
          </span>
        )}
      </button>
      <div className="-ml-14 flex items-center opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
        <button
          type="button"
          className="rounded-full p-1 text-text-secondary hover:bg-background-secondary hover:text-text-primary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-border-active"
          onClick={(event) => {
            event.stopPropagation();
            onNewChat();
          }}
          aria-label={`New chat in ${workspace.name}`}
        >
          <Plus className="size-4" />
        </button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              className="rounded-full p-1 text-text-secondary hover:bg-background-secondary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-border-active"
              aria-label={`Actions for ${workspace.name}`}
            >
              <MoreHorizontal className="size-4" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-52">
            <DropdownMenuItem onSelect={onNewChat}>New chat in this workspace</DropdownMenuItem>
            <DropdownMenuItem onSelect={onFilter}>Show its chats</DropdownMenuItem>
            <DropdownMenuItem onSelect={onEdit}>Edit</DropdownMenuItem>
            <DropdownMenuItem onSelect={onDuplicate}>Duplicate</DropdownMenuItem>
            <DropdownMenuItem onSelect={onReveal}>
              <FolderOpen className="size-4" /> Reveal primary folder
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={onExport}>Export metadata</DropdownMenuItem>
            <DropdownMenuItem
              onSelect={onDelete}
              disabled={isDefault}
              className="text-red-600 focus:text-red-600"
            >
              Delete
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}

function workspaceRowClass(selected: boolean): string {
  return cn(
    'flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2 py-1.5 text-xs text-text-primary',
    'transition-colors hover:bg-background-tertiary/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-border-active',
    selected && 'bg-background-tertiary'
  );
}
