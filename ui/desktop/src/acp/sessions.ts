import type {
  ForkSessionRequest,
  ListSessionsRequest,
  LoadSessionResponse,
  NewSessionRequest,
  SessionInfo,
} from '@agentclientprotocol/sdk';
import type {
  GoslingExtension,
  SessionArtifactDto,
  SessionImportSource,
} from '@repo-makeover/gosling-sdk';
import { getAcpClient } from './acpConnection';
import { DEFAULT_CHAT_TITLE } from '../contexts/ChatContext';
import type { ExtensionLoadResult } from '../types/extensions';
import type { Message } from '../types/message';
import type { GoslingMode, Session } from '../types/session';

interface GoslingSessionInfoMeta {
  messageCount?: number;
  createdAt?: string;
  lastMessageAt?: string;
  archivedAt?: string;
  projectId?: string;
  providerId?: string;
  modelId?: string;
  sessionType?: Session['session_type'];
  userSetName?: boolean;
  lastMessageSnippet?: string;
  additionalWorkingDirs?: string[];
  restrictToolsToWorkingDirs?: boolean;
  goslingMode?: GoslingMode;
  workspaceId?: string;
  workspaceName?: string;
  workspaceFolderRoots?: Session['workspace_folder_roots'];
  credentialProfileId?: string;
  credentialProfileName?: string;
  importedUntrusted?: boolean;
  importSource?: string;
  importOriginalWorkingDir?: string;
  researchLibraryPath?: string;
}

export const COMPACTED_SESSION_TAIL_LIMIT = 50;
const ARTIFACT_PAGE_LIMIT = 200;

/// Ceiling on artifacts accumulated in the renderer for one session. The loop
/// paginates but concatenated every page, so a session with a very large
/// artifact set was pulled into memory whole (MEM-GSL-009). The list is a UI
/// surface; beyond this many entries it is not usefully browsable anyway.
const ARTIFACT_TOTAL_LIMIT = 2000;

export async function acpListSessionArtifacts(sessionId: string): Promise<SessionArtifactDto[]> {
  const client = await getAcpClient();
  const artifacts: SessionArtifactDto[] = [];
  let cursor: string | null = null;

  do {
    const response = await client.gosling.sessionArtifactsList_unstable({
      sessionId,
      cursor,
      limit: ARTIFACT_PAGE_LIMIT,
    });
    artifacts.push(...response.artifacts);
    cursor = response.nextCursor ?? null;
    if (artifacts.length >= ARTIFACT_TOTAL_LIMIT) {
      console.warn(
        `Session ${sessionId} has more than ${ARTIFACT_TOTAL_LIMIT} artifacts; listing was truncated.`
      );
      return artifacts.slice(0, ARTIFACT_TOTAL_LIMIT);
    }
  } while (cursor);

  return artifacts;
}

export interface HistoryLoadMeta {
  mode?: 'compacted' | 'full' | string;
  tailLimit?: number;
  totalCount?: number;
  loadedCount?: number;
  oldestRowId?: number | null;
  newestRowId?: number | null;
  nextBeforeCursor?: string | null;
}

export interface SessionListItem {
  id: string;
  name: string;
  workingDir: string;
  updatedAt: string;
  messageCount: number;
  lastMessageAt?: string;
  lastMessageSnippet?: string;
  createdAt: string;
  archivedAt?: string;
  projectId?: string;
  providerId?: string;
  modelId?: string;
  userSetName?: boolean;
  workspaceId?: string;
  workspaceName?: string;
  importedUntrusted?: boolean;
  importSource?: string;
}

export interface SessionListPage {
  sessions: SessionListItem[];
  nextCursor: string | null;
}

export interface LoadSessionMeta {
  extensionResults?: ExtensionLoadResult[] | null;
  workingDir?: string;
  historyLoad?: HistoryLoadMeta;
  workspaceId?: string;
  workspaceName?: string;
  importedUntrusted?: boolean;
  importSource?: string;
}

