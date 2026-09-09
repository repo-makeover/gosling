import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { ChevronDown, ChevronRight, MoreHorizontal } from 'lucide-react';
import { motion } from 'framer-motion';
import { useNavigationContext } from './NavigationContext';
import { useNavigationSessions } from '../../hooks/useNavigationSessions';
import {
  NAV_ITEMS,
  SETTINGS_NAV_ITEM,
  getNavItemLabel,
  type NavItem,
} from '../../hooks/useNavigationItems';
import { AppEvents } from '../../constants/events';
import { InlineEditText } from '../common/InlineEditText';
import { SessionIndicators } from '../SessionIndicators';
import { acpDeleteSession, acpRenameSession, type SessionListItem } from '../../acp/sessions';
import { cn } from '../../utils';
import { defineMessages, useIntl } from '../../i18n';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import {
  ArchiveFolderNotConfiguredError,
  archiveSessionToConfiguredFolder,
  getArchiveFolder,
  getTrackedArchiveFile,
  removeTrackedArchiveFile,
} from '../../sessionArchive';
import { acpChatSessionActions } from '../../acp/chatSessionStore';
import { cancelAcpElicitationRequestsForSession } from '../../acp/elicitationRequests';
import { cancelAcpPermissionRequestsForSession } from '../../acp/permissionRequests';
import { errorMessage } from '../../utils/conversionUtils';
import { toast } from 'react-toastify';
import { WorkspaceSidebarSection } from '../workspaces/WorkspaceSidebarSection';

type StreamState = 'idle' | 'loading' | 'streaming' | 'error';

interface SessionStatus {
  workspaceId: string | null;
  streamState: StreamState;
  hasUnreadActivity: boolean;
}

const WORKSPACES_HEIGHT_KEY = 'workspaces_sidebar_height';
// Enough of each pane to stay usable and to keep its own header visible, so the
// divider can never be dragged far enough to hide a section entirely.
const MIN_WORKSPACES_HEIGHT = 72;
const MIN_CHATS_HEIGHT = 120;
const WORKSPACES_RESIZE_STEP = 24;

const i18n = defineMessages({
  chats: {
    id: 'navigationPanel.chats',
    defaultMessage: 'Chats',
  },
  resizeWorkspaces: {
    id: 'navigationPanel.resizeWorkspaces',
    defaultMessage: 'Resize workspaces list — drag, or use the arrow keys',
  },
  noChats: {
    id: 'navigationPanel.noChats',
    defaultMessage: 'No recent chats',
  },
  untitledSession: {
    id: 'navigationPanel.untitledSession',
    defaultMessage: 'Untitled session',
  },
  renameSession: {
    id: 'navigationPanel.renameSession',
    defaultMessage: 'Rename session',
  },
  archiveSession: {
    id: 'navigationPanel.archiveSession',
    defaultMessage: 'Archive session',
  },
  deleteSession: {
    id: 'navigationPanel.deleteSession',
    defaultMessage: 'Delete session',
  },
  moreSessionActions: {
    id: 'navigationPanel.moreSessionActions',
    defaultMessage: 'Session actions',
  },
  cancel: {
    id: 'navigationPanel.cancel',
    defaultMessage: 'Cancel',
  },
  archiveTitle: {
    id: 'navigationPanel.archiveTitle',
    defaultMessage: 'Archive Session',
  },
  archiveMessage: {
    id: 'navigationPanel.archiveMessage',
    defaultMessage:
      'Archive "{name}" to the configured folder and hide it from active session history?',
  },
  archiveDetail: {
    id: 'navigationPanel.archiveDetail',
    defaultMessage: 'Archive folder: {folder}. You can restore this session from Archived.',
  },
  deleteTitle: {
    id: 'navigationPanel.deleteTitle',
    defaultMessage: 'Delete Session',
  },
  deleteMessage: {
    id: 'navigationPanel.deleteMessage',
    defaultMessage:
      'Are you sure you want to permanently delete the session "{name}"? This action cannot be undone.',
  },
  deleteWithArchiveFileMessage: {
    id: 'navigationPanel.deleteWithArchiveFileMessage',
    defaultMessage:
      'Delete the session "{name}" permanently? Its tracked archive file will also be removed from disk.',
  },
  deleteTrackedFileDetail: {
    id: 'navigationPanel.deleteTrackedFileDetail',
    defaultMessage: 'Tracked archive file: {filePath}',
  },
  archiveFolderMissing: {
    id: 'navigationPanel.archiveFolderMissing',
    defaultMessage: 'Configure an archive folder in App Settings before archiving sessions.',
  },
  archiveSuccess: {
    id: 'navigationPanel.archiveSuccess',
    defaultMessage: 'Session archived successfully',
  },
  archiveFailed: {
    id: 'navigationPanel.archiveFailed',
    defaultMessage: 'Failed to archive session "{name}": {error}',
  },
  deleteSuccess: {
    id: 'navigationPanel.deleteSuccess',
    defaultMessage: 'Session deleted successfully',
  },
  deleteFailed: {
    id: 'navigationPanel.deleteFailed',
    defaultMessage: 'Failed to delete session "{name}": {error}',
  },
  deleteArchiveFileFailed: {
    id: 'navigationPanel.deleteArchiveFileFailed',
    defaultMessage: 'Deleted session, but failed to remove archive file on disk.',
  },
});

