use crate::config::unix_millis;
use crate::{DocumentStore, StoreError};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

const MAX_SESSION_ID_BYTES: usize = 128;
const MAX_PIPELINE_BYTES: usize = 256;
const MAX_STAGE_ID_BYTES: usize = 64;
const MAX_STATE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub document_id: String,
    pub pipeline: String,
    pub state_json: String,
    #[ts(type = "number")]
    pub cost_tokens: u64,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct StageRunRecord {
    #[ts(type = "number")]
    pub id: u64,
    pub session_id: String,
    pub stage_id: String,
    pub input_json: String,
    pub output_json: Option<String>,
    pub provider: Option<String>,
    #[ts(type = "number")]
    pub tokens: u64,
    #[ts(type = "number")]
    pub duration_ms: u64,
    #[ts(type = "number")]
    pub created_at: i64,
}

impl DocumentStore {
    /// Starts a durable drafting session for an already-saved document.
    ///
    /// # Errors
    ///
    /// Returns an error when IDs, pipeline metadata or state are invalid, the document is not
    /// saved, or `SQLite` cannot write the session.
    pub fn start_session(
        &self,
        session_id: &str,
        document_id: &str,
        pipeline: &str,
        state_json: &str,
    ) -> Result<(), StoreError> {
        validate_identifier(session_id, MAX_SESSION_ID_BYTES, "session ID")?;
        validate_identifier(document_id, 256, "document ID")?;
        validate_identifier(pipeline, MAX_PIPELINE_BYTES, "pipeline")?;
        validate_json_text(state_json)?;
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM documents WHERE id = ?1)",
            params![document_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(StoreError::DocumentNotFound(document_id.to_owned()));
        }
        let now = unix_millis()?;
        self.connection.execute(
            "INSERT INTO sessions(
                id, doc_id, pipeline, state_json, cost_tokens, cost_money, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, 0, 0, ?5, ?5)",
            params![session_id, document_id, pipeline, state_json, now],
        )?;
        Ok(())
    }

    /// Records one completed pipeline stage and updates the session token total.
    ///
    /// # Errors
    ///
    /// Returns an error when stage metadata is invalid, the session is missing, values overflow,
    /// or `SQLite` cannot write the record.
    #[allow(clippy::too_many_arguments)]
    pub fn record_stage_run(
        &self,
        session_id: &str,
        stage_id: &str,
        input_json: &str,
        output_json: Option<&str>,
        provider: Option<&str>,
        tokens: u64,
        duration_ms: u64,
    ) -> Result<(), StoreError> {
        validate_identifier(session_id, MAX_SESSION_ID_BYTES, "session ID")?;
        validate_identifier(stage_id, MAX_STAGE_ID_BYTES, "stage ID")?;
        validate_json_text(input_json)?;
        if let Some(output) = output_json {
            validate_json_text(output)?;
        }
        if let Some(provider) = provider {
            validate_identifier(provider, 128, "provider")?;
        }
        let tokens = i64::try_from(tokens)
            .map_err(|_| StoreError::InvalidSession("token count overflow".to_owned()))?;
        let duration_ms = i64::try_from(duration_ms)
            .map_err(|_| StoreError::InvalidSession("duration overflow".to_owned()))?;
        let now = unix_millis()?;
        let transaction = self.connection.unchecked_transaction()?;
        let session_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1)",
            params![session_id],
            |row| row.get(0),
        )?;
        if !session_exists {
            return Err(StoreError::SessionNotFound(session_id.to_owned()));
        }
        transaction.execute(
            "INSERT INTO stage_runs(
                session_id, stage_id, input_json, output_json, provider, tokens, duration_ms, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                session_id,
                stage_id,
                input_json,
                output_json,
                provider,
                tokens,
                duration_ms,
                now
            ],
        )?;
        transaction.execute(
            "UPDATE sessions
             SET cost_tokens = cost_tokens + ?1, updated_at = ?2
             WHERE id = ?3",
            params![tokens, now, session_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Marks a session complete and stores its final serialized state.
    ///
    /// # Errors
    ///
    /// Returns an error when the session ID/state is invalid, the session is missing, or `SQLite`
    /// cannot update it.
    pub fn complete_session(&self, session_id: &str, state_json: &str) -> Result<(), StoreError> {
        validate_identifier(session_id, MAX_SESSION_ID_BYTES, "session ID")?;
        validate_json_text(state_json)?;
        let changed = self.connection.execute(
            "UPDATE sessions SET state_json = ?1, updated_at = ?2 WHERE id = ?3",
            params![state_json, unix_millis()?, session_id],
        )?;
        if changed == 0 {
            return Err(StoreError::SessionNotFound(session_id.to_owned()));
        }
        Ok(())
    }

    /// Loads a session summary for diagnostics and recovery UI.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid IDs, a missing session, or a `SQLite` read failure.
    pub fn get_session(&self, session_id: &str) -> Result<SessionSummary, StoreError> {
        validate_identifier(session_id, MAX_SESSION_ID_BYTES, "session ID")?;
        self.connection
            .query_row(
                "SELECT id, doc_id, pipeline, state_json, cost_tokens, created_at, updated_at
                 FROM sessions WHERE id = ?1",
                params![session_id],
                |row| {
                    Ok(SessionSummary {
                        id: row.get(0)?,
                        document_id: row.get(1)?,
                        pipeline: row.get(2)?,
                        state_json: row.get(3)?,
                        cost_tokens: row.get::<_, i64>(4)?.try_into().map_err(|_| {
                            rusqlite::Error::FromSqlConversionFailure(
                                4,
                                rusqlite::types::Type::Integer,
                                Box::new(std::io::Error::other("negative token count")),
                            )
                        })?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::SessionNotFound(session_id.to_owned()))
    }

    /// Lists completed stage runs for a drafting session in execution order.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or missing session ID or a `SQLite` read failure.
    pub fn list_stage_runs(&self, session_id: &str) -> Result<Vec<StageRunRecord>, StoreError> {
        validate_identifier(session_id, MAX_SESSION_ID_BYTES, "session ID")?;
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1)",
            params![session_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(StoreError::SessionNotFound(session_id.to_owned()));
        }
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, stage_id, input_json, output_json, provider,
                    tokens, duration_ms, created_at
             FROM stage_runs WHERE session_id = ?1 ORDER BY id ASC",
        )?;
        let rows = statement.query_map(params![session_id], |row| {
            Ok(StageRunRecord {
                id: non_negative_integer(row, 0)?,
                session_id: row.get(1)?,
                stage_id: row.get(2)?,
                input_json: row.get(3)?,
                output_json: row.get(4)?,
                provider: row.get(5)?,
                tokens: non_negative_integer(row, 6)?,
                duration_ms: non_negative_integer(row, 7)?,
                created_at: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }
}

fn non_negative_integer(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    row.get::<_, i64>(column)?.try_into().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::other("negative unsigned value")),
        )
    })
}

fn validate_identifier(value: &str, max_bytes: usize, field: &str) -> Result<(), StoreError> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        Err(StoreError::InvalidSession(format!("{field} is invalid")))
    } else {
        Ok(())
    }
}

