import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Folder, FolderPlus, Plus, ShieldAlert, X } from 'lucide-react';
import { toast } from 'react-toastify';
import { defineMessages, useIntl } from '../i18n';
import {
  acpAddSessionWorkingDir,
  acpRemoveSessionWorkingDir,
  acpSetWorkingDirRestriction,
} from '../acp/sessions';
import type { Session } from '../types/session';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';
import { Switch } from './ui/switch';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from './ui/Tooltip';

const i18n = defineMessages({
  workingDirectories: {
    id: 'workingDirectoriesMenu.workingDirectories',
    defaultMessage: 'Working Directories',
  },
  directoriesShort: {
    id: 'workingDirectoriesMenu.directoriesShort',
    defaultMessage: 'Dirs',
  },
  addDirectoryShort: {
    id: 'workingDirectoriesMenu.addDirectoryShort',
    defaultMessage: 'Add Dir',
  },
  primary: {
    id: 'workingDirectoriesMenu.primary',
    defaultMessage: 'Primary',
  },
  addDirectory: {
    id: 'workingDirectoriesMenu.addDirectory',
    defaultMessage: 'Add directory…',
  },
  removeDirectory: {
    id: 'workingDirectoriesMenu.removeDirectory',
    defaultMessage: 'Remove directory',
  },
  recentDirectories: {
    id: 'workingDirectoriesMenu.recentDirectories',
    defaultMessage: 'Recent directories',
  },
  failedToAdd: {
    id: 'workingDirectoriesMenu.failedToAdd',
    defaultMessage: 'Failed to add working directory',
  },
  failedToRemove: {
    id: 'workingDirectoriesMenu.failedToRemove',
    defaultMessage: 'Failed to remove working directory',
  },
  alreadyAdded: {
    id: 'workingDirectoriesMenu.alreadyAdded',
    defaultMessage: 'That directory is already added',
  },
  fullAccessHint: {
    id: 'workingDirectoriesMenu.fullAccessHint',
    defaultMessage: 'Gosling has full read/write/run access inside every directory listed here.',
  },
  sessionAddedFullAccessHint: {
    id: 'workingDirectoriesMenu.sessionAddedFullAccessHint',
    defaultMessage:
      'Directories you add receive full read/write/run access in this session. Existing workspace folder permissions stay pinned.',
  },
  restrictToggleLabel: {
    id: 'workingDirectoriesMenu.restrictToggleLabel',
    defaultMessage: 'Restrict tools to working directories',
  },
  restrictToggleDescription: {
    id: 'workingDirectoriesMenu.restrictToggleDescription',
    defaultMessage:
      'Off by default. When on, actions outside the directories above need your approval, with an explanation of what and why.',
  },
  restrictToggleDescriptionWorkspace: {
    id: 'workingDirectoriesMenu.restrictToggleDescriptionWorkspace',
    defaultMessage:
      'Off by default. While on, providers that run their own tools (Claude Code CLI, Codex CLI, …) are blocked because Gosling can’t scope them to these directories, and other actions outside the directories above need your approval. The workspace folder policy is enforced either way.',
  },
  failedToUpdateRestriction: {
    id: 'workingDirectoriesMenu.failedToUpdateRestriction',
    defaultMessage: 'Failed to update working directory restriction',
  },
  workspacePinnedHint: {
    id: 'workingDirectoriesMenu.workspacePinnedHint',
    defaultMessage:
      'This session’s folders are pinned by its workspace. Edit the workspace and start a new session to change them.',
  },
  workspaceSessionOnlyHint: {
    id: 'workingDirectoriesMenu.workspaceSessionOnlyHint',
    defaultMessage:
      'Workspace folders remain pinned. Directories added here belong only to this session; other sessions and the workspace do not receive access.',
  },
});

interface WorkingDirectoriesMenuProps {
  session?: Session;
  onSessionChange: (updater: (session: Session) => Session) => void;
  className?: string;
  compact?: boolean;
  showCount?: boolean;
}

