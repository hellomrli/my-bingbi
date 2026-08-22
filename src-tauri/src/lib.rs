use odoc_engine::{
    DraftEngine, DraftEvent, DraftInput, DraftJournal, DraftOutcome, EngineError,
    PipelineDefinition, RerunOutcome, ReviewEngine, ReviewEvent, ReviewInput, ReviewOutcome,
    StageOutput,
};
use odoc_export::{
    ExportError, Gb9704Style, discover_system_fonts,
    extract_reference_text as parse_reference_text, odoc_to_typst, print_html, read_docx,
    render_typst_pdf, resolve_font_path, validate_docx_template, write_docx_with_style,
    write_docx_with_template,
};
use odoc_kb::{
    ChangeKind, FnsPendingChange, FnsProvider, FnsWatchConfig, FnsWatchEvent, KbConfig, KbError,
    KbHit, KbNote, KbProvider, KbProviderKind, KbQuery, LocalVaultProvider, McpProvider,
    hybrid_search, suggest_missing_frontmatter, watch_fns_notes,
};
use odoc_llm::{
    ChatDelta, ChatMessage, ChatRequest, ChatResponse, ChatRole, ConnectionTestResult, LlmError,
    LlmProvider, ProviderClient, ProviderKind, ProviderProfile, ProviderProfileInput,
};
use odoc_model::{DocumentType, ODoc, SecurityLevel};
use odoc_pack::{PackRegistry, PackSummary, StageId};
use odoc_store::{
    AppConfig, CrashEvent, DEFAULT_SECRET_SERVICE, DocumentStore, DocumentSummary,
    DocumentVersionSummary, KeyringSecretStore, LocalCrashReporter, SaveDocumentResult,
    SecretError, SecretStore, StoreError,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tauri::{Manager, State};
use tauri_plugin_updater::UpdaterExt;

struct AppState {
    documents: Mutex<DocumentStore>,
    secrets: Box<dyn SecretStore>,
    crash_reporter: LocalCrashReporter,
    kb_sync_generation: tokio::sync::watch::Sender<u64>,
    user_pack_root: PathBuf,
    pack_issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct IpcError {
    code: &'static str,
    message: &'static str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DraftStartRequest {
    document_id: String,
    material: String,
    provider_id: String,
    confirmed_doc_type: Option<DocumentType>,
    /// Reference text for imitate mode; its presence selects the imitate pipeline.
    #[serde(default)]
    model_text: Option<String>,
    /// Polish mode rewrites the supplied material in place.
    #[serde(default)]
    polish: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewStartRequest {
    document_id: String,
    provider_id: String,
    /// Optional reviewer focus, e.g. 只看格式.
    #[serde(default)]
    focus: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RerunStageRequest {
    session_id: String,
    stage_id: String,
}

const MAX_DRAFT_MATERIAL_BYTES: usize = 2 * 1024 * 1024;
const MAX_REFERENCE_FILE_BYTES: usize = 64 * 1024 * 1024;
const MAX_REVIEW_FOCUS_BYTES: usize = 512;
const KB_FNS_TOKEN_ACCOUNT: &str = "knowledge-base:fns:token";
const KB_FNS_WS_TOKEN_ACCOUNT: &str = "knowledge-base:fns:ws-token";
const KB_MCP_TOKEN_ACCOUNT: &str = "knowledge-base:mcp:token";
const MAX_KB_TOKEN_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct KbIndexProgress {
    status: String,
    processed: u32,
    total: u32,
    path: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveResult {
    path: String,
    vault: String,
    overwritten: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionDiff {
    document_id: String,
    left_seq: u32,
    right_seq: u32,
    left_text: String,
    right_text: String,
    added_lines: u32,
    removed_lines: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestoreVersionResult {
    document: ODoc,
    saved: SaveDocumentResult,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DocxExportResult {
    bytes: Vec<u8>,
    used_template: bool,
    fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PdfExportResult {
    Pdf { bytes: Vec<u8> },
    PrintPreview { html: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportCapabilities {
    custom_template_configured: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCheckResult {
    available: bool,
    version: Option<String>,
    installed: bool,
}

struct StoreDraftJournal<'a> {
    state: &'a AppState,
    provider_id: String,
}

impl DraftJournal for StoreDraftJournal<'_> {
    fn start(
        &self,
        session_id: &str,
        document_id: &str,
        pipeline: &str,
        state_json: &str,
    ) -> Result<(), EngineError> {
        lock_documents(self.state)
            .and_then(|store| {
                store
                    .start_session(session_id, document_id, pipeline, state_json)
                    .map_err(map_store_error)
            })
            .map_err(|_| EngineError::Journal)
    }

    fn record(
        &self,
        session_id: &str,
        input_json: &str,
        output: &StageOutput,
    ) -> Result<(), EngineError> {
        let stage_id = serde_json::to_value(output.stage_id)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or(EngineError::Journal)?;
        let output_json = serde_json::to_string(output).map_err(|_| EngineError::Journal)?;
        let tokens = output.usage.map_or(0, |usage| usage.total_tokens);
        lock_documents(self.state)
            .and_then(|store| {
                store
                    .record_stage_run(
                        session_id,
                        &stage_id,
                        input_json,
                        Some(&output_json),
                        Some(&self.provider_id),
                        u64::from(tokens),
                        output.duration_ms,
                    )
                    .map_err(map_store_error)
            })
            .map_err(|_| EngineError::Journal)
    }

    fn complete(&self, session_id: &str, outcome_json: &str) -> Result<(), EngineError> {
        lock_documents(self.state)
            .and_then(|store| {
                store
                    .complete_session(session_id, outcome_json)
                    .map_err(map_store_error)
            })
            .map_err(|_| EngineError::Journal)
    }

    fn stage_input(&self, session_id: &str, stage_id: &str) -> Result<Option<String>, EngineError> {
        lock_documents(self.state)
            .and_then(|store| {
                store
                    .stage_run_input(session_id, stage_id)
                    .map_err(map_store_error)
            })
            .map_err(|_| EngineError::Journal)
    }

    fn rerun_record(
        &self,
        session_id: &str,
        stage_id: &str,
        output: &StageOutput,
    ) -> Result<(), EngineError> {
        let output_json = serde_json::to_string(output).map_err(|_| EngineError::Journal)?;
        let tokens = output.usage.map_or(0, |usage| usage.total_tokens);
        lock_documents(self.state)
            .and_then(|store| {
                store
                    .replace_stage_run(
                        session_id,
                        stage_id,
                        &output_json,
                        u64::from(tokens),
                        output.duration_ms,
                    )
                    .map_err(map_store_error)
            })
            .map_err(|_| EngineError::Journal)
    }
}

#[tauri::command]
fn create_empty_document() -> ODoc {
    ODoc::empty()
}

#[tauri::command]
fn backend_version() -> &'static str {
    concat!("rust-core ", env!("CARGO_PKG_VERSION"))
}

#[tauri::command]
fn list_pack_summaries(packs: State<'_, PackRegistry>) -> Vec<PackSummary> {
    pack_summaries(&packs)
}

/// Checks the configured update endpoint for a newer version.
///
/// The updater plugin is only registered when `BINGBI_UPDATER_PUBKEY` is set at startup (the
/// release pipeline injects it); without it, this command reports the service as unconfigured
/// instead of failing.
#[tauri::command]
async fn check_for_update<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<UpdateCheckResult, IpcError> {
    let Ok(updater) = app.updater() else {
        return Err(IpcError {
            code: "update_unavailable",
            message: "更新服务未配置。",
        });
    };
    let update = updater.check().await.map_err(|_| IpcError {
        code: "update_check_failed",
        message: "检查更新失败，请稍后重试。",
    })?;
    Ok(update.map_or(
        UpdateCheckResult {
            available: false,
            version: None,
            installed: false,
        },
        |update| UpdateCheckResult {
            available: true,
            version: Some(update.version.to_string()),
            installed: false,
        },
    ))
}

/// Downloads and installs the pending update, then restarts the application.
#[tauri::command]
async fn apply_update<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), IpcError> {
    let updater = app.updater().map_err(|_| IpcError {
        code: "update_unavailable",
        message: "更新服务未配置。",
    })?;
    let update = updater
        .check()
        .await
        .map_err(|_| IpcError {
            code: "update_check_failed",
            message: "检查更新失败，请稍后重试。",
        })?
        .ok_or(IpcError {
            code: "update_unavailable",
            message: "当前已是最新版本。",
        })?;
    update
        .download_and_install(|_bytes, _total| {}, || {})
        .await
        .map_err(|_| IpcError {
            code: "update_install_failed",
            message: "更新下载或安装失败，请稍后重试。",
        })?;
    app.request_restart();
    Ok(())
}

#[tauri::command]
fn user_pack_root(state: State<'_, AppState>) -> Result<String, IpcError> {
    Ok(state.user_pack_root.to_string_lossy().into_owned())
}

#[tauri::command]
fn pack_load_issues(state: State<'_, AppState>) -> Result<Vec<String>, IpcError> {
    Ok(state.pack_issues.clone())
}

/// Lists opt-in Rust panic diagnostics retained only in the local application database.
#[tauri::command]
fn list_crash_events(
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
fn clear_crash_events(state: State<'_, AppState>) -> Result<u32, IpcError> {
    let deleted = lock_documents(&state)?
        .clear_crash_events()
        .map_err(map_store_error)?;
    Ok(deleted.try_into().unwrap_or(u32::MAX))
}

/// Creates the user pack root, seeding a commented example pack when the root is empty.
///
/// The example pack demonstrates custom deterministic rules: edit `rules/format.yaml` and
/// restart the app. Existing content is never touched.
fn ensure_user_pack_root(root: &std::path::Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(root)?;
    if root.read_dir()?.next().is_some() {
        return Ok(());
    }
    let pack_dir = root.join("local-corpus");
    std::fs::create_dir_all(pack_dir.join("rules"))?;
    std::fs::write(
        pack_dir.join("pack.toml"),
        r#"schema_version = "0.1"

[pack]
id = "local-corpus"
name = "本单位语料包"
version = "0.1.0"
description = "本单位规则与笔法（示例包，规则文件可直接编辑）"
license = "private"

[capabilities]
provides = ["format_rules", "language_rules"]

[[bindings]]
stage = "lint"
rules = ["rules/format.yaml"]
"#,
    )?;
    std::fs::write(
        pack_dir.join("rules/format.yaml"),
        r#"# 本单位自定义规则（示例）
# 每条规则是一个 `- id: ...` 条目，语法见内置包 packs/gb9704-format/rules/format.yaml。
# 取消注释即可启用示例规则，修改后重启应用生效。

# - id: unit-banned-word
#   name: 本单位禁用语
#   severity: error
#   type: lexicon_density
#   scope: body.blocks
#   lexicon: ["杜绝形式主义"]
#   max_per_100_chars: 0
#   message: 本单位禁用语，请更换表述

[]
"#,
    )?;
    std::fs::write(
        pack_dir.join("README.md"),
        "# 本单位语料包（示例）\n\n这是秉笔为用户自定义规则准备的示例知识包。修改\n`rules/format.yaml` 后重启应用生效；也可以复制本目录为一个新包\n（pack.toml 里的 id 必须与目录名一致）。\n",
    )?;
    Ok(())
}

#[tauri::command]
fn save_document(
    state: State<'_, AppState>,
    document: ODoc,
    stage: Option<String>,
) -> Result<SaveDocumentResult, IpcError> {
    let mut store = lock_documents(&state)?;
    store
        .save_document(&document, stage.as_deref())
        .map_err(map_store_error)
}

#[tauri::command]
fn load_document(state: State<'_, AppState>, id: String) -> Result<ODoc, IpcError> {
    lock_documents(&state)?
        .get_document(&id)
        .map_err(map_store_error)
}

#[tauri::command]
fn list_documents(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<DocumentSummary>, IpcError> {
    lock_documents(&state)?
        .list_documents(limit.unwrap_or(30))
        .map_err(map_store_error)
}

#[tauri::command]
fn list_sessions(
    state: State<'_, AppState>,
    document_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<odoc_store::SessionSummary>, IpcError> {
    let limit = limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(IpcError {
            code: "invalid_input",
            message: "起草记录数量范围无效。",
        });
    }
    lock_documents(&state)?
        .list_sessions(document_id.as_deref(), limit)
        .map_err(map_store_error)
}

#[tauri::command]
fn list_stage_runs(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<odoc_store::StageRunRecord>, IpcError> {
    lock_documents(&state)?
        .list_stage_runs(&session_id)
        .map_err(map_store_error)
}

#[tauri::command]
fn delete_document(state: State<'_, AppState>, id: String) -> Result<bool, IpcError> {
    lock_documents(&state)?
        .delete_document(&id)
        .map_err(map_store_error)
}

#[tauri::command]
fn list_document_versions(
    state: State<'_, AppState>,
    document_id: String,
) -> Result<Vec<DocumentVersionSummary>, IpcError> {
    lock_documents(&state)?
        .list_document_versions(&document_id)
        .map_err(map_store_error)
}

#[tauri::command]
fn load_document_version(
    state: State<'_, AppState>,
    document_id: String,
    seq: u32,
) -> Result<ODoc, IpcError> {
    lock_documents(&state)?
        .get_document_version(&document_id, seq)
        .map_err(map_store_error)
}

#[tauri::command]
fn restore_document_version(
    state: State<'_, AppState>,
    document_id: String,
    seq: u32,
) -> Result<RestoreVersionResult, IpcError> {
    let mut store = lock_documents(&state)?;
    let current = store.get_document(&document_id).map_err(map_store_error)?;
    let mut restored = store
        .get_document_version(&document_id, seq)
        .map_err(map_store_error)?;
    restored.id = current.id;
    restored.revision = current.revision.saturating_add(1);
    let saved = store
        .save_document(&restored, Some(&format!("restore:{seq}")))
        .map_err(map_store_error)?;
    Ok(RestoreVersionResult {
        document: restored,
        saved,
    })
}

#[tauri::command]
fn diff_document_versions(
    state: State<'_, AppState>,
    document_id: String,
    left_seq: u32,
    right_seq: u32,
) -> Result<VersionDiff, IpcError> {
    let store = lock_documents(&state)?;
    let left = store
        .get_document_version(&document_id, left_seq)
        .map_err(map_store_error)?;
    let right = store
        .get_document_version(&document_id, right_seq)
        .map_err(map_store_error)?;
    let left_text = left.to_plain_text();
    let right_text = right.to_plain_text();
    let (removed_lines, added_lines) = changed_line_counts(&left_text, &right_text);
    Ok(VersionDiff {
        document_id,
        left_seq,
        right_seq,
        left_text,
        right_text,
        added_lines,
        removed_lines,
    })
}

#[tauri::command]
fn load_app_config(state: State<'_, AppState>) -> Result<AppConfig, IpcError> {
    lock_documents(&state)?
        .app_config()
        .map_err(map_store_error)
}

#[tauri::command]
fn save_app_config(state: State<'_, AppState>, config: AppConfig) -> Result<(), IpcError> {
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
fn load_kb_config(state: State<'_, AppState>) -> Result<KbConfig, IpcError> {
    lock_documents(&state)?.kb_config().map_err(map_store_error)
}

#[tauri::command]
fn save_kb_config(state: State<'_, AppState>, config: KbConfig) -> Result<KbConfig, IpcError> {
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
fn set_kb_fns_token(state: State<'_, AppState>, token: String) -> Result<(), IpcError> {
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
fn has_kb_fns_token(state: State<'_, AppState>) -> Result<bool, IpcError> {
    state
        .secrets
        .has_secret(KB_FNS_TOKEN_ACCOUNT)
        .map_err(map_secret_error)
}

#[tauri::command]
fn delete_kb_fns_token(state: State<'_, AppState>) -> Result<bool, IpcError> {
    let deleted = state
        .secrets
        .delete_secret(KB_FNS_TOKEN_ACCOUNT)
        .map_err(map_secret_error)?;
    wake_kb_sync(&state);
    Ok(deleted)
}

#[tauri::command]
fn set_kb_fns_ws_token(state: State<'_, AppState>, token: String) -> Result<(), IpcError> {
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
fn has_kb_fns_ws_token(state: State<'_, AppState>) -> Result<bool, IpcError> {
    state
        .secrets
        .has_secret(KB_FNS_WS_TOKEN_ACCOUNT)
        .map_err(map_secret_error)
}

#[tauri::command]
fn delete_kb_fns_ws_token(state: State<'_, AppState>) -> Result<bool, IpcError> {
    let deleted = state
        .secrets
        .delete_secret(KB_FNS_WS_TOKEN_ACCOUNT)
        .map_err(map_secret_error)?;
    wake_kb_sync(&state);
    Ok(deleted)
}

#[tauri::command]
fn set_kb_mcp_token(state: State<'_, AppState>, token: String) -> Result<(), IpcError> {
    save_kb_secret(&state, KB_MCP_TOKEN_ACCOUNT, token, "MCP token 格式无效。")
}

#[tauri::command]
fn has_kb_mcp_token(state: State<'_, AppState>) -> Result<bool, IpcError> {
    state
        .secrets
        .has_secret(KB_MCP_TOKEN_ACCOUNT)
        .map_err(map_secret_error)
}

#[tauri::command]
fn delete_kb_mcp_token(state: State<'_, AppState>) -> Result<bool, IpcError> {
    state
        .secrets
        .delete_secret(KB_MCP_TOKEN_ACCOUNT)
        .map_err(map_secret_error)
}

#[tauri::command]
async fn kb_search(
    state: State<'_, AppState>,
    document_id: Option<String>,
    mut query: KbQuery,
) -> Result<Vec<KbHit>, IpcError> {
    enforce_kb_security(&state, document_id.as_deref())?;
    let (provider, config) = configured_kb_provider(&state)?;
    query.vault = Some(config.vault.clone());
    match hybrid_search(provider.as_ref(), &query).await {
        Ok(hits) => Ok(hits),
        Err(error) if config.index_enabled => {
            let store = lock_documents(&state)?;
            let cached = store
                .search_cached_kb(
                    &config.vault,
                    &query.keyword,
                    query.path_prefix.as_deref(),
                    &query.filters,
                    query.normalized_limit(),
                )
                .map_err(map_store_error)?;
            if cached.is_empty() {
                Err(map_kb_error(error))
            } else {
                Ok(cached
                    .into_iter()
                    .map(|hit| KbHit {
                        vault: hit.vault,
                        path: hit.path,
                        title: hit.title,
                        snippet: hit.snippet,
                        score: hit.score,
                        frontmatter: hit.frontmatter,
                    })
                    .collect())
            }
        }
        Err(error) => Err(map_kb_error(error)),
    }
}

#[tauri::command]
async fn kb_get(
    state: State<'_, AppState>,
    document_id: Option<String>,
    path: String,
) -> Result<KbNote, IpcError> {
    enforce_kb_security(&state, document_id.as_deref())?;
    let (provider, config) = configured_kb_provider(&state)?;
    match provider.get(&path).await {
        Ok(note) => Ok(note),
        Err(error) if config.index_enabled => lock_documents(&state)?
            .cached_kb_note(&config.vault, &path)
            .map_err(map_store_error)?
            .ok_or_else(|| map_kb_error(error)),
        Err(error) => Err(map_kb_error(error)),
    }
}

#[tauri::command]
fn kb_index_state(
    state: State<'_, AppState>,
    vault: Option<String>,
) -> Result<Option<odoc_store::KbIndexState>, IpcError> {
    let config = lock_documents(&state)?
        .kb_config()
        .map_err(map_store_error)?;
    let vault = vault.unwrap_or(config.vault);
    lock_documents(&state)?
        .kb_index_state(&vault)
        .map_err(map_store_error)
}

#[tauri::command]
async fn kb_rebuild_index(
    state: State<'_, AppState>,
    document_id: Option<String>,
    on_progress: Channel<KbIndexProgress>,
) -> Result<odoc_store::KbIndexState, IpcError> {
    enforce_kb_security(&state, document_id.as_deref())?;
    let (provider, config) = configured_kb_provider(&state)?;
    let vault = config.vault.clone();
    let provider_kind = provider_kind_name(config.provider_kind).to_owned();
    let started = odoc_store::KbIndexState {
        vault: vault.clone(),
        provider_kind: provider_kind.clone(),
        last_cursor: 0,
        note_count: 0,
        chunk_count: 0,
        status: "running".to_owned(),
        error_message: None,
        updated_at: now_millis(),
    };
    lock_documents(&state)?
        .save_kb_index_state(&started)
        .map_err(map_store_error)?;
    let _ = on_progress.send(KbIndexProgress {
        status: "running".to_owned(),
        processed: 0,
        total: 0,
        path: None,
        message: None,
    });
    let notes = match provider.list_notes(None, 20_000).await {
        Ok(notes) => notes,
        Err(error) => {
            let failed = odoc_store::KbIndexState {
                status: "error".to_owned(),
                error_message: Some(error.user_message().to_owned()),
                updated_at: now_millis(),
                ..started
            };
            lock_documents(&state)?
                .save_kb_index_state(&failed)
                .map_err(map_store_error)?;
            let _ = on_progress.send(KbIndexProgress {
                status: "error".to_owned(),
                processed: 0,
                total: 0,
                path: None,
                message: failed.error_message.clone(),
            });
            return Err(map_kb_error(error));
        }
    };
    let total = u32::try_from(notes.len()).unwrap_or(u32::MAX);
    {
        let store = lock_documents(&state)?;
        store.clear_kb_index(&vault).map_err(map_store_error)?;
        store
            .save_kb_index_state(&started)
            .map_err(map_store_error)?;
    }
    for (index, note) in notes.into_iter().enumerate() {
        let path = note.path.clone();
        let note = match prepare_kb_note(provider.as_ref(), note, config.auto_tagging_enabled).await
        {
            Ok(note) => note,
            Err(error) => {
                let failed = index_failure_state(&started, index, error.message.to_owned());
                lock_documents(&state)?
                    .save_kb_index_state(&failed)
                    .map_err(map_store_error)?;
                let _ = on_progress.send(KbIndexProgress {
                    status: "error".to_owned(),
                    processed: u32::try_from(index).unwrap_or(u32::MAX),
                    total,
                    path: Some(path),
                    message: Some(error.message.to_owned()),
                });
                return Err(error);
            }
        };
        if let Err(error) = lock_documents(&state)?.cache_kb_note(&note) {
            let failed = index_failure_state(&started, index, error.to_string());
            lock_documents(&state)?
                .save_kb_index_state(&failed)
                .map_err(map_store_error)?;
            let _ = on_progress.send(KbIndexProgress {
                status: "error".to_owned(),
                processed: u32::try_from(index).unwrap_or(u32::MAX),
                total,
                path: Some(note.path.clone()),
                message: failed.error_message.clone(),
            });
            return Err(map_store_error(error));
        }
        let _ = on_progress.send(KbIndexProgress {
            status: "running".to_owned(),
            processed: u32::try_from(index + 1).unwrap_or(u32::MAX),
            total,
            path: Some(note.path.clone()),
            message: None,
        });
    }
    let (note_count, chunk_count) = lock_documents(&state)?
        .kb_cache_counts(&vault)
        .map_err(map_store_error)?;
    let completed = odoc_store::KbIndexState {
        vault,
        provider_kind,
        last_cursor: 0,
        note_count,
        chunk_count,
        status: "ready".to_owned(),
        error_message: None,
        updated_at: now_millis(),
    };
    lock_documents(&state)?
        .save_kb_index_state(&completed)
        .map_err(map_store_error)?;
    let _ = on_progress.send(KbIndexProgress {
        status: "ready".to_owned(),
        processed: total,
        total,
        path: None,
        message: None,
    });
    Ok(completed)
}

#[tauri::command]
fn kb_clear_index(state: State<'_, AppState>, vault: Option<String>) -> Result<(), IpcError> {
    let config = lock_documents(&state)?
        .kb_config()
        .map_err(map_store_error)?;
    let vault = vault.unwrap_or(config.vault);
    lock_documents(&state)?
        .clear_kb_index(&vault)
        .map_err(map_store_error)
}

#[tauri::command]
async fn kb_sync_once(
    state: State<'_, AppState>,
    document_id: Option<String>,
) -> Result<odoc_store::KbIndexState, IpcError> {
    enforce_kb_security(&state, document_id.as_deref())?;
    let config = lock_documents(&state)?
        .kb_config()
        .map_err(map_store_error)?
        .normalized()
        .map_err(map_kb_error)?;
    if config.provider_kind != KbProviderKind::Fns || !config.sync_enabled {
        return Err(IpcError {
            code: "kb_sync_unsupported",
            message: "当前知识库配置不支持 FNS 增量同步。",
        });
    }
    let token = state
        .secrets
        .get_secret(KB_FNS_WS_TOKEN_ACCOUNT)
        .map_err(map_secret_error)?
        .ok_or(IpcError {
            code: "kb_ws_token_missing",
            message: "未配置 FNS WebSocket token，实时同步已关闭。",
        })?;
    let last_cursor = lock_documents(&state)?
        .kb_index_state(&config.vault)
        .map_err(map_store_error)?
        .map_or(0, |value| value.last_cursor);
    let mut events = watch_fns_notes(FnsWatchConfig {
        base_url: config.base_url.clone(),
        token,
        vault: config.vault.clone(),
        last_cursor,
    })
    .await
    .map_err(map_kb_error)?;
    let provider = FnsProvider::new(
        &config.base_url,
        state
            .secrets
            .get_secret(KB_FNS_TOKEN_ACCOUNT)
            .map_err(map_secret_error)?
            .ok_or(IpcError {
                code: "kb_token_missing",
                message: "尚未保存 FNS REST token。",
            })?,
        Some(config.vault.clone()),
    )
    .map_err(map_kb_error)?;
    let mut cursor = last_cursor;
    let mut cursor_received = false;
    let sync = async {
        while let Some(event) = events.recv().await {
            match event.map_err(map_kb_error)? {
                FnsWatchEvent::Cursor(value) => {
                    cursor = cursor.max(value);
                    cursor_received = true;
                    break;
                }
                FnsWatchEvent::Change(pending) => {
                    persist_fns_change(&state, &provider, pending).await?;
                }
            }
        }
        Ok::<(), IpcError>(())
    };
    tokio::time::timeout(Duration::from_secs(60), sync)
        .await
        .map_err(|_| IpcError {
            code: "kb_sync_timeout",
            message: "FNS 增量同步超时，游标未推进。",
        })??;
    if !cursor_received {
        return Err(IpcError {
            code: "kb_sync_incomplete",
            message: "FNS 增量同步未返回完成游标，请稍后重试。",
        });
    }
    save_fns_cursor(&state, &config.vault, cursor)?;
    lock_documents(&state)?
        .kb_index_state(&config.vault)
        .map_err(map_store_error)?
        .ok_or(IpcError {
            code: "kb_sync_incomplete",
            message: "FNS 增量同步状态未能保存。",
        })
}

#[tauri::command]
async fn archive_document_to_kb(
    state: State<'_, AppState>,
    document_id: String,
    path: String,
    overwrite: bool,
) -> Result<ArchiveResult, IpcError> {
    enforce_kb_security(&state, Some(&document_id))?;
    let document = lock_documents(&state)?
        .get_document(&document_id)
        .map_err(map_store_error)?;
    let (provider, config) = configured_kb_provider(&state)?;
    let path = normalize_archive_path(&path)?;
    let existed = match provider.get(&path).await {
        Ok(_) => true,
        Err(KbError::NotFound) => false,
        Err(error) => return Err(map_kb_error(error)),
    };
    if existed && !overwrite {
        return Err(IpcError {
            code: "kb_archive_exists",
            message: "知识库中已存在同名笔记，请确认覆盖或更换路径。",
        });
    }
    let markdown = document_to_markdown(&document);
    provider
        .write(&path, &markdown)
        .await
        .map_err(map_kb_error)?;
    if config.index_enabled {
        let note = provider.get(&path).await.map_err(map_kb_error)?;
        let note = prepare_kb_note(provider.as_ref(), note, config.auto_tagging_enabled).await?;
        lock_documents(&state)?
            .cache_kb_note(&note)
            .map_err(map_store_error)?;
    }
    Ok(ArchiveResult {
        path,
        vault: config.vault,
        overwritten: existed,
    })
}

#[tauri::command]
fn list_provider_profiles(state: State<'_, AppState>) -> Result<Vec<ProviderProfile>, IpcError> {
    lock_documents(&state)?
        .list_provider_profiles()
        .map_err(map_store_error)
}

#[tauri::command]
fn save_provider_profile(
    state: State<'_, AppState>,
    profile: ProviderProfileInput,
) -> Result<ProviderProfile, IpcError> {
    lock_documents(&state)?
        .save_provider_profile(profile)
        .map_err(map_store_error)
}

#[tauri::command]
async fn test_provider_connection(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<ConnectionTestResult, IpcError> {
    let client = provider_client(&state, &provider_id)?;
    let model = client.profile().model.clone();
    let capabilities = client.capabilities();
    let started = Instant::now();
    client
        .chat(connection_test_request())
        .await
        .map_err(map_llm_error)?;
    Ok(ConnectionTestResult {
        provider_id,
        model,
        latency_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
        capabilities,
    })
}

#[tauri::command]
async fn list_provider_models(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<Vec<String>, IpcError> {
    provider_client(&state, &provider_id)?
        .list_models()
        .await
        .map_err(map_llm_error)
}

#[tauri::command]
async fn chat_with_provider(
    state: State<'_, AppState>,
    provider_id: String,
    document_id: String,
    mut request: ChatRequest,
) -> Result<ChatResponse, IpcError> {
    bind_request_to_document(&state, &document_id, &mut request)?;
    provider_client(&state, &provider_id)?
        .chat(request)
        .await
        .map_err(map_llm_error)
}

#[tauri::command]
async fn chat_stream_with_provider(
    state: State<'_, AppState>,
    provider_id: String,
    document_id: String,
    mut request: ChatRequest,
    on_delta: Channel<ChatDelta>,
) -> Result<(), IpcError> {
    bind_request_to_document(&state, &document_id, &mut request)?;
    let client = provider_client(&state, &provider_id)?;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(32);
    let producer =
        tauri::async_runtime::spawn(async move { client.chat_stream(request, sender).await });

    while let Some(delta) = receiver.recv().await {
        if on_delta.send(delta).is_err() {
            producer.abort();
            return Err(map_llm_error(LlmError::Cancelled));
        }
    }
    producer
        .await
        .map_err(|_| map_llm_error(LlmError::Cancelled))?
        .map_err(map_llm_error)
}

#[tauri::command]
async fn draft_start<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    packs: State<'_, PackRegistry>,
    request: DraftStartRequest,
    on_event: Channel<DraftEvent>,
) -> Result<DraftOutcome, IpcError> {
    validate_draft_start_request(&request)?;
    let document = lock_documents(&state).and_then(|store| {
        store
            .get_document(&request.document_id)
            .map_err(map_store_error)
    })?;
    let provider = provider_client(&state, &request.provider_id)?;
    if !document.meta.security_level.allows_remote_provider() && !provider.capabilities().is_local {
        return Err(IpcError {
            code: "model_security_blocked",
            message: "当前密级仅允许本地模型，无法启动起草。",
        });
    }
    let pipeline_file = draft_pipeline_file(request.model_text.as_deref(), request.polish);
    let pipeline_path = builtin_pipeline_path(&app, pipeline_file).map_err(map_tauri_error)?;
    let pipeline = PipelineDefinition::load(pipeline_path).map_err(map_engine_error)?;
    let journal = StoreDraftJournal {
        state: &state,
        provider_id: request.provider_id,
    };
    let knowledge_base = optional_kb_provider(&state, document.meta.security_level)?;
    let mut engine =
        DraftEngine::new(pipeline, &packs, &provider, &journal).map_err(map_engine_error)?;
    if let Some(knowledge_base) = knowledge_base.as_deref() {
        engine = engine.with_knowledge_base(knowledge_base);
    }
    let input = DraftInput {
        material: request.material,
        model_text: request.model_text,
        initial_document: Some(document),
        confirmed_doc_type: request.confirmed_doc_type,
    };
    let (event_sender, mut event_receiver) = tokio::sync::mpsc::channel(32);
    let engine_run = engine.run(input, Some(event_sender));
    tokio::pin!(engine_run);
    let mut document_saved = false;
    loop {
        tokio::select! {
            result = &mut engine_run => {
                let outcome = result.map_err(map_engine_error)?;
                if !document_saved {
                    save_draft_document(&state, &outcome.document)?;
                }
                while let Some(event) = event_receiver.recv().await {
                    persist_completed_event(&state, &event, &mut document_saved)?;
                    on_event.send(event).map_err(|_| map_engine_error(EngineError::Cancelled))?;
                }
                return Ok(outcome);
            }
            event = event_receiver.recv() => match event {
                Some(event) => {
                    persist_completed_event(&state, &event, &mut document_saved)?;
                    on_event.send(event).map_err(|_| map_engine_error(EngineError::Cancelled))?;
                }
                None => return Err(map_engine_error(EngineError::Cancelled)),
            }
        }
    }
}

/// Re-executes one draft-pipeline stage from the input snapshot recorded in its session.
///
/// The provider, pipeline, knowledge pack and knowledge-base configuration are taken from the
/// session record rather than from the request, so a rerun always reproduces the configuration
/// the stage originally ran with. Only draft sessions support single-stage rerun; a review must
/// be re-run as a whole because the kernel cannot rebuild a review-only pipeline stage here.
#[tauri::command]
async fn rerun_stage<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    packs: State<'_, PackRegistry>,
    request: RerunStageRequest,
    on_event: Channel<DraftEvent>,
) -> Result<RerunOutcome, IpcError> {
    validate_rerun_stage_request(&request)?;
    let session = lock_documents(&state)?
        .get_session(&request.session_id)
        .map_err(map_store_error)?;
    let pipeline_file = match session.pipeline.as_str() {
        "draft" => "draft.yaml",
        "imitate" => "imitate.yaml",
        "polish" => "polish.yaml",
        _ => {
            return Err(IpcError {
                code: "rerun_unsupported",
                message: "审核流程暂不支持单步重跑，请整体重新审核。",
            });
        }
    };
    let document = lock_documents(&state)?
        .get_document(&session.document_id)
        .map_err(map_store_error)?;
    let provider_id = lock_documents(&state)?
        .list_stage_runs(&request.session_id)
        .map_err(map_store_error)?
        .into_iter()
        .rev()
        .find(|run| run.stage_id == request.stage_id)
        .and_then(|run| run.provider)
        .ok_or(IpcError {
            code: "rerun_unavailable",
            message: "该阶段尚未运行过，或其模型服务已被删除。",
        })?;
    let client = provider_client(&state, &provider_id)?;
    if !document.meta.security_level.allows_remote_provider() && !client.capabilities().is_local {
        return Err(IpcError {
            code: "model_security_blocked",
            message: "当前密级仅允许本地模型，无法重跑该阶段。",
        });
    }
    let pipeline_path = builtin_pipeline_path(&app, pipeline_file).map_err(map_tauri_error)?;
    let pipeline = PipelineDefinition::load(pipeline_path).map_err(map_engine_error)?;
    let journal = StoreDraftJournal {
        state: &state,
        provider_id,
    };
    let knowledge_base = optional_kb_provider(&state, document.meta.security_level)?;
    let mut engine =
        DraftEngine::new(pipeline, &packs, &client, &journal).map_err(map_engine_error)?;
    if let Some(knowledge_base) = knowledge_base.as_deref() {
        engine = engine.with_knowledge_base(knowledge_base);
    }
    let stage_id: StageId = serde_json::from_value(serde_json::Value::String(
        request.stage_id.clone(),
    ))
    .map_err(|_| IpcError {
        code: "invalid_input",
        message: "起草阶段标识无效。",
    })?;
    let (event_sender, mut event_receiver) = tokio::sync::mpsc::channel(32);
    let engine_run = engine.rerun_stage(&request.session_id, stage_id, Some(event_sender));
    tokio::pin!(engine_run);
    let mut document_saved = false;
    loop {
        tokio::select! {
            result = &mut engine_run => {
                let mut outcome = result.map_err(map_engine_error)?;
                // A rerun of an old session may replace a document that has since been
                // re-drafted; never let the replacement regress the stored revision.
                if outcome.document.revision < document.revision {
                    outcome.document.revision = document.revision;
                }
                if !document_saved {
                    save_draft_document(&state, &outcome.document)?;
                }
                while let Some(event) = event_receiver.recv().await {
                    persist_completed_event(&state, &event, &mut document_saved)?;
                    on_event.send(event).map_err(|_| map_engine_error(EngineError::Cancelled))?;
                }
                return Ok(outcome);
            }
            event = event_receiver.recv() => match event {
                Some(event) => {
                    persist_completed_event(&state, &event, &mut document_saved)?;
                    on_event.send(event).map_err(|_| map_engine_error(EngineError::Cancelled))?;
                }
                None => return Err(map_engine_error(EngineError::Cancelled)),
            }
        }
    }
}

/// Inspects a stored document and streams the review report back to the workbench.
///
/// The document is read from the store rather than taken from the request, so a review always
/// describes the revision the kernel holds.  No stage writes to it: reviewing an existing
/// document must never change it.
#[tauri::command]
async fn review_start<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    packs: State<'_, PackRegistry>,
    request: ReviewStartRequest,
    on_event: Channel<ReviewEvent>,
) -> Result<ReviewOutcome, IpcError> {
    let focus = validate_review_start_request(&request)?;
    let document = lock_documents(&state).and_then(|store| {
        store
            .get_document(&request.document_id)
            .map_err(map_store_error)
    })?;
    let provider = provider_client(&state, &request.provider_id)?;
    let pipeline_path = builtin_pipeline_path(&app, "review.yaml").map_err(map_tauri_error)?;
    let pipeline = PipelineDefinition::load(pipeline_path).map_err(map_engine_error)?;
    let journal = StoreDraftJournal {
        state: &state,
        provider_id: request.provider_id,
    };
    let engine =
        ReviewEngine::new(pipeline, &packs, &provider, &journal).map_err(map_engine_error)?;
    let (event_sender, mut event_receiver) = tokio::sync::mpsc::channel(32);
    let engine_run = engine.run(ReviewInput { document, focus }, Some(event_sender));
    tokio::pin!(engine_run);
    loop {
        tokio::select! {
            result = &mut engine_run => {
                let outcome = result.map_err(map_engine_error)?;
                while let Some(event) = event_receiver.recv().await {
                    on_event.send(event).map_err(|_| map_engine_error(EngineError::Cancelled))?;
                }
                return Ok(outcome);
            }
            event = event_receiver.recv() => match event {
                Some(event) => {
                    on_event.send(event).map_err(|_| map_engine_error(EngineError::Cancelled))?;
                }
                None => return Err(map_engine_error(EngineError::Cancelled)),
            }
        }
    }
}

/// Exports the current stored ODoc after a fresh kernel lint and fact check.
///
/// The frontend sends only an identifier.  Reading the document and its security-sensitive
/// facts here prevents a stale or browser-modified payload from bypassing export checks.
#[tauri::command]
fn export_docx(
    state: State<'_, AppState>,
    packs: State<'_, PackRegistry>,
    document_id: String,
) -> Result<DocxExportResult, IpcError> {
    let document = lock_documents(&state)?
        .get_document(&document_id)
        .map_err(map_store_error)?;
    ensure_exportable(&document, &packs)?;
    let style = export_style(&state, &packs)?;
    let template = lock_documents(&state)?
        .get_setting::<Vec<u8>>("export.docx_template")
        .map_err(map_store_error)?;
    if let Some(template) = template {
        match write_docx_with_template(&document, &template, &style) {
            Ok(bytes) => {
                return Ok(DocxExportResult {
                    bytes,
                    used_template: true,
                    fallback_reason: None,
                });
            }
            Err(_) => {
                let bytes = write_docx_with_style(&document, &style).map_err(map_export_error)?;
                return Ok(DocxExportResult {
                    bytes,
                    used_template: false,
                    fallback_reason: Some(
                        "自定义模板不可用，已回退到 GB/T 9704 严格生成。".to_owned(),
                    ),
                });
            }
        }
    }
    Ok(DocxExportResult {
        bytes: write_docx_with_style(&document, &style).map_err(map_export_error)?,
        used_template: false,
        fallback_reason: None,
    })
}

#[tauri::command]
fn import_docx(
    state: State<'_, AppState>,
    bytes: Vec<u8>,
) -> Result<RestoreVersionResult, IpcError> {
    let document = read_docx(&bytes).map_err(map_import_error)?;
    let saved = lock_documents(&state)?
        .save_document(&document, Some("docx_import"))
        .map_err(map_store_error)?;
    Ok(RestoreVersionResult { document, saved })
}

/// Extracts semantic text from a DOCX reference without saving or changing the current document.
///
/// This is intentionally separate from `import_docx`: an uploaded reference is only input to a
/// later drafting run and must not appear in the document library as an imported draft.
#[tauri::command]
fn extract_docx_text(bytes: Vec<u8>) -> Result<String, IpcError> {
    extract_reference_text("reference.docx".to_owned(), bytes)
}

/// Extracts bounded plain text from a binary drafting reference without importing it.
#[tauri::command]
fn extract_reference_text(file_name: String, bytes: Vec<u8>) -> Result<String, IpcError> {
    if bytes.is_empty() || bytes.len() > MAX_REFERENCE_FILE_BYTES {
        return Err(IpcError {
            code: "reference_too_large",
            message: "参考文件为空或超过 64 MiB 限制。",
        });
    }
    if file_name.trim().is_empty()
        || file_name.len() > 512
        || file_name.contains(['\0', '\r', '\n'])
    {
        return Err(IpcError {
            code: "reference_invalid",
            message: "参考文件名或文件结构无效，无法读取。",
        });
    }
    let text = parse_reference_text(&file_name, &bytes).map_err(map_reference_error)?;
    if text.trim().is_empty() || text.len() > MAX_DRAFT_MATERIAL_BYTES {
        return Err(IpcError {
            code: "reference_too_large",
            message: "参考文件提取后的文字为空或超过起草材料 2 MiB 限制。",
        });
    }
    Ok(text)
}

#[tauri::command]
fn export_pdf(
    state: State<'_, AppState>,
    packs: State<'_, PackRegistry>,
    document_id: String,
) -> Result<PdfExportResult, IpcError> {
    let document = lock_documents(&state)?
        .get_document(&document_id)
        .map_err(map_store_error)?;
    ensure_exportable(&document, &packs)?;
    let style = export_style(&state, &packs)?;

    // Zero-dependency PDF: render the document through the embedded Typst
    // engine using fonts discovered on the host system. When no usable CJK
    // font is available, fall back to the browser print preview (which does not
    // depend on any external binary either).
    let fonts = typst_fonts(&style);
    if fonts.is_empty() {
        return Ok(PdfExportResult::PrintPreview {
            html: print_html(&document, &style),
        });
    }
    let source = odoc_to_typst(&document, &style);
    let bytes = render_typst_pdf(&source, &fonts).map_err(map_pdf_error)?;
    Ok(PdfExportResult::Pdf { bytes })
}

/// Maps every distinct font family referenced by `style` to a discovered font
/// file on the host system. Families that cannot be resolved fall back to the
/// CJK serif font; an empty result means no usable font exists at all.
fn typst_fonts(style: &Gb9704Style) -> Vec<(String, PathBuf)> {
    let system_fonts = discover_system_fonts();
    let families = [
        &style.fonts.title,
        &style.fonts.body,
        &style.fonts.heading_l1,
        &style.fonts.heading_l2,
        &style.fonts.heading_l3,
        &style.fonts.heading_l4,
        &style.fonts.document_number,
    ];
    let mut seen = std::collections::HashSet::new();
    let mut fonts = Vec::new();
    for family in families {
        if !seen.insert(family.font.as_str()) {
            continue;
        }
        if let Some(path) = resolve_font_path(&family.font, &system_fonts) {
            fonts.push((family.font.clone(), path));
        }
    }
    fonts
}

fn map_pdf_error(error: ExportError) -> IpcError {
    let message = match error {
        ExportError::FontRead(..) | ExportError::InvalidFont(..) => {
            "PDF 生成失败：未找到可用的中文字体。"
        }
        ExportError::TypstCompile(..) | ExportError::TypstPdfExport(..) => {
            "PDF 生成失败：文档排版失败。"
        }
        _ => "PDF 生成失败。",
    };
    IpcError {
        code: "export_failed",
        message,
    }
}

#[tauri::command]
fn export_capabilities(state: State<'_, AppState>) -> Result<ExportCapabilities, IpcError> {
    let custom_template_configured = lock_documents(&state)?
        .get_setting::<Vec<u8>>("export.docx_template")
        .map_err(map_store_error)?
        .is_some();
    Ok(ExportCapabilities {
        custom_template_configured,
    })
}

#[tauri::command]
fn save_docx_template(state: State<'_, AppState>, bytes: Vec<u8>) -> Result<(), IpcError> {
    validate_docx_template(&bytes).map_err(map_template_error)?;
    lock_documents(&state)?
        .set_setting("export.docx_template", &bytes)
        .map_err(map_store_error)
}

#[tauri::command]
fn delete_docx_template(state: State<'_, AppState>) -> Result<(), IpcError> {
    lock_documents(&state)?
        .delete_setting("export.docx_template")
        .map(|_| ())
        .map_err(map_store_error)
}

fn ensure_exportable(document: &ODoc, packs: &PackRegistry) -> Result<(), IpcError> {
    let lint_report = odoc_lint::lint_document(document, packs).map_err(map_lint_error)?;
    let fact_check_report = odoc_lint::fact_check_document(document).map_err(map_lint_error)?;
    let fact_error_count: u32 = fact_check_report
        .findings
        .iter()
        .filter(|finding| finding.severity == odoc_lint::Severity::Error)
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    if lint_report.error_count.saturating_add(fact_error_count) > 0 {
        Err(IpcError {
            code: "export_blocked",
            message: "文档存在未解决的确定性错误，请修正后再导出。",
        })
    } else {
        Ok(())
    }
}

fn export_style(state: &AppState, packs: &PackRegistry) -> Result<Gb9704Style, IpcError> {
    let pack = packs.get("gb9704-format").ok_or(IpcError {
        code: "export_style_unavailable",
        message: "GB/T 9704 样式资源不可用。",
    })?;
    let resource = pack
        .resources_for_stage(odoc_pack::StageId::Export, None)
        .map_err(|_| IpcError {
            code: "export_style_unavailable",
            message: "GB/T 9704 样式资源不可用。",
        })?
        .into_iter()
        .find(|resource| resource.role == odoc_pack::ResourceRole::Style)
        .ok_or(IpcError {
            code: "export_style_unavailable",
            message: "GB/T 9704 样式资源不可用。",
        })?;
    let style_text = pack
        .read_text(&resource.relative_path)
        .map_err(|_| IpcError {
            code: "export_style_unavailable",
            message: "GB/T 9704 样式资源不可用。",
        })?;
    let mappings = lock_documents(state)?
        .app_config()
        .map_err(map_store_error)?
        .font_mappings;
    Gb9704Style::from_toml(&style_text)
        .map(|style| style.with_font_mappings(&mappings))
        .map_err(|_| IpcError {
            code: "export_style_invalid",
            message: "GB/T 9704 样式配置格式无效。",
        })
}

fn validate_draft_start_request(request: &DraftStartRequest) -> Result<(), IpcError> {
    const MAX_MODEL_TEXT_BYTES: usize = 2 * 1024 * 1024;
    let material_valid = !request.material.trim().is_empty()
        && request.material.len() <= MAX_DRAFT_MATERIAL_BYTES
        && !request.material.chars().any(|character| character == '\0');
    let model_text_valid = request.model_text.as_ref().is_none_or(|text| {
        text.len() <= MAX_MODEL_TEXT_BYTES && !text.chars().any(|character| character == '\0')
    });
    if material_valid && model_text_valid {
        Ok(())
    } else {
        Err(IpcError {
            code: "invalid_input",
            message: "起草材料格式无效。",
        })
    }
}

fn validate_rerun_stage_request(request: &RerunStageRequest) -> Result<(), IpcError> {
    const MAX_STAGE_ID_BYTES: usize = 64;
    let session_valid = !request.session_id.is_empty() && request.session_id.len() <= 128;
    let stage_valid = !request.stage_id.is_empty()
        && request.stage_id.len() <= MAX_STAGE_ID_BYTES
        && request
            .stage_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if session_valid && stage_valid {
        Ok(())
    } else {
        Err(IpcError {
            code: "invalid_input",
            message: "起草阶段重跑请求格式无效。",
        })
    }
}

/// Selects the pipeline file a draft request runs: polish mode wins, then a non-blank model
/// text means imitate mode.
fn draft_pipeline_file(model_text: Option<&str>, polish: bool) -> &'static str {
    if polish {
        "polish.yaml"
    } else if model_text.is_some_and(|text| !text.trim().is_empty()) {
        "imitate.yaml"
    } else {
        "draft.yaml"
    }
}

/// Normalizes the optional reviewer focus, rejecting oversized or control-character input.
///
/// An all-whitespace focus is treated as "no focus" rather than an error, because the field is
/// an empty text box in the UI.
fn validate_review_start_request(request: &ReviewStartRequest) -> Result<Option<String>, IpcError> {
    let Some(focus) = request.focus.as_ref() else {
        return Ok(None);
    };
    let trimmed = focus.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > MAX_REVIEW_FOCUS_BYTES || trimmed.chars().any(char::is_control) {
        return Err(IpcError {
            code: "invalid_input",
            message: "审核重点格式无效。",
        });
    }
    Ok(Some(trimmed.to_owned()))
}

fn persist_completed_event(
    state: &AppState,
    event: &DraftEvent,
    document_saved: &mut bool,
) -> Result<(), IpcError> {
    if let DraftEvent::Completed { outcome } = event
        && !*document_saved
    {
        save_draft_document(state, &outcome.document)?;
        *document_saved = true;
    }
    Ok(())
}

fn save_draft_document(state: &AppState, document: &ODoc) -> Result<(), IpcError> {
    lock_documents(state)?
        .save_document(document, Some("draft"))
        .map(|_| ())
        .map_err(map_store_error)
}

#[tauri::command]
fn set_provider_secret(
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
fn has_provider_secret(state: State<'_, AppState>, provider_id: String) -> Result<bool, IpcError> {
    let account = provider_secret_account(&provider_id)?;
    state.secrets.has_secret(&account).map_err(map_secret_error)
}

#[tauri::command]
fn delete_provider_secret(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<bool, IpcError> {
    let account = provider_secret_account(&provider_id)?;
    state
        .secrets
        .delete_secret(&account)
        .map_err(map_secret_error)
}

fn lock_documents<'a>(
    state: &'a AppState,
) -> Result<std::sync::MutexGuard<'a, DocumentStore>, IpcError> {
    state.documents.lock().map_err(|_| IpcError {
        code: "store_unavailable",
        message: "本地文档库暂不可用。",
    })
}

fn provider_secret_account(provider_id: &str) -> Result<String, IpcError> {
    let valid = !provider_id.is_empty()
        && provider_id.len() <= 64
        && provider_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if valid {
        Ok(format!("provider:{provider_id}:api-key"))
    } else {
        Err(IpcError {
            code: "invalid_input",
            message: "模型服务标识格式无效。",
        })
    }
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

fn wake_kb_sync(state: &AppState) {
    state
        .kb_sync_generation
        .send_modify(|generation| *generation = generation.wrapping_add(1));
}

async fn persist_fns_change(
    state: &AppState,
    provider: &FnsProvider,
    pending: FnsPendingChange,
) -> Result<(), IpcError> {
    let change = pending.event.clone();
    let auto_tagging_enabled = lock_documents(state)?
        .kb_config()
        .map_err(map_store_error)?
        .auto_tagging_enabled;
    let applied = match change.kind {
        ChangeKind::Deleted => lock_documents(state)?
            .remove_cached_kb_note(&change.vault, &change.path)
            .map_err(map_store_error),
        ChangeKind::Added | ChangeKind::Modified => {
            if let Some(old_path) = change.old_path.as_deref() {
                lock_documents(state)?
                    .remove_cached_kb_note(&change.vault, old_path)
                    .map_err(map_store_error)?;
            }
            match provider.get(&change.path).await {
                Ok(note) => match prepare_kb_note(provider, note, auto_tagging_enabled).await {
                    Ok(note) => lock_documents(state)?
                        .cache_kb_note(&note)
                        .map_err(map_store_error),
                    Err(error) => Err(error),
                },
                Err(KbError::NotFound) => lock_documents(state)?
                    .remove_cached_kb_note(&change.vault, &change.path)
                    .map_err(map_store_error),
                Err(error) => Err(map_kb_error(error)),
            }
        }
    };
    if let Err(error) = applied {
        pending.reject();
        return Err(error);
    }

    let (note_count, chunk_count) = lock_documents(state)?
        .kb_cache_counts(&change.vault)
        .map_err(map_store_error)?;
    let previous = lock_documents(state)?
        .kb_index_state(&change.vault)
        .map_err(map_store_error)?;
    let cursor = change
        .cursor
        .filter(|_| pending.advance_cursor)
        .unwrap_or_default()
        .max(previous.as_ref().map_or(0, |value| value.last_cursor));
    lock_documents(state)?
        .save_kb_index_state(&odoc_store::KbIndexState {
            vault: change.vault,
            provider_kind: "fns".to_owned(),
            last_cursor: cursor,
            note_count,
            chunk_count,
            status: "syncing".to_owned(),
            error_message: None,
            updated_at: now_millis(),
        })
        .map_err(map_store_error)?;
    pending.commit();
    Ok(())
}

async fn prepare_kb_note(
    provider: &dyn KbProvider,
    note: KbNote,
    auto_tagging_enabled: bool,
) -> Result<KbNote, IpcError> {
    if !auto_tagging_enabled {
        return Ok(note);
    }
    let fields = suggest_missing_frontmatter(&note);
    if fields.is_empty() {
        return Ok(note);
    }
    provider
        .patch_frontmatter(&note.path, &fields)
        .await
        .map_err(map_kb_error)?;
    provider.get(&note.path).await.map_err(map_kb_error)
}

fn save_fns_cursor(state: &AppState, vault: &str, cursor: i64) -> Result<(), IpcError> {
    let (note_count, chunk_count) = lock_documents(state)?
        .kb_cache_counts(vault)
        .map_err(map_store_error)?;
    let previous = lock_documents(state)?
        .kb_index_state(vault)
        .map_err(map_store_error)?;
    lock_documents(state)?
        .save_kb_index_state(&odoc_store::KbIndexState {
            vault: vault.to_owned(),
            provider_kind: "fns".to_owned(),
            last_cursor: cursor.max(previous.as_ref().map_or(0, |value| value.last_cursor)),
            note_count,
            chunk_count,
            status: "ready".to_owned(),
            error_message: None,
            updated_at: now_millis(),
        })
        .map_err(map_store_error)
}

struct BackgroundFnsConfig {
    config: KbConfig,
    rest_token: String,
    ws_token: String,
    last_cursor: i64,
}

fn background_fns_config(state: &AppState) -> Result<Option<BackgroundFnsConfig>, IpcError> {
    let config = lock_documents(state)?
        .kb_config()
        .map_err(map_store_error)?
        .normalized()
        .map_err(map_kb_error)?;
    if !config.enabled
        || config.provider_kind != KbProviderKind::Fns
        || !config.index_enabled
        || !config.sync_enabled
    {
        return Ok(None);
    }
    let Some(rest_token) = state
        .secrets
        .get_secret(KB_FNS_TOKEN_ACCOUNT)
        .map_err(map_secret_error)?
    else {
        return Ok(None);
    };
    let Some(ws_token) = state
        .secrets
        .get_secret(KB_FNS_WS_TOKEN_ACCOUNT)
        .map_err(map_secret_error)?
    else {
        return Ok(None);
    };
    let last_cursor = lock_documents(state)?
        .kb_index_state(&config.vault)
        .map_err(map_store_error)?
        .map_or(0, |value| value.last_cursor);
    Ok(Some(BackgroundFnsConfig {
        config,
        rest_token,
        ws_token,
        last_cursor,
    }))
}

async fn run_fns_subscription(
    state: &AppState,
    subscription: BackgroundFnsConfig,
) -> Result<(), IpcError> {
    let provider = FnsProvider::new(
        &subscription.config.base_url,
        subscription.rest_token,
        Some(subscription.config.vault.clone()),
    )
    .map_err(map_kb_error)?;
    let mut events = watch_fns_notes(FnsWatchConfig {
        base_url: subscription.config.base_url,
        token: subscription.ws_token,
        vault: subscription.config.vault.clone(),
        last_cursor: subscription.last_cursor,
    })
    .await
    .map_err(map_kb_error)?;
    while let Some(event) = events.recv().await {
        match event.map_err(map_kb_error)? {
            FnsWatchEvent::Change(pending) => {
                persist_fns_change(state, &provider, pending).await?;
            }
            FnsWatchEvent::Cursor(cursor) => {
                save_fns_cursor(state, &subscription.config.vault, cursor)?;
            }
        }
    }
    Err(IpcError {
        code: "kb_sync_disconnected",
        message: "FNS 实时同步连接已断开。",
    })
}

fn save_background_sync_error(state: &AppState, message: &str) {
    let Ok(config) =
        lock_documents(state).and_then(|store| store.kb_config().map_err(map_store_error))
    else {
        return;
    };
    if config.vault.is_empty() {
        return;
    }
    let previous = lock_documents(state)
        .ok()
        .and_then(|store| store.kb_index_state(&config.vault).ok().flatten());
    let (note_count, chunk_count) = lock_documents(state)
        .ok()
        .and_then(|store| store.kb_cache_counts(&config.vault).ok())
        .unwrap_or_default();
    let failed = odoc_store::KbIndexState {
        vault: config.vault,
        provider_kind: "fns".to_owned(),
        last_cursor: previous.map_or(0, |value| value.last_cursor),
        note_count,
        chunk_count,
        status: "error".to_owned(),
        error_message: Some(message.to_owned()),
        updated_at: now_millis(),
    };
    if let Ok(store) = lock_documents(state) {
        let _ = store.save_kb_index_state(&failed);
    }
}

async fn kb_sync_supervisor<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    mut generation: tokio::sync::watch::Receiver<u64>,
) {
    let mut retry_seconds = 1_u64;
    loop {
        let state = app.state::<AppState>();
        let subscription = match background_fns_config(&state) {
            Ok(Some(subscription)) => subscription,
            Ok(None) => {
                if generation.changed().await.is_err() {
                    return;
                }
                retry_seconds = 1;
                continue;
            }
            Err(error) => {
                save_background_sync_error(&state, error.message);
                tokio::select! {
                    result = generation.changed() => if result.is_err() { return; },
                    () = tokio::time::sleep(Duration::from_secs(retry_seconds)) => {},
                }
                retry_seconds = retry_seconds.saturating_mul(2).min(60);
                continue;
            }
        };
        let run = run_fns_subscription(&state, subscription);
        tokio::pin!(run);
        let disconnected = tokio::select! {
            result = generation.changed() => {
                if result.is_err() { return; }
                false
            }
            result = &mut run => {
                if let Err(error) = result {
                    save_background_sync_error(&state, error.message);
                }
                true
            }
        };
        if !disconnected {
            retry_seconds = 1;
            continue;
        }
        tokio::select! {
            result = generation.changed() => {
                if result.is_err() { return; }
                retry_seconds = 1;
            }
            () = tokio::time::sleep(Duration::from_secs(retry_seconds)) => {
                retry_seconds = retry_seconds.saturating_mul(2).min(60);
            }
        }
    }
}

fn provider_kind_name(kind: KbProviderKind) -> &'static str {
    match kind {
        KbProviderKind::Fns => "fns",
        KbProviderKind::LocalVault => "local_vault",
        KbProviderKind::Mcp => "mcp",
    }
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

fn index_failure_state(
    started: &odoc_store::KbIndexState,
    processed: usize,
    _detail: String,
) -> odoc_store::KbIndexState {
    odoc_store::KbIndexState {
        note_count: u32::try_from(processed).unwrap_or(u32::MAX),
        status: "error".to_owned(),
        error_message: Some("本地知识库索引写入失败。".to_owned()),
        updated_at: now_millis(),
        ..started.clone()
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_millis()).ok())
        .unwrap_or(0)
}

fn changed_line_counts(left: &str, right: &str) -> (u32, u32) {
    let left = left.lines().collect::<Vec<_>>();
    let right = right.lines().collect::<Vec<_>>();
    let common = longest_common_subsequence_len(&left, &right);
    let removed = left.len().saturating_sub(common);
    let added = right.len().saturating_sub(common);
    (
        u32::try_from(removed).unwrap_or(u32::MAX),
        u32::try_from(added).unwrap_or(u32::MAX),
    )
}

fn longest_common_subsequence_len(left: &[&str], right: &[&str]) -> usize {
    let mut previous = vec![0_usize; right.len().saturating_add(1)];
    let mut current = previous.clone();
    for left_line in left {
        for (index, right_line) in right.iter().enumerate() {
            current[index + 1] = if left_line == right_line {
                previous[index].saturating_add(1)
            } else {
                current[index].max(previous[index + 1])
            };
        }
        std::mem::swap(&mut previous, &mut current);
        current.fill(0);
    }
    previous[right.len()]
}

fn normalize_archive_path(value: &str) -> Result<String, IpcError> {
    let mut path = value.trim().replace('\\', "/");
    if !path.ends_with(".md") {
        path.push_str(".md");
    }
    let valid = !path.is_empty()
        && path.len() <= 1024
        && !path.starts_with('/')
        && !path.contains("//")
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..");
    if valid {
        Ok(path)
    } else {
        Err(IpcError {
            code: "invalid_input",
            message: "归档路径格式无效。",
        })
    }
}

fn document_to_markdown(document: &ODoc) -> String {
    let title = document.body.title.text.trim();
    let issuer = document
        .body
        .signer
        .as_ref()
        .map(|signer| signer.authority.trim())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            document
                .header
                .as_ref()
                .map(|header| header.issuing_authority_mark.trim())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_default();
    let year = document
        .body
        .date
        .as_ref()
        .and_then(|date| date.value.get(0..4))
        .filter(|value| value.chars().all(|character| character.is_ascii_digit()))
        .unwrap_or_default();
    let mut output = String::from("---\n");
    for (key, value) in [
        ("title", title),
        ("doc_type", document.meta.doc_type.as_str()),
        ("doc_type_zh", document.meta.doc_type.label_zh()),
        ("year", year),
        ("issuer", issuer),
        (
            "security_level",
            security_level_name(document.meta.security_level),
        ),
        ("odoc_id", document.id.as_str()),
    ] {
        if !value.is_empty() {
            output.push_str(key);
            output.push_str(": ");
            output.push_str(&yaml_string(value));
            output.push('\n');
        }
    }
    output.push_str("---\n\n");
    if !title.is_empty() {
        output.push_str("# ");
        output.push_str(title);
        output.push_str("\n\n");
    }
    for recipient in &document.body.main_recipients {
        output.push_str(recipient.trim());
        output.push_str("：\n\n");
    }
    for block in &document.body.blocks {
        match block {
            odoc_model::Block::Heading { level, content, .. } => {
                let depth = match level {
                    odoc_model::HeadingLevel::L1 => 2,
                    odoc_model::HeadingLevel::L2 => 3,
                    odoc_model::HeadingLevel::L3 => 4,
                    odoc_model::HeadingLevel::L4 => 5,
                };
                output.push_str(&"#".repeat(depth));
                output.push(' ');
                output.push_str(content.trim());
                output.push_str("\n\n");
            }
            odoc_model::Block::Paragraph { content, .. }
            | odoc_model::Block::Quote { content, .. } => {
                output.push_str(content.trim());
                output.push_str("\n\n");
            }
            odoc_model::Block::List { items, .. } => {
                for item in items {
                    output.push_str("- ");
                    output.push_str(item.content.trim());
                    output.push('\n');
                }
                output.push('\n');
            }
            odoc_model::Block::Table { rows, .. } => {
                for row in rows {
                    output.push_str("| ");
                    output.push_str(
                        &row.cells
                            .iter()
                            .map(|cell| cell.content.trim())
                            .collect::<Vec<_>>()
                            .join(" | "),
                    );
                    output.push_str(" |\n");
                }
                output.push('\n');
            }
        }
    }
    if let Some(signer) = &document.body.signer {
        output.push_str(signer.authority.trim());
        output.push_str("\n\n");
    }
    if let Some(date) = &document.body.date {
        output.push_str(date.value.trim());
        output.push('\n');
    }
    output
}

fn yaml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn security_level_name(level: SecurityLevel) -> &'static str {
    match level {
        SecurityLevel::Public => "public",
        SecurityLevel::Internal => "internal",
        SecurityLevel::Secret => "secret",
        SecurityLevel::Confidential => "confidential",
    }
}

fn provider_client(state: &AppState, provider_id: &str) -> Result<ProviderClient, IpcError> {
    let profile = lock_documents(state)?
        .provider_profile(provider_id)
        .map_err(map_store_error)?;
    let api_key = if matches!(
        profile.kind,
        ProviderKind::OpenAiCompatible | ProviderKind::Anthropic
    ) {
        let account = provider_secret_account(provider_id)?;
        state
            .secrets
            .get_secret(&account)
            .map_err(map_secret_error)?
    } else {
        None
    };
    ProviderClient::new(profile, api_key).map_err(map_llm_error)
}

fn configured_kb_provider(state: &AppState) -> Result<(Box<dyn KbProvider>, KbConfig), IpcError> {
    let config = lock_documents(state)?
        .kb_config()
        .map_err(map_store_error)?
        .normalized()
        .map_err(map_kb_error)?;
    if !config.enabled {
        return Err(IpcError {
            code: "kb_disabled",
            message: "知识库尚未启用，请先在设置中完成配置。",
        });
    }
    let provider: Box<dyn KbProvider> = match config.provider_kind {
        KbProviderKind::Fns => {
            let token = state
                .secrets
                .get_secret(KB_FNS_TOKEN_ACCOUNT)
                .map_err(map_secret_error)?
                .ok_or(IpcError {
                    code: "kb_token_missing",
                    message: "尚未保存 FNS token。",
                })?;
            Box::new(
                FnsProvider::new(&config.base_url, token, Some(config.vault.clone()))
                    .map_err(map_kb_error)?,
            )
        }
        KbProviderKind::LocalVault => Box::new(
            LocalVaultProvider::open(&config.local_path, config.vault.clone())
                .map_err(map_kb_error)?,
        ),
        KbProviderKind::Mcp => {
            let token = state
                .secrets
                .get_secret(KB_MCP_TOKEN_ACCOUNT)
                .map_err(map_secret_error)?
                .ok_or(IpcError {
                    code: "kb_token_missing",
                    message: "尚未保存 MCP token。",
                })?;
            Box::new(
                McpProvider::new(&config.mcp_url, &token, config.vault.clone())
                    .map_err(map_kb_error)?,
            )
        }
    };
    Ok((provider, config))
}

fn optional_kb_provider(
    state: &AppState,
    security_level: SecurityLevel,
) -> Result<Option<Box<dyn KbProvider>>, IpcError> {
    let config = lock_documents(state)?
        .kb_config()
        .map_err(map_store_error)?;
    if !config.enabled {
        return Ok(None);
    }
    if !security_level.allows_remote_provider() && !config.is_local() {
        return Err(IpcError {
            code: "kb_remote_blocked",
            message: "当前密级禁止将检索词发送到远程知识库，请改用本地 Vault 或本机 FNS。",
        });
    }
    configured_kb_provider(state).map(|(provider, _)| Some(provider))
}

fn enforce_kb_security(state: &AppState, document_id: Option<&str>) -> Result<(), IpcError> {
    let Some(document_id) = document_id else {
        return Ok(());
    };
    let security_level = lock_documents(state)?
        .get_document(document_id)
        .map_err(map_store_error)?
        .meta
        .security_level;
    let config = lock_documents(state)?
        .kb_config()
        .map_err(map_store_error)?;
    if config.enabled && !security_level.allows_remote_provider() && !config.is_local() {
        return Err(IpcError {
            code: "kb_remote_blocked",
            message: "当前密级禁止将检索词发送到远程知识库，请改用本地 Vault 或本机 FNS。",
        });
    }
    Ok(())
}

fn bind_request_to_document(
    state: &AppState,
    document_id: &str,
    request: &mut ChatRequest,
) -> Result<(), IpcError> {
    let stored_level = lock_documents(state)?
        .get_document(document_id)
        .map_err(map_store_error)?
        .meta
        .security_level;
    if !stored_level.allows_remote_provider() || !request.security_level.allows_remote_provider() {
        request.security_level = SecurityLevel::Secret;
    } else {
        request.security_level = stored_level;
    }
    Ok(())
}

fn connection_test_request() -> ChatRequest {
    ChatRequest {
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content: "只回复 OK".to_owned(),
        }],
        temperature: Some(0.0),
        max_tokens: Some(8),
        json_mode: false,
        security_level: SecurityLevel::Internal,
    }
}

fn map_store_error(error: StoreError) -> IpcError {
    match error {
        StoreError::DocumentNotFound(_) | StoreError::VersionNotFound { .. } => IpcError {
            code: "document_not_found",
            message: "没有找到指定的公文或版本。",
        },
        StoreError::ProviderNotFound(_) => IpcError {
            code: "provider_not_found",
            message: "没有找到指定的模型服务。",
        },
        StoreError::InvalidDocument(_)
        | StoreError::InvalidSettingKey(_)
        | StoreError::InvalidSession(_)
        | StoreError::InvalidProviderConfiguration
        | StoreError::InvalidKnowledgeBaseConfiguration => IpcError {
            code: "invalid_input",
            message: "提交的数据格式无效。",
        },
        StoreError::StaleRevision { .. } => IpcError {
            code: "stale_revision",
            message: "检测到较新的已保存版本，请重新打开公文后再修改。",
        },
        StoreError::Serialization(_) => IpcError {
            code: "serialization_failed",
            message: "公文数据无法序列化。",
        },
        StoreError::Sqlite(_)
        | StoreError::Io(_)
        | StoreError::ApplicationDirectoryUnavailable
        | StoreError::UnsupportedSchema { .. }
        | StoreError::SessionNotFound(_)
        | StoreError::InvalidSystemClock => IpcError {
            code: "store_unavailable",
            message: "本地文档库暂不可用。",
        },
    }
}

fn map_llm_error(error: LlmError) -> IpcError {
    IpcError {
        code: error.code(),
        message: error.user_message(),
    }
}

fn map_engine_error(error: EngineError) -> IpcError {
    IpcError {
        code: error.code(),
        message: error.user_message(),
    }
}

fn map_kb_error(error: KbError) -> IpcError {
    IpcError {
        code: error.code(),
        message: error.user_message(),
    }
}

fn map_lint_error(_error: odoc_lint::LintError) -> IpcError {
    IpcError {
        code: "lint_unavailable",
        message: "确定性质检暂不可用。",
    }
}

fn map_export_error(_error: odoc_export::ExportError) -> IpcError {
    IpcError {
        code: "export_failed",
        message: "DOCX 文件生成失败。",
    }
}

fn map_import_error(error: ExportError) -> IpcError {
    match error {
        ExportError::PackageTooLarge => IpcError {
            code: "import_too_large",
            message: "DOCX 文件超过 64 MiB，无法导入。",
        },
        _ => IpcError {
            code: "import_failed",
            message: "DOCX 文件结构无效或无法读取。",
        },
    }
}

fn map_reference_error(error: ExportError) -> IpcError {
    match error {
        ExportError::PackageTooLarge => IpcError {
            code: "reference_too_large",
            message: "参考文件超过 64 MiB，内部内容超过安全限制，或提取文字超过 2 MiB。",
        },
        ExportError::UnsupportedReferenceFormat => IpcError {
            code: "reference_unsupported",
            message: "暂不支持该参考文件格式。支持 PDF、OFD、DOC、DOCX、XLSX、XLS 和 WPS。",
        },
        _ => IpcError {
            code: "reference_invalid",
            message: "参考文件结构无效、已加密、为纯扫描件，或没有可提取的文字。",
        },
    }
}

fn map_template_error(error: ExportError) -> IpcError {
    match error {
        ExportError::TemplateMarkerMissing => IpcError {
            code: "template_marker_missing",
            message: "DOCX 模板须包含独立的 BINGBI_BODY 占位段落。",
        },
        ExportError::PackageTooLarge => IpcError {
            code: "template_too_large",
            message: "DOCX 模板超过 64 MiB，无法保存。",
        },
        _ => IpcError {
            code: "template_invalid",
            message: "DOCX 模板结构无效或无法读取。",
        },
    }
}

fn map_tauri_error(_error: tauri::Error) -> IpcError {
    IpcError {
        code: "pipeline_unavailable",
        message: "起草流程配置不可用。",
    }
}

fn map_secret_error(_error: SecretError) -> IpcError {
    IpcError {
        code: "secret_store_unavailable",
        message: "系统凭据库不可用或处于锁定状态。",
    }
}

fn pack_summaries(packs: &PackRegistry) -> Vec<PackSummary> {
    packs
        .iter()
        .map(|pack| PackSummary::from(pack.manifest()))
        .collect()
}

fn builtin_pack_root<R: tauri::Runtime>(app: &tauri::App<R>) -> Result<PathBuf, tauri::Error> {
    if cfg!(debug_assertions) {
        Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../packs"))
    } else {
        Ok(app.path().resource_dir()?.join("packs"))
    }
}

fn development_pipeline_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../pipelines")
        .join(file_name)
}

fn builtin_pipeline_path<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    file_name: &str,
) -> Result<PathBuf, tauri::Error> {
    if cfg!(debug_assertions) {
        Ok(development_pipeline_path(file_name))
    } else {
        Ok(app.path().resource_dir()?.join("pipelines").join(file_name))
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let database_path = DocumentStore::default_database_path()?;
            let documents = DocumentStore::open(&database_path)?;
            let crash_reporter = LocalCrashReporter::new(
                &database_path,
                env!("CARGO_PKG_VERSION"),
                documents.app_config()?.crash_reporting_enabled,
            );
            crash_reporter.install();
            let secrets = KeyringSecretStore::new(DEFAULT_SECRET_SERVICE)?;
            // The updater needs a release signing key; only the release pipeline has one, so
            // the plugin is registered conditionally and dev/CI builds stay signature-free.
            if let Ok(pubkey) = std::env::var("BINGBI_UPDATER_PUBKEY")
                && !pubkey.trim().is_empty()
            {
                app.handle().plugin(
                    tauri_plugin_updater::Builder::new()
                        .pubkey(pubkey.trim().to_owned())
                        .build(),
                )?;
            }
            let user_pack_root = DocumentStore::default_config_dir()?.join("packs");
            ensure_user_pack_root(&user_pack_root)?;
            let (packs, pack_issues) =
                PackRegistry::load_with_user_root(builtin_pack_root(app)?, &user_pack_root)?;
            let (kb_sync_generation, kb_sync_receiver) = tokio::sync::watch::channel(0_u64);
            app.manage(AppState {
                documents: Mutex::new(documents),
                secrets: Box::new(secrets),
                crash_reporter,
                kb_sync_generation,
                user_pack_root,
                pack_issues: pack_issues
                    .into_iter()
                    .map(|issue| issue.to_string())
                    .collect(),
            });
            app.manage(packs);
            tauri::async_runtime::spawn(kb_sync_supervisor(app.handle().clone(), kb_sync_receiver));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_empty_document,
            backend_version,
            list_pack_summaries,
            user_pack_root,
            pack_load_issues,
            list_crash_events,
            clear_crash_events,
            check_for_update,
            apply_update,
            save_document,
            load_document,
            list_documents,
            list_sessions,
            list_stage_runs,
            delete_document,
            list_document_versions,
            load_document_version,
            restore_document_version,
            diff_document_versions,
            load_app_config,
            save_app_config,
            load_kb_config,
            save_kb_config,
            set_kb_fns_token,
            has_kb_fns_token,
            delete_kb_fns_token,
            set_kb_fns_ws_token,
            has_kb_fns_ws_token,
            delete_kb_fns_ws_token,
            set_kb_mcp_token,
            has_kb_mcp_token,
            delete_kb_mcp_token,
            kb_search,
            kb_get,
            kb_index_state,
            kb_rebuild_index,
            kb_clear_index,
            kb_sync_once,
            archive_document_to_kb,
            list_provider_profiles,
            save_provider_profile,
            set_provider_secret,
            has_provider_secret,
            delete_provider_secret,
            test_provider_connection,
            list_provider_models,
            chat_with_provider,
            chat_stream_with_provider,
            draft_start,
            rerun_stage,
            review_start,
            export_docx,
            import_docx,
            extract_docx_text,
            extract_reference_text,
            export_pdf,
            export_capabilities,
            save_docx_template,
            delete_docx_template
        ])
        .run(tauri::generate_context!())
        .expect("failed to run the Bingbi desktop application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use odoc_engine::StageStatus;

    #[test]
    fn creates_an_empty_document_for_ipc() {
        let document = create_empty_document();
        assert_eq!(document.schema_version, odoc_model::ODOC_SCHEMA_VERSION);
        assert!(document.body.blocks.len() == 1);
    }

    #[test]
    fn reports_the_backend_version() {
        assert!(backend_version().starts_with("rust-core "));
    }

    #[test]
    fn exposes_validated_builtin_pack_summaries() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../packs");
        let packs = PackRegistry::load(root).expect("built-in packs should load");
        let summaries = pack_summaries(&packs);
        assert_eq!(
            summaries
                .iter()
                .map(|summary| summary.id.as_str())
                .collect::<Vec<_>>(),
            ["gb9704-format", "renmin-style"]
        );
        assert!(summaries.iter().all(|summary| !summary.version.is_empty()));
    }

    #[test]
    fn provider_secret_accounts_are_namespaced_and_validated() {
        assert_eq!(
            provider_secret_account("deepseek").unwrap(),
            "provider:deepseek:api-key"
        );
        assert!(provider_secret_account("../deepseek").is_err());
        assert!(provider_secret_account("").is_err());
    }

    #[test]
    fn knowledge_base_token_account_is_separate_from_model_credentials() {
        assert_eq!(KB_FNS_TOKEN_ACCOUNT, "knowledge-base:fns:token");
        assert_ne!(
            KB_FNS_TOKEN_ACCOUNT,
            provider_secret_account("fns").expect("provider account should build")
        );
    }

    #[test]
    fn storage_errors_are_redacted_for_ipc() {
        let error = map_store_error(StoreError::DocumentNotFound("secret-id".to_owned()));
        assert_eq!(error.code, "document_not_found");
        assert!(!error.message.contains("secret-id"));
    }

    #[test]
    fn llm_errors_are_stable_and_redacted_for_ipc() {
        let error = map_llm_error(LlmError::HttpStatus(401));
        assert_eq!(error.code, "llm_http_error");
        assert_eq!(error.message, "模型服务拒绝了凭据，请检查 API Key。");
    }

    #[test]
    fn knowledge_base_errors_are_stable_and_redacted_for_ipc() {
        let error = map_kb_error(KbError::HttpStatus(401));
        assert_eq!(error.code, "kb_http_error");
        assert!(!error.message.contains("401"));
    }

    #[test]
    fn connection_test_is_small_and_non_sensitive() {
        let request = connection_test_request();
        assert_eq!(request.security_level, SecurityLevel::Internal);
        assert_eq!(request.max_tokens, Some(8));
        assert_eq!(request.messages.len(), 1);
    }

    #[test]
    fn stored_document_security_cannot_be_downgraded_by_ipc_input() {
        let mut store = DocumentStore::open_in_memory().expect("store should open");
        let mut document = ODoc::empty();
        document.meta.security_level = SecurityLevel::Secret;
        store
            .save_document(&document, Some("test"))
            .expect("document should save");
        let state = AppState {
            documents: Mutex::new(store),
            secrets: Box::new(odoc_store::MemorySecretStore::default()),
            crash_reporter: LocalCrashReporter::new("/tmp/bingbi-test.sqlite3", "0.1.0", false),
            kb_sync_generation: tokio::sync::watch::channel(0_u64).0,
            user_pack_root: PathBuf::from("/tmp/user-packs"),
            pack_issues: Vec::new(),
        };
        let mut request = connection_test_request();
        assert_eq!(request.security_level, SecurityLevel::Internal);
        bind_request_to_document(&state, &document.id, &mut request).expect("security should bind");
        assert_eq!(request.security_level, SecurityLevel::Secret);
    }

    #[test]
    fn development_pipeline_paths_point_to_repository_resources() {
        for file_name in ["draft.yaml", "review.yaml", "imitate.yaml"] {
            let path = development_pipeline_path(file_name);
            assert!(path.ends_with(format!("pipelines/{file_name}")));
            assert!(
                path.is_file(),
                "{file_name} should ship with the repository"
            );
        }
    }

    #[test]
    fn builtin_pipelines_pass_kernel_validation() {
        let draft = PipelineDefinition::load(development_pipeline_path("draft.yaml"))
            .expect("draft pipeline should load");
        assert_eq!(draft.kind, odoc_engine::PipelineKind::Draft);
        let review = PipelineDefinition::load(development_pipeline_path("review.yaml"))
            .expect("review pipeline should load");
        assert_eq!(review.kind, odoc_engine::PipelineKind::Review);
        let imitate = PipelineDefinition::load(development_pipeline_path("imitate.yaml"))
            .expect("imitate pipeline should load");
        assert_eq!(imitate.kind, odoc_engine::PipelineKind::Imitate);
    }

    #[test]
    fn draft_request_rejects_empty_and_oversized_material() {
        let empty = DraftStartRequest {
            document_id: "doc".to_owned(),
            material: " \n".to_owned(),
            provider_id: "ollama".to_owned(),
            confirmed_doc_type: None,
            model_text: None,
            polish: false,
        };
        assert!(validate_draft_start_request(&empty).is_err());
        let oversized = DraftStartRequest {
            material: "x".repeat(MAX_DRAFT_MATERIAL_BYTES + 1),
            ..empty
        };
        assert!(validate_draft_start_request(&oversized).is_err());
    }

    #[test]
    fn reference_upload_stays_outside_document_import_and_has_stable_errors() {
        let error = extract_docx_text(Vec::new()).expect_err("empty reference should fail");
        assert_eq!(error.code, "reference_too_large");
        assert_eq!(
            map_reference_error(ExportError::InvalidXml).code,
            "reference_invalid"
        );
        assert_eq!(
            map_reference_error(ExportError::PackageTooLarge).code,
            "reference_too_large"
        );
    }

    #[test]
    fn draft_request_validates_model_text_and_selects_the_imitate_pipeline() {
        let base = DraftStartRequest {
            document_id: "doc".to_owned(),
            material: "申请事项".to_owned(),
            provider_id: "ollama".to_owned(),
            confirmed_doc_type: None,
            model_text: None,
            polish: false,
        };
        assert!(validate_draft_start_request(&base).is_ok());
        let imitating = DraftStartRequest {
            document_id: "doc".to_owned(),
            material: "申请事项".to_owned(),
            provider_id: "ollama".to_owned(),
            confirmed_doc_type: None,
            model_text: Some("范文正文".to_owned()),
            polish: false,
        };
        assert!(validate_draft_start_request(&imitating).is_ok());
        let oversized_model = DraftStartRequest {
            document_id: "doc".to_owned(),
            material: "申请事项".to_owned(),
            provider_id: "ollama".to_owned(),
            confirmed_doc_type: None,
            model_text: Some("x".repeat(2 * 1024 * 1024 + 1)),
            polish: false,
        };
        assert!(validate_draft_start_request(&oversized_model).is_err());
        let control_character_model = DraftStartRequest {
            document_id: "doc".to_owned(),
            material: "申请事项".to_owned(),
            provider_id: "ollama".to_owned(),
            confirmed_doc_type: None,
            model_text: Some("范文\0正文".to_owned()),
            polish: false,
        };
        assert!(validate_draft_start_request(&control_character_model).is_err());

        assert_eq!(draft_pipeline_file(None, false), "draft.yaml");
        assert_eq!(draft_pipeline_file(Some("   "), false), "draft.yaml");
        assert_eq!(draft_pipeline_file(Some("范文正文"), false), "imitate.yaml");
        assert_eq!(draft_pipeline_file(Some("范文正文"), true), "polish.yaml");
        assert_eq!(draft_pipeline_file(None, true), "polish.yaml");
    }

    #[test]
    fn rerun_journal_reads_and_replaces_stage_snapshots() {
        let mut store = DocumentStore::open_in_memory().expect("store should open");
        let document = ODoc::empty();
        store
            .save_document(&document, Some("initial"))
            .expect("document should save");
        store
            .start_session("session-1", &document.id, "draft", "{}")
            .expect("session should start");
        store
            .record_stage_run(
                "session-1",
                "deai",
                "{\"stage\":\"in\"}",
                Some("{\"v\":1}"),
                Some("ollama"),
                7,
                12,
            )
            .expect("stage should record");
        let state = AppState {
            documents: Mutex::new(store),
            secrets: Box::new(odoc_store::MemorySecretStore::default()),
            crash_reporter: LocalCrashReporter::new("/tmp/bingbi-test.sqlite3", "0.1.0", false),
            kb_sync_generation: tokio::sync::watch::channel(0_u64).0,
            user_pack_root: PathBuf::from("/tmp/user-packs"),
            pack_issues: Vec::new(),
        };
        let journal = StoreDraftJournal {
            state: &state,
            provider_id: "ollama".to_owned(),
        };
        assert_eq!(
            journal
                .stage_input("session-1", "deai")
                .expect("input should load")
                .as_deref(),
            Some("{\"stage\":\"in\"}")
        );
        assert!(
            journal
                .stage_input("session-1", "score")
                .expect("missing input should load")
                .is_none()
        );
        let output = StageOutput {
            stage_id: StageId::Deai,
            status: StageStatus::Completed,
            content: "新内容".to_owned(),
            json: None,
            usage: Some(odoc_llm::TokenUsage {
                prompt_tokens: 1,
                completion_tokens: 2,
                total_tokens: 3,
            }),
            duration_ms: 9,
        };
        journal
            .rerun_record("session-1", "deai", &output)
            .expect("rerun should record");
        let runs = state
            .documents
            .lock()
            .expect("store lock")
            .list_stage_runs("session-1")
            .expect("stage runs should load");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].tokens, 3);
        assert_eq!(runs[0].duration_ms, 9);
        assert_eq!(
            runs[0].output_json.as_deref(),
            Some(
                serde_json::to_string(&output)
                    .expect("output should serialize")
                    .as_str()
            )
        );
        assert_eq!(
            state
                .documents
                .lock()
                .expect("store lock")
                .get_session("session-1")
                .expect("session should load")
                .cost_tokens,
            3
        );
    }

    #[test]
    fn rerun_request_rejects_unknown_or_oversized_stage_ids() {
        let valid = RerunStageRequest {
            session_id: "session-1".to_owned(),
            stage_id: "factcheck".to_owned(),
        };
        assert!(validate_rerun_stage_request(&valid).is_ok());
        let unknown_stage = RerunStageRequest {
            session_id: "session-1".to_owned(),
            stage_id: "Deai".to_owned(),
        };
        assert!(validate_rerun_stage_request(&unknown_stage).is_err());
        let oversized = RerunStageRequest {
            session_id: "session-1".to_owned(),
            stage_id: "x".repeat(65),
        };
        assert!(validate_rerun_stage_request(&oversized).is_err());
        let empty_session = RerunStageRequest {
            session_id: String::new(),
            stage_id: "factcheck".to_owned(),
        };
        assert!(validate_rerun_stage_request(&empty_session).is_err());
    }

    #[test]
    fn user_pack_root_seeds_an_example_pack_only_when_empty() {
        let directory = tempfile::tempdir().expect("temp directory");
        let root = directory.path().join("packs");
        ensure_user_pack_root(&root).expect("example pack should be seeded");
        let manifest = std::fs::read_to_string(root.join("local-corpus/pack.toml"))
            .expect("example manifest should exist");
        assert!(manifest.contains("id = \"local-corpus\""));
        assert!(
            std::fs::read_to_string(root.join("local-corpus/rules/format.yaml"))
                .expect("example rules should exist")
                .contains("本单位自定义规则")
        );

        // The seeded example is a loadable pack with no rules.
        let builtin = directory.path().join("builtin");
        std::fs::create_dir_all(&builtin).expect("builtin root");
        let (registry, issues) =
            PackRegistry::load_with_user_root(&builtin, &root).expect("packs should load");
        assert!(issues.is_empty());
        assert_eq!(
            registry
                .get("local-corpus")
                .expect("example pack should load")
                .manifest()
                .pack
                .id,
            "local-corpus"
        );

        // Existing content is never touched.
        std::fs::write(root.join("keep.txt"), "keep me").expect("marker should be written");
        ensure_user_pack_root(&root).expect("existing root should stay untouched");
        assert!(root.join("keep.txt").is_file());
        assert!(!root.join("another-pack").exists());
    }

    #[test]
    fn review_request_normalizes_blank_focus_and_rejects_oversized_focus() {
        let blank = ReviewStartRequest {
            document_id: "doc".to_owned(),
            provider_id: "ollama".to_owned(),
            focus: Some("   ".to_owned()),
        };
        assert_eq!(
            validate_review_start_request(&blank).expect("blank是允许的"),
            None
        );
        let focused = ReviewStartRequest {
            focus: Some(" 只看格式 ".to_owned()),
            ..blank
        };
        assert_eq!(
            validate_review_start_request(&focused).expect("focus should normalize"),
            Some("只看格式".to_owned())
        );
        let oversized = ReviewStartRequest {
            focus: Some("重".repeat(MAX_REVIEW_FOCUS_BYTES)),
            ..focused
        };
        assert!(validate_review_start_request(&oversized).is_err());
    }
}
