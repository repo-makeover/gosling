import { useState, useSyncExternalStore } from 'react';
import { Button } from './ui/button';
import type { Permission } from '../types/permissions';
import {
  isAcpPermissionRequestPending,
  resolveAcpPermissionRequest,
  acpPermissionRequestIdentity,
  subscribeAcpPermissionRequests,
} from '../acp/permissionRequests';
import { listTools, setToolPermissions } from '../acp/permissions';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  allowOnce: {
    id: 'toolApprovalButtons.allowOnce',
    defaultMessage: 'Allow Once',
  },
  alwaysAllow: {
    id: 'toolApprovalButtons.alwaysAllow',
    defaultMessage: 'Always Allow',
  },
  alwaysAllowExtension: {
    id: 'toolApprovalButtons.alwaysAllowExtension',
    defaultMessage: 'Always Allow all {extensionName} tools',
  },
  alwaysAllowDomain: {
    id: 'toolApprovalButtons.alwaysAllowDomain',
    defaultMessage: 'Always allow {domain}',
  },
  deny: {
    id: 'toolApprovalButtons.deny',
    defaultMessage: 'Deny',
  },
  allowedOnce: {
    id: 'toolApprovalButtons.allowedOnce',
    defaultMessage: 'Allowed once',
  },
  alwaysAllowRequested: {
    id: 'toolApprovalButtons.alwaysAllowRequested',
    defaultMessage: 'Always Allow requested',
  },
  alwaysAllowedExtension: {
    id: 'toolApprovalButtons.alwaysAllowedExtension',
    defaultMessage: 'Always allowed ({extensionName} tools)',
  },
  alwaysAllowDomainRequested: {
    id: 'toolApprovalButtons.alwaysAllowDomainRequested',
    defaultMessage: 'Always allow {domain} requested',
  },
  denied: {
    id: 'toolApprovalButtons.denied',
    defaultMessage: 'Denied',
  },
  deniedOnce: {
    id: 'toolApprovalButtons.deniedOnce',
    defaultMessage: 'Denied once',
  },
  cancelled: {
    id: 'toolApprovalButtons.cancelled',
    defaultMessage: 'Cancelled',
  },
  staleApprovalRequest: {
    id: 'toolApprovalButtons.staleApprovalRequest',
    defaultMessage: 'This approval request is no longer active.',
  },
  failedToAllowExtension: {
    id: 'toolApprovalButtons.failedToAllowExtension',
    defaultMessage: 'Failed to update permissions for this extension',
  },
  failedToSubmitDecision: {
    id: 'toolApprovalButtons.failedToSubmitDecision',
    defaultMessage: 'Could not send your decision. The tool is still waiting for approval.',
  },
});

function extensionNameFromToolName(toolName: string): string | undefined {
  const [extensionName, ...rest] = toolName.split('__');
  return rest.length > 0 && extensionName ? extensionName : undefined;
}

const globalApprovalState = new Map<
  string,
  {
    decision: Permission | null;
    isClicked: boolean;
    bulkAllowedExtension?: string;
  }
>();

// The map outlives sessions so decisions survive remounts, but agents issue
// many approvals per session — without a cap it grows for the window's
// lifetime. Oldest entries belong to long-resolved requests, so evict those.
const MAX_APPROVAL_STATES = 500;

function recordApprovalState(
  id: string,
  state: { decision: Permission | null; isClicked: boolean; bulkAllowedExtension?: string }
) {
  if (!globalApprovalState.has(id) && globalApprovalState.size >= MAX_APPROVAL_STATES) {
    const oldest = globalApprovalState.keys().next().value;
    if (oldest !== undefined) {
      globalApprovalState.delete(oldest);
    }
  }
  globalApprovalState.set(id, state);
}

export interface ToolApprovalData {
  id: string;
  toolName: string;
  prompt?: string;
  domain?: string;
  sessionId: string;
  isClicked?: boolean;
}

export default function ToolApprovalButtons({ data }: { data: ToolApprovalData }) {
  const requestIdentity = useSyncExternalStore(subscribeAcpPermissionRequests, () =>
    acpPermissionRequestIdentity(data.sessionId, data.id)
  );
  return (
    <ApprovalRequestButtons key={requestIdentity} data={data} requestIdentity={requestIdentity} />
  );
}

