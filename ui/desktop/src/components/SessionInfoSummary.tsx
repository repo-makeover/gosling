import { useCallback, useMemo } from 'react';
import { Folder, ShieldAlert, X } from 'lucide-react';
import { toast } from 'react-toastify';
import { acpSetWorkingDirRestriction } from '../acp/sessions';
import type { Session } from '../types/session';
import { CredentialProfileSelector } from './bottom_menu/CredentialProfileSelector';
import { Switch } from './ui/switch';
import WorkingDirectoriesMenu from './WorkingDirectoriesMenu';

interface SessionInfoSummaryProps {
  session?: Session;
  onSessionChange: (updater: (session: Session) => Session) => void;
  liveTotalTokens?: number;
  liveAccumulatedCost?: number | null;
  onCollapse: () => void;
}

function directoryLabel(dir: string): string {
  const trimmed = dir.replace(/\/+$/, '');
  const leaf = trimmed.split('/').pop();
  return leaf && leaf.length > 0 ? leaf : dir;
}

function formatCount(value?: number | null): string {
  return value == null
    ? 'Not available'
    : new Intl.NumberFormat('en', {
        notation: 'compact',
        maximumFractionDigits: 1,
      }).format(value);
}

function formatCost(value?: number | null): string {
  if (value == null) return 'Not available';
  return new Intl.NumberFormat('en', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: value > 0 && value < 0.01 ? 4 : 2,
    maximumFractionDigits: value > 0 && value < 0.01 ? 4 : 2,
  }).format(value);
}

function modeLabel(mode?: Session['gosling_mode']): string {
  switch (mode) {
    case 'auto':
      return 'Autonomous';
    case 'approve':
      return 'Approval required';
    case 'smart_approve':
      return 'Smart approval';
    case 'chat':
      return 'Chat only';
    default:
      return 'Not available';
  }
}

