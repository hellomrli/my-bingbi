use crate::{DocumentStore, StoreError};
use odoc_kb::KbConfig;
use rusqlite::{OptionalExtension, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use ts_rs::TS;

const APP_CONFIG_KEY: &str = "app.config";
const KB_CONFIG_KEY: &str = "knowledge-base.config";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum AppTheme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[allow(
    clippy::struct_excessive_bools,
    reason = "settings record mirroring independent user toggles, not a state machine"
)]
pub struct AppConfig {
    pub schema_version: u32,
    pub theme: AppTheme,
    pub autosave_enabled: bool,
    pub font_mappings: BTreeMap<String, String>,
    #[serde(default)]
    pub first_run_completed: bool,
    #[serde(default)]
    pub auto_update_enabled: bool,
    #[serde(default)]
    pub crash_reporting_enabled: bool,
    #[serde(default)]
    pub active_pack_ids: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            theme: AppTheme::System,
            autosave_enabled: true,
            font_mappings: BTreeMap::new(),
            first_run_completed: false,
            auto_update_enabled: true,
            crash_reporting_enabled: false,
            active_pack_ids: Vec::new(),
        }
    }
}

impl DocumentStore {
    /// Loads the application configuration, returning defaults when it has not been saved yet.
    ///
    /// # Errors
    ///
    /// Returns an error when the settings table cannot be read or contains invalid JSON.
    pub fn app_config(&self) -> Result<AppConfig, StoreError> {
        Ok(self.get_setting(APP_CONFIG_KEY)?.unwrap_or_default())
    }

    /// Persists the complete non-sensitive application configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when serialization or the `SQLite` upsert fails.
    pub fn save_app_config(&self, config: &AppConfig) -> Result<(), StoreError> {
        self.set_setting(APP_CONFIG_KEY, config)
    }

    /// Loads non-sensitive knowledge-base settings, defaulting to a disabled FNS profile.
    ///
    /// # Errors
    ///
    /// Returns an error when the settings table cannot be read or contains incompatible JSON.
    pub fn kb_config(&self) -> Result<KbConfig, StoreError> {
        Ok(self.get_setting(KB_CONFIG_KEY)?.unwrap_or_default())
    }

    /// Validates, normalizes and stores knowledge-base metadata without credentials.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid provider metadata, serialization, or a failed `SQLite` write.
    pub fn save_kb_config(&self, config: KbConfig) -> Result<KbConfig, StoreError> {
        let normalized = config
            .normalized()
            .map_err(|_| StoreError::InvalidKnowledgeBaseConfiguration)?;
        self.set_setting(KB_CONFIG_KEY, &normalized)?;
        Ok(normalized)
    }

    /// Loads and deserializes a typed setting by key.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid key, a `SQLite` failure, or incompatible JSON.
    pub fn get_setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, StoreError> {
        validate_setting_key(key)?;
        let value: Option<String> = self
            .connection
            .query_row(
                "SELECT value_json FROM app_settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(StoreError::from)
    }

    /// Serializes and upserts a typed non-sensitive setting.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid key, serialization failure, invalid system time, or a
    /// `SQLite` write failure.
    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<(), StoreError> {
        validate_setting_key(key)?;
        let json = serde_json::to_string(value)?;
        self.connection.execute(
            "INSERT INTO app_settings(key, value_json, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
            params![key, json, unix_millis()?],
        )?;
        Ok(())
    }

    /// Deletes a setting and reports whether a row existed.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid key or a `SQLite` write failure.
    pub fn delete_setting(&self, key: &str) -> Result<bool, StoreError> {
        validate_setting_key(key)?;
        Ok(self
            .connection
            .execute("DELETE FROM app_settings WHERE key = ?1", params![key])?
            > 0)
    }
}

pub(crate) fn unix_millis() -> Result<i64, StoreError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::InvalidSystemClock)?
        .as_millis();
    i64::try_from(millis).map_err(|_| StoreError::InvalidSystemClock)
}

fn validate_setting_key(key: &str) -> Result<(), StoreError> {
    let valid = !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(StoreError::InvalidSettingKey(key.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_app_config_uses_defaults() {
        let store = DocumentStore::open_in_memory().expect("store should open");
        assert_eq!(
            store.app_config().expect("config should load"),
            AppConfig::default()
        );
    }

    #[test]
    fn app_config_round_trips() {
        let store = DocumentStore::open_in_memory().expect("store should open");
        let mut config = AppConfig {
            theme: AppTheme::Dark,
            autosave_enabled: false,
            ..AppConfig::default()
        };
        config
            .font_mappings
            .insert("仿宋_GB2312".to_owned(), "Noto Serif CJK SC".to_owned());
        store.save_app_config(&config).expect("config should save");
        assert_eq!(store.app_config().expect("config should load"), config);
    }

    #[test]
    fn generic_settings_validate_keys_and_delete() {
        let store = DocumentStore::open_in_memory().expect("store should open");
        store
            .set_setting("provider.default", &"ollama")
            .expect("setting should save");
        let value: Option<String> = store
            .get_setting("provider.default")
            .expect("setting should load");
        assert_eq!(value.as_deref(), Some("ollama"));
        assert!(
            store
                .delete_setting("provider.default")
                .expect("setting should delete")
        );
        assert!(matches!(
            store.set_setting("../secret", &true),
            Err(StoreError::InvalidSettingKey(_))
        ));
    }

    #[test]
    fn knowledge_base_config_round_trips_without_credentials() {
        let store = DocumentStore::open_in_memory().expect("store should open");
        assert_eq!(store.kb_config().unwrap(), KbConfig::default());
        let saved = store
            .save_kb_config(KbConfig {
                enabled: true,
                vault: " 政务库 ".to_owned(),
                ..KbConfig::default()
            })
            .expect("config should save");
        assert_eq!(saved.vault, "政务库");
        assert_eq!(store.kb_config().unwrap(), saved);
        let json = serde_json::to_string(&saved).unwrap();
        assert!(!json.to_ascii_lowercase().contains("token"));
    }
}
