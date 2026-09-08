// This file is auto-generated — do not edit manually.

export interface ExtMethodProvider {
  extMethod(
    method: string,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>>;
}

import type { Client } from "@agentclientprotocol/sdk";
import type {
  AddConfigExtensionRequest_unstable,
  AddSessionExtensionRequest_unstable,
  AddSessionWorkingDirRequest_unstable,
  ArchiveSessionRequest_unstable,
  CanonicalModelInfoRequest_unstable,
  CanonicalModelInfoResponse_unstable,
  ConfigReadAllRequest_unstable,
  ConfigReadAllResponse_unstable,
  ConfigReadRequest_unstable,
  ConfigReadResponse_unstable,
  ConfigRemoveRequest_unstable,
  ConfigUpsertRequest_unstable,
  CreateSourceRequest_unstable,
  CreateSourceResponse_unstable,
  CredentialProfileCreateRequest_unstable,
  CredentialProfileDeleteRequest_unstable,
  CredentialProfileDeleteResponse_unstable,
  CredentialProfileListRequest_unstable,
  CredentialProfileListResponse_unstable,
  CredentialProfileResponse_unstable,
  CredentialProfileTestRequest_unstable,
  CredentialProfileTestResponse_unstable,
  CredentialProfileUpdateRequest_unstable,
  CredentialProfileUsageRequest_unstable,
  CredentialProfileUsageResponse_unstable,
  CustomProviderCreateRequest_unstable,
  CustomProviderCreateResponse_unstable,
  CustomProviderDeleteRequest_unstable,
  CustomProviderDeleteResponse_unstable,
  CustomProviderReadRequest_unstable,
  CustomProviderReadResponse_unstable,
  CustomProviderUpdateRequest_unstable,
  CustomProviderUpdateResponse_unstable,
  DefaultsClearRequest_unstable,
  DefaultsReadRequest_unstable,
  DefaultsReadResponse_unstable,
  DefaultsSaveRequest_unstable,
  DeleteSessionRequest,
  DeleteSourceRequest_unstable,
  DiagnosticsGetRequest_unstable,
  DiagnosticsGetResponse_unstable,
  DictationConfigRequest_unstable,
  DictationConfigResponse_unstable,
  DictationModelSelectRequest_unstable,
  DictationSecretDeleteRequest_unstable,
  DictationSecretSaveRequest_unstable,
  DictationTranscribeRequest_unstable,
  DictationTranscribeResponse_unstable,
  DomainActionConfirmRequest_unstable,
  DomainActionConfirmResponse_unstable,
  DomainActionRequest_unstable,
  DomainActionResponse_unstable,
  DomainSnapshotRequest_unstable,
  DomainSnapshotResponse_unstable,
  DomainStatusNotification_unstable,
  ExportSessionRequest_unstable,
  ExportSessionResponse_unstable,
  ExportSourceRequest_unstable,
  ExportSourceResponse_unstable,
  GetAvailableExtensionsRequest_unstable,
  GetAvailableExtensionsResponse_unstable,
  GetConfigExtensionsRequest_unstable,
  GetConfigExtensionsResponse_unstable,
  GetOutputRevisionRequest_unstable,
  GetOutputRevisionResponse_unstable,
  GetPromptRequest_unstable,
  GetPromptResponse_unstable,
  GetSessionExtensionsRequest_unstable,
  GetSessionExtensionsResponse_unstable,
  GetSessionInfoRequest_unstable,
  GetSessionInfoResponse_unstable,
  GetSessionSummaryRequest_unstable,
  GetSessionSummaryResponse_unstable,
  GetToolsRequest_unstable,
  GetToolsResponse_unstable,
  GoslingSessionNotification_unstable,
  GoslingToolCallRequest_unstable,
  GoslingToolCallResponse_unstable,
  ImportSessionRequest_unstable,
  ImportSessionResponse_unstable,
  ImportSourcesRequest_unstable,
  ImportSourcesResponse_unstable,
  ListAgentMentionsRequest_unstable,
  ListAgentMentionsResponse_unstable,
  ListOutputRevisionsRequest_unstable,
  ListOutputRevisionsResponse_unstable,
  ListPromptsRequest_unstable,
  ListPromptsResponse_unstable,
  ListProvidersRequest_unstable,
  ListProvidersResponse_unstable,
  ListSessionArtifactsRequest_unstable,
  ListSessionArtifactsResponse_unstable,
  ListSessionMessagesRequest_unstable,
  ListSessionMessagesResponse_unstable,
  ListSlashCommandsRequest_unstable,
  ListSlashCommandsResponse_unstable,
  ListSourcesRequest_unstable,
  ListSourcesResponse_unstable,
  OnboardingImportApplyRequest_unstable,
  OnboardingImportApplyResponse_unstable,
  OnboardingImportScanRequest_unstable,
  OnboardingImportScanResponse_unstable,
  PreferencesReadRequest_unstable,
  PreferencesReadResponse_unstable,
  PreferencesRemoveRequest_unstable,
  PreferencesSaveRequest_unstable,
  PromptOperationResponse_unstable,
  ProviderCatalogListRequest_unstable,
  ProviderCatalogListResponse_unstable,
  ProviderCatalogTemplateRequest_unstable,
  ProviderCatalogTemplateResponse_unstable,
  ProviderConfigAuthenticateRequest_unstable,
  ProviderConfigChangeResponse_unstable,
  ProviderConfigDeleteRequest_unstable,
  ProviderConfigReadRequest_unstable,
  ProviderConfigReadResponse_unstable,
  ProviderConfigSaveRequest_unstable,
  ProviderConfigStatusRequest_unstable,
  ProviderConfigStatusResponse_unstable,
  ProviderSecretCustomAddRequest_unstable,
  ProviderSecretDeleteRequest_unstable,
  ProviderSecretsListRequest_unstable,
  ProviderSecretsListResponse_unstable,
  ProviderSetupCatalogListRequest_unstable,
  ProviderSetupCatalogListResponse_unstable,
  ProviderSupportedModelsListRequest_unstable,
  ProviderSupportedModelsListResponse_unstable,
  ReadResourceRequest_unstable,
  ReadResourceResponse_unstable,
  RecordSessionModelSwitchRequest_unstable,
  RecordSessionModelSwitchResponse_unstable,
  RefreshProviderInventoryRequest_unstable,
  RefreshProviderInventoryResponse_unstable,
  RemoveConfigExtensionRequest_unstable,
  RemoveSessionExtensionRequest_unstable,
  RemoveSessionWorkingDirRequest_unstable,
  RenameSessionRequest_unstable,
  ResetPromptRequest_unstable,
  RestoreOutputRevisionRequest_unstable,
  RestoreOutputRevisionResponse_unstable,
  SavePromptRequest_unstable,
  SearchSessionMessagesRequest_unstable,
  SearchSessionMessagesResponse_unstable,
  SessionWorkingDirsResponse_unstable,
  SetConfigExtensionEnabledRequest_unstable,
  SetSessionSystemPromptRequest_unstable,
  SetSessionWorkingDirRestrictionRequest_unstable,
  SetToolPermissionsRequest_unstable,
  SetToolPermissionsResponse_unstable,
  ShareSessionNostrRequest_unstable,
  ShareSessionNostrResponse_unstable,
  ShellArtifactListRequest_unstable,
  ShellArtifactListResponse_unstable,
  ShellCredentialListRequest_unstable,
  ShellCredentialListResponse_unstable,
  ShellDirectoryValidateRequest_unstable,
  ShellDirectoryValidateResponse_unstable,
  ShellHandoffPrepareRequest_unstable,
  ShellHandoffPrepareResponse_unstable,
  ShellLibraryAddImageRequest_unstable,
  ShellLibraryAddResponse_unstable,
  ShellLibraryAddTextRequest_unstable,
  ShellLibraryLinkFileRequest_unstable,
  ShellLibraryListRequest_unstable,
  ShellLibraryListResponse_unstable,
  ShellLibraryRemoveRequest_unstable,
  ShellLibraryRemoveResponse_unstable,
  ShellLibraryResolveRequest_unstable,
  ShellLibraryResolveResponse_unstable,
  ShellModuleListRequest_unstable,
  ShellModuleListResponse_unstable,
  ShellProvisioningReadRequest_unstable,
  ShellProvisioningReadResponse_unstable,
  ShellProvisioningValidateRequest_unstable,
  ShellProvisioningValidateResponse_unstable,
  SteerSessionRequest_unstable,
  SteerSessionResponse_unstable,
  SummarizerModelsListRequest_unstable,
  SummarizerModelsListResponse_unstable,
  TruncateSessionConversationRequest_unstable,
  UnarchiveSessionRequest_unstable,
  UpdateSessionProjectRequest_unstable,
  UpdateSourceRequest_unstable,
  UpdateSourceResponse_unstable,
  UpdateWorkingDirRequest_unstable,
  WorkspaceCreateOutputFolderRequest_unstable,
  WorkspaceCreateRequest_unstable,
  WorkspaceDeleteRequest_unstable,
  WorkspaceDeleteResponse_unstable,
  WorkspaceDuplicateRequest_unstable,
  WorkspaceExportRequest_unstable,
  WorkspaceExportResponse_unstable,
  WorkspaceImportRequest_unstable,
  WorkspaceListRequest_unstable,
  WorkspaceListResponse_unstable,
  WorkspaceResponse_unstable,
  WorkspaceSetActiveRequest_unstable,
  WorkspaceUpdateRequest_unstable,
  WorkspaceValidateRequest_unstable,
  WorkspaceValidationResponse_unstable,
} from './types.gen.js';
import {
  zCanonicalModelInfoResponse_unstable,
  zConfigReadAllResponse_unstable,
  zConfigReadResponse_unstable,
  zCreateSourceResponse_unstable,
  zCredentialProfileDeleteResponse_unstable,
  zCredentialProfileListResponse_unstable,
  zCredentialProfileResponse_unstable,
  zCredentialProfileTestResponse_unstable,
  zCredentialProfileUsageResponse_unstable,
  zCustomProviderCreateResponse_unstable,
  zCustomProviderDeleteResponse_unstable,
  zCustomProviderReadResponse_unstable,
  zCustomProviderUpdateResponse_unstable,
  zDefaultsReadResponse_unstable,
  zDiagnosticsGetResponse_unstable,
  zDictationConfigResponse_unstable,
  zDictationTranscribeResponse_unstable,
  zDomainActionConfirmResponse_unstable,
  zDomainActionResponse_unstable,
  zDomainSnapshotResponse_unstable,
  zDomainStatusNotification_unstable,
  zExportSessionResponse_unstable,
  zExportSourceResponse_unstable,
  zGetAvailableExtensionsResponse_unstable,
  zGetConfigExtensionsResponse_unstable,
  zGetOutputRevisionResponse_unstable,
  zGetPromptResponse_unstable,
  zGetSessionExtensionsResponse_unstable,
  zGetSessionInfoResponse_unstable,
  zGetSessionSummaryResponse_unstable,
  zGetToolsResponse_unstable,
  zGoslingSessionNotification_unstable,
  zGoslingToolCallResponse_unstable,
  zImportSessionResponse_unstable,
  zImportSourcesResponse_unstable,
  zListAgentMentionsResponse_unstable,
  zListOutputRevisionsResponse_unstable,
  zListPromptsResponse_unstable,
  zListProvidersResponse_unstable,
  zListSessionArtifactsResponse_unstable,
  zListSessionMessagesResponse_unstable,
  zListSlashCommandsResponse_unstable,
  zListSourcesResponse_unstable,
  zOnboardingImportApplyResponse_unstable,
  zOnboardingImportScanResponse_unstable,
  zPreferencesReadResponse_unstable,
  zPromptOperationResponse_unstable,
  zProviderCatalogListResponse_unstable,
  zProviderCatalogTemplateResponse_unstable,
  zProviderConfigChangeResponse_unstable,
  zProviderConfigReadResponse_unstable,
  zProviderConfigStatusResponse_unstable,
  zProviderSecretsListResponse_unstable,
  zProviderSetupCatalogListResponse_unstable,
  zProviderSupportedModelsListResponse_unstable,
  zReadResourceResponse_unstable,
  zRecordSessionModelSwitchResponse_unstable,
  zRefreshProviderInventoryResponse_unstable,
  zRestoreOutputRevisionResponse_unstable,
  zSearchSessionMessagesResponse_unstable,
  zSessionWorkingDirsResponse_unstable,
  zSetToolPermissionsResponse_unstable,
  zShareSessionNostrResponse_unstable,
  zShellArtifactListResponse_unstable,
  zShellCredentialListResponse_unstable,
  zShellDirectoryValidateResponse_unstable,
  zShellHandoffPrepareResponse_unstable,
  zShellLibraryAddResponse_unstable,
  zShellLibraryListResponse_unstable,
  zShellLibraryRemoveResponse_unstable,
  zShellLibraryResolveResponse_unstable,
  zShellModuleListResponse_unstable,
  zShellProvisioningReadResponse_unstable,
  zShellProvisioningValidateResponse_unstable,
  zSteerSessionResponse_unstable,
  zSummarizerModelsListResponse_unstable,
  zUpdateSourceResponse_unstable,
  zWorkspaceDeleteResponse_unstable,
  zWorkspaceExportResponse_unstable,
  zWorkspaceListResponse_unstable,
  zWorkspaceResponse_unstable,
  zWorkspaceValidationResponse_unstable,
} from './zod.gen.js';

export class GoslingExtClient {
  constructor(private conn: ExtMethodProvider) {}

