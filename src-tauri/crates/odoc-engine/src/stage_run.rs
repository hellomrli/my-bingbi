use crate::pipeline::{DraftJournal, elapsed_ms};
use crate::{EngineError, StageOutput, StageStatus};
use odoc_pack::StageId;
use std::time::Instant;

/// Dropping a command (for example, when its `WebView` disappears) must end its durable stage.
pub(crate) struct StageRun<'a, J: DraftJournal> {
    journal: &'a J,
    session_id: &'a str,
    input_json: &'a str,
    stage_id: StageId,
    started: Instant,
    rerun: bool,
    finished: bool,
}

impl<'a, J: DraftJournal> StageRun<'a, J> {
    pub(crate) fn start(
        journal: &'a J,
        session_id: &'a str,
        input_json: &'a str,
        stage_id: StageId,
        rerun: bool,
    ) -> Result<Self, EngineError> {
        journal.begin_stage(session_id, input_json, stage_id)?;
        Ok(Self {
            journal,
            session_id,
            input_json,
            stage_id,
            started: Instant::now(),
            rerun,
            finished: false,
        })
    }

    pub(crate) fn finish(&mut self, output: &mut StageOutput) -> Result<(), EngineError> {
        output.duration_ms = elapsed_ms(self.started);
        if self.rerun {
            self.journal
                .rerun_record(self.session_id, self.stage_id.as_str(), output)?;
        } else {
            self.journal
                .record(self.session_id, self.input_json, output)?;
        }
        self.finished = true;
        Ok(())
    }
}

impl<J: DraftJournal> Drop for StageRun<'_, J> {
    fn drop(&mut self) {
        if !self.finished {
            let mut output = failure_output(self.stage_id, &EngineError::Cancelled);
            let _ = self.finish(&mut output);
        }
    }
}

pub(crate) fn failure_output(stage_id: StageId, error: &EngineError) -> StageOutput {
    StageOutput {
        stage_id,
        status: if matches!(
            error.code(),
            "draft_cancelled" | "llm_cancelled" | "document_type_confirmation_required"
        ) {
            StageStatus::Cancelled
        } else {
            StageStatus::Failed
        },
        content: error.user_message().to_owned(),
        json: Some(serde_json::json!({ "code": error.code() }).to_string()),
        usage: None,
        duration_ms: 0,
    }
}
