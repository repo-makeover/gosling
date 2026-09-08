use super::*;
use gosling_acp_macros::custom_methods;

#[custom_methods]
impl GoslingAcpAgent {
    pub async fn dispatch_custom_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, agent_client_protocol::Error> {
        self.shell_runtime.enforce_custom_method(method)?;
        let result = super::presentation::ensure_custom_result_fits(
            self.handle_custom_request(method, params).await,
            method,
        );

        if let Err(error) = &result {
            tracing::error!(method, error = ?error, "ACP custom request failed");
        }

        result
    }

    #[custom_method(ShellProvisioningReadRequest)]
    async fn dispatch_read_shell_provisioning(
        &self,
        req: ShellProvisioningReadRequest,
    ) -> Result<ShellProvisioningReadResponse, agent_client_protocol::Error> {
        self.on_read_shell_provisioning(req).await
    }

    #[custom_method(ShellProvisioningValidateRequest)]
    async fn dispatch_validate_shell_provisioning(
        &self,
        req: ShellProvisioningValidateRequest,
    ) -> Result<ShellProvisioningValidateResponse, agent_client_protocol::Error> {
        self.on_validate_shell_provisioning(req).await
    }

    #[custom_method(ShellDirectoryValidateRequest)]
    async fn dispatch_validate_shell_directory(
        &self,
        req: ShellDirectoryValidateRequest,
    ) -> Result<ShellDirectoryValidateResponse, agent_client_protocol::Error> {
        Ok(self.on_validate_shell_directory(req))
    }

    #[custom_method(ShellCredentialListRequest)]
    async fn dispatch_list_shell_credentials(
        &self,
    ) -> Result<ShellCredentialListResponse, agent_client_protocol::Error> {
        Ok(self.on_list_shell_credentials().await)
    }

    #[custom_method(ShellModuleListRequest)]
    async fn dispatch_list_shell_modules(
        &self,
        req: ShellModuleListRequest,
    ) -> Result<ShellModuleListResponse, agent_client_protocol::Error> {
        self.on_list_shell_modules(req).await
    }

    #[custom_method(ShellArtifactListRequest)]
    async fn dispatch_list_shell_artifacts(
        &self,
        req: ShellArtifactListRequest,
    ) -> Result<ShellArtifactListResponse, agent_client_protocol::Error> {
        self.on_list_shell_artifacts(req).await
    }

    #[custom_method(ShellLibraryListRequest)]
    async fn dispatch_list_shell_library(
        &self,
        req: ShellLibraryListRequest,
    ) -> Result<ShellLibraryListResponse, agent_client_protocol::Error> {
        self.on_list_shell_library(req).await
    }

    #[custom_method(ShellLibraryAddTextRequest)]
    async fn dispatch_add_shell_library_text(
        &self,
        req: ShellLibraryAddTextRequest,
    ) -> Result<ShellLibraryAddResponse, agent_client_protocol::Error> {
        self.on_add_shell_library_text(req).await
    }

    #[custom_method(ShellLibraryAddImageRequest)]
    async fn dispatch_add_shell_library_image(
        &self,
        req: ShellLibraryAddImageRequest,
    ) -> Result<ShellLibraryAddResponse, agent_client_protocol::Error> {
        self.on_add_shell_library_image(req).await
    }

    #[custom_method(ShellLibraryLinkFileRequest)]
    async fn dispatch_link_shell_library_file(
        &self,
        req: ShellLibraryLinkFileRequest,
    ) -> Result<ShellLibraryAddResponse, agent_client_protocol::Error> {
        self.on_link_shell_library_file(req).await
    }

    #[custom_method(ShellLibraryRemoveRequest)]
    async fn dispatch_remove_shell_library_item(
        &self,
        req: ShellLibraryRemoveRequest,
    ) -> Result<ShellLibraryRemoveResponse, agent_client_protocol::Error> {
        self.on_remove_shell_library_item(req).await
    }

    #[custom_method(ShellLibraryResolveRequest)]
    async fn dispatch_resolve_shell_library(
        &self,
        req: ShellLibraryResolveRequest,
    ) -> Result<ShellLibraryResolveResponse, agent_client_protocol::Error> {
        self.on_resolve_shell_library(req).await
    }