  async shellProvisioningRead_unstable(
    params: ShellProvisioningReadRequest_unstable,
  ): Promise<ShellProvisioningReadResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/provisioning/read",
      params,
    );
    return zShellProvisioningReadResponse_unstable.parse(
      raw,
    ) as ShellProvisioningReadResponse_unstable;
  }

  async shellProvisioningValidate_unstable(
    params: ShellProvisioningValidateRequest_unstable,
  ): Promise<ShellProvisioningValidateResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/provisioning/validate",
      params,
    );
    return zShellProvisioningValidateResponse_unstable.parse(
      raw,
    ) as ShellProvisioningValidateResponse_unstable;
  }

  async shellDirectoryValidate_unstable(
    params: ShellDirectoryValidateRequest_unstable,
  ): Promise<ShellDirectoryValidateResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/directory/validate",
      params,
    );
    return zShellDirectoryValidateResponse_unstable.parse(
      raw,
    ) as ShellDirectoryValidateResponse_unstable;
  }

  async shellCredentialsList_unstable(
    params: ShellCredentialListRequest_unstable,
  ): Promise<ShellCredentialListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/credentials/list",
      params,
    );
    return zShellCredentialListResponse_unstable.parse(
      raw,
    ) as ShellCredentialListResponse_unstable;
  }

  async shellModulesList_unstable(
    params: ShellModuleListRequest_unstable,
  ): Promise<ShellModuleListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/modules/list",
      params,
    );
    return zShellModuleListResponse_unstable.parse(
      raw,
    ) as ShellModuleListResponse_unstable;
  }

  async shellSessionArtifactsList_unstable(
    params: ShellArtifactListRequest_unstable,
  ): Promise<ShellArtifactListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/session/artifacts/list",
      params,
    );
    return zShellArtifactListResponse_unstable.parse(
      raw,
    ) as ShellArtifactListResponse_unstable;
  }

  async shellSessionLibraryList_unstable(
    params: ShellLibraryListRequest_unstable,
  ): Promise<ShellLibraryListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/session/library/list",
      params,
    );
    return zShellLibraryListResponse_unstable.parse(
      raw,
    ) as ShellLibraryListResponse_unstable;
  }

  async shellSessionLibraryAddText_unstable(
    params: ShellLibraryAddTextRequest_unstable,
  ): Promise<ShellLibraryAddResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/session/library/add_text",
      params,
    );
    return zShellLibraryAddResponse_unstable.parse(
      raw,
    ) as ShellLibraryAddResponse_unstable;
  }

  async shellSessionLibraryAddImage_unstable(
    params: ShellLibraryAddImageRequest_unstable,
  ): Promise<ShellLibraryAddResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/session/library/add_image",
      params,
    );
    return zShellLibraryAddResponse_unstable.parse(
      raw,
    ) as ShellLibraryAddResponse_unstable;
  }

  async shellSessionLibraryLinkFile_unstable(
    params: ShellLibraryLinkFileRequest_unstable,
  ): Promise<ShellLibraryAddResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/session/library/link_file",
      params,
    );
    return zShellLibraryAddResponse_unstable.parse(
      raw,
    ) as ShellLibraryAddResponse_unstable;
  }

  async shellSessionLibraryRemove_unstable(
    params: ShellLibraryRemoveRequest_unstable,
  ): Promise<ShellLibraryRemoveResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/session/library/remove",
      params,
    );
    return zShellLibraryRemoveResponse_unstable.parse(
      raw,
    ) as ShellLibraryRemoveResponse_unstable;
  }

  async shellSessionLibraryResolve_unstable(
    params: ShellLibraryResolveRequest_unstable,
  ): Promise<ShellLibraryResolveResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/session/library/resolve",
      params,
    );
    return zShellLibraryResolveResponse_unstable.parse(
      raw,
    ) as ShellLibraryResolveResponse_unstable;
  }

  async shellDomainSnapshot_unstable(
    params: DomainSnapshotRequest_unstable,
  ): Promise<DomainSnapshotResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/domain/snapshot",
      params,
    );
    return zDomainSnapshotResponse_unstable.parse(
      raw,
    ) as DomainSnapshotResponse_unstable;
  }

  async shellDomainAction_unstable(
    params: DomainActionRequest_unstable,
  ): Promise<DomainActionResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/domain/action",
      params,
    );
    return zDomainActionResponse_unstable.parse(
      raw,
    ) as DomainActionResponse_unstable;
  }

  async shellDomainActionConfirm_unstable(
    params: DomainActionConfirmRequest_unstable,
  ): Promise<DomainActionConfirmResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/domain/action/confirm",
      params,
    );
    return zDomainActionConfirmResponse_unstable.parse(
      raw,
    ) as DomainActionConfirmResponse_unstable;
  }

  async shellHandoffPrepare_unstable(
    params: ShellHandoffPrepareRequest_unstable,
  ): Promise<ShellHandoffPrepareResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/shell/handoff/prepare",
      params,
    );
    return zShellHandoffPrepareResponse_unstable.parse(
      raw,
    ) as ShellHandoffPrepareResponse_unstable;
  }

  async sessionExtensionsAdd_unstable(
    params: AddSessionExtensionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/session/extensions/add",
      params,
    );
  }

  async sessionExtensionsRemove_unstable(
    params: RemoveSessionExtensionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/session/extensions/remove",
      params,
    );
  }

  async toolsList_unstable(
    params: GetToolsRequest_unstable,
  ): Promise<GetToolsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/tools/list",
      params,
    );
    return zGetToolsResponse_unstable.parse(raw) as GetToolsResponse_unstable;
  }

  async toolsPermissionsSet_unstable(
    params: SetToolPermissionsRequest_unstable,
  ): Promise<SetToolPermissionsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/tools/permissions/set",
      params,
    );
    return zSetToolPermissionsResponse_unstable.parse(
      raw,
    ) as SetToolPermissionsResponse_unstable;
  }

  async toolsCall_unstable(
    params: GoslingToolCallRequest_unstable,
  ): Promise<GoslingToolCallResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/tools/call",
      params,
    );
    return zGoslingToolCallResponse_unstable.parse(
      raw,
    ) as GoslingToolCallResponse_unstable;
  }

  async resourcesRead_unstable(
    params: ReadResourceRequest_unstable,
  ): Promise<ReadResourceResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/resources/read",
      params,
    );
    return zReadResourceResponse_unstable.parse(
      raw,
    ) as ReadResourceResponse_unstable;
  }

  async sessionWorkingDirUpdate_unstable(
    params: UpdateWorkingDirRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/session/working-dir/update",
      params,
    );
  }

  async sessionWorkingDirsAdd_unstable(
    params: AddSessionWorkingDirRequest_unstable,
  ): Promise<SessionWorkingDirsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/working-dirs/add",
      params,
    );
    return zSessionWorkingDirsResponse_unstable.parse(
      raw,
    ) as SessionWorkingDirsResponse_unstable;
  }

  async sessionWorkingDirsRemove_unstable(
    params: RemoveSessionWorkingDirRequest_unstable,
  ): Promise<SessionWorkingDirsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/working-dirs/remove",
      params,
    );
    return zSessionWorkingDirsResponse_unstable.parse(
      raw,
    ) as SessionWorkingDirsResponse_unstable;
  }

  async sessionWorkingDirsRestrict_unstable(
    params: SetSessionWorkingDirRestrictionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/session/working-dirs/restrict",
      params,
    );
  }

  async sessionSystemPromptSet_unstable(
    params: SetSessionSystemPromptRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/session/system-prompt/set",
      params,
    );
  }

  async sessionSteer_unstable(
    params: SteerSessionRequest_unstable,
  ): Promise<SteerSessionResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/steer",
      params,
    );
    return zSteerSessionResponse_unstable.parse(
      raw,
    ) as SteerSessionResponse_unstable;
  }

  async diagnosticsGet_unstable(
    params: DiagnosticsGetRequest_unstable,
  ): Promise<DiagnosticsGetResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/diagnostics/get",
      params,
    );
    return zDiagnosticsGetResponse_unstable.parse(
      raw,
    ) as DiagnosticsGetResponse_unstable;
  }

  async configPromptsList_unstable(
    params: ListPromptsRequest_unstable,
  ): Promise<ListPromptsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/config/prompts/list",
      params,
    );
    return zListPromptsResponse_unstable.parse(
      raw,
    ) as ListPromptsResponse_unstable;
  }

  async configPromptsGet_unstable(
    params: GetPromptRequest_unstable,
  ): Promise<GetPromptResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/config/prompts/get",
      params,
    );
    return zGetPromptResponse_unstable.parse(raw) as GetPromptResponse_unstable;
  }

  async configPromptsSave_unstable(
    params: SavePromptRequest_unstable,
  ): Promise<PromptOperationResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/config/prompts/save",
      params,
    );
    return zPromptOperationResponse_unstable.parse(
      raw,
    ) as PromptOperationResponse_unstable;
  }

  async configPromptsReset_unstable(
    params: ResetPromptRequest_unstable,
  ): Promise<PromptOperationResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/config/prompts/reset",
      params,
    );
    return zPromptOperationResponse_unstable.parse(
      raw,
    ) as PromptOperationResponse_unstable;
  }

  async sessionDelete(params: DeleteSessionRequest): Promise<void> {
    await this.conn.extMethod("session/delete", params);
  }

  async configExtensionsList_unstable(
    params: GetConfigExtensionsRequest_unstable,
  ): Promise<GetConfigExtensionsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/config/extensions/list",
      params,
    );
    return zGetConfigExtensionsResponse_unstable.parse(
      raw,
    ) as GetConfigExtensionsResponse_unstable;
  }

  async extensionsAvailable_unstable(
    params: GetAvailableExtensionsRequest_unstable,
  ): Promise<GetAvailableExtensionsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/extensions/available",
      params,
    );
    return zGetAvailableExtensionsResponse_unstable.parse(
      raw,
    ) as GetAvailableExtensionsResponse_unstable;
  }

  async configExtensionsAdd_unstable(
    params: AddConfigExtensionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/config/extensions/add",
      params,
    );
  }

  async configExtensionsRemove_unstable(
    params: RemoveConfigExtensionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/config/extensions/remove",
      params,
    );
  }

  async configExtensionsSetEnabled_unstable(
    params: SetConfigExtensionEnabledRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/config/extensions/set-enabled",
      params,
    );
  }

  async sessionExtensionsList_unstable(
    params: GetSessionExtensionsRequest_unstable,
  ): Promise<GetSessionExtensionsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/extensions/list",
      params,
    );
    return zGetSessionExtensionsResponse_unstable.parse(
      raw,
    ) as GetSessionExtensionsResponse_unstable;
  }

  async providersList_unstable(
    params: ListProvidersRequest_unstable,
  ): Promise<ListProvidersResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/list",
      params,
    );
    return zListProvidersResponse_unstable.parse(
      raw,
    ) as ListProvidersResponse_unstable;
  }

  async providersSupportedModelsList_unstable(
    params: ProviderSupportedModelsListRequest_unstable,
  ): Promise<ProviderSupportedModelsListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/supported-models/list",
      params,
    );
    return zProviderSupportedModelsListResponse_unstable.parse(
      raw,
    ) as ProviderSupportedModelsListResponse_unstable;
  }

  async summarizerSupportedModelsList_unstable(
    params: SummarizerModelsListRequest_unstable,
  ): Promise<SummarizerModelsListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/summarizer/supported-models/list",
      params,
    );
    return zSummarizerModelsListResponse_unstable.parse(
      raw,
    ) as SummarizerModelsListResponse_unstable;
  }

  async providersCatalogList_unstable(
    params: ProviderCatalogListRequest_unstable,
  ): Promise<ProviderCatalogListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/catalog/list",
      params,
    );
    return zProviderCatalogListResponse_unstable.parse(
      raw,
    ) as ProviderCatalogListResponse_unstable;
  }

  async providersSetupCatalogList_unstable(
    params: ProviderSetupCatalogListRequest_unstable,
  ): Promise<ProviderSetupCatalogListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/setup/catalog/list",
      params,
    );
    return zProviderSetupCatalogListResponse_unstable.parse(
      raw,
    ) as ProviderSetupCatalogListResponse_unstable;
  }

  async providersCatalogTemplate_unstable(
    params: ProviderCatalogTemplateRequest_unstable,
  ): Promise<ProviderCatalogTemplateResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/catalog/template",
      params,
    );
    return zProviderCatalogTemplateResponse_unstable.parse(
      raw,
    ) as ProviderCatalogTemplateResponse_unstable;
  }

  async providersCustomCreate_unstable(
    params: CustomProviderCreateRequest_unstable,
  ): Promise<CustomProviderCreateResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/custom/create",
      params,
    );
    return zCustomProviderCreateResponse_unstable.parse(
      raw,
    ) as CustomProviderCreateResponse_unstable;
  }

  async providersCustomRead_unstable(
    params: CustomProviderReadRequest_unstable,
  ): Promise<CustomProviderReadResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/custom/read",
      params,
    );
    return zCustomProviderReadResponse_unstable.parse(
      raw,
    ) as CustomProviderReadResponse_unstable;
  }

  async providersCustomUpdate_unstable(
    params: CustomProviderUpdateRequest_unstable,
  ): Promise<CustomProviderUpdateResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/custom/update",
      params,
    );
    return zCustomProviderUpdateResponse_unstable.parse(
      raw,
    ) as CustomProviderUpdateResponse_unstable;
  }

  async providersCustomDelete_unstable(
    params: CustomProviderDeleteRequest_unstable,
  ): Promise<CustomProviderDeleteResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/custom/delete",
      params,
    );
    return zCustomProviderDeleteResponse_unstable.parse(
      raw,
    ) as CustomProviderDeleteResponse_unstable;
  }

  async providersInventoryRefresh_unstable(
    params: RefreshProviderInventoryRequest_unstable,
  ): Promise<RefreshProviderInventoryResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/inventory/refresh",
      params,
    );
    return zRefreshProviderInventoryResponse_unstable.parse(
      raw,
    ) as RefreshProviderInventoryResponse_unstable;
  }

  async providersConfigRead_unstable(
    params: ProviderConfigReadRequest_unstable,
  ): Promise<ProviderConfigReadResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/config/read",
      params,
    );
    return zProviderConfigReadResponse_unstable.parse(
      raw,
    ) as ProviderConfigReadResponse_unstable;
  }

  async providersConfigStatus_unstable(
    params: ProviderConfigStatusRequest_unstable,
  ): Promise<ProviderConfigStatusResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/config/status",
      params,
    );
    return zProviderConfigStatusResponse_unstable.parse(
      raw,
    ) as ProviderConfigStatusResponse_unstable;
  }

  async providersConfigSave_unstable(
    params: ProviderConfigSaveRequest_unstable,
  ): Promise<ProviderConfigChangeResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/config/save",
      params,
    );
    return zProviderConfigChangeResponse_unstable.parse(
      raw,
    ) as ProviderConfigChangeResponse_unstable;
  }

  async providersConfigDelete_unstable(
    params: ProviderConfigDeleteRequest_unstable,
  ): Promise<ProviderConfigChangeResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/config/delete",
      params,
    );
    return zProviderConfigChangeResponse_unstable.parse(
      raw,
    ) as ProviderConfigChangeResponse_unstable;
  }

  async providersConfigAuthenticate_unstable(
    params: ProviderConfigAuthenticateRequest_unstable,
  ): Promise<ProviderConfigChangeResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/config/authenticate",
      params,
    );
    return zProviderConfigChangeResponse_unstable.parse(
      raw,
    ) as ProviderConfigChangeResponse_unstable;
  }

  async providersSecretsList_unstable(
    params: ProviderSecretsListRequest_unstable,
  ): Promise<ProviderSecretsListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/secrets/list",
      params,
    );
    return zProviderSecretsListResponse_unstable.parse(
      raw,
    ) as ProviderSecretsListResponse_unstable;
  }

  async providersSecretsDelete_unstable(
    params: ProviderSecretDeleteRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/providers/secrets/delete",
      params,
    );
  }

  async providersSecretsCustomAdd_unstable(
    params: ProviderSecretCustomAddRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/providers/secrets/custom/add",
      params,
    );
  }

  async providersCanonicalModelInfo_unstable(
    params: CanonicalModelInfoRequest_unstable,
  ): Promise<CanonicalModelInfoResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/providers/canonical-model-info",
      params,
    );
    return zCanonicalModelInfoResponse_unstable.parse(
      raw,
    ) as CanonicalModelInfoResponse_unstable;
  }

  async preferencesRead_unstable(
    params: PreferencesReadRequest_unstable,
  ): Promise<PreferencesReadResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/preferences/read",
      params,
    );
    return zPreferencesReadResponse_unstable.parse(
      raw,
    ) as PreferencesReadResponse_unstable;
  }

  async preferencesSave_unstable(
    params: PreferencesSaveRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_gosling/unstable/preferences/save", params);
  }

  async preferencesRemove_unstable(
    params: PreferencesRemoveRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_gosling/unstable/preferences/remove", params);
  }

  async configRead_unstable(
    params: ConfigReadRequest_unstable,
  ): Promise<ConfigReadResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/config/read",
      params,
    );
    return zConfigReadResponse_unstable.parse(
      raw,
    ) as ConfigReadResponse_unstable;
  }

  async configUpsert_unstable(
    params: ConfigUpsertRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_gosling/unstable/config/upsert", params);
  }

  async configRemove_unstable(
    params: ConfigRemoveRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_gosling/unstable/config/remove", params);
  }

  async configReadAll_unstable(
    params: ConfigReadAllRequest_unstable,
  ): Promise<ConfigReadAllResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/config/read-all",
      params,
    );
    return zConfigReadAllResponse_unstable.parse(
      raw,
    ) as ConfigReadAllResponse_unstable;
  }

  async defaultsRead_unstable(
    params: DefaultsReadRequest_unstable,
  ): Promise<DefaultsReadResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/defaults/read",
      params,
    );
    return zDefaultsReadResponse_unstable.parse(
      raw,
    ) as DefaultsReadResponse_unstable;
  }

  async defaultsSave_unstable(
    params: DefaultsSaveRequest_unstable,
  ): Promise<DefaultsReadResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/defaults/save",
      params,
    );
    return zDefaultsReadResponse_unstable.parse(
      raw,
    ) as DefaultsReadResponse_unstable;
  }

  async defaultsClear_unstable(
    params: DefaultsClearRequest_unstable,
  ): Promise<DefaultsReadResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/defaults/clear",
      params,
    );
    return zDefaultsReadResponse_unstable.parse(
      raw,
    ) as DefaultsReadResponse_unstable;
  }

  async onboardingImportScan_unstable(
    params: OnboardingImportScanRequest_unstable,
  ): Promise<OnboardingImportScanResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/onboarding/import/scan",
      params,
    );
    return zOnboardingImportScanResponse_unstable.parse(
      raw,
    ) as OnboardingImportScanResponse_unstable;
  }

  async onboardingImportApply_unstable(
    params: OnboardingImportApplyRequest_unstable,
  ): Promise<OnboardingImportApplyResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/onboarding/import/apply",
      params,
    );
    return zOnboardingImportApplyResponse_unstable.parse(
      raw,
    ) as OnboardingImportApplyResponse_unstable;
  }

  async sessionExport_unstable(
    params: ExportSessionRequest_unstable,
  ): Promise<ExportSessionResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/export",
      params,
    );
    return zExportSessionResponse_unstable.parse(
      raw,
    ) as ExportSessionResponse_unstable;
  }

  async sessionImport_unstable(
    params: ImportSessionRequest_unstable,
  ): Promise<ImportSessionResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/import",
      params,
    );
    return zImportSessionResponse_unstable.parse(
      raw,
    ) as ImportSessionResponse_unstable;
  }

  async sessionShareNostr_unstable(
    params: ShareSessionNostrRequest_unstable,
  ): Promise<ShareSessionNostrResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/share/nostr",
      params,
    );
    return zShareSessionNostrResponse_unstable.parse(
      raw,
    ) as ShareSessionNostrResponse_unstable;
  }

  async sessionInfo_unstable(
    params: GetSessionInfoRequest_unstable,
  ): Promise<GetSessionInfoResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/info",
      params,
    );
    return zGetSessionInfoResponse_unstable.parse(
      raw,
    ) as GetSessionInfoResponse_unstable;
  }

  async sessionModelSwitchRecord_unstable(
    params: RecordSessionModelSwitchRequest_unstable,
  ): Promise<RecordSessionModelSwitchResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/model-switch-record",
      params,
    );
    return zRecordSessionModelSwitchResponse_unstable.parse(
      raw,
    ) as RecordSessionModelSwitchResponse_unstable;
  }

  async sessionMessagesList_unstable(
    params: ListSessionMessagesRequest_unstable,
  ): Promise<ListSessionMessagesResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/messages/list",
      params,
    );
    return zListSessionMessagesResponse_unstable.parse(
      raw,
    ) as ListSessionMessagesResponse_unstable;
  }

  async sessionArtifactsList_unstable(
    params: ListSessionArtifactsRequest_unstable,
  ): Promise<ListSessionArtifactsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/artifacts/list",
      params,
    );
    return zListSessionArtifactsResponse_unstable.parse(
      raw,
    ) as ListSessionArtifactsResponse_unstable;
  }

  async sessionMessagesSearch_unstable(
    params: SearchSessionMessagesRequest_unstable,
  ): Promise<SearchSessionMessagesResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/messages/search",
      params,
    );
    return zSearchSessionMessagesResponse_unstable.parse(
      raw,
    ) as SearchSessionMessagesResponse_unstable;
  }

  async sessionOutputsHistory_unstable(
    params: ListOutputRevisionsRequest_unstable,
  ): Promise<ListOutputRevisionsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/outputs/history",
      params,
    );
    return zListOutputRevisionsResponse_unstable.parse(
      raw,
    ) as ListOutputRevisionsResponse_unstable;
  }

  async sessionOutputsRevision_unstable(
    params: GetOutputRevisionRequest_unstable,
  ): Promise<GetOutputRevisionResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/outputs/revision",
      params,
    );
    return zGetOutputRevisionResponse_unstable.parse(
      raw,
    ) as GetOutputRevisionResponse_unstable;
  }

  async sessionOutputsRestore_unstable(
    params: RestoreOutputRevisionRequest_unstable,
  ): Promise<RestoreOutputRevisionResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/outputs/restore",
      params,
    );
    return zRestoreOutputRevisionResponse_unstable.parse(
      raw,
    ) as RestoreOutputRevisionResponse_unstable;
  }

  async sessionSummaryGet_unstable(
    params: GetSessionSummaryRequest_unstable,
  ): Promise<GetSessionSummaryResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/session/summary/get",
      params,
    );
    return zGetSessionSummaryResponse_unstable.parse(
      raw,
    ) as GetSessionSummaryResponse_unstable;
  }

  async sessionConversationTruncate_unstable(
    params: TruncateSessionConversationRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/session/conversation/truncate",
      params,
    );
  }

  async sessionProjectUpdate_unstable(
    params: UpdateSessionProjectRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/session/project/update",
      params,
    );
  }

  async sessionRename_unstable(
    params: RenameSessionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_gosling/unstable/session/rename", params);
  }

  async sessionArchive_unstable(
    params: ArchiveSessionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_gosling/unstable/session/archive", params);
  }

  async sessionUnarchive_unstable(
    params: UnarchiveSessionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_gosling/unstable/session/unarchive", params);
  }

  async sourcesCreate_unstable(
    params: CreateSourceRequest_unstable,
  ): Promise<CreateSourceResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/sources/create",
      params,
    );
    return zCreateSourceResponse_unstable.parse(
      raw,
    ) as CreateSourceResponse_unstable;
  }

  async sourcesList_unstable(
    params: ListSourcesRequest_unstable,
  ): Promise<ListSourcesResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/sources/list",
      params,
    );
    return zListSourcesResponse_unstable.parse(
      raw,
    ) as ListSourcesResponse_unstable;
  }

  async agentMentionsList_unstable(
    params: ListAgentMentionsRequest_unstable,
  ): Promise<ListAgentMentionsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/agent-mentions/list",
      params,
    );
    return zListAgentMentionsResponse_unstable.parse(
      raw,
    ) as ListAgentMentionsResponse_unstable;
  }

  async slashCommandsList_unstable(
    params: ListSlashCommandsRequest_unstable,
  ): Promise<ListSlashCommandsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/slash-commands/list",
      params,
    );
    return zListSlashCommandsResponse_unstable.parse(
      raw,
    ) as ListSlashCommandsResponse_unstable;
  }

  async sourcesUpdate_unstable(
    params: UpdateSourceRequest_unstable,
  ): Promise<UpdateSourceResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/sources/update",
      params,
    );
    return zUpdateSourceResponse_unstable.parse(
      raw,
    ) as UpdateSourceResponse_unstable;
  }

  async sourcesDelete_unstable(
    params: DeleteSourceRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_gosling/unstable/sources/delete", params);
  }

  async sourcesExport_unstable(
    params: ExportSourceRequest_unstable,
  ): Promise<ExportSourceResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/sources/export",
      params,
    );
    return zExportSourceResponse_unstable.parse(
      raw,
    ) as ExportSourceResponse_unstable;
  }

  async sourcesImport_unstable(
    params: ImportSourcesRequest_unstable,
  ): Promise<ImportSourcesResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/sources/import",
      params,
    );
    return zImportSourcesResponse_unstable.parse(
      raw,
    ) as ImportSourcesResponse_unstable;
  }

  async dictationTranscribe_unstable(
    params: DictationTranscribeRequest_unstable,
  ): Promise<DictationTranscribeResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/dictation/transcribe",
      params,
    );
    return zDictationTranscribeResponse_unstable.parse(
      raw,
    ) as DictationTranscribeResponse_unstable;
  }

  async dictationConfig_unstable(
    params: DictationConfigRequest_unstable,
  ): Promise<DictationConfigResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/dictation/config",
      params,
    );
    return zDictationConfigResponse_unstable.parse(
      raw,
    ) as DictationConfigResponse_unstable;
  }

  async dictationSecretSave_unstable(
    params: DictationSecretSaveRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/dictation/secret/save",
      params,
    );
  }

  async dictationSecretDelete_unstable(
    params: DictationSecretDeleteRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/dictation/secret/delete",
      params,
    );
  }

  async dictationModelsSelect_unstable(
    params: DictationModelSelectRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_gosling/unstable/dictation/models/select",
      params,
    );
  }

  async workspacesList_unstable(
    params: WorkspaceListRequest_unstable,
  ): Promise<WorkspaceListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/workspaces/list",
      params,
    );
    return zWorkspaceListResponse_unstable.parse(
      raw,
    ) as WorkspaceListResponse_unstable;
  }

  async workspacesCreate_unstable(
    params: WorkspaceCreateRequest_unstable,
  ): Promise<WorkspaceResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/workspaces/create",
      params,
    );
    return zWorkspaceResponse_unstable.parse(raw) as WorkspaceResponse_unstable;
  }

  async workspacesUpdate_unstable(
    params: WorkspaceUpdateRequest_unstable,
  ): Promise<WorkspaceResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/workspaces/update",
      params,
    );
    return zWorkspaceResponse_unstable.parse(raw) as WorkspaceResponse_unstable;
  }

  async workspacesDuplicate_unstable(
    params: WorkspaceDuplicateRequest_unstable,
  ): Promise<WorkspaceResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/workspaces/duplicate",
      params,
    );
    return zWorkspaceResponse_unstable.parse(raw) as WorkspaceResponse_unstable;
  }

  async workspacesDelete_unstable(
    params: WorkspaceDeleteRequest_unstable,
  ): Promise<WorkspaceDeleteResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/workspaces/delete",
      params,
    );
    return zWorkspaceDeleteResponse_unstable.parse(
      raw,
    ) as WorkspaceDeleteResponse_unstable;
  }

  async workspacesActiveSet_unstable(
    params: WorkspaceSetActiveRequest_unstable,
  ): Promise<WorkspaceResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/workspaces/active/set",
      params,
    );
    return zWorkspaceResponse_unstable.parse(raw) as WorkspaceResponse_unstable;
  }

  async workspacesValidate_unstable(
    params: WorkspaceValidateRequest_unstable,
  ): Promise<WorkspaceValidationResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/workspaces/validate",
      params,
    );
    return zWorkspaceValidationResponse_unstable.parse(
      raw,
    ) as WorkspaceValidationResponse_unstable;
  }

  async workspacesExport_unstable(
    params: WorkspaceExportRequest_unstable,
  ): Promise<WorkspaceExportResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/workspaces/export",
      params,
    );
    return zWorkspaceExportResponse_unstable.parse(
      raw,
    ) as WorkspaceExportResponse_unstable;
  }

  async workspacesImport_unstable(
    params: WorkspaceImportRequest_unstable,
  ): Promise<WorkspaceResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/workspaces/import",
      params,
    );
    return zWorkspaceResponse_unstable.parse(raw) as WorkspaceResponse_unstable;
  }

  async workspacesOutputCreate_unstable(
    params: WorkspaceCreateOutputFolderRequest_unstable,
  ): Promise<WorkspaceValidationResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/workspaces/output/create",
      params,
    );
    return zWorkspaceValidationResponse_unstable.parse(
      raw,
    ) as WorkspaceValidationResponse_unstable;
  }

  async credentialProfilesList_unstable(
    params: CredentialProfileListRequest_unstable,
  ): Promise<CredentialProfileListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/credential-profiles/list",
      params,
    );
    return zCredentialProfileListResponse_unstable.parse(
      raw,
    ) as CredentialProfileListResponse_unstable;
  }

  async credentialProfilesCreate_unstable(
    params: CredentialProfileCreateRequest_unstable,
  ): Promise<CredentialProfileResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/credential-profiles/create",
      params,
    );
    return zCredentialProfileResponse_unstable.parse(
      raw,
    ) as CredentialProfileResponse_unstable;
  }

  async credentialProfilesUpdate_unstable(
    params: CredentialProfileUpdateRequest_unstable,
  ): Promise<CredentialProfileResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/credential-profiles/update",
      params,
    );
    return zCredentialProfileResponse_unstable.parse(
      raw,
    ) as CredentialProfileResponse_unstable;
  }

  async credentialProfilesDelete_unstable(
    params: CredentialProfileDeleteRequest_unstable,
  ): Promise<CredentialProfileDeleteResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/credential-profiles/delete",
      params,
    );
    return zCredentialProfileDeleteResponse_unstable.parse(
      raw,
    ) as CredentialProfileDeleteResponse_unstable;
  }

  async credentialProfilesUsage_unstable(
    params: CredentialProfileUsageRequest_unstable,
  ): Promise<CredentialProfileUsageResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/credential-profiles/usage",
      params,
    );
    return zCredentialProfileUsageResponse_unstable.parse(
      raw,
    ) as CredentialProfileUsageResponse_unstable;
  }

  async credentialProfilesTest_unstable(
    params: CredentialProfileTestRequest_unstable,
  ): Promise<CredentialProfileTestResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_gosling/unstable/credential-profiles/test",
      params,
    );
    return zCredentialProfileTestResponse_unstable.parse(
      raw,
    ) as CredentialProfileTestResponse_unstable;
  }
}

