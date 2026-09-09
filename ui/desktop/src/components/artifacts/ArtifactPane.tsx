import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertTriangle,
  BookOpen,
  ClipboardCopy,
  Copy,
  ExternalLink,
  File,
  FileInput,
  FileOutput,
  FileText,
  FolderOpen,
  Image as ImageIcon,
  PanelRightClose,
  Save,
  X,
} from 'lucide-react';
import type { ShellLibraryItemSummary } from '@repo-makeover/gosling-sdk';
import { toast } from 'react-toastify';
import { defineMessages, useIntl } from '../../i18n';
import { useArtifactWorkbench } from '../../contexts/ArtifactWorkbenchContext';
import { cn } from '../../utils';
import MarkdownContent from '../MarkdownContent';
import { Button } from '../ui/button';
import { Switch } from '../ui/switch';
import { ARTIFACT_REPOSITORY_BATCH_LIMIT } from '../../utils/artifactRepository';
import {
  addSandboxCsp,
  hasDisplayedFileExtension,
  isArtifactPreviewable,
  parseCsv,
} from './artifactUtils';
import type { ArtifactTab } from './types';
import { useArtifactRouter } from '../../contexts/ArtifactRouterContext';
import { errorMessage } from '../../utils/conversionUtils';
import {
  defaultSettings,
  isSettingValue,
  OUTPUT_FILE_EXTENSIONS_CHANGED_EVENT,
} from '../../utils/settings';
import { listSessionLibraryInputs } from '../../acp/sessionLibraryInputs';
import { acpChatSessionController } from '../../acp/chatSessionController';
import { describeAcpError } from '../../acp/errors';
import { setSessionInputSelected, useSelectedSessionInputs } from '../../acp/sessionInputSelection';
import { MAX_RESEARCH_INITIAL_INPUTS } from '../../types/sessionExperience';
import { SessionInputControls } from './SessionInputControls';
import type { ResearchLibraryFile } from '../../utils/researchLibrary';
import { documentTitleFromContent, supportsDocumentTitle } from '../../utils/documentTitle';
import { ArtifactFileList } from './ArtifactFileList';
import { OutputHistory } from './OutputHistory';
import { ARTIFACT_TIMESTAMPS_REFRESH_EVENT } from '../../types/artifactFileTimestamps';

const i18n = defineMessages({
  outputs: { id: 'artifactPane.outputs', defaultMessage: 'Outputs' },
  hideRepositoryFiles: {
    id: 'artifactPane.hideRepositoryFiles',
    defaultMessage: 'Hide repository files',
  },
  repositoryFilesHidden: {
    id: 'artifactPane.repositoryFilesHidden',
    defaultMessage: '{count} hidden',
  },
  checkingRepositories: {
    id: 'artifactPane.checkingRepositories',
    defaultMessage: 'Checking repository folders…',
  },
  repositoryCheckUnavailable: {
    id: 'artifactPane.repositoryCheckUnavailable',
    defaultMessage: 'Some files could not be checked and remain visible.',
  },
  repositoryFilterEmpty: {
    id: 'artifactPane.repositoryFilterEmpty',
    defaultMessage: 'All matching outputs are hidden by this filter.',
  },
  inputs: { id: 'artifactPane.inputs', defaultMessage: 'Inputs' },
  library: { id: 'artifactPane.library', defaultMessage: 'Library' },
  missing: { id: 'artifactPane.missing', defaultMessage: 'Missing' },
  blocked: { id: 'artifactPane.blocked', defaultMessage: 'Blocked' },
  truncated: { id: 'artifactPane.truncated', defaultMessage: 'Truncated' },
  closePane: { id: 'artifactPane.closePane', defaultMessage: 'Close inputs and outputs pane' },
  closeAllTabs: { id: 'artifactPane.closeAllTabs', defaultMessage: 'Close all' },
  openFile: { id: 'artifactPane.openFile', defaultMessage: 'Open file' },
  emptyTitle: { id: 'artifactPane.emptyTitle', defaultMessage: 'View an output or deliverable' },
  emptyBody: {
    id: 'artifactPane.emptyBody',
    defaultMessage: 'Open a local file, or send a tool result here from the conversation.',
  },
  previewFailed: { id: 'artifactPane.previewFailed', defaultMessage: 'Preview unavailable' },
  grantAccessToPreview: {
    id: 'artifactPane.grantAccessToPreview',
    defaultMessage: 'Select this file to grant access and preview it',
  },
  previewTruncated: {
    id: 'artifactPane.previewTruncated',
    defaultMessage: 'This preview is truncated. Open the file for the complete output.',
  },
  unsupported: {
    id: 'artifactPane.unsupported',
    defaultMessage: 'This file type does not have an in-app preview yet.',
  },
  loading: { id: 'artifactPane.loading', defaultMessage: 'Loading…' },
  copyPath: { id: 'artifactPane.copyPath', defaultMessage: 'Copy path' },
  copyContents: { id: 'artifactPane.copyContents', defaultMessage: 'Copy contents' },
  copiedContents: { id: 'artifactPane.copiedContents', defaultMessage: 'Contents copied' },
  copyContentsFailed: {
    id: 'artifactPane.copyContentsFailed',
    defaultMessage: 'Unable to copy contents: {error}',
  },
  copyContentsTextOnly: {
    id: 'artifactPane.copyContentsTextOnly',
    defaultMessage: 'Copy contents is available for text documents',
  },
  reveal: { id: 'artifactPane.reveal', defaultMessage: 'Reveal' },
  openExternal: { id: 'artifactPane.openExternal', defaultMessage: 'Open externally' },
  saveCopy: { id: 'artifactPane.saveCopy', defaultMessage: 'Save a copy' },
  savedCopy: { id: 'artifactPane.savedCopy', defaultMessage: 'Artifact copy saved' },
  saveCopyFailed: {
    id: 'artifactPane.saveCopyFailed',
    defaultMessage: 'Unable to save artifact: {error}',
  },
  inputEmptyTitle: {
    id: 'artifactPane.inputEmptyTitle',
    defaultMessage: 'No inputs for this session',
  },
  inputEmptyBody: {
    id: 'artifactPane.inputEmptyBody',
    defaultMessage: 'Uploaded files and pasted content added to the session will appear here.',
  },
  inputLoadFailed: {
    id: 'artifactPane.inputLoadFailed',
    defaultMessage: 'Unable to load session inputs.',
  },
  retryInputs: { id: 'artifactPane.retryInputs', defaultMessage: 'Retry' },
  selectInput: {
    id: 'artifactPane.selectInput',
    defaultMessage: 'Include {name} with the next message',
  },
  libraryEmptyTitle: {
    id: 'artifactPane.libraryEmptyTitle',
    defaultMessage: 'No research documents yet',
  },
  libraryEmptyBody: {
    id: 'artifactPane.libraryEmptyBody',
    defaultMessage: 'Reports and tutorials produced by Deep Research will appear here.',
  },
  libraryLoadFailed: {
    id: 'artifactPane.libraryLoadFailed',
    defaultMessage: 'Unable to load the Research Library.',
  },
  libraryTruncated: {
    id: 'artifactPane.libraryTruncated',
    defaultMessage:
      'Showing the first 500 files. Open the Research Library folder to browse the complete collection.',
  },
  sessionScope: { id: 'artifactPane.sessionScope', defaultMessage: 'Session' },
  projectScope: { id: 'artifactPane.projectScope', defaultMessage: 'Project' },
});

