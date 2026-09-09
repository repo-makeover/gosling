use super::*;
use gosling_providers::thinking::ThinkingEffort;

const SECRET_MASK_PREFIX_LEN: usize = 4;
const SECRET_MASK_FALLBACK: &str = "***";

fn mask_secret(secret: serde_json::Value) -> String {
    let as_string = match secret {
        serde_json::Value::String(s) => s,
        _ => serde_json::to_string(&secret).unwrap_or_else(|_| secret.to_string()),
    };

    let prefix: String = as_string.chars().take(SECRET_MASK_PREFIX_LEN).collect();
    if as_string.chars().count() <= SECRET_MASK_PREFIX_LEN {
        return SECRET_MASK_FALLBACK.to_string();
    }

    format!("{prefix}{SECRET_MASK_FALLBACK}")
}

impl GoslingAcpAgent {
    pub(super) async fn on_preferences_read(
        &self,
        req: PreferencesReadRequest,
    ) -> Result<PreferencesReadResponse, agent_client_protocol::Error> {
        let config = self.config()?;
        let keys = if req.keys.is_empty() {
            PREFERENCE_DEFS.iter().map(|def| def.key).collect()
        } else {
            req.keys
        };
        let mut values = Vec::with_capacity(keys.len());

        for key in keys {
            let def = preference_def(key)?;
            let value = match config.get_param::<serde_json::Value>(def.config_key) {
                Ok(value) => value,
                Err(crate::config::ConfigError::NotFound(_)) => serde_json::Value::Null,
                Err(e) => {
                    return Err(agent_client_protocol::Error::internal_error().data(e.to_string()))
                }
            };
            values.push(PreferenceValue { key, value });
        }

        Ok(PreferencesReadResponse { values })
    }

    pub(super) async fn on_preferences_save(
        &self,
        req: PreferencesSaveRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let config = self.config()?;
        let mut updates = Vec::with_capacity(req.values.len());

        for preference in &req.values {
            let def = preference_def(preference.key)?;
            let value = (def.prepare)(&preference.value)?;
            updates.push((def.config_key.to_string(), value));
        }

        validate_compaction_preferences(config, &updates)?;

        config.set_param_values(&updates).internal_err()?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_preferences_remove(
        &self,
        req: PreferencesRemoveRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let config = self.config()?;
        let removals = req
            .keys
            .iter()
            .map(|key| {
                Ok((
                    preference_def(*key)?.config_key.to_string(),
                    serde_json::Value::Null,
                ))
            })
            .collect::<Result<Vec<_>, agent_client_protocol::Error>>()?;
        validate_compaction_preferences(config, &removals)?;
        for key in req.keys {
            let def = preference_def(key)?;
            config.delete(def.config_key).internal_err()?;
        }
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_config_read(
        &self,
        req: ConfigReadRequest,
    ) -> Result<ConfigReadResponse, agent_client_protocol::Error> {
        let config = self.config()?;

        if req.key == "GOSLING_PROVIDER" || req.key == "active_provider" {
            let value = config
                .get_gosling_provider()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null);
            return Ok(ConfigReadResponse { value });
        }
        if req.key == "GOSLING_MODEL" {
            let value = config
                .get_gosling_model()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null);
            return Ok(ConfigReadResponse { value });
        }

        let value = match config.get(&req.key, req.is_secret) {
            Ok(value) if req.is_secret => serde_json::Value::String(mask_secret(value)),
            Ok(value) => value,
            Err(crate::config::ConfigError::NotFound(_)) => serde_json::Value::Null,
            Err(e) => {
                return Err(agent_client_protocol::Error::internal_error().data(e.to_string()))
            }
        };
        Ok(ConfigReadResponse { value })
    }