export interface GoslingExtNotifications {
  unstable_sessionUpdate?: (
    notification: GoslingSessionNotification_unstable,
  ) => Promise<void>;
  unstable_shellDomainStatus?: (
    notification: DomainStatusNotification_unstable,
  ) => Promise<void>;
}

export interface GoslingExtAgentRequests {}

export type GoslingClientCallbacks = Omit<
  Client,
  "extNotification" | "extMethod"
> &
  Partial<Pick<Client, "extNotification" | "extMethod">> &
  GoslingExtNotifications &
  GoslingExtAgentRequests;

export function installGoslingExtNotificationDispatcher(
  callbacks: GoslingClientCallbacks,
): Client {
  const dispatcher: Pick<Client, "extNotification"> = {
    extNotification: async (method, params) => {
      switch (method) {
        case "_gosling/unstable/session/update": {
          const parsed = zGoslingSessionNotification_unstable.parse(
            params,
          ) as GoslingSessionNotification_unstable;
          await callbacks.unstable_sessionUpdate?.(parsed);
          return;
        }
        case "_gosling/unstable/shell/domain/status": {
          const parsed = zDomainStatusNotification_unstable.parse(
            params,
          ) as DomainStatusNotification_unstable;
          await callbacks.unstable_shellDomainStatus?.(parsed);
          return;
        }
        default:
          await callbacks.extNotification?.(method, params);
          return;
      }
    },
  };
  return new Proxy(callbacks, {
    get(target, property) {
      if (property === "extNotification") {
        return dispatcher.extNotification;
      }

      const value = Reflect.get(target, property, target);
      return typeof value === "function" ? value.bind(target) : value;
    },
  }) as Client;
}

export function installGoslingExtAgentRequestDispatcher(
  callbacks: GoslingClientCallbacks,
): Client {
  const dispatcher: Pick<Client, "extMethod"> = {
    extMethod: async (method, params) => {
      switch (method) {
        default:
          if (callbacks.extMethod) {
            return await callbacks.extMethod(method, params);
          }
          throw new Error(`unhandled ext method: ${method}`);
      }
    },
  };
  return new Proxy(callbacks, {
    get(target, property) {
      if (property === "extMethod") {
        return dispatcher.extMethod;
      }

      const value = Reflect.get(target, property, target);
      return typeof value === "function" ? value.bind(target) : value;
    },
  }) as Client;
}
