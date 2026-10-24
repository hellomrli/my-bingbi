//! Configuration, credentials, crash events, and pack management commands.

use crate::commands::kb::wake_kb_sync;
use crate::error::{IpcError, map_kb_error, map_secret_error, map_store_error};
use crate::state::AppState;
use crate::util::{
    KB_FNS_TOKEN_ACCOUNT, KB_FNS_WS_TOKEN_ACCOUNT, KB_MCP_TOKEN_ACCOUNT, lock_documents,
    pack_summaries, provider_secret_account, active_packs,
};
use odoc_kb::{KbConfig, KbProviderKind, LocalVaultProvider};
use odoc_llm::{ProviderProfile, ProviderProfileInput};
use odoc_pack::{PackRegistry, PackSummary, TemplateSummary};
use odoc_store::{AppConfig, CrashEvent};
use tauri::State;

const MAX_KB_TOKEN_BYTES: usize = 16 * 1024;

#[tauri::command]
pub(crate) fn list_pack_summaries(packs: State<'_, PackRegistry>) -> Vec<PackSummary> {
    pack_summaries(&packs)
}

/// Lists the document templates available to the drafting UI.
#[tauri::command]
pub(crate) fn list_templates(state: State<'_, AppState>, packs: State<'_, PackRegistry>) -> Result<Vec<TemplateSummary>, IpcError> {
    Ok(active_packs(&state, &packs)?.list_templates())
}

#[tauri::command]
pub(crate) fn user_pack_root(state: State<'_, AppState>) -> Result<String, IpcError> {
    Ok(state.user_pack_root.to_string_lossy().into_owned())
}

#[tauri::command]
pub(crate) fn pack_load_issues(state: State<'_, AppState>) -> Result<Vec<String>, IpcError> {
    Ok(state.pack_issues.clone())
}

/// Lists opt-in Rust panic diagnostics retained only in the local application database.
#[tauri::command]
pub(crate) fn list_crash_events(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<CrashEvent>, IpcError> {
    let limit = limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(IpcError {
            code: "invalid_input",
            message: "诊断记录数量范围无效。",
        });
    }
    lock_documents(&state)?
        .list_crash_events(limit)
        .map_err(map_store_error)
}

/// Clears every locally retained panic diagnostic and returns the number removed.
#[tauri::command]
pub(crate) fn clear_crash_events(state: State<'_, AppState>) -> Result<u32, IpcError> {
    let deleted = lock_documents(&state)?
        .clear_crash_events()
        .map_err(map_store_error)?;
    Ok(deleted.try_into().unwrap_or(u32::MAX))
}

#[tauri::command]
pub(crate) fn load_app_config(state: State<'_, AppState>) -> Result<AppConfig, IpcError> {
    lock_documents(&state)?
        .app_config()
        .map_err(map_store_error)
}

#[tauri::command]
pub(crate) fn save_app_config(
    state: State<'_, AppState>,
    config: AppConfig,
) -> Result<(), IpcError> {
    validate_app_config(&config)?;
    lock_documents(&state)?
        .save_app_config(&config)
        .map_err(map_store_error)?;
    state
        .crash_reporter
        .set_enabled(config.crash_reporting_enabled);
    Ok(())
}

#[tauri::command]
pub(crate) fn load_kb_config(state: State<'_, AppState>) -> Result<KbConfig, IpcError> {
    lock_documents(&state)?.kb_config().map_err(map_store_error)
}

#[tauri::command]
pub(crate) fn save_kb_config(
    state: State<'_, AppState>,
    config: KbConfig,
) -> Result<KbConfig, IpcError> {
    let normalized = config.normalized().map_err(map_kb_error)?;
    if normalized.enabled && normalized.provider_kind == KbProviderKind::LocalVault {
        LocalVaultProvider::open(&normalized.local_path, normalized.vault.clone())
            .map_err(map_kb_error)?;
    }
    let store = lock_documents(&state)?;
    let previous = store.kb_config().map_err(map_store_error)?;
    if kb_cache_identity(&previous) != kb_cache_identity(&normalized) {
        if !previous.vault.trim().is_empty() {
            store
                .clear_kb_index(&previous.vault)
                .map_err(map_store_error)?;
        }
        if previous.vault != normalized.vault && !normalized.vault.is_empty() {
            store
                .clear_kb_index(&normalized.vault)
                .map_err(map_store_error)?;
        }
    }
    let saved = store.save_kb_config(normalized).map_err(map_store_error)?;
    drop(store);
    wake_kb_sync(&state);
    Ok(saved)
}

#[tauri::command]
pub(crate) fn set_kb_fns_token(state: State<'_, AppState>, token: String) -> Result<(), IpcError> {
    let token = token.trim();
    if token.is_empty() || token.len() > MAX_KB_TOKEN_BYTES || token.chars().any(char::is_control) {
        return Err(IpcError {
            code: "invalid_input",
            message: "FNS token 格式无效。",
        });
    }
    state
        .secrets
        .set_secret(KB_FNS_TOKEN_ACCOUNT, token)
        .map_err(map_secret_error)?;
    wake_kb_sync(&state);
    Ok(())
}