    #[custom_method(DomainSnapshotRequest)]
    async fn dispatch_domain_snapshot(
        &self,
        req: DomainSnapshotRequest,
    ) -> Result<DomainSnapshotResponse, agent_client_protocol::Error> {
        self.on_domain_snapshot(req).await
    }

    #[custom_method(DomainActionRequest)]
    async fn dispatch_domain_action(
        &self,
        req: DomainActionRequest,
    ) -> Result<DomainActionResponse, agent_client_protocol::Error> {
        self.on_domain_action(req).await
    }

    #[custom_method(DomainActionConfirmRequest)]
    async fn dispatch_domain_action_confirm(
        &self,
        req: DomainActionConfirmRequest,
    ) -> Result<DomainActionConfirmResponse, agent_client_protocol::Error> {
        self.on_domain_action_confirm(req).await
    }

    #[custom_method(ShellHandoffPrepareRequest)]
    async fn dispatch_prepare_shell_handoff(
        &self,
        req: ShellHandoffPrepareRequest,
    ) -> Result<ShellHandoffPrepareResponse, agent_client_protocol::Error> {
        Ok(self.on_prepare_shell_handoff(req))
    }

    #[custom_method(AddSessionExtensionRequest)]
    async fn dispatch_add_session_extension(
        &self,
        req: AddSessionExtensionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_add_session_extension(req).await
    }

    #[custom_method(RemoveSessionExtensionRequest)]
    async fn dispatch_remove_session_extension(
        &self,
        req: RemoveSessionExtensionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_remove_session_extension(req).await
    }

    #[custom_method(GetToolsRequest)]
    async fn dispatch_get_tools(
        &self,
        req: GetToolsRequest,
    ) -> Result<GetToolsResponse, agent_client_protocol::Error> {
        self.on_get_tools(req).await
    }

    #[custom_method(SetToolPermissionsRequest)]
    async fn dispatch_set_tool_permissions(
        &self,
        req: SetToolPermissionsRequest,
    ) -> Result<SetToolPermissionsResponse, agent_client_protocol::Error> {
        self.on_set_tool_permissions(req).await
    }

    #[custom_method(GoslingToolCallRequest)]
    async fn dispatch_call_tool(
        &self,
        req: GoslingToolCallRequest,
    ) -> Result<GoslingToolCallResponse, agent_client_protocol::Error> {
        self.on_call_tool(req).await
    }

    #[custom_method(ReadResourceRequest)]
    async fn dispatch_read_resource(
        &self,
        req: ReadResourceRequest,
    ) -> Result<ReadResourceResponse, agent_client_protocol::Error> {
        self.on_read_resource(req).await
    }

    #[custom_method(UpdateWorkingDirRequest)]
    async fn dispatch_update_working_dir(
        &self,
        req: UpdateWorkingDirRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_update_working_dir(req).await
    }

    #[custom_method(AddSessionWorkingDirRequest)]
    async fn dispatch_add_session_working_dir(
        &self,
        req: AddSessionWorkingDirRequest,
    ) -> Result<SessionWorkingDirsResponse, agent_client_protocol::Error> {
        self.on_add_session_working_dir(req).await
    }

    #[custom_method(RemoveSessionWorkingDirRequest)]
    async fn dispatch_remove_session_working_dir(
        &self,
        req: RemoveSessionWorkingDirRequest,
    ) -> Result<SessionWorkingDirsResponse, agent_client_protocol::Error> {
        self.on_remove_session_working_dir(req).await
    }

    #[custom_method(SetSessionWorkingDirRestrictionRequest)]
    async fn dispatch_set_session_working_dir_restriction(
        &self,
        req: SetSessionWorkingDirRestrictionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_set_session_working_dir_restriction(req).await
    }

    #[custom_method(SetSessionSystemPromptRequest)]
    async fn dispatch_set_session_system_prompt(
        &self,
        req: SetSessionSystemPromptRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_set_session_system_prompt(req).await
    }

