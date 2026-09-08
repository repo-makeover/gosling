import type { RequestPermissionRequest, RequestPermissionResponse } from '@agentclientprotocol/sdk';
import type { Permission } from '../types/permissions';
import { acpChatSessionActions, acpPermissionUserInputRequestId } from './chatSessionStore';

interface PendingPermissionRequest {
  request: RequestPermissionRequest;
  resolve: (response: RequestPermissionResponse) => void;
}

const pendingRequests = new Map<string, PendingPermissionRequest>();
const requestGenerations = new Map<string, number>();
const listeners = new Set<() => void>();
let nextGeneration = 0;

export function subscribeAcpPermissionRequests(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function acpPermissionRequestIdentity(sessionId: string, toolCallId: string): string {
  const key = permissionRequestKey(sessionId, toolCallId);
  return JSON.stringify([sessionId, toolCallId, requestGenerations.get(key) ?? 0]);
}

function notifyPermissionRequests(): void {
  for (const listener of listeners) listener();
}

export async function requestAcpPermission(
  request: RequestPermissionRequest
): Promise<RequestPermissionResponse> {
  const key = permissionRequestKey(request.sessionId, request.toolCall.toolCallId);
  const previous = pendingRequests.get(key);
  if (previous) {
    previous.resolve(cancelledPermissionResponse());
  }

  return new Promise<RequestPermissionResponse>((resolve) => {
    pendingRequests.set(key, { request, resolve });
    requestGenerations.delete(key);
    requestGenerations.set(key, ++nextGeneration);
    for (const oldKey of requestGenerations.keys()) {
      if (requestGenerations.size <= 500) break;
      if (!pendingRequests.has(oldKey)) requestGenerations.delete(oldKey);
    }
    acpChatSessionActions.applyPermissionRequest(request);
    notifyPermissionRequests();
  });
}

// Non-consuming check for whether a permission request is still pending, so
// a caller can validate liveness before an irreversible side effect (e.g. a
// bulk backend permission mutation) rather than only discovering staleness
// after the side effect already happened.
export function isAcpPermissionRequestPending(
  sessionId: string,
  toolCallId: string,
  expectedIdentity?: string
): boolean {
  return (
    pendingRequests.has(permissionRequestKey(sessionId, toolCallId)) &&
    (expectedIdentity === undefined ||
      expectedIdentity === acpPermissionRequestIdentity(sessionId, toolCallId))
  );
}

export function resolveAcpPermissionRequest(
  sessionId: string,
  toolCallId: string,
  action: Permission,
  expectedIdentity?: string
): boolean {
  const key = permissionRequestKey(sessionId, toolCallId);
  const pending = pendingRequests.get(key);
  if (!pending || !isAcpPermissionRequestPending(sessionId, toolCallId, expectedIdentity)) {
    return false;
  }
  if (action !== 'cancel' && !permissionOptionIdForAction(pending.request, action)) {
    return false;
  }

  pendingRequests.delete(key);
  acpChatSessionActions.resolveUserInputRequest(
    sessionId,
    acpPermissionUserInputRequestId(toolCallId)
  );
  pending.resolve(permissionResponseForAction(pending.request, action));
  notifyPermissionRequests();
  return true;
}

export function cancelAcpPermissionRequestsForSession(sessionId: string): void {
  for (const [key, pending] of pendingRequests) {
    if (pending.request.sessionId === sessionId) {
      pendingRequests.delete(key);
      pending.resolve(cancelledPermissionResponse());
    }
  }
  notifyPermissionRequests();
}

function permissionResponseForAction(
  request: RequestPermissionRequest,
  action: Permission
): RequestPermissionResponse {
  if (action === 'cancel') {
    return cancelledPermissionResponse();
  }

  const optionId = permissionOptionIdForAction(request, action);
  if (!optionId) {
    return cancelledPermissionResponse();
  }

  return {
    outcome: {
      outcome: 'selected',
      optionId,
    },
  };
}

function permissionOptionIdForAction(
  request: RequestPermissionRequest,
  action: Permission
): string | undefined {
  // The domain-scoped option shares `allow_always`'s kind with the tool-wide
  // one (ACP has no domain-scoped kind), so it can only be told apart by its
  // distinct option id rather than by kind.
  if (action === 'always_allow_domain') {
    return request.options.find((candidate) => candidate.optionId === 'allow_always_domain')
      ?.optionId;
  }

  const kind = permissionOptionKindForAction(action);
  if (!kind) {
    return undefined;
  }

  return request.options.find(
    (candidate) => candidate.kind === kind && candidate.optionId !== 'allow_always_domain'
  )?.optionId;
}

function permissionOptionKindForAction(action: Permission) {
  switch (action) {
    case 'allow_once':
      return 'allow_once';
    case 'always_allow':
      return 'allow_always';
    case 'deny_once':
      return 'reject_once';
    case 'always_deny':
      return 'reject_always';
    case 'always_allow_domain':
    case 'cancel':
      return undefined;
  }
}

function cancelledPermissionResponse(): RequestPermissionResponse {
  return {
    outcome: {
      outcome: 'cancelled',
    },
  };
}

function permissionRequestKey(sessionId: string, toolCallId: string): string {
  return JSON.stringify([sessionId, toolCallId]);
}