export default function WorkingDirectoriesMenu({
  session,
  onSessionChange,
  className,
  compact = false,
  showCount = true,
}: WorkingDirectoriesMenuProps) {
  const intl = useIntl();
  const [isMenuOpen, setIsMenuOpen] = useState(false);
  const [isAdding, setIsAdding] = useState(false);
  const [recentDirs, setRecentDirs] = useState<string[]>([]);
  const refreshVersionRef = useRef(0);

  const workingDir = session?.working_dir;
  const additionalWorkingDirs = useMemo(
    () => session?.additional_working_dirs ?? [],
    [session?.additional_working_dirs]
  );
  const isWorkspacePinned = Boolean(session?.workspace_id);

  const refreshRecentDirs = useCallback(async () => {
    const version = ++refreshVersionRef.current;
    const recent = await window.electron.listRecentDirs().catch(() => []);
    if (version !== refreshVersionRef.current) return;
    setRecentDirs(recent);
  }, []);

  useEffect(() => {
    if (!isMenuOpen) return;
    void refreshRecentDirs();
  }, [isMenuOpen, refreshRecentDirs]);

  const addDirectory = useCallback(
    async (dir: string) => {
      if (!session) return;
      if (dir === workingDir || additionalWorkingDirs.includes(dir)) {
        toast.info(intl.formatMessage(i18n.alreadyAdded));
        return;
      }

      setIsAdding(true);
      try {
        const result = await acpAddSessionWorkingDir(session.id, dir);
        window.electron.addRecentDir(dir);
        onSessionChange((current) => ({
          ...current,
          additional_working_dirs: result.additionalWorkingDirs,
        }));
      } catch (error) {
        console.error('[WorkingDirectoriesMenu] Failed to add working directory:', error);
        toast.error(intl.formatMessage(i18n.failedToAdd));
      } finally {
        setIsAdding(false);
      }
    },
    [session, workingDir, additionalWorkingDirs, onSessionChange, intl]
  );

  const removeDirectory = useCallback(
    async (dir: string) => {
      if (!session || isWorkspacePinned) return;

      try {
        const result = await acpRemoveSessionWorkingDir(session.id, dir);
        onSessionChange((current) => ({
          ...current,
          additional_working_dirs: result.additionalWorkingDirs,
        }));
      } catch (error) {
        console.error('[WorkingDirectoriesMenu] Failed to remove working directory:', error);
        toast.error(intl.formatMessage(i18n.failedToRemove));
      }
    },
    [session, onSessionChange, intl, isWorkspacePinned]
  );

  const handleChooseDirectory = useCallback(async () => {
    const result = await window.electron.sessionDirectoryChooser();
    if (result.canceled || result.filePaths.length === 0) return;
    await addDirectory(result.filePaths[0]);
  }, [addDirectory]);

  const toggleRestriction = useCallback(
    async (restrict: boolean) => {
      if (!session) return;
      const previous = session.restrict_tools_to_working_dirs ?? false;
      onSessionChange((current) => ({ ...current, restrict_tools_to_working_dirs: restrict }));
      try {
        await acpSetWorkingDirRestriction(session.id, restrict);
      } catch (error) {
        console.error('[WorkingDirectoriesMenu] Failed to update restriction:', error);
        toast.error(intl.formatMessage(i18n.failedToUpdateRestriction));
        onSessionChange((current) => ({
          ...current,
          restrict_tools_to_working_dirs: previous,
        }));
      }
    },
    [session, onSessionChange, intl]
  );

  if (!session || !workingDir) {
    return null;
  }

  const accessLabel = (path: string) => {
    const normalize = (value: string) => {
      const normalized = value.replace(/\\/g, '/').replace(/\/+$/, '');
      return /^[a-z]:\//i.test(normalized) ? normalized.toLowerCase() : normalized;
    };
    const roots = session.workspace_folder_roots ?? [];
    const normalized = normalize(path);
    const readOnly = roots.some(
      (root) =>
        root.access === 'read' &&
        (normalized === normalize(root.path) || normalized.startsWith(`${normalize(root.path)}/`))
    );
    if (readOnly) return 'Read-only';
    return roots.some((root) => normalize(root.path) === normalized)
      ? 'Read/write/run'
      : isWorkspacePinned
        ? 'Workspace policy'
        : 'Read/write/run';
  };

  const filteredRecentDirs = recentDirs.filter(
    (dir) => dir && dir !== workingDir && !additionalWorkingDirs.includes(dir)
  );

  const triggerLabel =
    additionalWorkingDirs.length === 0
      ? intl.formatMessage(i18n.addDirectoryShort)
      : intl.formatMessage(i18n.directoriesShort);

  return (
    <TooltipProvider>
      <Tooltip>
        <DropdownMenu open={isMenuOpen} onOpenChange={setIsMenuOpen}>
          <TooltipTrigger asChild>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                className={`no-drag flex items-center gap-1 text-xs transition-colors ${className ?? 'text-text-secondary hover:text-text-primary'}`}
              >
                <FolderPlus size={14} />
                {!compact && <span>{triggerLabel}</span>}
                {showCount && (
                  <span className="tabular-nums text-[11px] text-text-secondary">
                    {additionalWorkingDirs.length + 1}
                  </span>
                )}
              </button>
            </DropdownMenuTrigger>
          </TooltipTrigger>
          <DropdownMenuContent className="z-[300] w-80" side="bottom" align="end">
            <DropdownMenuLabel>{intl.formatMessage(i18n.workingDirectories)}</DropdownMenuLabel>
            <DropdownMenuItem disabled>
              <Folder className="mr-2 h-4 w-4" />
              <span className="truncate flex-1">{workingDir}</span>
              <span className="ml-2 text-[10px] uppercase text-text-secondary">
                {intl.formatMessage(i18n.primary)} · {accessLabel(workingDir)}
              </span>
            </DropdownMenuItem>

            {additionalWorkingDirs.map((dir) => (
              <DropdownMenuItem key={dir} onSelect={(event) => event.preventDefault()}>
                <Folder className="mr-2 h-4 w-4" />
                <span className="truncate flex-1" title={dir}>
                  {dir}
                </span>
                <span className="text-[10px] text-text-secondary">{accessLabel(dir)}</span>
                {!isWorkspacePinned && (
                  <button
                    type="button"
                    aria-label={intl.formatMessage(i18n.removeDirectory)}
                    className="ml-2 rounded p-0.5 hover:bg-background-secondary"
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      void removeDirectory(dir);
                    }}
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                )}
              </DropdownMenuItem>
            ))}

            {filteredRecentDirs.length > 0 && (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuLabel>{intl.formatMessage(i18n.recentDirectories)}</DropdownMenuLabel>
                {filteredRecentDirs.slice(0, 5).map((dir) => (
                  <DropdownMenuItem key={dir} onSelect={() => void addDirectory(dir)}>
                    <Folder className="mr-2 h-4 w-4" />
                    <span className="truncate">{dir}</span>
                  </DropdownMenuItem>
                ))}
              </>
            )}

            <DropdownMenuSeparator />
            {isWorkspacePinned && (
              <div className="px-2 py-1.5 flex items-start gap-2">
                <ShieldAlert className="h-3.5 w-3.5 shrink-0 mt-0.5 text-text-secondary" />
                <p className="text-[11px] leading-snug text-text-secondary">
                  {intl.formatMessage(i18n.workspaceSessionOnlyHint)}
                </p>
              </div>
            )}
            <DropdownMenuItem disabled={isAdding} onSelect={() => void handleChooseDirectory()}>
              <Plus className="mr-2 h-4 w-4" />
              <span>{intl.formatMessage(i18n.addDirectory)}</span>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <div className="px-2 py-1.5 text-[11px] leading-snug text-text-secondary flex gap-1.5">
              <FolderPlus className="h-3.5 w-3.5 shrink-0 mt-0.5" />
              <span>
                {intl.formatMessage(
                  isWorkspacePinned ? i18n.sessionAddedFullAccessHint : i18n.fullAccessHint
                )}
              </span>
            </div>
            <DropdownMenuSeparator />
            <div
              className="px-2 py-1.5 flex items-start gap-2"
              onClick={(event) => event.stopPropagation()}
            >
              <ShieldAlert className="h-3.5 w-3.5 shrink-0 mt-0.5 text-text-secondary" />
              <div className="flex-1">
                <div className="flex items-center justify-between gap-2">
                  <span className="text-xs">{intl.formatMessage(i18n.restrictToggleLabel)}</span>
                  <Switch
                    checked={session.restrict_tools_to_working_dirs ?? false}
                    onCheckedChange={(checked) => void toggleRestriction(checked)}
                  />
                </div>
                <p className="text-[11px] leading-snug text-text-secondary mt-0.5">
                  {intl.formatMessage(
                    session.workspace_id
                      ? i18n.restrictToggleDescriptionWorkspace
                      : i18n.restrictToggleDescription
                  )}
                </p>
              </div>
            </div>
          </DropdownMenuContent>
        </DropdownMenu>
        <TooltipContent side="bottom">{intl.formatMessage(i18n.workingDirectories)}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
