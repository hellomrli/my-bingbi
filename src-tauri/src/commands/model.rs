//! Provider connectivity, model listing, and chat commands.

use crate::error::{IpcError, map_llm_error, map_store_error};
use crate::state::AppState;
use crate::util::{lock_documents, provider_client};
use odoc_llm::{
    ChatDelta, ChatMessage, ChatRequest, ChatResponse, ChatRole, ConnectionTestResult, LlmError,
    LlmProvider,
};
use odoc_model::SecurityLevel;
use std::time::Instant;
use tauri::State;
use tauri::ipc::Channel;

#[tauri::command]
pub(crate) async fn test_provider_connection(
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
pub(crate) async fn list_provider_models(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<Vec<String>, IpcError> {
    provider_client(&state, &provider_id)?
        .list_models()
        .await
        .map_err(map_llm_error)
}

#[tauri::command]
pub(crate) async fn chat_with_provider(
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
pub(crate) async fn chat_stream_with_provider(
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
        // This is an OUTPUT cap, not the model's context window: reasoning models can spend
        // the whole completion budget on hidden thinking and return an empty message, which
        // Cline's gateway turns into HTTP 500. The probe stays bounded but leaves generous
        // room above the reasoning peaks observed across Cline Pass models.
        max_tokens: Some(512),
        json_mode: false,
        security_level: SecurityLevel::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use odoc_model::ODoc;
    use odoc_store::{DocumentStore, LocalCrashReporter};
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[test]
    fn connection_test_is_bounded_and_non_sensitive() {
        let request = connection_test_request();
        assert_eq!(request.security_level, SecurityLevel::Internal);
        assert_eq!(request.max_tokens, Some(512));
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
}