export interface AcpLoadSessionResult {
  sessionInfo: SessionInfo;
  response: LoadSessionResponse;
  meta: LoadSessionMeta;
}

const inFlightSessionLoads = new Map<string, Promise<AcpLoadSessionResult>>();

function parseSessionResponseMeta(rawMeta: unknown): LoadSessionMeta {
  const meta = (rawMeta ?? {}) as LoadSessionMeta;
  return {
    extensionResults: meta.extensionResults,
    workingDir: typeof meta.workingDir === 'string' ? meta.workingDir : undefined,
    historyLoad: isHistoryLoadMeta(meta.historyLoad) ? meta.historyLoad : undefined,
    workspaceId: typeof meta.workspaceId === 'string' ? meta.workspaceId : undefined,
    workspaceName: typeof meta.workspaceName === 'string' ? meta.workspaceName : undefined,
    importedUntrusted: meta.importedUntrusted === true,
    importSource: typeof meta.importSource === 'string' ? meta.importSource : undefined,
  };
}

function isHistoryLoadMeta(value: unknown): value is HistoryLoadMeta {
  return typeof value === 'object' && value !== null;
}

export function parseLoadMeta(response: LoadSessionResponse): LoadSessionMeta {
  return parseSessionResponseMeta(response._meta);
}

function sessionInfoMeta(s: SessionInfo): GoslingSessionInfoMeta {
  return (s._meta ?? {}) as GoslingSessionInfoMeta;
}

export function sessionInfoToSession(s: SessionInfo, loadMeta: LoadSessionMeta = {}): Session {
  const meta = sessionInfoMeta(s);
  const createdAt = meta.createdAt ?? s.updatedAt ?? '';
  const updatedAt = s.updatedAt ?? createdAt;
  const modelConfig: Session['model_config'] = meta.modelId
    ? {
        model_name: meta.modelId,
        toolshim: false,
      }
    : null;

  return {
    id: String(s.sessionId),
    name: s.title ?? DEFAULT_CHAT_TITLE,
    working_dir: loadMeta.workingDir ?? s.cwd,
    additional_working_dirs: meta.additionalWorkingDirs ?? [],
    restrict_tools_to_working_dirs: meta.restrictToolsToWorkingDirs ?? false,
    ...(meta.researchLibraryPath ? { research_library_path: meta.researchLibraryPath } : {}),
    created_at: createdAt,
    updated_at: updatedAt,
    last_message_at: meta.lastMessageAt,
    message_count: meta.messageCount ?? 0,
    extension_data: {},
    archived_at: meta.archivedAt,
    project_id: meta.projectId,
    provider_name: meta.providerId,
    model_config: modelConfig,
    session_type: meta.sessionType,
    user_set_name: meta.userSetName,
    last_message_snippet: meta.lastMessageSnippet,
    gosling_mode: meta.goslingMode,
    workspace_id: meta.workspaceId ?? loadMeta.workspaceId,
    workspace_name: meta.workspaceName ?? loadMeta.workspaceName,
    workspace_folder_roots: meta.workspaceFolderRoots,
    credential_profile_id: meta.credentialProfileId,
    credential_profile_name: meta.credentialProfileName,
    imported_untrusted: meta.importedUntrusted ?? loadMeta.importedUntrusted,
    import_source: meta.importSource ?? loadMeta.importSource,
    import_original_working_dir: meta.importOriginalWorkingDir,
  };
}

function sessionInfoToListItem(s: SessionInfo): SessionListItem {
  const meta = sessionInfoMeta(s);
  return {
    id: String(s.sessionId),
    name: s.title ?? DEFAULT_CHAT_TITLE,
    workingDir: s.cwd,
    updatedAt: s.updatedAt ?? '',
    messageCount: meta.messageCount ?? 0,
    lastMessageAt: meta.lastMessageAt,
    lastMessageSnippet: meta.lastMessageSnippet,
    createdAt: meta.createdAt ?? s.updatedAt ?? '',
    archivedAt: meta.archivedAt,
    projectId: meta.projectId,
    providerId: meta.providerId,
    modelId: meta.modelId,
    userSetName: meta.userSetName,
    workspaceId: meta.workspaceId,
    workspaceName: meta.workspaceName,
    importedUntrusted: meta.importedUntrusted,
    importSource: meta.importSource,
  };
}

