use crate::config::unix_millis;
use crate::{DocumentStore, StoreError};
use odoc_llm::{ProviderKind, ProviderProfile, ProviderProfileInput, builtin_provider_profiles};
use rusqlite::{OptionalExtension, params};
use std::collections::BTreeMap;
use std::str::FromStr;

type ProviderRow = (String, String, String, String, String, bool, bool);

impl DocumentStore {
    /// Lists configured model providers with built-in defaults filled in when not overridden.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider table cannot be read or contains invalid metadata.
    pub fn list_provider_profiles(&self) -> Result<Vec<ProviderProfile>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, kind, display_name, base_url, model, enabled, is_local
             FROM providers ORDER BY display_name COLLATE NOCASE, id",
        )?;
        let rows = statement.query_map([], read_provider_row)?;
        let mut profiles = builtin_provider_profiles()
            .into_iter()
            .map(|profile| (profile.id.clone(), profile))
            .collect::<BTreeMap<_, _>>();
        for row in rows {
            let profile = profile_from_row(row?)?;
            profiles.insert(profile.id.clone(), profile);
        }
        let mut profiles = profiles.into_values().collect::<Vec<_>>();
        profiles.sort_by(|left, right| {
            left.display_name
                .to_lowercase()
                .cmp(&right.display_name.to_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(profiles)
    }

    /// Loads one configured or built-in model provider.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ProviderNotFound`] when the identifier does not exist, or another
    /// store error when persisted metadata cannot be read or validated.
    pub fn provider_profile(&self, id: &str) -> Result<ProviderProfile, StoreError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, kind, display_name, base_url, model, enabled, is_local
                 FROM providers WHERE id = ?1",
                params![id],
                read_provider_row,
            )
            .optional()?;
        if let Some(row) = row {
            return profile_from_row(row);
        }
        builtin_provider_profiles()
            .into_iter()
            .find(|profile| profile.id == id)
            .ok_or_else(|| StoreError::ProviderNotFound(id.to_owned()))
    }

    /// Validates and persists non-sensitive provider metadata.
    ///
    /// API keys are intentionally absent from this input and must remain in the secret store.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid metadata, an invalid system clock, or a failed `SQLite` upsert.
    pub fn save_provider_profile(
        &self,
        input: ProviderProfileInput,
    ) -> Result<ProviderProfile, StoreError> {
        let profile = ProviderProfile::try_from(input)
            .map_err(|_| StoreError::InvalidProviderConfiguration)?;
        let now = unix_millis()?;
        self.connection.execute(
            "INSERT INTO providers(
                id, kind, display_name, base_url, model, enabled, is_local, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT(id) DO UPDATE SET
                kind = excluded.kind,
                display_name = excluded.display_name,
                base_url = excluded.base_url,
                model = excluded.model,
                enabled = excluded.enabled,
                is_local = excluded.is_local,
                updated_at = excluded.updated_at",
            params![
                profile.id,
                profile.kind.as_str(),
                profile.display_name,
                profile.base_url,
                profile.model,
                profile.enabled,
                profile.is_local,
                now,
            ],
        )?;
        Ok(profile)
    }
}

fn read_provider_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

fn profile_from_row(row: ProviderRow) -> Result<ProviderProfile, StoreError> {
    let (id, kind, display_name, base_url, model, enabled, is_local) = row;
    let kind =
        ProviderKind::from_str(&kind).map_err(|_| StoreError::InvalidProviderConfiguration)?;
    let profile = ProviderProfile {
        id,
        kind,
        display_name,
        base_url,
        model,
        enabled,
        is_local,
    };
    profile
        .validate()
        .map_err(|_| StoreError::InvalidProviderConfiguration)?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_profiles_are_available_without_persisting_secrets() {
        let store = DocumentStore::open_in_memory().expect("store should open");
        let profiles = store
            .list_provider_profiles()
            .expect("profiles should load");
        assert!(profiles.iter().any(|profile| profile.id == "ollama"));
        assert!(
            profiles
                .iter()
                .any(|profile| profile.id == "openai-compatible")
        );
    }

    #[test]
    fn provider_profiles_validate_round_trip_and_override_defaults() {
        let store = DocumentStore::open_in_memory().expect("store should open");
        let saved = store
            .save_provider_profile(ProviderProfileInput {
                id: "openai-compatible".to_owned(),
                kind: ProviderKind::OpenAiCompatible,
                display_name: "Private gateway".to_owned(),
                base_url: "http://localhost:8080/v1/".to_owned(),
                model: "qwen".to_owned(),
                enabled: false,
            })
            .expect("profile should save");
        assert!(saved.is_local);
        assert_eq!(saved.base_url, "http://localhost:8080/v1");
        assert_eq!(
            store
                .provider_profile("openai-compatible")
                .expect("profile should load"),
            saved
        );
        assert_eq!(
            store
                .list_provider_profiles()
                .expect("profiles should list")
                .iter()
                .filter(|profile| profile.id == "openai-compatible")
                .count(),
            1
        );
    }

    #[test]
    fn provider_profiles_reject_urls_that_can_hide_credentials() {
        let store = DocumentStore::open_in_memory().expect("store should open");
        let error = store
            .save_provider_profile(ProviderProfileInput {
                id: "bad".to_owned(),
                kind: ProviderKind::OpenAiCompatible,
                display_name: "Bad".to_owned(),
                base_url: "https://user:key@example.com".to_owned(),
                model: "model".to_owned(),
                enabled: true,
            })
            .expect_err("credential-bearing URL should fail");
        assert!(matches!(error, StoreError::InvalidProviderConfiguration));
    }
}