    #[custom_method(SteerSessionRequest)]
    async fn dispatch_steer_session(
        &self,
        req: SteerSessionRequest,
    ) -> Result<SteerSessionResponse, agent_client_protocol::Error> {
        self.on_steer_session(req).await
    }

    #[custom_method(DiagnosticsGetRequest)]
    async fn dispatch_get_diagnostics(
        &self,
        req: DiagnosticsGetRequest,
    ) -> Result<DiagnosticsGetResponse, agent_client_protocol::Error> {
        self.on_get_diagnostics(req).await
    }

    #[custom_method(ListPromptsRequest)]
    async fn dispatch_list_prompts(
        &self,
        req: ListPromptsRequest,
    ) -> Result<ListPromptsResponse, agent_client_protocol::Error> {
        self.on_list_prompts(req).await
    }

    #[custom_method(GetPromptRequest)]
    async fn dispatch_get_prompt(
        &self,
        req: GetPromptRequest,
    ) -> Result<GetPromptResponse, agent_client_protocol::Error> {
        self.on_get_prompt(req).await
    }

    #[custom_method(SavePromptRequest)]
    async fn dispatch_save_prompt(
        &self,
        req: SavePromptRequest,
    ) -> Result<PromptOperationResponse, agent_client_protocol::Error> {
        self.on_save_prompt(req).await
    }

    #[custom_method(ResetPromptRequest)]
    async fn dispatch_reset_prompt(
        &self,
        req: ResetPromptRequest,
    ) -> Result<PromptOperationResponse, agent_client_protocol::Error> {
        self.on_reset_prompt(req).await
    }

    #[custom_method(DeleteSessionRequest)]
    async fn dispatch_delete_session(
        &self,
        req: DeleteSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_delete_session(req).await
    }

    #[custom_method(GetConfigExtensionsRequest)]
    async fn dispatch_get_config_extensions(
        &self,
    ) -> Result<GetConfigExtensionsResponse, agent_client_protocol::Error> {
        self.on_get_config_extensions().await
    }

    #[custom_method(GetAvailableExtensionsRequest)]
    async fn dispatch_get_available_extensions(
        &self,
    ) -> Result<GetAvailableExtensionsResponse, agent_client_protocol::Error> {
        self.on_get_available_extensions().await
    }

    #[custom_method(AddConfigExtensionRequest)]
    async fn dispatch_add_config_extension(
        &self,
        req: AddConfigExtensionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_add_config_extension(req).await
    }

    #[custom_method(RemoveConfigExtensionRequest)]
    async fn dispatch_remove_config_extension(
        &self,
        req: RemoveConfigExtensionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_remove_config_extension(req).await
    }

    #[custom_method(SetConfigExtensionEnabledRequest)]
    async fn dispatch_set_config_extension_enabled(
        &self,
        req: SetConfigExtensionEnabledRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_set_config_extension_enabled(req).await
    }

    #[custom_method(GetSessionExtensionsRequest)]
    async fn dispatch_get_session_extensions(
        &self,
        req: GetSessionExtensionsRequest,
    ) -> Result<GetSessionExtensionsResponse, agent_client_protocol::Error> {
        self.on_get_session_extensions(req).await
    }

    #[custom_method(ListProvidersRequest)]
    async fn dispatch_list_providers(
        &self,
        req: ListProvidersRequest,
    ) -> Result<ListProvidersResponse, agent_client_protocol::Error> {
        self.on_list_providers(req).await
    }

    #[custom_method(ProviderSupportedModelsListRequest)]
    async fn dispatch_list_provider_supported_models(
        &self,
        req: ProviderSupportedModelsListRequest,
    ) -> Result<ProviderSupportedModelsListResponse, agent_client_protocol::Error> {
        self.on_list_provider_supported_models(req).await
    }

    #[custom_method(SummarizerModelsListRequest)]
    async fn dispatch_list_summarizer_models(
        &self,
        req: SummarizerModelsListRequest,
    ) -> Result<SummarizerModelsListResponse, agent_client_protocol::Error> {
        self.on_list_summarizer_models(req).await
    }