    pub(super) async fn on_config_upsert(
        &self,
        req: ConfigUpsertRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let config = self.config()?;

        if let Some(def) = PREFERENCE_DEFS.iter().find(|def| {
            def.config_key == req.key
                && matches!(
                    def.key,
                    PreferenceKey::AutoCompactThreshold | PreferenceKey::AutoCompactReduction
                )
        }) {
            if req.is_secret {
                return Err(agent_client_protocol::Error::invalid_params()
                    .data("Preferences cannot be stored as secrets"));
            }
            let value = (def.prepare)(&req.value)?;
            validate_compaction_preferences(config, &[(req.key.clone(), value)])?;
        }

        if req.key == "GOSLING_PROVIDER" {
            if let Some(name) = req.value.as_str() {
                let model = crate::config::get_provider_entry(config, name)
                    .map(|e| e.model)
                    .or_else(|| config.get_gosling_model().ok())
                    .unwrap_or_default();
                crate::config::set_active_provider(config, name, &model).internal_err()?;
                return Ok(EmptyResponse {});
            }
        }
        if req.key == "GOSLING_MODEL" {
            if let Some(model) = req.value.as_str() {
                if let Ok(provider) = config.get_gosling_provider() {
                    crate::config::set_active_provider(config, &provider, model).internal_err()?;
                    return Ok(EmptyResponse {});
                }
            }
        }

        config
            .set(&req.key, &req.value, req.is_secret)
            .internal_err()?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_config_remove(
        &self,
        req: ConfigRemoveRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let config = self.config()?;

        if !req.is_secret {
            validate_compaction_preferences(config, &[(req.key.clone(), serde_json::Value::Null)])?;
        }
        if req.is_secret {
            config.delete_secret(&req.key).internal_err()?;
        } else if req.key == "GOSLING_PROVIDER" || req.key == "active_provider" {
            config.delete("active_provider").internal_err()?;
            config.delete("GOSLING_PROVIDER").internal_err()?;
        } else if req.key == "GOSLING_MODEL" {
            if let Ok(provider) = config.get_gosling_provider() {
                crate::config::set_active_provider(config, &provider, "").internal_err()?;
            }
            config.delete("GOSLING_MODEL").internal_err()?;
        } else {
            config.delete(&req.key).internal_err()?;
        }

        Ok(EmptyResponse {})
    }

    pub(super) async fn on_config_read_all(
        &self,
        _req: ConfigReadAllRequest,
    ) -> Result<ConfigReadAllResponse, agent_client_protocol::Error> {
        let config = self.config()?;
        let values = config.all_values().internal_err()?;
        Ok(ConfigReadAllResponse { config: values })
    }

    pub(super) async fn on_defaults_read(
        &self,
        _req: DefaultsReadRequest,
    ) -> Result<DefaultsReadResponse, agent_client_protocol::Error> {
        let config = self.config()?;
        Ok(DefaultsReadResponse {
            provider_id: config.get_gosling_provider().ok(),
            model_id: config.get_gosling_model().ok(),
        })
    }

    pub(super) async fn on_defaults_save(
        &self,
        req: DefaultsSaveRequest,
    ) -> Result<DefaultsReadResponse, agent_client_protocol::Error> {
        let provider_id = req.provider_id.trim().to_string();
        if provider_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("providerId cannot be empty")
            );
        }

        let model_id = req.model_id.and_then(|model| {
            let model = model.trim().to_string();
            (!model.is_empty()).then_some(model)
        });

        let entries = self
            .provider_inventory
            .entries(std::slice::from_ref(&provider_id))
            .await
            .internal_err_ctx("Failed to read provider inventory")?;
        let Some(entry) = entries
            .into_iter()
            .find(|entry| entry.provider_id == provider_id)
        else {
            return Err(agent_client_protocol::Error::invalid_params()
                .data(format!("Unknown provider: {provider_id}")));
        };

        if !entry.configured {
            return Err(agent_client_protocol::Error::invalid_params()
                .data(format!("Provider is not configured: {provider_id}")));
        }

        if let Some(model_id) = model_id.as_deref() {
            self.validate_model_for_provider(&provider_id, model_id)
                .await?;
        }

        let config = self.config()?;
        let model = model_id.clone().unwrap_or_else(|| {
            crate::config::get_provider_entry(config, &provider_id)
                .map(|e| e.model)
                .unwrap_or_default()
        });
        crate::config::set_active_provider(config, &provider_id, &model)
            .internal_err_ctx("Failed to save default provider")?;

        Ok(DefaultsReadResponse {
            provider_id: Some(provider_id),
            model_id,
        })
    }

    pub(super) async fn on_defaults_clear(
        &self,
        _req: DefaultsClearRequest,
    ) -> Result<DefaultsReadResponse, agent_client_protocol::Error> {
        let config = self.config()?;
        crate::config::clear_active_provider(config)
            .internal_err_ctx("Failed to clear default provider")?;

        Ok(DefaultsReadResponse {
            provider_id: None,
            model_id: None,
        })
    }
}

struct PreferenceDef {
    key: PreferenceKey,
    config_key: &'static str,
    prepare: fn(&serde_json::Value) -> Result<serde_json::Value, agent_client_protocol::Error>,
}

const PREFERENCE_DEFS: &[PreferenceDef] = &[
    PreferenceDef {
        key: PreferenceKey::AutoCompactThreshold,
        config_key: "GOSLING_AUTO_COMPACT_THRESHOLD",
        prepare: prepare_auto_compact_threshold,
    },
    PreferenceDef {
        key: PreferenceKey::AutoCompactReduction,
        config_key: "GOSLING_AUTO_COMPACT_REDUCTION",
        prepare: prepare_auto_compact_reduction,
    },
    PreferenceDef {
        key: PreferenceKey::GoslingThinkingEffort,
        config_key: "GOSLING_THINKING_EFFORT",
        prepare: prepare_thinking_effort,
    },
    PreferenceDef {
        key: PreferenceKey::VoiceAutoSubmitPhrases,
        config_key: "VOICE_AUTO_SUBMIT_PHRASES",
        prepare: prepare_voice_auto_submit_phrases,
    },
    PreferenceDef {
        key: PreferenceKey::VoiceDictationProvider,
        config_key: "VOICE_DICTATION_PROVIDER",
        prepare: prepare_voice_dictation_provider,
    },
    PreferenceDef {
        key: PreferenceKey::VoiceDictationPreferredMic,
        config_key: "VOICE_DICTATION_PREFERRED_MIC",
        prepare: prepare_voice_dictation_preferred_mic,
    },
];