export interface SessionListFilter {
  keyword?: string;
  archiveState?: SessionArchiveState;
  includeLastMessageSnippet?: boolean;
  workspaceId?: string;
  includeUnassigned?: boolean;
}

const SESSION_LIST_TYPES = ['user', 'scheduled'] as const;
export type SessionArchiveState = 'active' | 'archived' | 'all';

export async function acpListSessions(
  cursor?: string | null,
  filter?: SessionListFilter
): Promise<SessionListPage> {
  const client = await getAcpClient();
  const request: ListSessionsRequest = {};
  if (cursor) {
    request.cursor = cursor;
  }
  const meta: Record<string, unknown> = { types: SESSION_LIST_TYPES };
  const keyword = filter?.keyword?.trim();
  if (keyword) {
    meta.query = keyword;
  }
  if (filter?.workspaceId) {
    meta.workspaceId = filter.workspaceId;
    meta.includeUnassigned = filter.includeUnassigned ?? false;
  }
  meta.gosling = {
    archiveState: filter?.archiveState ?? 'active',
    includeLastMessageSnippet: filter?.includeLastMessageSnippet ?? false,
  };
  request._meta = meta;
  const response = await client.listSessions(request);
  return {
    sessions: response.sessions.map(sessionInfoToListItem),
    nextCursor: response.nextCursor ?? null,
  };
}

export async function acpListRecentSessions(
  maxSessions: number,
  archiveState: SessionArchiveState = 'active',
  filter?: Pick<SessionListFilter, 'workspaceId' | 'includeUnassigned'>
): Promise<SessionListItem[]> {
  if (maxSessions <= 0) {
    return [];
  }

  const client = await getAcpClient();
  const response = await client.listSessions({
    _meta: {
      types: SESSION_LIST_TYPES,
      gosling: {
        archiveState,
        includeLastMessageSnippet: false,
      },
      ...(filter?.workspaceId
        ? {
            workspaceId: filter.workspaceId,
            includeUnassigned: filter.includeUnassigned ?? false,
          }
        : {}),
    },
  });
  return response.sessions.slice(0, maxSessions).map(sessionInfoToListItem);
}

export async function acpGetSessionListItem(sessionId: string): Promise<SessionListItem> {
  const client = await getAcpClient();
  const response = await client.gosling.sessionInfo_unstable({ sessionId });
  return sessionInfoToListItem(response.session);
}

export async function acpLoadSession(sessionId: string): Promise<AcpLoadSessionResult> {
  const pendingLoad = inFlightSessionLoads.get(sessionId);
  if (pendingLoad) {
    return pendingLoad;
  }

  const loadPromise = loadAcpSession(sessionId);
  inFlightSessionLoads.set(sessionId, loadPromise);
  try {
    return await loadPromise;
  } finally {
    if (inFlightSessionLoads.get(sessionId) === loadPromise) {
      inFlightSessionLoads.delete(sessionId);
    }
  }
}

export function isAcpSessionLoadInFlight(sessionId: string): boolean {
  return inFlightSessionLoads.has(sessionId);
}

async function loadAcpSession(sessionId: string): Promise<AcpLoadSessionResult> {
  const client = await getAcpClient();
  const initialSessionInfoResponse = await client.gosling.sessionInfo_unstable({ sessionId });
  const initialSessionInfo = initialSessionInfoResponse.session;
  const response = await client.loadSession({
    sessionId,
    cwd: initialSessionInfo.cwd,
    mcpServers: [],
    _meta: {
      gosling: {
        loadMode: 'compacted',
        tailLimit: COMPACTED_SESSION_TAIL_LIMIT,
      },
    },
  });
  // Loading can populate missing provider/model metadata.
  const sessionInfoResponse = await client.gosling.sessionInfo_unstable({ sessionId });

  return {
    sessionInfo: sessionInfoResponse.session,
    response,
    meta: parseLoadMeta(response),
  };
}

