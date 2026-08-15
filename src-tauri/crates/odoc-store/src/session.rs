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

    /// Lists drafting sessions, newest first, optionally restricted to one document.
    ///
    /// Session rows carry the pipeline kind and total token cost, which the workbench shows as
    /// the drafting cost panel. The `limit` must be between 1 and 100.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid document filter or limit, or a `SQLite` read failure.
    pub fn list_sessions(
        &self,
        document_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<SessionSummary>, StoreError> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::InvalidSession(
                "session limit out of range".to_owned(),
            ));
        }
        if let Some(document_id) = document_id {
            validate_identifier(document_id, MAX_SESSION_ID_BYTES, "document ID")?;
        }
        let mut statement = self.connection.prepare(
            "SELECT id, doc_id, pipeline, state_json, cost_tokens, created_at, updated_at
             FROM sessions
             WHERE (?1 IS NULL OR doc_id = ?1)
             ORDER BY updated_at DESC, id DESC
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![document_id, i64::from(limit)], |row| {
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
        })?;
        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }
        Ok(sessions)
    }

    /// Loads the input snapshot of the latest run of one pipeline stage.
    ///
    /// Used by single-stage rerun to reconstruct the exact context the stage originally saw.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid IDs or a `SQLite` read failure.
    pub fn stage_run_input(
        &self,
        session_id: &str,
        stage_id: &str,
    ) -> Result<Option<String>, StoreError> {
        validate_identifier(session_id, MAX_SESSION_ID_BYTES, "session ID")?;
        validate_identifier(stage_id, MAX_STAGE_ID_BYTES, "stage ID")?;
        self.connection
            .query_row(
                "SELECT input_json FROM stage_runs
                 WHERE session_id = ?1 AND stage_id = ?2
                 ORDER BY id DESC LIMIT 1",
                params![session_id, stage_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::from)
    }

    /// Replaces the output of the latest run of one stage with a re-run result,
    /// keeping the session token total consistent with the new usage.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid metadata, a missing session or stage run, values
    /// that overflow, or a `SQLite` write failure.
    pub fn replace_stage_run(
        &self,
        session_id: &str,
        stage_id: &str,
        output_json: &str,
        tokens: u64,
        duration_ms: u64,
    ) -> Result<(), StoreError> {
        validate_identifier(session_id, MAX_SESSION_ID_BYTES, "session ID")?;
        validate_identifier(stage_id, MAX_STAGE_ID_BYTES, "stage ID")?;
        validate_json_text(output_json)?;
        let tokens = i64::try_from(tokens)
            .map_err(|_| StoreError::InvalidSession("token count overflow".to_owned()))?;
        let duration_ms = i64::try_from(duration_ms)
            .map_err(|_| StoreError::InvalidSession("duration overflow".to_owned()))?;
        let now = unix_millis()?;
        let transaction = self.connection.unchecked_transaction()?;
        let target: Option<(i64, i64)> = transaction
            .query_row(
                "SELECT id, tokens FROM stage_runs
                 WHERE session_id = ?1 AND stage_id = ?2
                 ORDER BY id DESC LIMIT 1",
                params![session_id, stage_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((run_id, previous_tokens)) = target else {
            return Err(StoreError::SessionNotFound(session_id.to_owned()));
        };
        transaction.execute(
            "UPDATE stage_runs SET output_json = ?1, tokens = ?2, duration_ms = ?3, created_at = ?4
             WHERE id = ?5",
            params![output_json, tokens, duration_ms, now, run_id],
        )?;
        transaction.execute(
            "UPDATE sessions
             SET cost_tokens = cost_tokens - ?1 + ?2, updated_at = ?3
             WHERE id = ?4",
            params![previous_tokens, tokens, now, session_id],
        )?;
        transaction.commit()?;
        Ok(())
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

    #[test]
    fn list_sessions_orders_newest_first_and_filters_by_document() {
        let mut store = DocumentStore::open_in_memory().expect("store should open");
        let first = ODoc::empty();
        let mut second = ODoc::empty();
        second.body.title.text = "另一篇".to_owned();
        store
            .save_document(&first, Some("initial"))
            .expect("first document should save");
        store
            .save_document(&second, Some("initial"))
            .expect("second document should save");
        store
            .start_session("session-a", &first.id, "draft", "{}")
            .expect("session a should start");
        store
            .start_session("session-b", &first.id, "imitate", "{}")
            .expect("session b should start");
        store
            .start_session("session-c", &second.id, "draft", "{}")
            .expect("session c should start");
        store
            .record_stage_run(
                "session-a",
                "draft",
                "{}",
                Some("{}"),
                Some("ollama"),
                11,
                4,
            )
            .expect("stage should record");
        store
            .record_stage_run(
                "session-b",
                "draft",
                "{}",
                Some("{}"),
                Some("ollama"),
                22,
                4,
            )
            .expect("stage should record");
        store
            .record_stage_run(
                "session-c",
                "draft",
                "{}",
                Some("{}"),
                Some("ollama"),
                33,
                4,
            )
            .expect("stage should record");
        // The inserts all land in the same millisecond; age them apart to exercise the ordering.
        for (session, offset) in [
            ("session-b", 3_000_i64),
            ("session-c", 2_000),
            ("session-a", 1_000),
        ] {
            store
                .connection
                .execute(
                    "UPDATE sessions SET updated_at = updated_at + ?1 WHERE id = ?2",
                    params![offset, session],
                )
                .expect("session age should update");
        }

        let all = store
            .list_sessions(None, 100)
            .expect("sessions should load");
        assert_eq!(all.len(), 3);
        // session-b recorded last, so it is newest.
        assert_eq!(all[0].id, "session-b");
        assert_eq!(all[0].pipeline, "imitate");
        assert_eq!(all[0].cost_tokens, 22);
        assert_eq!(all[1].id, "session-c");
        assert_eq!(all[2].id, "session-a");

        let filtered = store
            .list_sessions(Some(&first.id), 100)
            .expect("filtered sessions should load");
        assert_eq!(
            filtered
                .iter()
                .map(|session| session.id.as_str())
                .collect::<Vec<_>>(),
            ["session-b", "session-a"]
        );

        let limited = store
            .list_sessions(None, 2)
            .expect("limited sessions should load");
        assert_eq!(limited.len(), 2);
        assert!(matches!(
            store.list_sessions(None, 0),
            Err(StoreError::InvalidSession(_))
        ));
        assert!(matches!(
            store.list_sessions(Some(""), 10),
            Err(StoreError::InvalidSession(_))
        ));
    }

    #[test]
    fn stage_run_input_and_replace_keep_the_latest_run_consistent() {
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

        let input = store
            .stage_run_input("session-1", "deai")
            .expect("stage input should load");
        assert_eq!(input.as_deref(), Some("{\"stage\":\"in\"}"));
        assert!(
            store
                .stage_run_input("session-1", "score")
                .expect("missing stage input should load")
                .is_none()
        );

        store
            .replace_stage_run("session-1", "deai", "{\"v\":2}", 9, 20)
            .expect("stage should replace");
        let summary = store.get_session("session-1").expect("session should load");
        assert_eq!(summary.cost_tokens, 9);
        let runs = store
            .list_stage_runs("session-1")
            .expect("stage runs should load");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].tokens, 9);
        assert_eq!(runs[0].output_json.as_deref(), Some("{\"v\":2}"));

        assert!(matches!(
            store.replace_stage_run("session-1", "facts", "{}", 1, 1),
            Err(StoreError::SessionNotFound(_))
        ));
    }
}