    #[custom_method(ProviderCatalogListRequest)]
    async fn dispatch_list_provider_catalog(
        &self,
        req: ProviderCatalogListRequest,
    ) -> Result<ProviderCatalogListResponse, agent_client_protocol::Error> {
        self.on_list_provider_catalog(req).await
    }

    #[custom_method(ProviderSetupCatalogListRequest)]
    async fn dispatch_list_provider_setup_catalog(
        &self,
        req: ProviderSetupCatalogListRequest,
    ) -> Result<ProviderSetupCatalogListResponse, agent_client_protocol::Error> {
        self.on_list_provider_setup_catalog(req).await
    }

    #[custom_method(ProviderCatalogTemplateRequest)]
    async fn dispatch_get_provider_catalog_template(
        &self,
        req: ProviderCatalogTemplateRequest,
    ) -> Result<ProviderCatalogTemplateResponse, agent_client_protocol::Error> {
        self.on_get_provider_catalog_template(req).await
    }

    #[custom_method(CustomProviderCreateRequest)]
    async fn dispatch_create_custom_provider(
        &self,
        req: CustomProviderCreateRequest,
    ) -> Result<CustomProviderCreateResponse, agent_client_protocol::Error> {
        self.on_create_custom_provider(req).await
    }

    #[custom_method(CustomProviderReadRequest)]
    async fn dispatch_read_custom_provider(
        &self,
        req: CustomProviderReadRequest,
    ) -> Result<CustomProviderReadResponse, agent_client_protocol::Error> {
        self.on_read_custom_provider(req).await
    }

    #[custom_method(CustomProviderUpdateRequest)]
    async fn dispatch_update_custom_provider(
        &self,
        req: CustomProviderUpdateRequest,
    ) -> Result<CustomProviderUpdateResponse, agent_client_protocol::Error> {
        self.on_update_custom_provider(req).await
    }

    #[custom_method(CustomProviderDeleteRequest)]
    async fn dispatch_delete_custom_provider(
        &self,
        req: CustomProviderDeleteRequest,
    ) -> Result<CustomProviderDeleteResponse, agent_client_protocol::Error> {
        self.on_delete_custom_provider(req).await
    }

    #[custom_method(RefreshProviderInventoryRequest)]
    async fn dispatch_refresh_provider_inventory(
        &self,
        req: RefreshProviderInventoryRequest,
    ) -> Result<RefreshProviderInventoryResponse, agent_client_protocol::Error> {
        self.on_refresh_provider_inventory(req).await
    }

    #[custom_method(ProviderConfigReadRequest)]
    async fn dispatch_read_provider_config(
        &self,
        req: ProviderConfigReadRequest,
    ) -> Result<ProviderConfigReadResponse, agent_client_protocol::Error> {
        self.on_read_provider_config(req).await
    }

    #[custom_method(ProviderConfigStatusRequest)]
    async fn dispatch_provider_config_status(
        &self,
        req: ProviderConfigStatusRequest,
    ) -> Result<ProviderConfigStatusResponse, agent_client_protocol::Error> {
        self.on_provider_config_status(req).await
    }

    #[custom_method(ProviderConfigSaveRequest)]
    async fn dispatch_save_provider_config(
        &self,
        req: ProviderConfigSaveRequest,
    ) -> Result<ProviderConfigChangeResponse, agent_client_protocol::Error> {
        self.on_save_provider_config(req).await
    }

    #[custom_method(ProviderConfigDeleteRequest)]
    async fn dispatch_delete_provider_config(
        &self,
        req: ProviderConfigDeleteRequest,
    ) -> Result<ProviderConfigChangeResponse, agent_client_protocol::Error> {
        self.on_delete_provider_config(req).await
    }

    #[custom_method(ProviderConfigAuthenticateRequest)]
    async fn dispatch_authenticate_provider_config(
        &self,
        req: ProviderConfigAuthenticateRequest,
    ) -> Result<ProviderConfigChangeResponse, agent_client_protocol::Error> {
        self.on_authenticate_provider_config(req).await
    }

    #[custom_method(ProviderSecretsListRequest)]
    async fn dispatch_list_provider_secrets(
        &self,
        req: ProviderSecretsListRequest,
    ) -> Result<ProviderSecretsListResponse, agent_client_protocol::Error> {
        self.on_list_provider_secrets(req).await
    }

