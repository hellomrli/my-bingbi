//! Draft, rerun, and review pipeline commands.

use crate::commands::kb::optional_kb_provider;
use crate::dto::{DraftStartRequest, RerunStageRequest, ReviewStartRequest};
use crate::error::{IpcError, map_engine_error, map_store_error, map_tauri_error};
use crate::state::AppState;
use crate::util::{
    MAX_DRAFT_MATERIAL_BYTES, builtin_pipeline_path, lock_documents, provider_client,
};
use odoc_engine::{
    DraftEngine, DraftEvent, DraftInput, DraftJournal, DraftOutcome, EngineError,
    PipelineDefinition, RerunOutcome, ReviewEngine, ReviewEvent, ReviewInput, ReviewOutcome,
    StageOutput,
};
use odoc_llm::LlmProvider;
use odoc_model::ODoc;
use odoc_pack::{PackRegistry, StageId};
use tauri::State;
use tauri::ipc::Channel;

const MAX_REVIEW_FOCUS_BYTES: usize = 512;

#[tauri::command]
pub(crate) async fn draft_start<R: tauri::Runtime>(
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
        template_id: request.template_id,
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
/// The provider and pipeline come from the session record rather than from the request, so a
/// rerun reproduces the model configuration the stage originally ran with. The knowledge base
/// and knowledge packs, however, are read from the current settings: rerunning the retrieve
/// stage after the user switched vaults queries the new vault. Only draft sessions support
/// single-stage rerun; a review must be re-run as a whole because the kernel cannot rebuild a
/// review-only pipeline stage here.
#[tauri::command]
pub(crate) async fn rerun_stage<R: tauri::Runtime>(
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
pub(crate) async fn review_start<R: tauri::Runtime>(
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

#[cfg(test)]
mod tests {
    use super::*;
    use odoc_engine::StageStatus;
    use odoc_store::{DocumentStore, LocalCrashReporter};
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[test]
    fn draft_request_rejects_empty_and_oversized_material() {
        let empty = DraftStartRequest {
            document_id: "doc".to_owned(),
            material: " \n".to_owned(),
            provider_id: "ollama".to_owned(),
            confirmed_doc_type: None,
            model_text: None,
            template_id: None,
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
    fn draft_request_validates_model_text_and_selects_the_imitate_pipeline() {
        let base = DraftStartRequest {
            document_id: "doc".to_owned(),
            material: "申请事项".to_owned(),
            provider_id: "ollama".to_owned(),
            confirmed_doc_type: None,
            model_text: None,
            template_id: None,
            polish: false,
        };
        assert!(validate_draft_start_request(&base).is_ok());
        let imitating = DraftStartRequest {
            document_id: "doc".to_owned(),
            material: "申请事项".to_owned(),
            provider_id: "ollama".to_owned(),
            confirmed_doc_type: None,
            model_text: Some("范文正文".to_owned()),
            template_id: None,
            polish: false,
        };
        assert!(validate_draft_start_request(&imitating).is_ok());
        let oversized_model = DraftStartRequest {
            document_id: "doc".to_owned(),
            material: "申请事项".to_owned(),
            provider_id: "ollama".to_owned(),
            confirmed_doc_type: None,
            model_text: Some("x".repeat(2 * 1024 * 1024 + 1)),
            template_id: None,
            polish: false,
        };
        assert!(validate_draft_start_request(&oversized_model).is_err());
        let control_character_model = DraftStartRequest {
            document_id: "doc".to_owned(),
            material: "申请事项".to_owned(),
            provider_id: "ollama".to_owned(),
            confirmed_doc_type: None,
            model_text: Some("范文\0正文".to_owned()),
            template_id: None,
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
