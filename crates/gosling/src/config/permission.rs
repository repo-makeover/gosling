use crate::config::paths::Paths;
use fs2::FileExt;
use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};
use tracing;
use utoipa::ToSchema;

const PERMISSION_FILE: &str = "permission.yaml";

static PERMISSION_MANAGERS: LazyLock<Mutex<HashMap<PathBuf, Weak<PermissionManager>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A stored decision within one policy category; other inspectors may still restrict a call.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    AlwaysAllow,
    AskBefore,
    NeverAllow,
}

/// The YAML representation of one category's principals, grouped by decision.
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct PermissionConfig {
    pub always_allow: Vec<String>,
    pub ask_before: Vec<String>,
    pub never_allow: Vec<String>,
}

/// Reads current on-disk policy and serializes permission updates across processes.
///
/// Lookups reload the file so existing handles observe revocations. An unreadable
/// policy returns `NeverAllow`; failed writes leave the stored policy unchanged.
#[derive(Debug)]
pub struct PermissionManager {
    config_path: PathBuf,
}

const USER_PERMISSION: &str = "user";
const SMART_APPROVE_PERMISSION: &str = "smart_approve";
const EGRESS_DOMAIN_PERMISSION: &str = "egress_domain";
const ACP_PROVIDER_PERMISSION: &str = "acp_provider";

impl PermissionManager {
    /// Validates an existing policy at startup, panicking if it cannot be read or parsed.
    pub fn new(config_dir: PathBuf) -> Self {
        let permission_path = config_dir.join(PERMISSION_FILE);
        let _: HashMap<String, PermissionConfig> = if permission_path.exists() {
            let file_contents =
                fs::read_to_string(&permission_path).expect("Failed to read permission.yaml");
            serde_yaml::from_str(&file_contents).unwrap_or_else(|e| {
                tracing::error!(
                    "Failed to parse {}: {}. Refusing to start with corrupted permission config.",
                    permission_path.display(),
                    e,
                );
                panic!(
                    "Corrupted permission config at {}. Fix or remove the file to continue.",
                    permission_path.display(),
                );
            })
        } else {
            // Directory creation failure is deferred to the normal read/write error paths.
            if let Err(e) = fs::create_dir_all(&config_dir) {
                tracing::error!("Failed to create config directory {config_dir:?}: {e}");
            }
            HashMap::new()
        };
        PermissionManager {
            config_path: permission_path,
        }
    }

    pub fn instance() -> Arc<PermissionManager> {
        Self::for_config_dir(Paths::config_dir())
    }

    pub fn for_config_dir(config_dir: PathBuf) -> Arc<PermissionManager> {
        let mut managers = PERMISSION_MANAGERS
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(manager) = managers.get(&config_dir).and_then(Weak::upgrade) {
            return manager;
        }

        let manager = Arc::new(Self::new(config_dir.clone()));
        managers.insert(config_dir, Arc::downgrade(&manager));
        manager
    }

    fn acquire_lock(&self, exclusive: bool) -> anyhow::Result<fs::File> {
        // Lock a stable sidecar: atomic replacement changes the YAML file's inode.
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(self.config_path.with_extension("yaml.lock"))?;
        if exclusive {
            FileExt::lock_exclusive(&file)?;
        } else {
            FileExt::lock_shared(&file)?;
        }
        Ok(file)
    }