    #[custom_method(ProviderSecretDeleteRequest)]
    async fn dispatch_delete_provider_secret(
        &self,
        req: ProviderSecretDeleteRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_delete_provider_secret(req).await
    }

    #[custom_method(ProviderSecretCustomAddRequest)]
    async fn dispatch_add_custom_provider_secret(
        &self,
        req: ProviderSecretCustomAddRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_add_custom_provider_secret(req).await
    }

    #[custom_method(CanonicalModelInfoRequest)]
    async fn dispatch_canonical_model_info(
        &self,
        req: CanonicalModelInfoRequest,
    ) -> Result<CanonicalModelInfoResponse, agent_client_protocol::Error> {
        self.on_canonical_model_info(req).await
    }

    #[custom_method(PreferencesReadRequest)]
    async fn dispatch_preferences_read(
        &self,
        req: PreferencesReadRequest,
    ) -> Result<PreferencesReadResponse, agent_client_protocol::Error> {
        self.on_preferences_read(req).await
    }

    #[custom_method(PreferencesSaveRequest)]
    async fn dispatch_preferences_save(
        &self,
        req: PreferencesSaveRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_preferences_save(req).await
    }

    #[custom_method(PreferencesRemoveRequest)]
    async fn dispatch_preferences_remove(
        &self,
        req: PreferencesRemoveRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_preferences_remove(req).await
    }

    #[custom_method(ConfigReadRequest)]
    async fn dispatch_config_read(
        &self,
        req: ConfigReadRequest,
    ) -> Result<ConfigReadResponse, agent_client_protocol::Error> {
        self.on_config_read(req).await
    }

    #[custom_method(ConfigUpsertRequest)]
    async fn dispatch_config_upsert(
        &self,
        req: ConfigUpsertRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_config_upsert(req).await
    }

    #[custom_method(ConfigRemoveRequest)]
    async fn dispatch_config_remove(
        &self,
        req: ConfigRemoveRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_config_remove(req).await
    }

    #[custom_method(ConfigReadAllRequest)]
    async fn dispatch_config_read_all(
        &self,
        req: ConfigReadAllRequest,
    ) -> Result<ConfigReadAllResponse, agent_client_protocol::Error> {
        self.on_config_read_all(req).await
    }

    #[custom_method(DefaultsReadRequest)]
    async fn dispatch_defaults_read(
        &self,
        req: DefaultsReadRequest,
    ) -> Result<DefaultsReadResponse, agent_client_protocol::Error> {
        self.on_defaults_read(req).await
    }

    #[custom_method(DefaultsSaveRequest)]
    async fn dispatch_defaults_save(
        &self,
        req: DefaultsSaveRequest,
    ) -> Result<DefaultsReadResponse, agent_client_protocol::Error> {
        self.on_defaults_save(req).await
    }

    #[custom_method(DefaultsClearRequest)]
    async fn dispatch_defaults_clear(
        &self,
        req: DefaultsClearRequest,
    ) -> Result<DefaultsReadResponse, agent_client_protocol::Error> {
        self.on_defaults_clear(req).await
    }

    #[custom_method(OnboardingImportScanRequest)]
    async fn dispatch_onboarding_import_scan(
        &self,
        req: OnboardingImportScanRequest,
    ) -> Result<OnboardingImportScanResponse, agent_client_protocol::Error> {
        self.on_onboarding_import_scan(req).await
    }

    #[custom_method(OnboardingImportApplyRequest)]
    async fn dispatch_onboarding_import_apply(
        &self,
        req: OnboardingImportApplyRequest,
    ) -> Result<OnboardingImportApplyResponse, agent_client_protocol::Error> {
        self.on_onboarding_import_apply(req).await
    }

    #[custom_method(ExportSessionRequest)]
    async fn dispatch_export_session(
        &self,
        req: ExportSessionRequest,
    ) -> Result<ExportSessionResponse, agent_client_protocol::Error> {
        self.on_export_session(req).await
    }