fn preference_def(
    key: PreferenceKey,
) -> Result<&'static PreferenceDef, agent_client_protocol::Error> {
    PREFERENCE_DEFS
        .iter()
        .find(|def| def.key == key)
        .ok_or_else(|| {
            agent_client_protocol::Error::internal_error()
                .data(format!("Missing preference definition for {key:?}"))
        })
}

// Null updates represent removals, which restore the runtime default for that preference.
fn validate_compaction_preferences(
    config: &Config,
    updates: &[(String, serde_json::Value)],
) -> Result<(), agent_client_protocol::Error> {
    const THRESHOLD: &str = "GOSLING_AUTO_COMPACT_THRESHOLD";
    const REDUCTION: &str = "GOSLING_AUTO_COMPACT_REDUCTION";
    if !updates
        .iter()
        .any(|(key, _)| key == THRESHOLD || key == REDUCTION)
    {
        return Ok(());
    }
    let resulting_value = |key: &str, default: f64| {
        updates
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map_or_else(
                || config.get_param::<f64>(key).unwrap_or(default),
                |(_, value)| value.as_f64().unwrap_or(default),
            )
    };
    let threshold = resulting_value(THRESHOLD, crate::context_mgmt::DEFAULT_COMPACTION_THRESHOLD);
    let reduction = resulting_value(
        REDUCTION,
        crate::context_mgmt::DEFAULT_AUTO_COMPACT_REDUCTION,
    );
    crate::context_mgmt::validate_compaction_settings(threshold, reduction)
        .map_err(|error| agent_client_protocol::Error::invalid_params().data(error.to_string()))
}

fn prepare_auto_compact_threshold(
    value: &serde_json::Value,
) -> Result<serde_json::Value, agent_client_protocol::Error> {
    let Some(threshold) = value.as_f64() else {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("autoCompactThreshold must be a number"));
    };
    crate::context_mgmt::validate_compaction_settings(threshold, 0.0)
        .map_err(|error| agent_client_protocol::Error::invalid_params().data(error.to_string()))?;

    Ok(value.clone())
}

fn prepare_auto_compact_reduction(
    value: &serde_json::Value,
) -> Result<serde_json::Value, agent_client_protocol::Error> {
    let Some(reduction) = value.as_f64() else {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("autoCompactReduction must be a number"));
    };
    crate::context_mgmt::validate_compaction_settings(0.0, reduction)
        .map_err(|error| agent_client_protocol::Error::invalid_params().data(error.to_string()))?;

    Ok(value.clone())
}

fn prepare_thinking_effort(
    value: &serde_json::Value,
) -> Result<serde_json::Value, agent_client_protocol::Error> {
    let Some(value) = value.as_str() else {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("goslingThinkingEffort must be a string"));
    };
    let effort = value.parse::<ThinkingEffort>().map_err(|err| {
        agent_client_protocol::Error::invalid_params()
            .data(format!("Invalid goslingThinkingEffort: {err}"))
    })?;

    Ok(serde_json::Value::String(effort.to_string()))
}

fn prepare_voice_auto_submit_phrases(
    value: &serde_json::Value,
) -> Result<serde_json::Value, agent_client_protocol::Error> {
    if !value.is_string() {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("voiceAutoSubmitPhrases must be a string"));
    }

    Ok(value.clone())
}

fn prepare_voice_dictation_provider(
    value: &serde_json::Value,
) -> Result<serde_json::Value, agent_client_protocol::Error> {
    let Some(value) = value.as_str() else {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("voiceDictationProvider must be a string"));
    };
    if !is_supported_voice_dictation_provider(value) {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("voiceDictationProvider is not supported"));
    }

    Ok(serde_json::Value::String(value.to_string()))
}

fn prepare_voice_dictation_preferred_mic(
    value: &serde_json::Value,
) -> Result<serde_json::Value, agent_client_protocol::Error> {
    let Some(value) = value.as_str() else {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("voiceDictationPreferredMic must be a string"));
    };
    if value.is_empty() {
        return Err(agent_client_protocol::Error::invalid_params()
            .data("voiceDictationPreferredMic must be non-empty"));
    }

    Ok(serde_json::Value::String(value.to_string()))
}

fn is_supported_voice_dictation_provider(value: &str) -> bool {
    matches!(value, "openai" | "groq" | "elevenlabs" | "__disabled__")
}

#[cfg(test)]
mod tests {
    use super::mask_secret;

    #[test]
    fn masks_secrets_without_exposing_their_length() {
        assert_eq!(
            mask_secret(serde_json::Value::String("abcdefgh".to_string())),
            "abcd***"
        );
        assert_eq!(
            mask_secret(serde_json::Value::String("abcdefghijklmnop".to_string())),
            "abcd***"
        );
        assert_eq!(
            mask_secret(serde_json::Value::String("abc".to_string())),
            "***"
        );
    }
}