const navItemClass = (active: boolean) =>
  cn(
    'flex flex-row items-center gap-3 outline-none no-drag w-full',
    'rounded-full px-3 py-2 text-sm font-medium transition-colors',
    active
      ? 'bg-background-tertiary text-text-primary'
      : 'text-text-primary hover:bg-background-tertiary/60'
  );

interface NavRowProps {
  item: NavItem;
  active: boolean;
  onClick: () => void;
}

const NavRow: React.FC<NavRowProps> = ({ item, active, onClick }) => {
  const intl = useIntl();
  const Icon = item.icon;
  return (
    <button onClick={onClick} className={navItemClass(active)}>
      <Icon className="w-5 h-5 flex-shrink-0 text-text-secondary" />
      <span className="text-left flex-1 truncate">{getNavItemLabel(item, intl)}</span>
      {item.getTag && (
        <span className="text-xs font-mono text-text-secondary">{item.getTag()}</span>
      )}
    </button>
  );
};

interface SessionRowProps {
  session: SessionListItem;
  active: boolean;
  status: SessionStatus | undefined;
  onClick: () => void;
  onRenameRequested: (sessionId: string) => void;
  onArchiveRequested: (session: SessionListItem) => void;
  onDeleteRequested: (session: SessionListItem) => void;
  renameToken?: number;
}