    #[custom_method(ImportSessionRequest)]
    async fn dispatch_import_session(
        &self,
        req: ImportSessionRequest,
    ) -> Result<ImportSessionResponse, agent_client_protocol::Error> {
        self.on_import_session(req).await
    }

    #[custom_method(ShareSessionNostrRequest)]
    async fn dispatch_share_session_nostr(
        &self,
        req: ShareSessionNostrRequest,
    ) -> Result<ShareSessionNostrResponse, agent_client_protocol::Error> {
        self.on_share_session_nostr(req).await
    }

    #[custom_method(GetSessionInfoRequest)]
    async fn dispatch_get_session_info(
        &self,
        req: GetSessionInfoRequest,
    ) -> Result<GetSessionInfoResponse, agent_client_protocol::Error> {
        self.on_get_session_info(req).await
    }

    #[custom_method(RecordSessionModelSwitchRequest)]
    async fn dispatch_record_session_model_switch(
        &self,
        req: RecordSessionModelSwitchRequest,
    ) -> Result<RecordSessionModelSwitchResponse, agent_client_protocol::Error> {
        self.on_record_session_model_switch(req).await
    }

    #[custom_method(ListSessionMessagesRequest)]
    async fn dispatch_list_session_messages(
        &self,
        req: ListSessionMessagesRequest,
    ) -> Result<ListSessionMessagesResponse, agent_client_protocol::Error> {
        self.on_list_session_messages(req).await
    }

    #[custom_method(ListSessionArtifactsRequest)]
    async fn dispatch_list_session_artifacts(
        &self,
        req: ListSessionArtifactsRequest,
    ) -> Result<ListSessionArtifactsResponse, agent_client_protocol::Error> {
        self.on_list_session_artifacts(req).await
    }

    #[custom_method(SearchSessionMessagesRequest)]
    async fn dispatch_search_session_messages(
        &self,
        req: SearchSessionMessagesRequest,
    ) -> Result<SearchSessionMessagesResponse, agent_client_protocol::Error> {
        self.on_search_session_messages(req).await
    }

    #[custom_method(ListOutputRevisionsRequest)]
    async fn dispatch_list_output_revisions(
        &self,
        req: ListOutputRevisionsRequest,
    ) -> Result<ListOutputRevisionsResponse, agent_client_protocol::Error> {
        self.session_manager
            .list_output_revisions(req)
            .await
            .map_err(|error| agent_client_protocol::Error::invalid_params().data(error.to_string()))
    }

    #[custom_method(GetOutputRevisionRequest)]
    async fn dispatch_get_output_revision(
        &self,
        req: GetOutputRevisionRequest,
    ) -> Result<GetOutputRevisionResponse, agent_client_protocol::Error> {
        self.session_manager
            .get_output_revision(req)
            .await
            .map_err(|error| agent_client_protocol::Error::invalid_params().data(error.to_string()))
    }

    #[custom_method(RestoreOutputRevisionRequest)]
    async fn dispatch_restore_output_revision(
        &self,
        req: RestoreOutputRevisionRequest,
    ) -> Result<RestoreOutputRevisionResponse, agent_client_protocol::Error> {
        self.session_manager
            .restore_output_revision(req)
            .await
            .map_err(|error| agent_client_protocol::Error::invalid_params().data(error.to_string()))
    }

    #[custom_method(GetSessionSummaryRequest)]
    async fn dispatch_get_session_summary(
        &self,
        req: GetSessionSummaryRequest,
    ) -> Result<GetSessionSummaryResponse, agent_client_protocol::Error> {
        self.on_get_session_summary(req).await
    }

    #[custom_method(TruncateSessionConversationRequest)]
    async fn dispatch_truncate_session_conversation(
        &self,
        req: TruncateSessionConversationRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_truncate_session_conversation(req).await
    }

    #[custom_method(UpdateSessionProjectRequest)]
    async fn dispatch_update_session_project(
        &self,
        req: UpdateSessionProjectRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_update_session_project(req).await
    }

    #[custom_method(RenameSessionRequest)]
    async fn dispatch_rename_session(
        &self,
        req: RenameSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_rename_session(req).await
    }