#[tauri::command]
pub(crate) fn has_kb_fns_token(state: State<'_, AppState>) -> Result<bool, IpcError> {
    state
        .secrets
        .has_secret(KB_FNS_TOKEN_ACCOUNT)
        .map_err(map_secret_error)
}

#[tauri::command]
pub(crate) fn delete_kb_fns_token(state: State<'_, AppState>) -> Result<bool, IpcError> {
    let deleted = state
        .secrets
        .delete_secret(KB_FNS_TOKEN_ACCOUNT)
        .map_err(map_secret_error)?;
    wake_kb_sync(&state);
    Ok(deleted)
}

#[tauri::command]
pub(crate) fn set_kb_fns_ws_token(
    state: State<'_, AppState>,
    token: String,
) -> Result<(), IpcError> {
    save_kb_secret(
        &state,
        KB_FNS_WS_TOKEN_ACCOUNT,
        token,
        "FNS WebSocket token 格式无效。",
    )?;
    wake_kb_sync(&state);
    Ok(())
}

#[tauri::command]
pub(crate) fn has_kb_fns_ws_token(state: State<'_, AppState>) -> Result<bool, IpcError> {
    state
        .secrets
        .has_secret(KB_FNS_WS_TOKEN_ACCOUNT)
        .map_err(map_secret_error)
}

#[tauri::command]
pub(crate) fn delete_kb_fns_ws_token(state: State<'_, AppState>) -> Result<bool, IpcError> {
    let deleted = state
        .secrets
        .delete_secret(KB_FNS_WS_TOKEN_ACCOUNT)
        .map_err(map_secret_error)?;
    wake_kb_sync(&state);
    Ok(deleted)
}

#[tauri::command]
pub(crate) fn set_kb_mcp_token(state: State<'_, AppState>, token: String) -> Result<(), IpcError> {
    save_kb_secret(&state, KB_MCP_TOKEN_ACCOUNT, token, "MCP token 格式无效。")
}

#[tauri::command]
pub(crate) fn has_kb_mcp_token(state: State<'_, AppState>) -> Result<bool, IpcError> {
    state
        .secrets
        .has_secret(KB_MCP_TOKEN_ACCOUNT)
        .map_err(map_secret_error)
}

#[tauri::command]
pub(crate) fn delete_kb_mcp_token(state: State<'_, AppState>) -> Result<bool, IpcError> {
    state
        .secrets
        .delete_secret(KB_MCP_TOKEN_ACCOUNT)
        .map_err(map_secret_error)
}

#[tauri::command]
pub(crate) fn list_provider_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderProfile>, IpcError> {
    lock_documents(&state)?
        .list_provider_profiles()
        .map_err(map_store_error)
}

#[tauri::command]
pub(crate) fn save_provider_profile(
    state: State<'_, AppState>,
    profile: ProviderProfileInput,
) -> Result<ProviderProfile, IpcError> {
    lock_documents(&state)?
        .save_provider_profile(profile)
        .map_err(map_store_error)
}

#[tauri::command]
pub(crate) fn set_provider_secret(
    state: State<'_, AppState>,
    provider_id: String,
    secret: String,
) -> Result<(), IpcError> {
    let account = provider_secret_account(&provider_id)?;
    state
        .secrets
        .set_secret(&account, &secret)
        .map_err(map_secret_error)
}

#[tauri::command]
pub(crate) fn has_provider_secret(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<bool, IpcError> {
    let account = provider_secret_account(&provider_id)?;
    state.secrets.has_secret(&account).map_err(map_secret_error)
}

#[tauri::command]
pub(crate) fn delete_provider_secret(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<bool, IpcError> {
    let account = provider_secret_account(&provider_id)?;
    state
        .secrets
        .delete_secret(&account)
        .map_err(map_secret_error)
}

fn save_kb_secret(
    state: &AppState,
    account: &str,
    value: String,
    message: &'static str,
) -> Result<(), IpcError> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_KB_TOKEN_BYTES || value.chars().any(char::is_control) {
        return Err(IpcError {
            code: "invalid_input",
            message,
        });
    }
    state
        .secrets
        .set_secret(account, value)
        .map_err(map_secret_error)
}

fn validate_app_config(config: &AppConfig) -> Result<(), IpcError> {
    let mappings_valid = config.font_mappings.len() <= 128
        && config.font_mappings.iter().all(|(source, target)| {
            !source.trim().is_empty()
                && source.len() <= 256
                && !target.trim().is_empty()
                && target.len() <= 256
                && !source.chars().any(char::is_control)
                && !target.chars().any(char::is_control)
        });
    let packs_valid = config.active_pack_ids.len() <= 128
        && config.active_pack_ids.iter().all(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        });
    if config.schema_version == 1 && mappings_valid && packs_valid {
        Ok(())
    } else {
        Err(IpcError {
            code: "invalid_input",
            message: "应用设置格式无效。",
        })
    }
}

fn kb_cache_identity(config: &KbConfig) -> (KbProviderKind, &str, &str, &str) {
    (
        config.provider_kind,
        config.vault.trim(),
        config.base_url.trim_end_matches('/'),
        match config.provider_kind {
            KbProviderKind::Fns => "",
            KbProviderKind::LocalVault => config.local_path.trim(),
            KbProviderKind::Mcp => config.mcp_url.trim_end_matches('/'),
        },
    )
}