type InventoryTab = 'inputs' | 'library' | 'outputs';

interface PreviewData {
  content: string;
  encoding: 'base64' | 'utf8';
  error: string | null;
  filePath?: string;
  sizeBytes?: number;
  truncated: boolean;
}

const ARTIFACT_ACCESS_RETRY_COUNT = 3;
const ARTIFACT_ACCESS_RETRY_DELAY_MS = 100;

function isRetryableArtifactAccessError(error: string | null): boolean {
  return error?.includes('Renderer file access denied for path outside approved roots') ?? false;
}

function mimeTypeForTab(tab: ArtifactTab): string {
  if (tab.source.type === 'content') return tab.source.mimeType;
  switch (tab.kind) {
    case 'svg':
      return 'image/svg+xml';
    case 'image': {
      const extension = tab.source.path.split('.').pop()?.toLowerCase();
      if (extension === 'jpg' || extension === 'jpeg') return 'image/jpeg';
      if (extension === 'gif') return 'image/gif';
      if (extension === 'webp') return 'image/webp';
      return 'image/png';
    }
    default:
      return 'text/plain';
  }
}

function JsonPreview({ content, jsonl }: { content: string; jsonl: boolean }) {
  let formatted = content;
  try {
    formatted = jsonl
      ? content
          .split('\n')
          .filter(Boolean)
          .map((line) => JSON.stringify(JSON.parse(line), null, 2))
          .join('\n')
      : JSON.stringify(JSON.parse(content), null, 2);
  } catch {
    // Keep the original text visible when an incomplete or malformed file is being inspected.
  }
  return <pre className="whitespace-pre-wrap break-words font-mono text-xs p-4">{formatted}</pre>;
}