    #[custom_method(ArchiveSessionRequest)]
    async fn dispatch_archive_session(
        &self,
        req: ArchiveSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_archive_session(req).await
    }

    #[custom_method(UnarchiveSessionRequest)]
    async fn dispatch_unarchive_session(
        &self,
        req: UnarchiveSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_unarchive_session(req).await
    }

    #[custom_method(CreateSourceRequest)]
    async fn dispatch_create_source(
        &self,
        req: CreateSourceRequest,
    ) -> Result<CreateSourceResponse, agent_client_protocol::Error> {
        self.on_create_source(req).await
    }

    #[custom_method(ListSourcesRequest)]
    async fn dispatch_list_sources(
        &self,
        req: ListSourcesRequest,
    ) -> Result<ListSourcesResponse, agent_client_protocol::Error> {
        self.on_list_sources(req).await
    }

    #[custom_method(ListAgentMentionsRequest)]
    async fn dispatch_list_agent_mentions(
        &self,
        req: ListAgentMentionsRequest,
    ) -> Result<ListAgentMentionsResponse, agent_client_protocol::Error> {
        self.on_list_agent_mentions(req).await
    }

    #[custom_method(ListSlashCommandsRequest)]
    async fn dispatch_list_slash_commands(
        &self,
        req: ListSlashCommandsRequest,
    ) -> Result<ListSlashCommandsResponse, agent_client_protocol::Error> {
        self.on_list_slash_commands(req).await
    }

    #[custom_method(UpdateSourceRequest)]
    async fn dispatch_update_source(
        &self,
        req: UpdateSourceRequest,
    ) -> Result<UpdateSourceResponse, agent_client_protocol::Error> {
        self.on_update_source(req).await
    }

    #[custom_method(DeleteSourceRequest)]
    async fn dispatch_delete_source(
        &self,
        req: DeleteSourceRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_delete_source(req).await
    }

    #[custom_method(ExportSourceRequest)]
    async fn dispatch_export_source(
        &self,
        req: ExportSourceRequest,
    ) -> Result<ExportSourceResponse, agent_client_protocol::Error> {
        self.on_export_source(req).await
    }

    #[custom_method(ImportSourcesRequest)]
    async fn dispatch_import_sources(
        &self,
        req: ImportSourcesRequest,
    ) -> Result<ImportSourcesResponse, agent_client_protocol::Error> {
        self.on_import_sources(req).await
    }

    #[custom_method(DictationTranscribeRequest)]
    async fn dispatch_dictation_transcribe(
        &self,
        req: DictationTranscribeRequest,
    ) -> Result<DictationTranscribeResponse, agent_client_protocol::Error> {
        self.on_dictation_transcribe(req).await
    }

    #[custom_method(DictationConfigRequest)]
    async fn dispatch_dictation_config(
        &self,
        _req: DictationConfigRequest,
    ) -> Result<DictationConfigResponse, agent_client_protocol::Error> {
        self.on_dictation_config(_req).await
    }

    #[custom_method(DictationSecretSaveRequest)]
    async fn dispatch_dictation_secret_save(
        &self,
        req: DictationSecretSaveRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_dictation_secret_save(req).await
    }

    #[custom_method(DictationSecretDeleteRequest)]
    async fn dispatch_dictation_secret_delete(
        &self,
        req: DictationSecretDeleteRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_dictation_secret_delete(req).await
    }

    #[custom_method(DictationModelSelectRequest)]
    async fn dispatch_dictation_model_select(
        &self,
        req: DictationModelSelectRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_dictation_model_select(req).await
    }

    #[custom_method(WorkspaceListRequest)]
    async fn dispatch_workspace_list(
        &self,
        req: WorkspaceListRequest,
    ) -> Result<WorkspaceListResponse, agent_client_protocol::Error> {
        self.on_workspace_list(req).await
    }

    #[custom_method(WorkspaceCreateRequest)]
    async fn dispatch_workspace_create(
        &self,
        req: WorkspaceCreateRequest,
    ) -> Result<WorkspaceResponse, agent_client_protocol::Error> {
        self.on_workspace_create(req).await
    }