export interface SessionMessagesPage {
  messages: Message[];
  nextBeforeCursor: string | null;
  totalCount: number;
  oldestRowId?: number | null;
  newestRowId?: number | null;
}

export async function acpListSessionMessages(
  sessionId: string,
  beforeCursor?: string | null,
  limit = COMPACTED_SESSION_TAIL_LIMIT
): Promise<SessionMessagesPage> {
  const client = await getAcpClient();
  const response = await client.gosling.sessionMessagesList_unstable({
    sessionId,
    ...(beforeCursor ? { beforeCursor } : {}),
    limit,
  });
  return {
    messages: response.messages as Message[],
    nextBeforeCursor: response.nextBeforeCursor ?? null,
    totalCount: response.totalCount,
    oldestRowId: response.oldestRowId,
    newestRowId: response.newestRowId,
  };
}

export interface SessionMessageSearchMatch {
  rowId: number;
  messageId?: string | null;
  role: string;
  snippet: string;
  created: number;
  beforeCursor?: string | null;
}

export async function acpSearchSessionMessages(
  sessionId: string,
  query: string,
  limit = 20
): Promise<SessionMessageSearchMatch[]> {
  const client = await getAcpClient();
  const response = await client.gosling.sessionMessagesSearch_unstable({
    sessionId,
    query,
    limit,
  });
  return response.matches;
}

export interface AcpNewSessionResult {
  sessionId: string;
  sessionInfo: SessionInfo;
  meta: LoadSessionMeta;
}

export interface AcpWorkspaceLaunchOptions {
  workingDir?: string;
  credentialProfileId?: string;
  additionalFolders?: string[];
  provider?: string;
  model?: string;
  researchLibraryPath?: string;
}

export async function acpNewSession(
  cwd: string,
  goslingExtensions: GoslingExtension[],
  workspaceId?: string,
  workspaceLaunchOptions?: AcpWorkspaceLaunchOptions
): Promise<AcpNewSessionResult> {
  const client = await getAcpClient();
  const meta: Record<string, unknown> = { client: 'gosling-desktop' };
  if (goslingExtensions.length > 0) {
    meta.enabledExtensions = goslingExtensions;
  }
  if (workspaceId) {
    meta.workspaceId = workspaceId;
  }
  if (workspaceLaunchOptions?.workingDir) {
    meta.workspaceWorkingDir = workspaceLaunchOptions.workingDir;
  }
  if (workspaceLaunchOptions?.credentialProfileId) {
    meta.workspaceCredentialProfileId = workspaceLaunchOptions.credentialProfileId;
  }
  if (workspaceLaunchOptions?.additionalFolders?.length) {
    meta.workspaceAdditionalFolders = workspaceLaunchOptions.additionalFolders;
  }
  if (workspaceLaunchOptions?.provider) {
    meta.provider = workspaceLaunchOptions.provider;
  }
  if (workspaceLaunchOptions?.model) {
    meta.model = workspaceLaunchOptions.model;
  }
  if (workspaceLaunchOptions?.researchLibraryPath) {
    meta.researchLibraryPath = workspaceLaunchOptions.researchLibraryPath;
  }
  const request: NewSessionRequest = { cwd, mcpServers: [], _meta: meta };
  const response = await client.newSession(request);
  const sessionId = String(response.sessionId);
  const sessionInfoResponse = await client.gosling.sessionInfo_unstable({ sessionId });

  return {
    sessionId,
    sessionInfo: sessionInfoResponse.session,
    meta: parseSessionResponseMeta(response._meta),
  };
}

export async function acpDeleteSession(sessionId: string): Promise<void> {
  const client = await getAcpClient();
  await client.gosling.sessionDelete({ sessionId });
}