function CsvPreview({ content }: { content: string }) {
  const rows = parseCsv(content);
  if (rows.length === 0) return null;
  const [header, ...body] = rows;
  return (
    <div className="overflow-auto h-full p-3">
      <table className="min-w-full border-collapse text-xs">
        <thead className="sticky top-0 bg-background-secondary">
          <tr>
            {header.map((cell, index) => (
              <th
                key={index}
                className="border border-border-primary px-2 py-1.5 text-left font-medium"
              >
                {cell}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {body.map((row, rowIndex) => (
            <tr key={rowIndex}>
              {header.map((_, columnIndex) => (
                <td
                  key={columnIndex}
                  className="border border-border-primary px-2 py-1.5 align-top"
                >
                  {row[columnIndex] ?? ''}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function formatInputSize(sizeBytes: number): string {
  if (sizeBytes < 1024) return `${sizeBytes} B`;
  if (sizeBytes < 1024 * 1024) return `${Math.ceil(sizeBytes / 1024)} KB`;
  return `${(sizeBytes / (1024 * 1024)).toFixed(1)} MB`;
}

function InputIcon({ kind }: { kind: ShellLibraryItemSummary['kind'] }) {
  if (kind === 'image') return <ImageIcon className="h-4 w-4 shrink-0 text-text-secondary" />;
  if (kind === 'text') return <FileText className="h-4 w-4 shrink-0 text-text-secondary" />;
  return <File className="h-4 w-4 shrink-0 text-text-secondary" />;
}

function Preview({
  tab,
  data,
  onGrantAccess,
}: {
  tab: ArtifactTab;
  data: PreviewData;
  onGrantAccess: () => void;
}) {
  const intl = useIntl();
  if (data.error) {
    return (
      <div className="m-4 rounded-lg border border-border-primary p-4 text-sm">
        <div className="flex items-center gap-2 font-medium">
          <AlertTriangle className="h-4 w-4" />
          {intl.formatMessage(i18n.previewFailed)}
        </div>
        <p className="mt-2 text-text-secondary">{data.error}</p>
        {isRetryableArtifactAccessError(data.error) && (
          <Button className="mt-3" variant="outline" size="sm" onClick={onGrantAccess}>
            <FolderOpen className="mr-2 h-4 w-4" />
            {intl.formatMessage(i18n.grantAccessToPreview)}
          </Button>
        )}
      </div>
    );
  }

  if (
    data.truncated &&
    (tab.kind === 'html' || tab.kind === 'image' || tab.kind === 'pdf' || tab.kind === 'svg')
  ) {
    return (
      <div className="m-4 rounded-lg border border-border-primary p-4 text-sm text-text-secondary">
        {intl.formatMessage(i18n.previewTruncated)}
      </div>
    );
  }

  switch (tab.kind) {
    case 'markdown':
      return (
        <div className="p-5">
          <MarkdownContent content={data.content} />
        </div>
      );
    case 'csv':
      return <CsvPreview content={data.content} />;
    case 'json':
      return <JsonPreview content={data.content} jsonl={false} />;
    case 'jsonl':
      return <JsonPreview content={data.content} jsonl />;
    case 'html':
      return (
        <iframe
          className="h-full w-full border-0 bg-white"
          sandbox="allow-scripts"
          referrerPolicy="no-referrer"
          srcDoc={addSandboxCsp(data.content)}
          title={tab.title}
        />
      );
    case 'image':
    case 'svg':
      return (
        <div className="flex min-h-full items-center justify-center p-4 bg-background-secondary">
          <img
            className="max-h-full max-w-full object-contain"
            src={`data:${mimeTypeForTab(tab)};base64,${data.content}`}
            alt={tab.title}
          />
        </div>
      );
    case 'pdf':
      return (
        <iframe
          className="h-full w-full border-0 bg-white"
          src={`data:application/pdf;base64,${data.content}`}
          title={tab.title}
        />
      );
    case 'graphml':
    case 'code':
    case 'text':
      return (
        <pre className="whitespace-pre-wrap break-words font-mono text-xs p-4">{data.content}</pre>
      );
    default:
      return (
        <div className="p-5 text-sm text-text-secondary">
          {intl.formatMessage(i18n.unsupported)}
        </div>
      );
  }
}

export function ArtifactPane() {
  const intl = useIntl();
  const { saveArtifact } = useArtifactRouter();
  const {
    activeTab,
    activeTabId,
    artifacts,
    trashedArtifacts = [],
    closeTab,
    closeAllTabs,
    forgetTrashedFiles,
    hideRepositoryFiles,
    openFile,
    openArtifact,
    resolveFilePath,
    setActiveTabId,
    setIsOpen,
    setHideRepositoryFiles,
    setWidth,
    tabs,
    visibleSessionId,
    width,
  } = useArtifactWorkbench();
  const [inventoryTab, setInventoryTab] = useState<InventoryTab>('outputs');
  const [inputs, setInputs] = useState<ShellLibraryItemSummary[]>([]);
  const [inputsLoading, setInputsLoading] = useState(false);
  const [inputsError, setInputsError] = useState<string | null>(null);
  const [inputsRevision, setInputsRevision] = useState(0);
  const selectedInputs = useSelectedSessionInputs(visibleSessionId);
  const visibleSessionIdRef = useRef(visibleSessionId);
  visibleSessionIdRef.current = visibleSessionId;
  const [researchLibraryFiles, setResearchLibraryFiles] = useState<ResearchLibraryFile[]>([]);
  const [researchLibraryTruncated, setResearchLibraryTruncated] = useState(false);
  const [researchLibraryPath, setResearchLibraryPath] = useState<string | null>(null);
  const [researchLibraryLoading, setResearchLibraryLoading] = useState(false);
  const [researchLibraryError, setResearchLibraryError] = useState(false);
  const [preview, setPreview] = useState<PreviewData | null>(null);
  const [previewRevision, setPreviewRevision] = useState(0);
  const [copyingContents, setCopyingContents] = useState(false);
  const [titleRefresh, setTitleRefresh] = useState(0);
  const [titleCache, setTitleCache] = useState<Record<string, { revision: string; title: string }>>(
    {}
  );
  const [loading, setLoading] = useState(false);
  const [outputFileExtensions, setOutputFileExtensions] = useState<string[]>(
    defaultSettings.outputFileExtensions
  );
  const receivedOutputFileExtensionsChange = useRef(false);
  const researchLibraryRevision = useRef(0);
  const [repositoryClassification, setRepositoryClassification] = useState<{
    artifacts: typeof artifacts;
    paths: Set<string>;
    unavailable: boolean;
  } | null>(null);

  const refreshResearchLibrary = useCallback(
    async (background = false) => {
      const revision = ++researchLibraryRevision.current;
      if (!background) setResearchLibraryLoading(true);
      setResearchLibraryError(false);
      try {
        const [libraryPath, listing] = await Promise.all([
          window.electron.getResearchLibraryPath(),
          window.electron.listResearchLibraryFiles(),
        ]);
        if (revision !== researchLibraryRevision.current) return;
        setResearchLibraryPath(libraryPath);
        setResearchLibraryFiles(listing.files);
        setResearchLibraryTruncated(listing.truncated);
      } catch {
        if (revision !== researchLibraryRevision.current) return;
        if (background) {
          toast.error(intl.formatMessage(i18n.libraryLoadFailed));
        } else {
          setResearchLibraryFiles([]);
          setResearchLibraryTruncated(false);
          setResearchLibraryError(true);
        }
      } finally {
        if (revision === researchLibraryRevision.current) setResearchLibraryLoading(false);
      }
    },
    [intl]
  );

  useEffect(() => {
    void refreshResearchLibrary();
  }, [refreshResearchLibrary]);

  useEffect(() => {
    if (!visibleSessionId) {
      setInputs([]);
      setInputsError(null);
      setInputsLoading(false);
      return;
    }

    let cancelled = false;
    setInputs([]);
    setInputsLoading(true);
    setInputsError(null);
    void acpChatSessionController
      .loadSession(visibleSessionId)
      .then((loaded) => {
        if (!loaded) throw new Error('The session could not be loaded. Retry after reconnecting.');
        if (cancelled) return [];
        return listSessionLibraryInputs(visibleSessionId);
      })
      .then((items) => {
        if (!cancelled) setInputs(items);
      })
      .catch((cause) => {
        if (!cancelled) {
          setInputs([]);
          setInputsError(describeAcpError(cause));
        }
      })
      .finally(() => {
        if (!cancelled) setInputsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [visibleSessionId, inputsRevision]);

  useEffect(() => {
    let cancelled = false;
    void window.electron.getSetting('outputFileExtensions').then((extensions) => {
      if (
        !cancelled &&
        !receivedOutputFileExtensionsChange.current &&
        isSettingValue('outputFileExtensions', extensions)
      ) {
        setOutputFileExtensions(extensions);
      }
    });

    const handleChange = (event: Event) => {
      const extensions = (event as CustomEvent<unknown>).detail;
      if (isSettingValue('outputFileExtensions', extensions)) {
        receivedOutputFileExtensionsChange.current = true;
        setOutputFileExtensions(extensions);
      }
    };
    window.addEventListener(OUTPUT_FILE_EXTENSIONS_CHANGED_EVENT, handleChange);
    return () => {
      cancelled = true;
      window.removeEventListener(OUTPUT_FILE_EXTENSIONS_CHANGED_EVENT, handleChange);
    };
  }, []);

  const extensionMatchedArtifacts = useMemo(
    () =>
      artifacts.filter((artifact) =>
        hasDisplayedFileExtension(artifact.displayPath, outputFileExtensions)
      ),
    [artifacts, outputFileExtensions]
  );

  useEffect(() => {
    if (!hideRepositoryFiles) return;
    let cancelled = false;
    const filePaths = extensionMatchedArtifacts.map((artifact) => artifact.resolvedPath);
    void (async () => {
      const paths = new Set<string>();
      let unavailable = false;
      for (let offset = 0; offset < filePaths.length; offset += ARTIFACT_REPOSITORY_BATCH_LIMIT) {
        try {
          const result = await window.electron.classifyArtifactRepositories(
            filePaths.slice(offset, offset + ARTIFACT_REPOSITORY_BATCH_LIMIT)
          );
          if (cancelled) return;
          result.repositoryPaths.forEach((filePath) => paths.add(filePath));
          unavailable ||= result.unavailablePaths.length > 0;
        } catch {
          unavailable = true;
        }
        if (cancelled) return;
      }
      if (!cancelled) {
        setRepositoryClassification({ artifacts: extensionMatchedArtifacts, paths, unavailable });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [extensionMatchedArtifacts, hideRepositoryFiles]);

  const currentClassification =
    repositoryClassification?.artifacts === extensionMatchedArtifacts
      ? repositoryClassification
      : null;
  const displayedArtifacts = useMemo(
    () =>
      hideRepositoryFiles
        ? extensionMatchedArtifacts.filter(
            (artifact) => !currentClassification?.paths.has(artifact.resolvedPath)
          )
        : extensionMatchedArtifacts,
    [extensionMatchedArtifacts, hideRepositoryFiles, currentClassification]
  );

  useEffect(() => {
    const refresh = () => setTitleRefresh((value) => value + 1);
    window.addEventListener('focus', refresh);
    window.addEventListener(ARTIFACT_TIMESTAMPS_REFRESH_EVENT, refresh);
    return () => {
      window.removeEventListener('focus', refresh);
      window.removeEventListener(ARTIFACT_TIMESTAMPS_REFRESH_EVENT, refresh);
    };
  }, []);

  const titleRequests = useMemo(() => {
    const requests = new Map<string, { filePath: string; revision: string }>();
    for (const artifact of displayedArtifacts) {
      if (supportsDocumentTitle(artifact.resolvedPath)) {
        requests.set(artifact.resolvedPath, {
          filePath: artifact.resolvedPath,
          revision: JSON.stringify([artifact.lastSeenAt, titleRefresh]),
        });
      }
    }
    for (const file of researchLibraryFiles) {
      if (supportsDocumentTitle(file.path)) {
        requests.set(file.path, {
          filePath: file.path,
          revision: JSON.stringify([file.modifiedAt, titleRefresh]),
        });
      }
    }
    return Array.from(requests.values());
  }, [displayedArtifacts, researchLibraryFiles, titleRefresh]);

  const documentTitles = useMemo(
    () =>
      Object.fromEntries(
        titleRequests.map(({ filePath, revision }) => [
          filePath,
          titleCache[filePath]?.revision === revision ? titleCache[filePath].title : '',
        ])
      ),
    [titleRequests, titleCache]
  );

  useEffect(() => {
    const pending = titleRequests.filter(
      ({ filePath, revision }) => titleCache[filePath]?.revision !== revision
    );
    if (pending.length === 0) return;
    let cancelled = false;
    void (async () => {
      const results: Record<string, { revision: string; title: string }> = {};
      // The title IPC accepts at most 200 files, independently of inventory pagination.
      for (let offset = 0; offset < pending.length; offset += 200) {
        const batch = pending.slice(offset, offset + 200);
        const titles = await window.electron.readArtifactTitles(
          batch.map(({ filePath }) => ({ filePath }))
        );
        if (cancelled) return;
        for (const { filePath, revision } of batch) {
          results[filePath] = { revision, title: titles[filePath] ?? '' };
        }
      }
      if (!cancelled) setTitleCache((previous) => ({ ...previous, ...results }));
    })().catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [titleRequests, titleCache]);

  useEffect(() => {
    if (!activeTab || activeTab.source.type === 'content' || activeTab.kind === 'unknown') {
      setLoading(false);
    }
    if (!activeTab) {
      setPreview(null);
      return;
    }
    if (activeTab.source.type === 'content') {
      setPreview({
        content: activeTab.source.content,
        encoding: activeTab.source.encoding,
        error: null,
        truncated: false,
      });
      return;
    }
    if (activeTab.kind === 'unknown') {
      setPreview({ content: '', encoding: 'utf8', error: null, truncated: false });
      return;
    }

    let cancelled = false;
    const sourcePath = activeTab.source.path;
    const sourceBaseDirectory = activeTab.source.baseDirectory;
    setLoading(true);
    const readPreview = async () => {
      for (let attempt = 0; attempt <= ARTIFACT_ACCESS_RETRY_COUNT; attempt += 1) {
        const response = await window.electron.readArtifactFile(sourcePath, sourceBaseDirectory);
        if (cancelled) return;
        if (
          !isRetryableArtifactAccessError(response.error) ||
          attempt === ARTIFACT_ACCESS_RETRY_COUNT
        ) {
          setPreview(response);
          if (response.found && response.filePath !== sourcePath) {
            resolveFilePath(activeTab.id, response.filePath);
          }
          return;
        }

        await new Promise((resolve) =>
          setTimeout(resolve, ARTIFACT_ACCESS_RETRY_DELAY_MS * (attempt + 1))
        );
      }
    };
    void readPreview().finally(() => {
      if (!cancelled) setLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [activeTab, resolveFilePath, previewRevision]);

  const chooseFile = async () => {
    const selected = await window.electron.selectArtifactFile(
      activeTab?.source.type === 'file' ? activeTab.source.path : undefined
    );
    if (!selected) return;
    if (!isArtifactPreviewable(selected)) {
      toast.info(intl.formatMessage(i18n.unsupported));
      return;
    }
    openFile(selected);
  };

  const selectInventoryTab = (tab: InventoryTab) => {
    setInventoryTab(tab);
    if (tab === 'inputs') setInputsRevision((revision) => revision + 1);
    if (tab === 'library') void refreshResearchLibrary();
  };

  const resizeFrom = (event: React.PointerEvent) => {
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = width;
    const move = (moveEvent: globalThis.PointerEvent) =>
      setWidth(startWidth + startX - moveEvent.clientX);
    const stop = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', stop);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', stop);
  };

  const filePath = activeTab?.source.type === 'file' ? activeTab.source.path : null;
  const previewTitle = useMemo(() => {
    if (!activeTab) return null;
    if (filePath && documentTitles[filePath]) return documentTitles[filePath];
    if (preview?.encoding === 'utf8' && !preview.error && supportsDocumentTitle(activeTab.title)) {
      return documentTitleFromContent(preview.content);
    }
    return null;
  }, [activeTab, documentTitles, filePath, preview]);
  const fileBaseDirectory =
    activeTab?.source.type === 'file' ? activeTab.source.baseDirectory : undefined;

  const selectedArtifactPath =
    activeTab?.source.type === 'file' ? activeTab.source.path : undefined;

  const artifactStatus = (displayPath: string) => {
    if (displayPath !== selectedArtifactPath || !preview) return null;
    if (preview.truncated) return intl.formatMessage(i18n.truncated);
    if (!preview.error) return null;
    return intl.formatMessage(
      /not found|no such file|missing/i.test(preview.error) ? i18n.missing : i18n.blocked
    );
  };

  const supportsCopyContents = activeTab && !['image', 'pdf', 'unknown'].includes(activeTab.kind);

  const copyContents = async () => {
    if (!activeTab || !supportsCopyContents || copyingContents || loading || preview?.error) return;
    setCopyingContents(true);
    try {
      if (activeTab.source.type === 'file') {
        await window.electron.copyArtifactContents(
          activeTab.source.path,
          activeTab.source.baseDirectory
        );
      } else {
        const source = activeTab.source;
        const text =
          source.encoding === 'utf8'
            ? source.content
            : new TextDecoder('utf-8', { fatal: true }).decode(
                Uint8Array.from(window.atob(source.content), (character) => character.charCodeAt(0))
              );
        await window.electron.writeClipboardText(text);
      }
      toast.success(intl.formatMessage(i18n.copiedContents));
    } catch (cause) {
      toast.error(
        intl.formatMessage(i18n.copyContentsFailed, { error: errorMessage(cause, 'Unknown error') })
      );
    } finally {
      setCopyingContents(false);
    }
  };

  const saveCopy = async () => {
    if (!activeTab) return;
    try {
      const source =
        activeTab.source.type === 'file'
          ? {
              type: 'file' as const,
              path: activeTab.source.path,
              baseDirectory: activeTab.source.baseDirectory,
            }
          : {
              type: 'content' as const,
              content: activeTab.source.content,
              encoding: activeTab.source.encoding,
            };
      const result = await saveArtifact({
        workspaceId: activeTab.workspaceId,
        mimeType: mimeTypeForTab(activeTab),
        suggestedName: activeTab.title,
        title: intl.formatMessage(i18n.saveCopy),
        source,
      });
      if (!result.canceled) toast.success(intl.formatMessage(i18n.savedCopy));
    } catch (cause) {
      toast.error(
        intl.formatMessage(i18n.saveCopyFailed, {
          error: errorMessage(cause, 'Unknown error'),
        })
      );
    }
  };

  return (
    <div className="relative flex h-full flex-col overflow-hidden rounded-xl border border-border-primary bg-background-primary">
      <div
        className="absolute inset-y-0 -left-1 z-10 w-2 cursor-col-resize"
        onPointerDown={resizeFrom}
      />
      <div className="no-drag flex h-11 shrink-0 items-center gap-1 border-b border-border-primary px-2">
        <div className="flex h-full items-end gap-1" role="tablist" aria-label="Session inventory">
          {(['inputs', 'outputs', 'library'] as const).map((tab) => {
            const active = inventoryTab === tab;
            const count =
              tab === 'inputs'
                ? inputs.length
                : tab === 'library'
                  ? researchLibraryFiles.length
                  : displayedArtifacts.length;
            const label = intl.formatMessage(
              tab === 'inputs' ? i18n.inputs : tab === 'library' ? i18n.library : i18n.outputs
            );
            const countLabel =
              tab === 'library' && researchLibraryTruncated ? `${count}+` : String(count);
            const Icon = tab === 'inputs' ? FileInput : tab === 'library' ? BookOpen : FileOutput;
            return (
              <button
                key={tab}
                type="button"
                role="tab"
                aria-selected={active}
                aria-label={`${label} ${countLabel}`}
                onClick={() => selectInventoryTab(tab)}
                className={cn(
                  'flex h-10 items-center gap-1.5 border-b-2 px-2 text-xs font-medium',
                  active
                    ? 'border-text-primary text-text-primary'
                    : 'border-transparent text-text-secondary hover:text-text-primary'
                )}
              >
                <Icon className="h-4 w-4" />
                <span>{label}</span>
                <span
                  data-testid={`${tab}-count`}
                  className="min-w-5 rounded-md border border-border-primary bg-background-secondary px-1.5 py-0.5 text-center text-[10px] leading-none"
                >
                  {countLabel}
                </span>
              </button>
            );
          })}
        </div>
        <div className="ml-auto flex items-center gap-1">
          {inventoryTab === 'outputs' && (
            <Button
              variant="ghost"
              size="xs"
              className="no-drag"
              onClick={() => void chooseFile()}
              title={intl.formatMessage(i18n.openFile)}
            >
              <FolderOpen className="h-4 w-4" />
            </Button>
          )}
          {inventoryTab === 'library' && researchLibraryPath && (
            <Button
              variant="ghost"
              size="xs"
              className="no-drag"
              onClick={() => void window.electron.openDirectoryInExplorer(researchLibraryPath)}
              title="Open Research Library folder"
            >
              <FolderOpen className="h-4 w-4" />
            </Button>
          )}
          <Button
            variant="ghost"
            size="xs"
            className="no-drag"
            onClick={() => setIsOpen(false)}
            title={intl.formatMessage(i18n.closePane)}
          >
            <PanelRightClose className="h-4 w-4" />
          </Button>
        </div>
      </div>

      {inventoryTab === 'inputs' ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <SessionInputControls
            key={visibleSessionId}
            sessionId={visibleSessionId}
            onAdded={() => {
              if (visibleSessionIdRef.current === visibleSessionId) {
                setInputsRevision((revision) => revision + 1);
              }
            }}
          />
          {inputsLoading ? (
            <div className="flex h-full items-center justify-center text-sm text-text-secondary">
              {intl.formatMessage(i18n.loading)}
            </div>
          ) : inputsError ? (
            <div className="flex flex-col items-center justify-center p-8 text-center" role="alert">
              <AlertTriangle className="h-8 w-8 text-text-secondary" />
              <p className="mt-3 text-sm text-text-secondary">
                {intl.formatMessage(i18n.inputLoadFailed)}
              </p>
              <p className="mt-2 break-words text-xs text-text-secondary">{inputsError}</p>
              <Button
                className="mt-3"
                variant="outline"
                size="sm"
                onClick={() => setInputsRevision((revision) => revision + 1)}
              >
                {intl.formatMessage(i18n.retryInputs)}
              </Button>
            </div>
          ) : inputs.length === 0 ? (
            <div className="flex flex-col items-center justify-center p-8 text-center">
              <FileInput className="h-8 w-8 text-text-secondary" />
              <h2 className="mt-3 text-sm font-medium">
                {intl.formatMessage(i18n.inputEmptyTitle)}
              </h2>
              <p className="mt-1 max-w-xs text-xs text-text-secondary">
                {intl.formatMessage(i18n.inputEmptyBody)}
              </p>
            </div>
          ) : (
            <ul className="py-1" aria-label={intl.formatMessage(i18n.inputs)}>
              {inputs.map((input) => (
                <li
                  key={input.id}
                  className="flex items-center gap-2 border-b border-border-primary px-3 py-2.5 last:border-b-0"
                >
                  <input
                    type="checkbox"
                    checked={selectedInputs.includes(input.id)}
                    disabled={
                      !selectedInputs.includes(input.id) &&
                      (input.status === 'missing' ||
                        selectedInputs.length >= MAX_RESEARCH_INITIAL_INPUTS)
                    }
                    aria-label={intl.formatMessage(i18n.selectInput, { name: input.name })}
                    onChange={(event) => {
                      if (visibleSessionId)
                        setSessionInputSelected(visibleSessionId, input.id, event.target.checked);
                    }}
                  />
                  <InputIcon kind={input.kind} />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-xs text-text-primary">{input.name}</span>
                    <span className="block truncate text-[10px] text-text-secondary">
                      {input.mimeType} ·{' '}
                      {intl.formatMessage(
                        input.scope === 'project' ? i18n.projectScope : i18n.sessionScope
                      )}{' '}
                      · {formatInputSize(input.sizeBytes)}
                    </span>
                  </span>
                  {input.status === 'missing' && (
                    <span className="rounded-md border border-border-primary px-1.5 py-0.5 text-[10px] text-text-secondary">
                      {intl.formatMessage(i18n.missing)}
                    </span>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
      ) : inventoryTab === 'library' ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          {researchLibraryLoading ? (
            <div className="flex h-full items-center justify-center text-sm text-text-secondary">
              {intl.formatMessage(i18n.loading)}
            </div>
          ) : researchLibraryError ? (
            <div className="flex h-full flex-col items-center justify-center p-8 text-center">
              <AlertTriangle className="h-8 w-8 text-text-secondary" />
              <p className="mt-3 text-sm text-text-secondary">
                {intl.formatMessage(i18n.libraryLoadFailed)}
              </p>
            </div>
          ) : researchLibraryFiles.length === 0 ? (
            <div className="flex h-full flex-col items-center justify-center p-8 text-center">
              <BookOpen className="h-8 w-8 text-text-secondary" />
              <h2 className="mt-3 text-sm font-medium">
                {intl.formatMessage(i18n.libraryEmptyTitle)}
              </h2>
              <p className="mt-1 max-w-xs text-xs text-text-secondary">
                {intl.formatMessage(i18n.libraryEmptyBody)}
              </p>
            </div>
          ) : (
            <div>
              {researchLibraryTruncated && (
                <div
                  role="status"
                  className="m-3 flex items-start gap-2 rounded-md border border-border-primary px-3 py-2 text-xs text-text-secondary"
                >
                  <AlertTriangle className="h-4 w-4 shrink-0" />
                  {intl.formatMessage(i18n.libraryTruncated)}
                </div>
              )}
              <ArtifactFileList
                key={`library:${researchLibraryPath}`}
                label={intl.formatMessage(i18n.library)}
                items={researchLibraryFiles.map((file) => ({
                  path: file.path,
                  name: documentTitles[file.path] || file.name,
                  detail: `${file.relativePath} · ${formatInputSize(file.sizeBytes)}`,
                  timestampRevision: file.modifiedAt,
                  active: false,
                }))}
                onOpen={(path) => {
                  openFile(path);
                  setInventoryTab('outputs');
                }}
                onDeleted={(paths) => {
                  forgetTrashedFiles(paths);
                  setResearchLibraryFiles((files) =>
                    files.filter((file) => !paths.includes(file.path))
                  );
                  void refreshResearchLibrary(true);
                }}
              />
            </div>
          )}
        </div>
      ) : (
        <>
          <div className="shrink-0 border-b border-border-primary px-3 py-2 text-xs">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <label className="flex cursor-pointer items-center gap-2">
                <Switch
                  variant="mono"
                  checked={hideRepositoryFiles}
                  onCheckedChange={setHideRepositoryFiles}
                  aria-label={intl.formatMessage(i18n.hideRepositoryFiles)}
                />
                {intl.formatMessage(i18n.hideRepositoryFiles)}
              </label>
              {hideRepositoryFiles && (
                <span className="text-text-secondary" role="status">
                  {intl.formatMessage(i18n.repositoryFilesHidden, {
                    count: extensionMatchedArtifacts.length - displayedArtifacts.length,
                  })}
                </span>
              )}
            </div>
            {hideRepositoryFiles && !currentClassification && (
              <p className="mt-2 text-text-secondary" role="status">
                {intl.formatMessage(i18n.checkingRepositories)}
              </p>
            )}
            {hideRepositoryFiles && currentClassification?.unavailable && (
              <p className="mt-2 text-text-secondary" role="status">
                {intl.formatMessage(i18n.repositoryCheckUnavailable)}
              </p>
            )}
            {hideRepositoryFiles &&
              extensionMatchedArtifacts.length > 0 &&
              displayedArtifacts.length === 0 && (
                <p className="mt-2 text-text-secondary">
                  {intl.formatMessage(i18n.repositoryFilterEmpty)}
                </p>
              )}
          </div>
          {visibleSessionId && trashedArtifacts.length > 0 && (
            <details className="max-h-52 shrink-0 overflow-y-auto border-b border-border-primary p-3">
              <summary className="cursor-pointer text-xs">
                Saved history for removed outputs ({trashedArtifacts.length})
              </summary>
              <p className="py-2 text-xs text-text-secondary">
                Saved revisions remain after Trash or chat deletion. Export them here; restore the
                file from Trash before using Restore revision.
              </p>
              {trashedArtifacts.map((artifact) => (
                <div key={artifact.resolvedPath}>
                  <p className="break-all text-xs">{artifact.displayPath}</p>
                  <OutputHistory sessionId={visibleSessionId} path={artifact.resolvedPath} />
                </div>
              ))}
            </details>
          )}
          {artifacts.length > extensionMatchedArtifacts.length && (
            <p role="status" className="px-3 py-2 text-xs text-text-secondary">
              {artifacts.length - extensionMatchedArtifacts.length} outputs hidden by file
              extensions. Change Outputs file extensions in Settings.
            </p>
          )}
          {displayedArtifacts.length > 0 && (
            <div className="max-h-52 shrink-0 overflow-y-auto border-b border-border-primary py-1">
              <ArtifactFileList
                key={`outputs:${visibleSessionId}:${hideRepositoryFiles}`}
                outputSessionId={visibleSessionId ?? undefined}
                onRestored={() => setPreviewRevision((revision) => revision + 1)}
                label={intl.formatMessage(i18n.outputs)}
                items={displayedArtifacts.map((artifact) => ({
                  path: artifact.resolvedPath,
                  timestampRevision: artifact.lastSeenAt,
                  name: documentTitles[artifact.resolvedPath] || artifact.displayPath,
                  detail: documentTitles[artifact.resolvedPath]
                    ? `${artifact.displayPath} · ${artifact.relation}`
                    : `${artifact.relation} · ${artifact.provenance.replace(/_/g, ' ')}`,
                  active:
                    activeTab?.source.type === 'file' &&
                    (activeTab.source.path === artifact.resolvedPath ||
                      (activeTab.source.path === artifact.displayPath &&
                        activeTab.source.baseDirectory === artifact.baseWorkingDir)),
                  status: artifactStatus(artifact.displayPath) || undefined,
                }))}
                onOpen={(path) => {
                  const artifact = displayedArtifacts.find((item) => item.resolvedPath === path);
                  if (artifact) openArtifact(artifact);
                }}
                onDeleted={(paths) => {
                  forgetTrashedFiles(paths);
                  void refreshResearchLibrary(true);
                }}
              />
            </div>
          )}

          {tabs.length > 0 && (
            <div className="flex shrink-0 overflow-x-auto border-b border-border-primary">
              {tabs.map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => setActiveTabId(tab.id)}
                  className={cn(
                    'group flex max-w-52 shrink-0 items-center gap-2 border-r border-border-primary px-3 py-2 text-xs',
                    tab.id === activeTabId
                      ? 'bg-background-secondary text-text-primary'
                      : 'text-text-secondary hover:bg-background-secondary/60'
                  )}
                >
                  <span className="truncate">{tab.title}</span>
                  <X
                    className="h-3 w-3 shrink-0 opacity-60 hover:opacity-100"
                    onClick={(event) => {
                      event.stopPropagation();
                      closeTab(tab.id);
                    }}
                  />
                </button>
              ))}
              <Button
                variant="ghost"
                size="xs"
                className="ml-auto shrink-0 self-center"
                onClick={closeAllTabs}
                title={intl.formatMessage(i18n.closeAllTabs)}
              >
                {intl.formatMessage(i18n.closeAllTabs)}
              </Button>
            </div>
          )}

          {activeTab && (
            <div className="shrink-0 border-b border-border-primary px-3 py-2">
              <div
                className="truncate text-sm font-medium text-text-primary"
                title={previewTitle ?? activeTab.title}
              >
                {previewTitle ?? activeTab.title}
              </div>
              <div
                className="truncate font-mono text-[10px] text-text-secondary"
                title={filePath ?? activeTab.title}
              >
                {filePath ?? activeTab.title}
              </div>
            </div>
          )}

          <div className="min-h-0 flex-1 overflow-auto">
            {!activeTab ? (
              <div className="flex h-full flex-col items-center justify-center p-8 text-center">
                <FileOutput className="h-8 w-8 text-text-secondary" />
                <h2 className="mt-3 text-sm font-medium">{intl.formatMessage(i18n.emptyTitle)}</h2>
                <p className="mt-1 max-w-xs text-xs text-text-secondary">
                  {intl.formatMessage(i18n.emptyBody)}
                </p>
                <Button
                  className="mt-4"
                  variant="outline"
                  size="sm"
                  onClick={() => void chooseFile()}
                >
                  <FolderOpen className="mr-2 h-4 w-4" />
                  {intl.formatMessage(i18n.openFile)}
                </Button>
              </div>
            ) : loading || !preview ? (
              <div className="flex h-full items-center justify-center text-sm text-text-secondary">
                {intl.formatMessage(i18n.loading)}
              </div>
            ) : (
              <>
                {preview.truncated &&
                  activeTab.kind !== 'html' &&
                  activeTab.kind !== 'image' &&
                  activeTab.kind !== 'pdf' &&
                  activeTab.kind !== 'svg' && (
                    <div className="m-3 flex items-center gap-2 rounded-md border border-border-primary px-3 py-2 text-xs text-text-secondary">
                      <AlertTriangle className="h-4 w-4" />
                      {intl.formatMessage(i18n.previewTruncated)}
                    </div>
                  )}
                <Preview tab={activeTab} data={preview} onGrantAccess={() => void chooseFile()} />
              </>
            )}
          </div>

          {activeTab && (
            <div className="flex shrink-0 items-center gap-1 border-t border-border-primary px-2 py-1.5">
              <span className="min-w-0 flex-1" />
              <Button
                variant="ghost"
                size="xs"
                title={intl.formatMessage(i18n.saveCopy)}
                aria-label={intl.formatMessage(i18n.saveCopy)}
                disabled={loading || Boolean(preview?.error)}
                onClick={() => void saveCopy()}
              >
                <Save className="h-3.5 w-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="xs"
                title={intl.formatMessage(
                  supportsCopyContents ? i18n.copyContents : i18n.copyContentsTextOnly
                )}
                aria-label={intl.formatMessage(i18n.copyContents)}
                disabled={
                  !supportsCopyContents ||
                  loading ||
                  !preview ||
                  Boolean(preview.error) ||
                  copyingContents
                }
                onClick={() => void copyContents()}
              >
                <ClipboardCopy className="h-3.5 w-3.5" />
              </Button>
              {filePath && (
                <>
                  <Button
                    variant="ghost"
                    size="xs"
                    title={intl.formatMessage(i18n.copyPath)}
                    aria-label={intl.formatMessage(i18n.copyPath)}
                    onClick={() => void window.electron.writeClipboardText(filePath)}
                  >
                    <Copy className="h-3.5 w-3.5" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="xs"
                    title={intl.formatMessage(i18n.reveal)}
                    onClick={() =>
                      void window.electron.revealArtifactFile(filePath, fileBaseDirectory)
                    }
                  >
                    <FolderOpen className="h-3.5 w-3.5" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="xs"
                    title={intl.formatMessage(i18n.openExternal)}
                    onClick={() =>
                      void window.electron.openArtifactFile(filePath, fileBaseDirectory)
                    }
                  >
                    <ExternalLink className="h-3.5 w-3.5" />
                  </Button>
                </>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}