    #[custom_method(WorkspaceUpdateRequest)]
    async fn dispatch_workspace_update(
        &self,
        req: WorkspaceUpdateRequest,
    ) -> Result<WorkspaceResponse, agent_client_protocol::Error> {
        self.on_workspace_update(req).await
    }

    #[custom_method(WorkspaceDuplicateRequest)]
    async fn dispatch_workspace_duplicate(
        &self,
        req: WorkspaceDuplicateRequest,
    ) -> Result<WorkspaceResponse, agent_client_protocol::Error> {
        self.on_workspace_duplicate(req).await
    }

    #[custom_method(WorkspaceDeleteRequest)]
    async fn dispatch_workspace_delete(
        &self,
        req: WorkspaceDeleteRequest,
    ) -> Result<WorkspaceDeleteResponse, agent_client_protocol::Error> {
        self.on_workspace_delete(req).await
    }

    #[custom_method(WorkspaceSetActiveRequest)]
    async fn dispatch_workspace_set_active(
        &self,
        req: WorkspaceSetActiveRequest,
    ) -> Result<WorkspaceResponse, agent_client_protocol::Error> {
        self.on_workspace_set_active(req).await
    }

    #[custom_method(WorkspaceValidateRequest)]
    async fn dispatch_workspace_validate(
        &self,
        req: WorkspaceValidateRequest,
    ) -> Result<WorkspaceValidationResponse, agent_client_protocol::Error> {
        self.on_workspace_validate(req).await
    }

    #[custom_method(WorkspaceExportRequest)]
    async fn dispatch_workspace_export(
        &self,
        req: WorkspaceExportRequest,
    ) -> Result<WorkspaceExportResponse, agent_client_protocol::Error> {
        self.on_workspace_export(req).await
    }

    #[custom_method(WorkspaceImportRequest)]
    async fn dispatch_workspace_import(
        &self,
        req: WorkspaceImportRequest,
    ) -> Result<WorkspaceResponse, agent_client_protocol::Error> {
        self.on_workspace_import(req).await
    }

    #[custom_method(WorkspaceCreateOutputFolderRequest)]
    async fn dispatch_workspace_create_output_folder(
        &self,
        req: WorkspaceCreateOutputFolderRequest,
    ) -> Result<WorkspaceValidationResponse, agent_client_protocol::Error> {
        self.on_workspace_create_output_folder(req).await
    }

    #[custom_method(CredentialProfileListRequest)]
    async fn dispatch_credential_profile_list(
        &self,
        req: CredentialProfileListRequest,
    ) -> Result<CredentialProfileListResponse, agent_client_protocol::Error> {
        self.on_credential_profile_list(req).await
    }

    #[custom_method(CredentialProfileCreateRequest)]
    async fn dispatch_credential_profile_create(
        &self,
        req: CredentialProfileCreateRequest,
    ) -> Result<CredentialProfileResponse, agent_client_protocol::Error> {
        self.on_credential_profile_create(req).await
    }

    #[custom_method(CredentialProfileUpdateRequest)]
    async fn dispatch_credential_profile_update(
        &self,
        req: CredentialProfileUpdateRequest,
    ) -> Result<CredentialProfileResponse, agent_client_protocol::Error> {
        self.on_credential_profile_update(req).await
    }

    #[custom_method(CredentialProfileDeleteRequest)]
    async fn dispatch_credential_profile_delete(
        &self,
        req: CredentialProfileDeleteRequest,
    ) -> Result<CredentialProfileDeleteResponse, agent_client_protocol::Error> {
        self.on_credential_profile_delete(req).await
    }

    #[custom_method(CredentialProfileUsageRequest)]
    async fn dispatch_credential_profile_usage(
        &self,
        req: CredentialProfileUsageRequest,
    ) -> Result<CredentialProfileUsageResponse, agent_client_protocol::Error> {
        self.on_credential_profile_usage(req).await
    }

    #[custom_method(CredentialProfileTestRequest)]
    async fn dispatch_credential_profile_test(
        &self,
        req: CredentialProfileTestRequest,
    ) -> Result<CredentialProfileTestResponse, agent_client_protocol::Error> {
        self.on_credential_profile_test(req).await
    }
}