function ApprovalRequestButtons({
  data,
  requestIdentity,
}: {
  data: ToolApprovalData;
  requestIdentity: string;
}) {
  const intl = useIntl();
  const { id, toolName, prompt, domain, sessionId, isClicked: initialIsClicked } = data;

  const storedState = globalApprovalState.get(requestIdentity);
  const [decision, setDecision] = useState<Permission | null>(storedState?.decision ?? null);
  const [isClicked, setIsClicked] = useState(storedState?.isClicked ?? initialIsClicked ?? false);
  const [approvalError, setApprovalError] = useState<string | null>(null);
  const [isAllowingExtension, setIsAllowingExtension] = useState(false);
  const [bulkAllowedExtension, setBulkAllowedExtension] = useState<string | null>(
    storedState?.bulkAllowedExtension ?? null
  );

  const extensionName = extensionNameFromToolName(toolName);

  const setResolvedDecision = (action: Permission, extension?: string) => {
    recordApprovalState(requestIdentity, {
      decision: action,
      isClicked: true,
      bulkAllowedExtension: extension,
    });
    setDecision(action);
    setIsClicked(true);
    setApprovalError(null);
  };

  const handleAction = async (action: Permission) => {
    try {
      if (resolveAcpPermissionRequest(sessionId, id, action, requestIdentity)) {
        setResolvedDecision(action);
      } else {
        setApprovalError(intl.formatMessage(i18n.staleApprovalRequest));
      }
    } catch (err) {
      // Only the stale path surfaced anything; a thrown error left the
      // buttons looking dead with the tool still blocked. (WFG-GOS-004)
      console.error('Error confirming tool action:', err);
      setApprovalError(intl.formatMessage(i18n.failedToSubmitDecision));
    }
  };

  const handleAlwaysAllowExtension = async () => {
    if (!extensionName) {
      await handleAction('always_allow');
      return;
    }

    if (!isAcpPermissionRequestPending(sessionId, id, requestIdentity)) {
      setApprovalError(intl.formatMessage(i18n.staleApprovalRequest));
      return;
    }

    setIsAllowingExtension(true);
    try {
      const tools = await listTools(sessionId, extensionName);
      const toolPermissions = (tools.length > 0 ? tools.map((t) => t.name) : [toolName]).map(
        (name) => ({ toolName: name, permission: 'always_allow' as const })
      );
      if (!isAcpPermissionRequestPending(sessionId, id, requestIdentity)) {
        setApprovalError(intl.formatMessage(i18n.staleApprovalRequest));
        return;
      }
      await setToolPermissions(toolPermissions);

      if (!resolveAcpPermissionRequest(sessionId, id, 'always_allow', requestIdentity)) {
        setApprovalError(intl.formatMessage(i18n.staleApprovalRequest));
        return;
      }

      setBulkAllowedExtension(extensionName);
      setResolvedDecision('always_allow', extensionName);
    } catch (err) {
      console.error('Error allowing extension tools:', err);
      setApprovalError(intl.formatMessage(i18n.failedToAllowExtension));
    } finally {
      setIsAllowingExtension(false);
    }
  };

  if (isClicked && decision) {
    const statusMessages: Record<Permission, string> = {
      allow_once: intl.formatMessage(i18n.allowedOnce),
      always_allow:
        bulkAllowedExtension && decision === 'always_allow'
          ? intl.formatMessage(i18n.alwaysAllowedExtension, {
              extensionName: bulkAllowedExtension,
            })
          : intl.formatMessage(i18n.alwaysAllowRequested),
      always_allow_domain: intl.formatMessage(i18n.alwaysAllowDomainRequested, {
        domain: domain ?? '',
      }),
      always_deny: intl.formatMessage(i18n.denied),
      deny_once: intl.formatMessage(i18n.deniedOnce),
      cancel: intl.formatMessage(i18n.cancelled),
    };
    return (
      <p className="text-sm text-muted-foreground mt-2">
        {toolName} - {statusMessages[decision]}
      </p>
    );
  }

  return (
    <>
      {/*
        Visual weight follows consequence (WEB-GOS-001). Previously all three
        affirmative buttons were `secondary` — so "Always Allow", which outlives
        this call, looked identical to the single-call "Allow Once" — while
        Deny was `outline`, the faintest control on the row. The persistent
        grants are now de-emphasized and grouped after Deny, so the two
        one-shot decisions read as the default pair and a lasting grant takes a
        deliberate look to find.
      */}
      <div className="flex items-center gap-2 mt-2 flex-wrap">
        <Button
          className="rounded-full"
          variant="secondary"
          onClick={() => handleAction('allow_once')}
        >
          {intl.formatMessage(i18n.allowOnce)}
        </Button>
        <Button
          className="rounded-full"
          variant="secondary"
          onClick={() => handleAction('deny_once')}
        >
          {intl.formatMessage(i18n.deny)}
        </Button>
        {!prompt && (
          <Button
            className="rounded-full"
            variant="ghost"
            onClick={() => handleAction('always_allow')}
          >
            {intl.formatMessage(i18n.alwaysAllow)}
          </Button>
        )}
        {prompt && domain && (
          <Button
            className="rounded-full"
            variant="ghost"
            onClick={() => handleAction('always_allow_domain')}
          >
            {intl.formatMessage(i18n.alwaysAllowDomain, { domain })}
          </Button>
        )}
        {!prompt && extensionName && (
          <Button
            className="rounded-full"
            variant="ghost"
            disabled={isAllowingExtension}
            onClick={() => void handleAlwaysAllowExtension()}
          >
            {intl.formatMessage(i18n.alwaysAllowExtension, { extensionName })}
          </Button>
        )}
      </div>
      {approvalError && (
        <p className="text-sm text-red-500 mt-2" role="alert">
          {approvalError}
        </p>
      )}
    </>
  );
}