export default function SessionInfoSummary({
  session,
  onSessionChange,
  liveTotalTokens,
  liveAccumulatedCost,
  onCollapse,
}: SessionInfoSummaryProps) {
  const directories = useMemo(
    () =>
      session?.working_dir ? [session.working_dir, ...(session.additional_working_dirs ?? [])] : [],
    [session?.working_dir, session?.additional_working_dirs]
  );

  const toggleRestriction = useCallback(
    async (restrict: boolean) => {
      if (!session) return;
      const previous = session.restrict_tools_to_working_dirs ?? false;
      onSessionChange((current) => ({ ...current, restrict_tools_to_working_dirs: restrict }));
      try {
        await acpSetWorkingDirRestriction(session.id, restrict);
      } catch (error) {
        console.error('[SessionInfoSummary] Failed to update restriction:', error);
        toast.error('Failed to update working directory restriction');
        onSessionChange((current) => ({
          ...current,
          restrict_tools_to_working_dirs: previous,
        }));
      }
    },
    [session, onSessionChange]
  );

  if (!session) return null;

  const workspaceLabel = session.workspace_name?.trim() || 'Unassigned workspace';
  const accumulatedInputTokens = session.accumulated_usage?.input_tokens;
  const accumulatedOutputTokens = session.accumulated_usage?.output_tokens;
  const computedAccumulatedTokens =
    accumulatedInputTokens == null && accumulatedOutputTokens == null
      ? null
      : (accumulatedInputTokens ?? 0) + (accumulatedOutputTokens ?? 0);
  const totalTokens =
    liveTotalTokens ??
    session.usage?.total_tokens ??
    session.accumulated_usage?.total_tokens ??
    computedAccumulatedTokens;
  const accumulatedCost = liveAccumulatedCost ?? session.accumulated_cost;

  return (
    <section
      aria-label="Chat information"
      className="w-[340px] max-w-[calc(100vw-2rem)] max-h-[min(34rem,calc(100vh-7rem))] overflow-y-auto rounded-2xl border border-border-primary bg-background-secondary p-3 shadow-xl"
    >
      <div className="mb-3 flex items-start justify-between gap-3 border-b border-border-primary pb-2">
        <div className="min-w-0">
          <div className="text-sm font-semibold text-text-primary">Chat info</div>
          <div className="truncate text-xs text-text-secondary" title={session.name}>
            {session.name}
          </div>
        </div>
        <button
          type="button"
          aria-label="Collapse chat information"
          className="shrink-0 rounded-md p-1 text-text-secondary transition-colors hover:bg-background-tertiary hover:text-text-primary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-border-active"
          onClick={onCollapse}
        >
          <X className="size-3.5" />
        </button>
      </div>

      <div className="space-y-3">
        <InfoRow label="Workspace" value={workspaceLabel} />

        <div className="flex items-center justify-between gap-3">
          <span className="text-xs text-text-secondary">Credential</span>
          <CredentialProfileSelector
            credentialProfileId={session.credential_profile_id}
            credentialProfileName={session.credential_profile_name}
            surface="header"
          />
        </div>

        <div>
          <div className="mb-1.5 flex items-center justify-between gap-3">
            <div>
              <div className="text-xs text-text-secondary">Working directories</div>
              <div className="text-[11px] text-text-secondary">
                {directories.length} {directories.length === 1 ? 'directory' : 'directories'}
              </div>
            </div>
            <WorkingDirectoriesMenu
              session={session}
              onSessionChange={onSessionChange}
              className="rounded-full border border-border-primary bg-background-primary px-2 py-1 text-text-primary hover:bg-background-tertiary"
              compact
              showCount={false}
            />
          </div>
          <div className="max-h-32 space-y-1 overflow-y-auto">
            {directories.map((dir, index) => (
              <div
                key={dir}
                className="flex min-w-0 items-center gap-2 rounded-lg bg-background-primary/70 px-2 py-1.5"
                title={dir}
              >
                <Folder className="size-3.5 shrink-0 text-text-secondary" />
                <span className="min-w-0 flex-1 truncate text-xs text-text-primary">
                  {directoryLabel(dir)}
                </span>
                <span className="shrink-0 text-[10px] uppercase tracking-wide text-text-secondary">
                  {index === 0 ? 'Primary' : 'Additional'}
                </span>
              </div>
            ))}
          </div>
        </div>

        <div className="grid grid-cols-2 gap-x-4 gap-y-2 border-t border-border-primary pt-3">
          <Telemetry label="Provider" value={session.provider_name || 'Not available'} />
          <Telemetry label="Model" value={session.model_config?.model_name || 'Not available'} />
          <Telemetry label="Mode" value={modeLabel(session.gosling_mode)} />
          <Telemetry label="Messages" value={formatCount(session.message_count)} />
          <Telemetry label="Tokens" value={formatCount(totalTokens)} />
          <Telemetry label="Cost" value={formatCost(accumulatedCost)} />
        </div>

        <div className="flex items-start gap-2 rounded-lg bg-background-primary/70 px-2 py-2 text-[11px] text-text-secondary">
          <ShieldAlert className="mt-0.5 size-3.5 shrink-0" />
          <div className="flex flex-1 items-center justify-between gap-2">
            <span>
              {session.restrict_tools_to_working_dirs
                ? 'Tools are restricted to the listed directories. Providers that run their own tools (Claude Code CLI, Codex CLI, …) are blocked while this is on.'
                : session.workspace_id
                  ? 'Workspace folder policy is enforced, including read-only folders and approval for out-of-scope mutations.'
                  : 'Tools are not restricted to the listed directories.'}
            </span>
            <Switch
              checked={session.restrict_tools_to_working_dirs ?? false}
              onCheckedChange={(checked) => void toggleRestriction(checked)}
            />
          </div>
        </div>
      </div>
    </section>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-xs text-text-secondary">{label}</span>
      <span className="min-w-0 truncate text-xs font-medium text-text-primary" title={value}>
        {value}
      </span>
    </div>
  );
}

function Telemetry({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <div className="text-[10px] uppercase tracking-wide text-text-secondary">{label}</div>
      <div className="truncate text-xs text-text-primary" title={value}>
        {value}
      </div>
    </div>
  );
}