    // Callers hold the sidecar lock, either for a snapshot or the full write transaction.
    fn read_permissions_file_unlocked(&self) -> anyhow::Result<HashMap<String, PermissionConfig>> {
        match fs::read_to_string(&self.config_path) {
            Ok(contents) => Ok(serde_yaml::from_str(&contents)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(error) => Err(error.into()),
        }
    }

    fn read_permissions_snapshot(&self) -> anyhow::Result<HashMap<String, PermissionConfig>> {
        let _lock = self.acquire_lock(false)?;
        self.read_permissions_file_unlocked()
    }

    fn mutate_permissions(
        &self,
        mutate: impl FnOnce(&mut HashMap<String, PermissionConfig>),
    ) -> anyhow::Result<()> {
        let _lock = self.acquire_lock(true)?;
        // Reload under the write lock so a stale handle cannot overwrite another writer.
        let mut permissions = self.read_permissions_file_unlocked()?;
        mutate(&mut permissions);
        let yaml = serde_yaml::to_string(&permissions)?;
        crate::config::base::write_file_atomic(&self.config_path, &yaml)?;
        Ok(())
    }

    fn apply_permission_updates(
        &self,
        category: &str,
        updates: &[(String, PermissionLevel)],
    ) -> Result<(), String> {
        self.mutate_permissions(|map| {
            let permission_config = map.entry(category.to_string()).or_default();

            for (principal_name, level) in updates {
                permission_config
                    .always_allow
                    .retain(|principal| principal != principal_name);
                permission_config
                    .ask_before
                    .retain(|principal| principal != principal_name);
                permission_config
                    .never_allow
                    .retain(|principal| principal != principal_name);

                match level {
                    PermissionLevel::AlwaysAllow => {
                        permission_config.always_allow.push(principal_name.clone())
                    }
                    PermissionLevel::AskBefore => {
                        permission_config.ask_before.push(principal_name.clone())
                    }
                    PermissionLevel::NeverAllow => {
                        permission_config.never_allow.push(principal_name.clone())
                    }
                }
            }
        })
        .map_err(|error| error.to_string())
    }

    /// Returns policy category names, or an empty list after logging a read failure.
    pub fn get_permission_names(&self) -> Vec<String> {
        match self.read_permissions_snapshot() {
            Ok(map) => map.keys().cloned().collect(),
            Err(error) => {
                tracing::error!(%error, path = ?self.config_path, "Could not read permission names");
                Vec::new()
            }
        }
    }

    pub fn get_user_permission(&self, principal_name: &str) -> Option<PermissionLevel> {
        self.get_permission(USER_PERMISSION, principal_name)
    }

    pub fn get_smart_approve_permission(&self, principal_name: &str) -> Option<PermissionLevel> {
        self.get_permission(SMART_APPROVE_PERMISSION, principal_name)
    }

    pub fn get_egress_domain_permission(&self, domain: &str) -> Option<PermissionLevel> {
        self.get_permission(EGRESS_DOMAIN_PERMISSION, domain)
    }

    /// Looks up a provider/tool pair without sharing authority with another provider.
    pub fn get_acp_provider_permission(
        &self,
        provider_name: &str,
        tool_name: &str,
    ) -> Option<PermissionLevel> {
        self.get_permission(
            ACP_PROVIDER_PERMISSION,
            &acp_provider_principal(provider_name, tool_name),
        )
    }

    pub fn get_config_path(&self) -> &Path {
        self.config_path.as_path()
    }

    /// Tool annotations are supplied by the MCP server and can only tighten
    /// policy. A server may accurately declare a tool as mutating, but it
    /// cannot grant itself authority by claiming that a tool is read-only.
    pub fn apply_tool_annotations(&self, tools: &[Tool]) {
        let mut mutating_tool_names = Vec::new();
        for tool in tools {
            let Some(annotations) = &tool.annotations else {
                continue;
            };
            if annotations.read_only_hint == Some(false) {
                mutating_tool_names.push(tool.name.to_string());
            }
        }
        if !mutating_tool_names.is_empty() {
            self.bulk_update_smart_approve_permissions(
                &mutating_tool_names,
                PermissionLevel::AskBefore,
            );
        }
    }

    fn bulk_update_smart_approve_permissions(&self, tool_names: &[String], level: PermissionLevel) {
        let updates = tool_names
            .iter()
            .cloned()
            .map(|tool_name| (tool_name, level.clone()))
            .collect::<Vec<_>>();
        if let Err(e) = self.apply_permission_updates(SMART_APPROVE_PERMISSION, &updates) {
            tracing::error!(
                security.event_type = "permission_persist_failed",
                error = %e,
                path = ?self.config_path,
                "tool annotations could not be saved"
            );
        }
    }

    fn get_permission(&self, category: &str, principal_name: &str) -> Option<PermissionLevel> {
        let permissions = match self.read_permissions_snapshot() {
            Ok(permissions) => permissions,
            Err(error) => {
                tracing::error!(
                    security.event_type = "permission_read_failed",
                    %error,
                    path = ?self.config_path,
                    "Permission state is unavailable; refusing to reuse authority"
                );
                return Some(PermissionLevel::NeverAllow);
            }
        };
        if let Some(permission_config) = permissions.get(category) {
            // A denial outranks the other levels: a principal listed in never_allow stays denied
            // even when a stale entry also lists it under always_allow or ask_before.
            if permission_config
                .never_allow
                .contains(&principal_name.to_string())
            {
                return Some(PermissionLevel::NeverAllow);
            } else if permission_config
                .always_allow
                .contains(&principal_name.to_string())
            {
                return Some(PermissionLevel::AlwaysAllow);
            } else if permission_config
                .ask_before
                .contains(&principal_name.to_string())
            {
                return Some(PermissionLevel::AskBefore);
            }
        }
        None
    }

    pub fn update_user_permission(
        &self,
        principal_name: &str,
        level: PermissionLevel,
    ) -> Result<(), String> {
        self.update_permission(USER_PERMISSION, principal_name, level)
    }

    /// Commits the entire batch under one lock; an error means no update was published.
    pub fn bulk_update_user_permissions(
        &self,
        updates: &[(String, PermissionLevel)],
    ) -> Result<(), String> {
        self.apply_permission_updates(USER_PERMISSION, updates)
    }

    pub fn update_smart_approve_permission(
        &self,
        principal_name: &str,
        level: PermissionLevel,
    ) -> Result<(), String> {
        self.update_permission(SMART_APPROVE_PERMISSION, principal_name, level)
    }

    pub fn update_egress_domain_permission(
        &self,
        domain: &str,
        level: PermissionLevel,
    ) -> Result<(), String> {
        self.update_permission(EGRESS_DOMAIN_PERMISSION, domain, level)
    }

    pub fn update_acp_provider_permission(
        &self,
        provider_name: &str,
        tool_name: &str,
        level: PermissionLevel,
    ) -> Result<(), String> {
        self.update_permission(
            ACP_PROVIDER_PERMISSION,
            &acp_provider_principal(provider_name, tool_name),
            level,
        )
    }

    fn update_permission(
        &self,
        category: &str,
        principal_name: &str,
        level: PermissionLevel,
    ) -> Result<(), String> {
        self.apply_permission_updates(category, &[(principal_name.to_string(), level)])
    }

    /// Removes all permission entries in an extension's tool namespace.
    /// An empty extension name clears every category's entries.
    pub fn remove_extension(&self, extension_name: &str) -> anyhow::Result<()> {
        self.mutate_permissions(|map| {
            let prefix = format!("{extension_name}__");
            let belongs_to_extension = |principal: &String| {
                extension_name.is_empty() || principal.starts_with(prefix.as_str())
            };
            for permission_config in map.values_mut() {
                permission_config
                    .always_allow
                    .retain(|principal| !belongs_to_extension(principal));
                permission_config
                    .ask_before
                    .retain(|principal| !belongs_to_extension(principal));
                permission_config
                    .never_allow
                    .retain(|principal| !belongs_to_extension(principal));
            }
        })
    }
}

fn acp_provider_principal(provider_name: &str, tool_name: &str) -> String {
    // Tuple encoding keeps provider and tool names distinct even when they contain separators.
    serde_json::to_string(&(provider_name, tool_name))
        .expect("ACP provider permission principal must serialize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::ToolAnnotations;
    use rmcp::object;
    use tempfile::TempDir;

    // Helper function to create a test instance of PermissionManager with a temp dir
    fn create_test_permission_manager() -> (PermissionManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let manager = PermissionManager::new(temp_dir.path().to_path_buf());
        (manager, temp_dir)
    }

    #[test]
    fn config_directory_reuses_one_permission_manager() {
        let temp_dir = TempDir::new().unwrap();
        let first = PermissionManager::for_config_dir(temp_dir.path().to_path_buf());
        let second = PermissionManager::for_config_dir(temp_dir.path().to_path_buf());

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn acp_provider_permissions_are_scoped_and_durable() {
        let temp_dir = TempDir::new().unwrap();
        let manager = PermissionManager::new(temp_dir.path().to_path_buf());
        manager
            .update_acp_provider_permission("provider-a", "Bash", PermissionLevel::AlwaysAllow)
            .unwrap();

        let reloaded = PermissionManager::new(temp_dir.path().to_path_buf());
        assert_eq!(
            reloaded.get_acp_provider_permission("provider-a", "Bash"),
            Some(PermissionLevel::AlwaysAllow)
        );
        assert_eq!(
            reloaded.get_acp_provider_permission("provider-b", "Bash"),
            None
        );
        assert_eq!(
            reloaded.get_acp_provider_permission("provider-a", "Read"),
            None
        );
    }

    // A `NeverAllow` that fails to write used to be swallowed by `persist`,
    // so the operator was told nothing and the denial silently did not survive
    // a restart (STT-GOS-005).
    #[cfg(unix)]
    #[test]
    fn a_permission_that_cannot_be_saved_reports_the_failure() {
        let (manager, temp_dir) = create_test_permission_manager();
        std::fs::remove_dir(temp_dir.path()).unwrap();
        std::fs::write(temp_dir.path(), "not a directory").unwrap();

        let result =
            manager.update_user_permission("developer__shell", PermissionLevel::NeverAllow);

        std::fs::remove_file(temp_dir.path()).unwrap();
        std::fs::create_dir(temp_dir.path()).unwrap();

        assert!(
            result.is_err(),
            "a permission write that cannot be persisted must not report success"
        );
        assert_eq!(manager.get_user_permission("developer__shell"), None);
    }

    #[test]
    fn a_permission_that_saves_cleanly_reports_success() {
        let (manager, _temp_dir) = create_test_permission_manager();
        assert!(manager
            .update_user_permission("developer__shell", PermissionLevel::NeverAllow)
            .is_ok());
    }

    #[test]
    fn bulk_user_permission_update_persists_all_entries_together() {
        let (manager, _temp_dir) = create_test_permission_manager();
        manager
            .bulk_update_user_permissions(&[
                ("extension__one".to_string(), PermissionLevel::AlwaysAllow),
                ("extension__two".to_string(), PermissionLevel::NeverAllow),
            ])
            .unwrap();

        assert_eq!(
            manager.get_user_permission("extension__one"),
            Some(PermissionLevel::AlwaysAllow)
        );
        assert_eq!(
            manager.get_user_permission("extension__two"),
            Some(PermissionLevel::NeverAllow)
        );
    }

    #[test]
    fn test_get_permission_names_empty() {
        let (manager, _temp_dir) = create_test_permission_manager();

        assert!(manager.get_permission_names().is_empty());
    }

    #[test]
    fn test_update_user_permission() {
        let (manager, _temp_dir) = create_test_permission_manager();
        manager
            .update_user_permission("tool1", PermissionLevel::AlwaysAllow)
            .unwrap();

        let permission = manager.get_user_permission("tool1");
        assert_eq!(permission, Some(PermissionLevel::AlwaysAllow));
    }

    #[test]
    fn test_update_smart_approve_permission() {
        let (manager, _temp_dir) = create_test_permission_manager();
        manager
            .update_smart_approve_permission("tool2", PermissionLevel::AskBefore)
            .unwrap();

        let permission = manager.get_smart_approve_permission("tool2");
        assert_eq!(permission, Some(PermissionLevel::AskBefore));
    }

    #[test]
    fn test_get_permission_not_found() {
        let (manager, _temp_dir) = create_test_permission_manager();

        let permission = manager.get_user_permission("non_existent_tool");
        assert_eq!(permission, None);
    }

    #[test]
    fn test_permission_levels() {
        let (manager, _temp_dir) = create_test_permission_manager();

        manager
            .update_user_permission("tool4", PermissionLevel::AlwaysAllow)
            .unwrap();
        manager
            .update_user_permission("tool5", PermissionLevel::AskBefore)
            .unwrap();
        manager
            .update_user_permission("tool6", PermissionLevel::NeverAllow)
            .unwrap();

        // Check the permission levels
        assert_eq!(
            manager.get_user_permission("tool4"),
            Some(PermissionLevel::AlwaysAllow)
        );
        assert_eq!(
            manager.get_user_permission("tool5"),
            Some(PermissionLevel::AskBefore)
        );
        assert_eq!(
            manager.get_user_permission("tool6"),
            Some(PermissionLevel::NeverAllow)
        );
    }

    #[test]
    fn test_permission_update_replaces_existing_level() {
        let (manager, _temp_dir) = create_test_permission_manager();

        // Initially AlwaysAllow
        manager
            .update_user_permission("tool7", PermissionLevel::AlwaysAllow)
            .unwrap();
        assert_eq!(
            manager.get_user_permission("tool7"),
            Some(PermissionLevel::AlwaysAllow)
        );

        // Now change to NeverAllow
        manager
            .update_user_permission("tool7", PermissionLevel::NeverAllow)
            .unwrap();
        assert_eq!(
            manager.get_user_permission("tool7"),
            Some(PermissionLevel::NeverAllow)
        );

        // Ensure it's removed from other levels
        let map = manager.read_permissions_snapshot().unwrap();
        let config = map.get(USER_PERMISSION).unwrap();
        assert!(!config.always_allow.contains(&"tool7".to_string()));
        assert!(!config.ask_before.contains(&"tool7".to_string()));
        assert!(config.never_allow.contains(&"tool7".to_string()));
    }

    #[test]
    fn test_remove_extension() {
        let (manager, _temp_dir) = create_test_permission_manager();
        manager
            .update_user_permission("prefix__tool1", PermissionLevel::AlwaysAllow)
            .unwrap();
        manager
            .update_user_permission("nonprefix__tool2", PermissionLevel::AlwaysAllow)
            .unwrap();
        manager
            .update_user_permission("prefix__tool3", PermissionLevel::AskBefore)
            .unwrap();
        manager
            .update_user_permission("prefix-extra__tool4", PermissionLevel::AlwaysAllow)
            .unwrap();

        // Remove entries starting with "prefix"
        manager.remove_extension("prefix").unwrap();

        let map = manager.read_permissions_snapshot().unwrap();
        let config = map.get(USER_PERMISSION).unwrap();

        // Verify entries with "prefix" are removed
        assert!(!config.always_allow.contains(&"prefix__tool1".to_string()));
        assert!(!config.ask_before.contains(&"prefix__tool3".to_string()));

        // Verify other entries remain
        assert!(config
            .always_allow
            .contains(&"nonprefix__tool2".to_string()));
        assert!(config
            .always_allow
            .contains(&"prefix-extra__tool4".to_string()));
    }

    #[test]
    fn test_remove_extension_fails_closed_when_storage_is_unreadable() {
        let (manager, _temp_dir) = create_test_permission_manager();
        manager
            .update_user_permission("prefix__tool1", PermissionLevel::AlwaysAllow)
            .unwrap();
        fs::remove_file(&manager.config_path).unwrap();
        fs::create_dir(&manager.config_path).unwrap();

        assert!(manager.remove_extension("prefix").is_err());
        assert_eq!(
            manager.get_user_permission("prefix__tool1"),
            Some(PermissionLevel::NeverAllow)
        );
    }

    #[test]
    fn test_persisted_never_allow_takes_precedence_over_other_levels() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join(PERMISSION_FILE),
            r#"user:
  always_allow:
    - denied_from_allow
    - allowed
  ask_before:
    - denied_from_ask
    - prompted
  never_allow:
    - denied_from_allow
    - denied_from_ask
    - denied
"#,
        )
        .unwrap();

        let manager = PermissionManager::new(temp_dir.path().to_path_buf());

        assert_eq!(
            manager.get_user_permission("denied_from_allow"),
            Some(PermissionLevel::NeverAllow)
        );
        assert_eq!(
            manager.get_user_permission("denied_from_ask"),
            Some(PermissionLevel::NeverAllow)
        );
        assert_eq!(
            manager.get_user_permission("allowed"),
            Some(PermissionLevel::AlwaysAllow)
        );
        assert_eq!(
            manager.get_user_permission("prompted"),
            Some(PermissionLevel::AskBefore)
        );
        assert_eq!(
            manager.get_user_permission("denied"),
            Some(PermissionLevel::NeverAllow)
        );
        assert_eq!(manager.get_user_permission("unknown"), None);
    }

    #[test]
    #[should_panic(expected = "Corrupted permission config")]
    fn test_corrupted_permission_file_panics() {
        let temp_dir = TempDir::new().unwrap();
        let permission_path = temp_dir.path().join(PERMISSION_FILE);
        fs::write(&permission_path, "{{invalid yaml: [broken").unwrap();
        PermissionManager::new(temp_dir.path().to_path_buf());
    }

    use test_case::test_case;

    #[test_case(
        vec![Tool::new("tool".to_string(), String::new(), object!({"type": "object"}))
            .annotate(ToolAnnotations::new().read_only(false))],
        Some(PermissionLevel::AskBefore);
        "write_annotation_caches_ask"
    )]
    #[test_case(
        vec![Tool::new("tool".to_string(), String::new(), object!({"type": "object"}))],
        None;
        "unannotated_left_uncached"
    )]
    #[test_case(
        vec![Tool::new("tool".to_string(), String::new(), object!({"type": "object"}))
            .annotate(ToolAnnotations::new().read_only(true))],
        None;
        "readonly_annotation_cannot_grant_authority"
    )]
    #[test_case(
        vec![Tool::new("delete_all_records".to_string(), String::new(), object!({"type": "object"}))
            .annotate(ToolAnnotations::new().read_only(true))],
        None;
        "readonly_annotation_with_destructive_name_is_not_trusted"
    )]
    fn test_apply_tool_annotations(tools: Vec<Tool>, expect_cache: Option<PermissionLevel>) {
        let (manager, _temp_dir) = create_test_permission_manager();
        let tool_name = tools[0].name.to_string();
        manager.apply_tool_annotations(&tools);
        assert_eq!(
            manager.get_smart_approve_permission(&tool_name),
            expect_cache
        );
    }
}