export async function acpCloseSession(sessionId: string): Promise<void> {
  const client = await getAcpClient();
  await client.unstable_closeSession({ sessionId });
}

export async function acpRenameSession(sessionId: string, title: string): Promise<void> {
  const client = await getAcpClient();
  await client.gosling.sessionRename_unstable({ sessionId, title });
}

export async function acpArchiveSession(sessionId: string): Promise<void> {
  const client = await getAcpClient();
  await client.gosling.sessionArchive_unstable({ sessionId });
}

export async function acpUnarchiveSession(sessionId: string): Promise<void> {
  const client = await getAcpClient();
  await client.gosling.sessionUnarchive_unstable({ sessionId });
}

export async function acpUpdateWorkingDir(sessionId: string, workingDir: string): Promise<void> {
  const client = await getAcpClient();
  await client.gosling.sessionWorkingDirUpdate_unstable({ sessionId, workingDir });
}

export async function acpSetSessionMode(sessionId: string, mode: GoslingMode): Promise<void> {
  const client = await getAcpClient();
  await client.setSessionMode({ sessionId, modeId: mode });
}

export async function acpAppendSessionSystemPrompt(
  sessionId: string,
  key: string,
  text: string
): Promise<void> {
  const client = await getAcpClient();
  await client.gosling.sessionSystemPromptSet_unstable({
    sessionId,
    mode: 'append',
    key,
    text,
  });
}

export interface SessionWorkingDirs {
  workingDir: string;
  additionalWorkingDirs: string[];
}

export async function acpAddSessionWorkingDir(
  sessionId: string,
  workingDir: string
): Promise<SessionWorkingDirs> {
  const client = await getAcpClient();
  const response = await client.gosling.sessionWorkingDirsAdd_unstable({ sessionId, workingDir });
  return {
    workingDir: response.workingDir,
    additionalWorkingDirs: response.additionalWorkingDirs,
  };
}

export async function acpRemoveSessionWorkingDir(
  sessionId: string,
  workingDir: string
): Promise<SessionWorkingDirs> {
  const client = await getAcpClient();
  const response = await client.gosling.sessionWorkingDirsRemove_unstable({
    sessionId,
    workingDir,
  });
  return {
    workingDir: response.workingDir,
    additionalWorkingDirs: response.additionalWorkingDirs,
  };
}

export async function acpSetWorkingDirRestriction(
  sessionId: string,
  restrict: boolean
): Promise<void> {
  const client = await getAcpClient();
  await client.gosling.sessionWorkingDirsRestrict_unstable({ sessionId, restrict });
}

export async function acpTruncateSessionConversation(
  sessionId: string,
  truncateFrom: number
): Promise<void> {
  const client = await getAcpClient();
  await client.gosling.sessionConversationTruncate_unstable({ sessionId, truncateFrom });
}

export async function acpForkSession(
  sessionId: string,
  conversationBefore?: number
): Promise<string> {
  const client = await getAcpClient();
  const sessionInfo = await client.gosling.sessionInfo_unstable({ sessionId });
  const { cwd } = sessionInfo.session;
  const request: ForkSessionRequest = { sessionId, cwd };
  if (conversationBefore !== undefined) {
    request._meta = { conversationBefore };
  }
  const response = await client.unstable_forkSession(request);
  return String(response.sessionId);
}

export async function acpExportSession(sessionId: string): Promise<string> {
  const client = await getAcpClient();
  const response = await client.gosling.sessionExport_unstable({ sessionId });
  return response.data;
}

export async function acpImportSession(
  input: string,
  source: SessionImportSource,
  workingDir: string
): Promise<void> {
  const client = await getAcpClient();
  await client.gosling.sessionImport_unstable({ input, source, workingDir });
}

export async function acpShareSessionNostr(sessionId: string, relays: string[]) {
  const client = await getAcpClient();
  return await client.gosling.sessionShareNostr_unstable({ sessionId, relays });
}