const SessionRow: React.FC<SessionRowProps> = ({
  session,
  active,
  status,
  onClick,
  onRenameRequested,
  onArchiveRequested,
  onDeleteRequested,
  renameToken,
}) => {
  const intl = useIntl();
  const [isEditing, setIsEditing] = useState(false);
  const isStreaming = status?.streamState === 'streaming';
  const hasError = status?.streamState === 'error';
  const hasUnread = status?.hasUnreadActivity ?? false;

  return (
    <div
      onClick={() => !isEditing && onClick()}
      className={cn(
        'group flex items-center gap-2 px-3 py-1.5 rounded-full cursor-pointer text-sm',
        'hover:bg-background-tertiary/60 transition-colors',
        active && 'bg-background-tertiary'
      )}
    >
      <InlineEditText
        value={session.name}
        onSave={async (newName) => {
          await acpRenameSession(session.id, newName);
          window.dispatchEvent(
            new CustomEvent(AppEvents.SESSION_RENAMED, {
              detail: { sessionId: session.id, newName, userInitiated: true },
            })
          );
        }}
        placeholder={intl.formatMessage(i18n.untitledSession)}
        disabled={isStreaming}
        singleClickEdit={false}
        className="truncate text-text-primary flex-1 !px-0 !py-0 hover:bg-transparent"
        editClassName="!text-sm"
        editToken={renameToken}
        onEditStart={() => setIsEditing(true)}
        onEditEnd={() => setIsEditing(false)}
      />
      <div className="flex items-center gap-1">
        <SessionIndicators isStreaming={isStreaming} hasUnread={hasUnread} hasError={hasError} />
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              className="rounded-full p-1 text-text-secondary opacity-0 transition-opacity hover:bg-background-secondary hover:text-text-primary focus-visible:opacity-100 focus-visible:outline-none group-hover:opacity-100"
              onClick={(event) => event.stopPropagation()}
              aria-label={intl.formatMessage(i18n.moreSessionActions)}
            >
              <MoreHorizontal className="h-4 w-4" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-44">
            <DropdownMenuItem onSelect={() => onRenameRequested(session.id)}>
              {intl.formatMessage(i18n.renameSession)}
            </DropdownMenuItem>
            <DropdownMenuItem disabled={isStreaming} onSelect={() => onArchiveRequested(session)}>
              {intl.formatMessage(i18n.archiveSession)}
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={isStreaming}
              onSelect={() => onDeleteRequested(session)}
              className="text-red-600 focus:text-red-600"
            >
              {intl.formatMessage(i18n.deleteSession)}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
};

export const Navigation: React.FC<{ className?: string }> = ({ className }) => {
  const intl = useIntl();
  const { isNavExpanded } = useNavigationContext();
  const location = useLocation();
  const navigate = useNavigate();

  const isActive = useCallback((path: string) => location.pathname === path, [location.pathname]);

  const { recentSessions, activeSessionId, fetchSessions, handleNavClick, handleSessionClick } =
    useNavigationSessions();

  const [sessionStatuses, setSessionStatuses] = useState<Map<string, SessionStatus>>(new Map());
  const unreadWorkspaceIds = useMemo(
    () =>
      new Set(
        [...sessionStatuses.values()]
          .filter(
            (status) =>
              status.hasUnreadActivity &&
              status.streamState !== 'streaming' &&
              status.streamState !== 'error'
          )
          .map((status) => status.workspaceId)
          .filter((workspaceId): workspaceId is string => workspaceId !== null)
      ),
    [sessionStatuses]
  );
  const [renameRequest, setRenameRequest] = useState<{ sessionId: string; token: number } | null>(
    null
  );
  const [archiveDialog, setArchiveDialog] = useState<{
    folder: string;
    session: SessionListItem;
  } | null>(null);
  const [deleteDialog, setDeleteDialog] = useState<{
    filePath?: string;
    hasTrackedArchiveFile: boolean;
    session: SessionListItem;
  } | null>(null);

  useEffect(() => {
    const handleStatusUpdate = (event: Event) => {
      const { sessionId, workspaceId, streamState } = (
        event as CustomEvent<{
          sessionId: string;
          workspaceId: string | null;
          streamState: StreamState;
        }>
      ).detail;
      setSessionStatuses((prev) => {
        const existing = prev.get(sessionId);
        const shouldMarkUnread = existing?.streamState === 'streaming' && streamState === 'idle';
        const next = new Map(prev);
        next.set(sessionId, {
          workspaceId,
          streamState,
          hasUnreadActivity: existing?.hasUnreadActivity || shouldMarkUnread,
        });
        return next;
      });
    };

    const handleSessionRemoved = (event: Event) => {
      const { sessionId } = (event as CustomEvent<{ sessionId: string }>).detail;
      setSessionStatuses((prev) => {
        const next = new Map(prev);
        next.delete(sessionId);
        return next;
      });
    };

    window.addEventListener(AppEvents.SESSION_STATUS_UPDATE, handleStatusUpdate);
    window.addEventListener(AppEvents.SESSION_ARCHIVED, handleSessionRemoved);
    window.addEventListener(AppEvents.SESSION_DELETED, handleSessionRemoved);
    return () => {
      window.removeEventListener(AppEvents.SESSION_STATUS_UPDATE, handleStatusUpdate);
      window.removeEventListener(AppEvents.SESSION_ARCHIVED, handleSessionRemoved);
      window.removeEventListener(AppEvents.SESSION_DELETED, handleSessionRemoved);
    };
  }, []);

  const clearUnread = useCallback((sessionId: string) => {
    setSessionStatuses((prev) => {
      const status = prev.get(sessionId);
      if (status?.hasUnreadActivity) {
        const next = new Map(prev);
        next.set(sessionId, { ...status, hasUnreadActivity: false });
        return next;
      }
      return prev;
    });
  }, []);

  const navFocusRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (isNavExpanded) {
      fetchSessions();
      requestAnimationFrame(() => navFocusRef.current?.focus());
    }
  }, [isNavExpanded, fetchSessions]);

  const [isChatsExpanded, setIsChatsExpanded] = useState(true);

  const workspacesPaneRef = useRef<HTMLDivElement>(null);
  // `null` keeps the original behaviour — grow with the content up to 45% of the
  // panel. A stored number means the user has dragged the divider, and their
  // split is honoured even when the list is shorter than it.
  const [workspacesHeight, setWorkspacesHeight] = useState<number | null>(() => {
    const stored = Number(window.localStorage.getItem(WORKSPACES_HEIGHT_KEY));
    return Number.isFinite(stored) && stored > 0 ? stored : null;
  });

  const clampWorkspacesHeight = useCallback((height: number) => {
    const paneTop = workspacesPaneRef.current?.getBoundingClientRect().top;
    const panelBottom = navFocusRef.current?.getBoundingClientRect().bottom;
    if (paneTop === undefined || panelBottom === undefined) {
      return Math.max(height, MIN_WORKSPACES_HEIGHT);
    }
    const maxHeight = Math.max(MIN_WORKSPACES_HEIGHT, panelBottom - paneTop - MIN_CHATS_HEIGHT);
    return Math.min(Math.max(height, MIN_WORKSPACES_HEIGHT), maxHeight);
  }, []);

  const commitWorkspacesHeight = useCallback((height: number) => {
    window.localStorage.setItem(WORKSPACES_HEIGHT_KEY, String(height));
  }, []);

  // A window that shrinks can leave a stored split taller than the panel, which
  // would squeeze the chats list out of view.
  useEffect(() => {
    const reclamp = () =>
      setWorkspacesHeight((current) => (current === null ? null : clampWorkspacesHeight(current)));
    window.addEventListener('resize', reclamp);
    return () => window.removeEventListener('resize', reclamp);
  }, [clampWorkspacesHeight]);

  const resizeWorkspacesFrom = useCallback(
    (event: React.PointerEvent) => {
      event.preventDefault();
      const startY = event.clientY;
      // Measured rather than read from state, so the first drag out of the
      // auto-sized default starts exactly where the divider is drawn.
      const startHeight =
        workspacesPaneRef.current?.getBoundingClientRect().height ?? MIN_WORKSPACES_HEIGHT;
      let latest = startHeight;
      const move = (moveEvent: globalThis.PointerEvent) => {
        latest = clampWorkspacesHeight(startHeight + moveEvent.clientY - startY);
        setWorkspacesHeight(latest);
      };
      const stop = () => {
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', stop);
        commitWorkspacesHeight(latest);
      };
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', stop);
    },
    [clampWorkspacesHeight, commitWorkspacesHeight]
  );

  const nudgeWorkspacesHeight = useCallback(
    (delta: number) => {
      const current =
        workspacesPaneRef.current?.getBoundingClientRect().height ?? MIN_WORKSPACES_HEIGHT;
      const next = clampWorkspacesHeight(current + delta);
      setWorkspacesHeight(next);
      commitWorkspacesHeight(next);
    },
    [clampWorkspacesHeight, commitWorkspacesHeight]
  );

  const handleDividerKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (event.key === 'ArrowUp') {
        event.preventDefault();
        nudgeWorkspacesHeight(-WORKSPACES_RESIZE_STEP);
      } else if (event.key === 'ArrowDown') {
        event.preventDefault();
        nudgeWorkspacesHeight(WORKSPACES_RESIZE_STEP);
      }
    },
    [nudgeWorkspacesHeight]
  );

  const resetWorkspacesHeight = useCallback(() => {
    window.localStorage.removeItem(WORKSPACES_HEIGHT_KEY);
    setWorkspacesHeight(null);
  }, []);

  const navigateAwayFromCurrentSession = useCallback(
    (sessionId: string, tab: 'active' | 'archived') => {
      if (location.pathname === '/pair' && activeSessionId === sessionId) {
        navigate(tab === 'archived' ? '/sessions?tab=archived' : '/sessions');
      }
    },
    [activeSessionId, location.pathname, navigate]
  );

  const handleArchiveRequested = useCallback(
    async (session: SessionListItem) => {
      const folder = await getArchiveFolder();
      if (!folder) {
        toast.error(intl.formatMessage(i18n.archiveFolderMissing));
        navigate('/settings?section=app');
        return;
      }

      setArchiveDialog({ session, folder });
    },
    [intl, navigate]
  );

  const handleDeleteRequested = useCallback(async (session: SessionListItem) => {
    const filePath = await getTrackedArchiveFile(session.id);
    setDeleteDialog({
      session,
      filePath,
      hasTrackedArchiveFile: filePath != null,
    });
  }, []);

  const handleConfirmArchive = useCallback(async () => {
    if (!archiveDialog) {
      return;
    }

    const { session } = archiveDialog;
    setArchiveDialog(null);

    try {
      await archiveSessionToConfiguredFolder(session.id, session.name);
      cancelAcpPermissionRequestsForSession(session.id);
      cancelAcpElicitationRequestsForSession(session.id);
      acpChatSessionActions.deleteSnapshot(session.id);
      window.dispatchEvent(
        new CustomEvent(AppEvents.SESSION_ARCHIVED, { detail: { sessionId: session.id } })
      );
      navigateAwayFromCurrentSession(session.id, 'archived');
      toast.success(intl.formatMessage(i18n.archiveSuccess));
      await fetchSessions();
    } catch (error) {
      if (error instanceof ArchiveFolderNotConfiguredError) {
        toast.error(intl.formatMessage(i18n.archiveFolderMissing));
        navigate('/settings?section=app');
        return;
      }

      toast.error(
        intl.formatMessage(i18n.archiveFailed, {
          name: session.name,
          error: errorMessage(error, 'Unknown error'),
        })
      );
    }
  }, [archiveDialog, fetchSessions, intl, navigate, navigateAwayFromCurrentSession]);

  const handleConfirmDelete = useCallback(async () => {
    if (!deleteDialog) {
      return;
    }

    const { session, hasTrackedArchiveFile } = deleteDialog;
    setDeleteDialog(null);

    try {
      await acpDeleteSession(session.id);
      const archiveFileRemoval = await removeTrackedArchiveFile(session.id);
      cancelAcpPermissionRequestsForSession(session.id);
      cancelAcpElicitationRequestsForSession(session.id);
      acpChatSessionActions.deleteSnapshot(session.id);
      window.dispatchEvent(
        new CustomEvent(AppEvents.SESSION_DELETED, { detail: { sessionId: session.id } })
      );
      navigateAwayFromCurrentSession(session.id, 'active');
      toast.success(intl.formatMessage(i18n.deleteSuccess));
      if (hasTrackedArchiveFile && !archiveFileRemoval.removed) {
        toast.error(intl.formatMessage(i18n.deleteArchiveFileFailed));
      }
      await fetchSessions();
    } catch (error) {
      toast.error(
        intl.formatMessage(i18n.deleteFailed, {
          name: session.name,
          error: errorMessage(error, 'Unknown error'),
        })
      );
    }
  }, [deleteDialog, fetchSessions, intl, navigateAwayFromCurrentSession]);

  if (!isNavExpanded) return null;

  return (
    <motion.div
      ref={navFocusRef}
      tabIndex={-1}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
      className={cn('bg-background-primary outline-none flex flex-col h-full', className)}
    >
      <div className="h-[48px] no-drag" />

      {/* Nav items */}
      <div className="px-2 flex flex-col gap-0.5">
        {NAV_ITEMS.map((item) => (
          <NavRow
            key={item.id}
            item={item}
            active={isActive(item.path)}
            onClick={() => handleNavClick(item.path)}
          />
        ))}
      </div>

      <div
        ref={workspacesPaneRef}
        className={cn('mt-3 shrink-0 overflow-y-auto', workspacesHeight === null && 'max-h-[45%]')}
        style={workspacesHeight === null ? undefined : { height: workspacesHeight }}
      >
        <WorkspaceSidebarSection
          unreadWorkspaceIds={unreadWorkspaceIds}
          onNewChat={(workspaceId) => navigate('/', { state: { initialWorkspaceId: workspaceId } })}
        />
      </div>

      {/* Divider — drag to re-split workspaces against chats, double-click to
          go back to sizing with the content */}
      <div
        role="separator"
        aria-orientation="horizontal"
        aria-label={intl.formatMessage(i18n.resizeWorkspaces)}
        tabIndex={0}
        onPointerDown={resizeWorkspacesFrom}
        onDoubleClick={resetWorkspacesHeight}
        onKeyDown={handleDividerKeyDown}
        className="group relative h-2 shrink-0 cursor-row-resize focus-visible:outline-none"
      >
        <span className="pointer-events-none absolute inset-x-0 top-1/2 h-px -translate-y-1/2 bg-border-secondary transition-colors group-hover:bg-border-active group-focus-visible:bg-border-active" />
      </div>

      {/* Chats section — takes remaining vertical space */}
      <div className="flex-1 min-h-0 flex flex-col mt-2">
        <button
          onClick={() => setIsChatsExpanded((v) => !v)}
          className="flex items-center gap-1 px-4 py-1 text-xs font-semibold uppercase tracking-wider text-text-secondary hover:text-text-primary transition-colors self-start"
        >
          {isChatsExpanded ? (
            <ChevronDown className="w-3 h-3" />
          ) : (
            <ChevronRight className="w-3 h-3" />
          )}
          <span>{intl.formatMessage(i18n.chats)}</span>
        </button>
        {isChatsExpanded && (
          <div className="flex-1 min-h-0 overflow-y-auto px-2 pb-2 mt-1">
            {recentSessions.length === 0 ? (
              <div className="px-3 py-2 text-xs text-text-secondary">
                {intl.formatMessage(i18n.noChats)}
              </div>
            ) : (
              recentSessions.map((session) => (
                <SessionRow
                  key={session.id}
                  session={session}
                  active={session.id === activeSessionId}
                  status={sessionStatuses.get(session.id)}
                  onClick={() => {
                    clearUnread(session.id);
                    handleSessionClick(session.id);
                  }}
                  onRenameRequested={(sessionId) =>
                    setRenameRequest((prev) => ({
                      sessionId,
                      token: prev?.token && prev.sessionId === sessionId ? prev.token + 1 : 1,
                    }))
                  }
                  onArchiveRequested={(session) => void handleArchiveRequested(session)}
                  onDeleteRequested={(session) => void handleDeleteRequested(session)}
                  renameToken={
                    renameRequest?.sessionId === session.id ? renameRequest.token : undefined
                  }
                />
              ))
            )}
          </div>
        )}
      </div>

      {/* Settings pinned to bottom */}
      <div className="px-2 pt-2 pb-2 border-t border-border-secondary">
        <NavRow
          item={SETTINGS_NAV_ITEM}
          active={isActive(SETTINGS_NAV_ITEM.path)}
          onClick={() => handleNavClick(SETTINGS_NAV_ITEM.path)}
        />
      </div>

      <ConfirmationModal
        isOpen={archiveDialog != null}
        title={intl.formatMessage(i18n.archiveTitle)}
        message={intl.formatMessage(i18n.archiveMessage, {
          name: archiveDialog?.session.name ?? '',
        })}
        detail={
          archiveDialog ? (
            <div>{intl.formatMessage(i18n.archiveDetail, { folder: archiveDialog.folder })}</div>
          ) : undefined
        }
        confirmLabel={intl.formatMessage(i18n.archiveTitle)}
        cancelLabel={intl.formatMessage(i18n.cancel)}
        onConfirm={() => void handleConfirmArchive()}
        onCancel={() => setArchiveDialog(null)}
      />

      <ConfirmationModal
        isOpen={deleteDialog != null}
        title={intl.formatMessage(i18n.deleteTitle)}
        message={
          deleteDialog?.hasTrackedArchiveFile
            ? intl.formatMessage(i18n.deleteWithArchiveFileMessage, {
                name: deleteDialog.session.name,
              })
            : intl.formatMessage(i18n.deleteMessage, {
                name: deleteDialog?.session.name ?? '',
              })
        }
        detail={
          deleteDialog?.hasTrackedArchiveFile && deleteDialog.filePath ? (
            <div>
              {intl.formatMessage(i18n.deleteTrackedFileDetail, {
                filePath: deleteDialog.filePath,
              })}
            </div>
          ) : undefined
        }
        confirmLabel={intl.formatMessage(i18n.deleteTitle)}
        cancelLabel={intl.formatMessage(i18n.cancel)}
        confirmVariant="destructive"
        onConfirm={() => void handleConfirmDelete()}
        onCancel={() => setDeleteDialog(null)}
      />
    </motion.div>
  );
};