fn validate_json_text(value: &str) -> Result<(), StoreError> {
    if value.trim().is_empty() || value.len() > MAX_STATE_BYTES {
        return Err(StoreError::InvalidSession(
            "session JSON is invalid".to_owned(),
        ));
    }
    serde_json::from_str::<serde_json::Value>(value)
        .map(|_| ())
        .map_err(|_| StoreError::InvalidSession("session JSON is invalid".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use odoc_model::ODoc;

    #[test]
    fn session_and_stage_records_round_trip() {
        let mut store = DocumentStore::open_in_memory().expect("store should open");
        let document = ODoc::empty();
        store
            .save_document(&document, Some("initial"))
            .expect("document should save");
        store
            .start_session(
                "session-1",
                &document.id,
                "draft",
                "{\"status\":\"running\"}",
            )
            .expect("session should start");
        store
            .record_stage_run(
                "session-1",
                "doctype",
                "{}",
                Some("{\"ok\":true}"),
                Some("ollama"),
                7,
                12,
            )
            .expect("stage should record");
        store
            .complete_session("session-1", "{\"status\":\"completed\"}")
            .expect("session should complete");
        let summary = store.get_session("session-1").expect("session should load");
        assert_eq!(summary.cost_tokens, 7);
        assert_eq!(summary.state_json, "{\"status\":\"completed\"}");
        let runs = store
            .list_stage_runs("session-1")
            .expect("stage runs should load");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].stage_id, "doctype");
        assert_eq!(runs[0].provider.as_deref(), Some("ollama"));
        assert_eq!(runs[0].tokens, 7);
    }

    #[test]
    fn session_requires_saved_document_and_rejects_bad_json() {
        let store = DocumentStore::open_in_memory().expect("store should open");
        assert!(matches!(
            store.start_session("session-1", "missing", "draft", "{}"),
            Err(StoreError::DocumentNotFound(_))
        ));
        assert!(matches!(
            store.start_session("session-1", "missing", "draft", "\0"),
            Err(StoreError::InvalidSession(_))
        ));
    }
}
