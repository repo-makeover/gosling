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

// This display history survives remounts; it does not establish saved permissions.
const resolvedApprovalStates = new Map<
  string,
  {
    decision: Permission | null;
    isClicked: boolean;
    bulkAllowedExtension?: string;
  }
>();

// Bound history for the window's lifetime without evicting pending requests.
const MAX_APPROVAL_STATES = 500;

function rememberResolvedApproval(
  requestIdentity: string,
  state: { decision: Permission | null; isClicked: boolean; bulkAllowedExtension?: string }
) {
  if (
    !resolvedApprovalStates.has(requestIdentity) &&
    resolvedApprovalStates.size >= MAX_APPROVAL_STATES
  ) {
    const oldestRequestIdentity = resolvedApprovalStates.keys().next().value;
    if (oldestRequestIdentity !== undefined) {
      resolvedApprovalStates.delete(oldestRequestIdentity);
    }
  }
  resolvedApprovalStates.set(requestIdentity, state);
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
  // Reused tool-call IDs must start with fresh button state for each request generation.
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

  const storedState = resolvedApprovalStates.get(requestIdentity);
  const [decision, setDecision] = useState<Permission | null>(storedState?.decision ?? null);
  const [isClicked, setIsClicked] = useState(storedState?.isClicked ?? initialIsClicked ?? false);
  const [approvalError, setApprovalError] = useState<string | null>(null);
  const [isAllowingExtension, setIsAllowingExtension] = useState(false);
  const [bulkAllowedExtension, setBulkAllowedExtension] = useState<string | null>(
    storedState?.bulkAllowedExtension ?? null
  );

  const extensionName = extensionNameFromToolName(toolName);

  const setResolvedDecision = (action: Permission, extension?: string) => {
    rememberResolvedApproval(requestIdentity, {
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
      // Delivery failures need visible feedback while the tool may still be waiting. (WFG-GOS-004)
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
      const extensionToolNames = tools.length > 0 ? tools.map((tool) => tool.name) : [toolName];
      const toolPermissions = extensionToolNames.map((name) => ({
        toolName: name,
        permission: 'always_allow' as const,
      }));
      // Tool discovery can outlive a request; recheck before persisting grants.
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
        Keep one-time decisions prominent and persistent grants secondary
        because those grants outlive this request. (WEB-GOS-001)
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
