//! Knowledge-base search, indexing, archiving, and subscription sync commands.

use crate::dto::{ArchiveResult, KbIndexProgress};
use crate::error::{IpcError, map_kb_error, map_secret_error, map_store_error};
use crate::state::AppState;
use crate::util::{
    KB_FNS_TOKEN_ACCOUNT, KB_FNS_WS_TOKEN_ACCOUNT, KB_MCP_TOKEN_ACCOUNT, lock_documents,
};
use odoc_kb::{
    ChangeKind, FnsPendingChange, FnsProvider, FnsWatchConfig, FnsWatchEvent, KbConfig, KbError,
    KbHit, KbNote, KbProvider, KbProviderKind, KbQuery, LocalVaultProvider, McpProvider,
    hybrid_search, suggest_missing_frontmatter, watch_fns_notes,
};
use odoc_model::{ODoc, SecurityLevel};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::ipc::Channel;
use tauri::{Manager, State};

/// Mirrors `odoc_kb::types::MAX_NOTE_BYTES`, which is not re-exported.
const MAX_CREATED_NOTE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreatedKbNote {
    pub(crate) path: String,
    pub(crate) title: String,
}

#[tauri::command]
pub(crate) async fn kb_search(
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
pub(crate) async fn kb_get(
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
pub(crate) fn kb_index_state(
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
pub(crate) async fn kb_rebuild_index(
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
pub(crate) fn kb_clear_index(
    state: State<'_, AppState>,
    vault: Option<String>,
) -> Result<(), IpcError> {
    let config = lock_documents(&state)?
        .kb_config()
        .map_err(map_store_error)?;
    let vault = vault.unwrap_or(config.vault);
    lock_documents(&state)?
        .clear_kb_index(&vault)
        .map_err(map_store_error)
}

/// Creates a local Markdown vault inside the app config directory (or at a caller-supplied path).
///
/// The folders are intentionally boring: `收件箱` receives notes authored in the app, while
/// `政策` and `公文` are for material the user brings from elsewhere. Existing files are never
/// overwritten.
#[tauri::command]
pub(crate) fn kb_create_local_vault(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<String, IpcError> {
    let target = match path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => PathBuf::from(value),
        None => state.user_pack_root.parent().map_or_else(
            || PathBuf::from("knowledge-base"),
            |parent| parent.join("knowledge-base"),
        ),
    };
    if !target.is_absolute() {
        return Err(IpcError {
            code: "invalid_input",
            message: "知识库目录必须是绝对路径。",
        });
    }
    prepare_local_vault(&target)?;
    Ok(target.to_string_lossy().into_owned())
}

fn prepare_local_vault(target: &Path) -> Result<(), IpcError> {
    for directory in ["", "收件箱", "政策", "公文"] {
        let directory = if directory.is_empty() {
            target.to_path_buf()
        } else {
            target.join(directory)
        };
        fs::create_dir_all(&directory).map_err(|_| IpcError {
            code: "kb_vault_create_failed",
            message: "无法创建知识库目录，请检查路径与权限。",
        })?;
    }
    let readme = target.join("README.md");
    if !readme.exists() {
        fs::write(readme, LOCAL_VAULT_README).map_err(|_| IpcError {
            code: "kb_vault_create_failed",
            message: "知识库目录已创建，但写入说明文件失败。",
        })?;
    }
    Ok(())
}

/// Writes one Markdown note into the configured knowledge base and returns its vault-relative path.
#[tauri::command]
pub(crate) async fn kb_create_note(
    state: State<'_, AppState>,
    document_id: Option<String>,
    title: String,
    content: String,
    doc_type: Option<String>,
) -> Result<CreatedKbNote, IpcError> {
    enforce_kb_security(&state, document_id.as_deref())?;
    let title = title.trim().to_owned();
    let slug = sanitize_note_title(&title)?;
    let content = content.trim();
    if content.is_empty() {
        return Err(IpcError {
            code: "invalid_input",
            message: "资料内容不能为空。",
        });
    }
    if content.len() > MAX_CREATED_NOTE_BYTES {
        return Err(IpcError {
            code: "invalid_input",
            message: "资料内容超过 4 MiB 上限。",
        });
    }
    let (provider, _config) = configured_kb_provider(&state)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = format!("收件箱/{stamp}-{slug}.md");
    let markdown = render_note_markdown(&title, doc_type.as_deref(), content);
    provider
        .write(&path, &markdown)
        .await
        .map_err(map_kb_error)?;
    Ok(CreatedKbNote { path, title })
}

fn sanitize_note_title(value: &str) -> Result<String, IpcError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(IpcError {
            code: "invalid_input",
            message: "资料标题不能为空。",
        });
    }
    if trimmed.chars().count() > 120 {
        return Err(IpcError {
            code: "invalid_input",
            message: "资料标题不能超过 120 个字符。",
        });
    }
    if trimmed.chars().any(char::is_control) {
        return Err(IpcError {
            code: "invalid_input",
            message: "资料标题不能包含控制字符。",
        });
    }
    let mut cleaned = String::with_capacity(trimmed.len());
    for character in trimmed.chars() {
        if matches!(
            character,
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
        ) {
            cleaned.push('-');
        } else if matches!(character, '\n' | '\r' | '\t') {
            cleaned.push(' ');
        } else {
            cleaned.push(character);
        }
    }
    let cleaned = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|character| character == ' ' || character == '.')
        .to_owned();
    if cleaned.is_empty() {
        return Err(IpcError {
            code: "invalid_input",
            message: "资料标题不能只包含路径保留字符。",
        });
    }
    Ok(cleaned)
}

fn render_note_markdown(title: &str, doc_type: Option<&str>, content: &str) -> String {
    let mut markdown = String::from("---\n");
    markdown.push_str(&format!("title: \"{}\"\n", escape_yaml(title)));
    if let Some(doc_type) = doc_type.filter(|value| valid_doc_type(value)) {
        markdown.push_str(&format!("doc_type: {doc_type}\n"));
    }
    markdown.push_str("source: 秉笔\n---\n\n");
    markdown.push_str(content);
    markdown.push('\n');
    markdown
}

fn escape_yaml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn valid_doc_type(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

const LOCAL_VAULT_README: &str = "---\ntitle: 知识库说明\n---\n# 秉笔本地知识库\n\n\
这个目录可以直接用 Obsidian 打开，也可以只作为普通 Markdown 文件夹使用。\n\n\
- `收件箱/`：在秉笔「知识库」设置里新建的资料。\n\
- `政策/`：政策文件、法规、上级来文。\n\
- `公文/`：历史公文、讲话稿、总结等写作素材。\n\n\
在秉笔设置中将接入方式选为「本地 Vault」，Vault 目录指向本目录，然后重建索引即可检索和引用。\n\
资料原文件始终留在本机，不会上传。\n";

#[tauri::command]
pub(crate) async fn kb_sync_once(
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
pub(crate) async fn archive_document_to_kb(
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

pub(crate) fn wake_kb_sync(state: &AppState) {
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

pub(crate) async fn kb_sync_supervisor<R: tauri::Runtime>(
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
        KbProviderKind::LocalVault => {
            if !Path::new(&config.local_path).is_dir() {
                return Err(IpcError {
                    code: "kb_path_missing",
                    message: "Vault 目录不存在，请先点「创建本地知识库」，或填写已有目录。",
                });
            }
            Box::new(
                LocalVaultProvider::open(&config.local_path, config.vault.clone())
                    .map_err(map_kb_error)?,
            )
        }
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

pub(crate) fn optional_kb_provider(
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

#[cfg(test)]
mod note_tests {
    use super::{prepare_local_vault, render_note_markdown, sanitize_note_title};

    #[test]
    fn note_titles_become_safe_file_names() {
        assert_eq!(
            sanitize_note_title("  设备/采购:计划  ").expect("title should sanitize"),
            "设备-采购-计划"
        );
        assert_eq!(
            sanitize_note_title("  防汛   会议  ").expect("title should sanitize"),
            "防汛 会议"
        );
        assert!(sanitize_note_title("   ").is_err());
        assert!(sanitize_note_title("...").is_err());
        assert!(sanitize_note_title(r#"a/b\c:d*e?f"g<h>i|j"#).is_ok());
    }

    #[test]
    fn rendered_notes_have_frontmatter_and_an_escaped_title() {
        let markdown =
            render_note_markdown(r#"关于"设备"的请示"#, Some("request"), "拟购置设备 3 台。");
        assert!(markdown.starts_with("---\ntitle: \"关于\\\"设备\\\"的请示\"\n"));
        assert!(markdown.contains("doc_type: request"));
        assert!(markdown.contains("source: 秉笔"));
        assert!(markdown.contains("拟购置设备 3 台。"));
        assert!(markdown.ends_with('\n'));
    }

    #[test]
    fn local_vault_creation_is_idempotent() {
        let root = tempfile::tempdir().expect("temp dir");
        let target = root.path().join("vault");
        prepare_local_vault(&target).expect("vault should create");
        for entry in ["收件箱", "政策", "公文", "README.md"] {
            assert!(target.join(entry).exists(), "{entry} should exist");
        }
        let readme = target.join("README.md");
        std::fs::write(&readme, "custom").expect("readme should be writable");
        prepare_local_vault(&target).expect("vault should repair");
        assert_eq!(
            std::fs::read_to_string(readme).expect("readme should read"),
            "custom"
        );
    }

    #[test]
    fn rendered_notes_parse_back_with_title_and_content() {
        let markdown = render_note_markdown("防汛工作要点", None, "汛前完成隐患排查。");
        let note = odoc_kb::parse_markdown_note("本地资料库", "收件箱/x.md", &markdown)
            .expect("note should parse");
        assert_eq!(note.title, "防汛工作要点");
        assert!(note.content.contains("隐患排查"));
    }

    #[test]
    fn invalid_doc_types_are_omitted_from_frontmatter() {
        let markdown = render_note_markdown("资料", Some("Request Type"), "正文");
        assert!(!markdown.contains("doc_type:"));
    }
}
