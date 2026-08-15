use crate::config::unix_millis;
use crate::{DocumentStore, StoreError};
use odoc_model::{DocumentStatus, DocumentType, ODoc};
use rusqlite::types::Type;
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use ts_rs::TS;

const MAX_DOCUMENT_ID_BYTES: usize = 256;
const MAX_LIST_LIMIT: u32 = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct SaveDocumentResult {
    pub document_id: String,
    pub version_seq: u32,
    pub created: bool,
    pub version_created: bool,
    #[ts(type = "number")]
    pub saved_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct DocumentSummary {
    pub id: String,
    pub title: String,
    pub doc_type: DocumentType,
    pub revision: u32,
    pub status: DocumentStatus,
    pub version_count: u32,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct DocumentVersionSummary {
    pub seq: u32,
    pub odoc_revision: u32,
    pub stage: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
}

impl DocumentStore {
    /// Saves the current document and appends an immutable version when its JSON changed.
    ///
    /// # Errors
    ///
    /// Returns an error when the document is invalid, serialization fails, the clock is invalid,
    /// or any statement in the save transaction fails.
    pub fn save_document(
        &mut self,
        document: &ODoc,
        stage: Option<&str>,
    ) -> Result<SaveDocumentResult, StoreError> {
        validate_document(document)?;
        let json = serde_json::to_string(document)?;
        let now = unix_millis()?;
        let transaction = self.connection.transaction()?;
        let previous: Option<(String, u32)> = transaction
            .query_row(
                "SELECT odoc_json, revision FROM documents WHERE id = ?1",
                params![document.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let created = previous.is_none();
        if let Some((previous_json, stored_revision)) = &previous
            && document.revision < *stored_revision
            && previous_json != &json
        {
            return Err(StoreError::StaleRevision {
                id: document.id.clone(),
                incoming: document.revision,
                stored: *stored_revision,
            });
        }
        let version_created = previous
            .as_ref()
            .is_none_or(|(previous_json, _)| previous_json != &json);
        let title = display_title(document);
        let doc_type = enum_text(document.meta.doc_type)?;
        let status = enum_text(document.meta.status)?;

        transaction.execute(
            "INSERT INTO documents(
                id, title, doc_type, odoc_json, revision, status, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                doc_type = excluded.doc_type,
                odoc_json = excluded.odoc_json,
                revision = excluded.revision,
                status = excluded.status,
                updated_at = excluded.updated_at",
            params![
                document.id,
                title,
                doc_type,
                json,
                document.revision,
                status,
                now
            ],
        )?;

        let current_seq = latest_version_seq(&transaction, &document.id)?;
        let version_seq = if version_created {
            let next_seq = current_seq.checked_add(1).ok_or_else(|| {
                StoreError::InvalidDocument("version sequence overflow".to_owned())
            })?;
            transaction.execute(
                "INSERT INTO doc_versions(
                    doc_id, seq, odoc_json, odoc_revision, stage, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    document.id,
                    next_seq,
                    json,
                    document.revision,
                    normalize_stage(stage),
                    now
                ],
            )?;
            next_seq
        } else {
            current_seq
        };
        transaction.commit()?;

        Ok(SaveDocumentResult {
            document_id: document.id.clone(),
            version_seq,
            created,
            version_created,
            saved_at: now,
        })
    }

    /// Loads the current snapshot of a document.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid ID, missing document, `SQLite` failure, or invalid `ODoc`
    /// JSON.
    pub fn get_document(&self, id: &str) -> Result<ODoc, StoreError> {
        validate_document_id(id)?;
        let json: Option<String> = self
            .connection
            .query_row(
                "SELECT odoc_json FROM documents WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let json = json.ok_or_else(|| StoreError::DocumentNotFound(id.to_owned()))?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Loads one immutable saved version of a document.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid ID, missing version, `SQLite` failure, or invalid `ODoc`
    /// JSON.
    pub fn get_document_version(&self, id: &str, seq: u32) -> Result<ODoc, StoreError> {
        validate_document_id(id)?;
        if seq == 0 {
            return Err(StoreError::VersionNotFound {
                doc_id: id.to_owned(),
                seq,
            });
        }
        let json: Option<String> = self
            .connection
            .query_row(
                "SELECT odoc_json FROM doc_versions WHERE doc_id = ?1 AND seq = ?2",
                params![id, seq],
                |row| row.get(0),
            )
            .optional()?;
        let json = json.ok_or_else(|| StoreError::VersionNotFound {
            doc_id: id.to_owned(),
            seq,
        })?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Lists recently updated documents, capped at 200 entries.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` cannot execute the query or stored enum values are invalid.
    pub fn list_documents(&self, limit: u32) -> Result<Vec<DocumentSummary>, StoreError> {
        let limit = limit.clamp(1, MAX_LIST_LIMIT);
        let mut statement = self.connection.prepare(
            "SELECT
                d.id, d.title, d.doc_type, d.revision, d.status,
                COUNT(v.id), d.created_at, d.updated_at
             FROM documents d
             LEFT JOIN doc_versions v ON v.doc_id = d.id
             GROUP BY d.id
             ORDER BY d.updated_at DESC, d.id ASC
             LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], |row| {
            let doc_type_text: String = row.get(2)?;
            let status_text: String = row.get(4)?;
            Ok(DocumentSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                doc_type: parse_enum(&doc_type_text, 2)?,
                revision: row.get(3)?,
                status: parse_enum(&status_text, 4)?,
                version_count: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Lists all immutable versions of a document in newest-first order.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or missing document ID or a `SQLite` query failure.
    pub fn list_document_versions(
        &self,
        id: &str,
    ) -> Result<Vec<DocumentVersionSummary>, StoreError> {
        validate_document_id(id)?;
        if !self.document_exists(id)? {
            return Err(StoreError::DocumentNotFound(id.to_owned()));
        }
        let mut statement = self.connection.prepare(
            "SELECT seq, odoc_revision, stage, created_at
             FROM doc_versions WHERE doc_id = ?1 ORDER BY seq DESC",
        )?;
        let rows = statement.query_map(params![id], |row| {
            Ok(DocumentVersionSummary {
                seq: row.get(0)?,
                odoc_revision: row.get(1)?,
                stage: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Deletes a document and its dependent versions, sessions, and findings.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid document ID or a `SQLite` write failure.
    pub fn delete_document(&self, id: &str) -> Result<bool, StoreError> {
        validate_document_id(id)?;
        Ok(self
            .connection
            .execute("DELETE FROM documents WHERE id = ?1", params![id])?
            > 0)
    }

    fn document_exists(&self, id: &str) -> Result<bool, StoreError> {
        let count: u32 = self.connection.query_row(
            "SELECT COUNT(*) FROM documents WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

fn latest_version_seq(transaction: &Transaction<'_>, id: &str) -> Result<u32, StoreError> {
    Ok(transaction.query_row(
        "SELECT COALESCE(MAX(seq), 0) FROM doc_versions WHERE doc_id = ?1",
        params![id],
        |row| row.get(0),
    )?)
}

fn validate_document(document: &ODoc) -> Result<(), StoreError> {
    validate_document_id(&document.id)?;
    if document.schema_version.trim().is_empty() {
        return Err(StoreError::InvalidDocument(
            "schemaVersion cannot be empty".to_owned(),
        ));
    }
    if document.body.id.trim().is_empty() || document.body.title.id.trim().is_empty() {
        return Err(StoreError::InvalidDocument(
            "body and title node IDs cannot be empty".to_owned(),
        ));
    }
    Ok(())
}

fn validate_document_id(id: &str) -> Result<(), StoreError> {
    if id.trim().is_empty() || id.len() > MAX_DOCUMENT_ID_BYTES || id.chars().any(char::is_control)
    {
        Err(StoreError::InvalidDocument(
            "document ID must be 1-256 bytes without control characters".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn display_title(document: &ODoc) -> &str {
    let title = document.body.title.text.trim();
    if title.is_empty() {
        "未命名公文"
    } else {
        title
    }
}

fn normalize_stage(stage: Option<&str>) -> Option<&str> {
    stage.map(str::trim).filter(|value| !value.is_empty())
}

fn enum_text<T: Serialize>(value: T) -> Result<String, StoreError> {
    match serde_json::to_value(value)? {
        serde_json::Value::String(text) => Ok(text),
        _ => Err(StoreError::InvalidDocument(
            "enum did not serialize as a string".to_owned(),
        )),
    }
}

fn parse_enum<T>(text: &str, column: usize) -> Result<T, rusqlite::Error>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(serde_json::Value::String(text.to_owned())).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            Type::Text,
            Box::new(error) as Box<dyn StdError + Send + Sync>,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use odoc_model::Block;

    #[test]
    fn saves_loads_and_lists_a_document() {
        let mut store = DocumentStore::open_in_memory().expect("store should open");
        let mut document = ODoc::empty();
        document.body.title.text = "关于测试存储的请示".to_owned();

        let saved = store
            .save_document(&document, Some("user_save"))
            .expect("document should save");
        assert!(saved.created);
        assert!(saved.version_created);
        assert_eq!(saved.version_seq, 1);
        assert_eq!(store.get_document(&document.id).unwrap(), document);

        let summaries = store.list_documents(20).expect("documents should list");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].title, "关于测试存储的请示");
        assert_eq!(summaries[0].doc_type, DocumentType::Request);
        assert_eq!(summaries[0].version_count, 1);
    }

    #[test]
    fn identical_saves_do_not_create_duplicate_versions() {
        let mut store = DocumentStore::open_in_memory().expect("store should open");
        let document = ODoc::empty();
        store
            .save_document(&document, Some("user_save"))
            .expect("first save should succeed");
        let second = store
            .save_document(&document, Some("autosave"))
            .expect("second save should succeed");
        assert!(!second.created);
        assert!(!second.version_created);
        assert_eq!(second.version_seq, 1);
        assert_eq!(
            store
                .list_document_versions(&document.id)
                .expect("versions should list")
                .len(),
            1
        );
    }

    #[test]
    fn changed_documents_create_immutable_versions() {
        let mut store = DocumentStore::open_in_memory().expect("store should open");
        let mut document = ODoc::empty();
        document.body.title.text = "第一版".to_owned();
        store
            .save_document(&document, Some("draft"))
            .expect("first version should save");

        document.revision += 1;
        document.body.title.text = "第二版".to_owned();
        document.body.blocks.push(Block::Paragraph {
            id: "block-second".to_owned(),
            content: "新增内容".to_owned(),
        });
        let result = store
            .save_document(&document, Some("user_save"))
            .expect("second version should save");
        assert_eq!(result.version_seq, 2);
        assert_eq!(
            store
                .get_document_version(&document.id, 1)
                .expect("first version should load")
                .body
                .title
                .text,
            "第一版"
        );
        assert_eq!(
            store
                .get_document_version(&document.id, 2)
                .expect("second version should load")
                .body
                .title
                .text,
            "第二版"
        );
    }

    #[test]
    fn deletion_cascades_to_versions() {
        let mut store = DocumentStore::open_in_memory().expect("store should open");
        let document = ODoc::empty();
        store
            .save_document(&document, None)
            .expect("document should save");
        assert!(
            store
                .delete_document(&document.id)
                .expect("document should delete")
        );
        assert!(matches!(
            store.get_document(&document.id),
            Err(StoreError::DocumentNotFound(_))
        ));
        let version_count: u32 = store
            .connection
            .query_row("SELECT COUNT(*) FROM doc_versions", [], |row| row.get(0))
            .expect("version count should query");
        assert_eq!(version_count, 0);
    }

    #[test]
    fn invalid_document_ids_are_rejected() {
        let mut store = DocumentStore::open_in_memory().expect("store should open");
        let mut document = ODoc::empty();
        document.id = "\n".to_owned();
        assert!(matches!(
            store.save_document(&document, None),
            Err(StoreError::InvalidDocument(_))
        ));
    }

    #[test]
    fn stale_revisions_cannot_overwrite_newer_content() {
        let mut store = DocumentStore::open_in_memory().expect("store should open");
        let mut document = ODoc::empty();
        document.revision = 2;
        document.body.title.text = "较新内容".to_owned();
        store
            .save_document(&document, Some("autosave"))
            .expect("newer document should save");

        let mut stale = document.clone();
        stale.revision = 1;
        stale.body.title.text = "过期内容".to_owned();
        assert!(matches!(
            store.save_document(&stale, Some("autosave")),
            Err(StoreError::StaleRevision {
                incoming: 1,
                stored: 2,
                ..
            })
        ));
        assert_eq!(
            store.get_document(&document.id).unwrap().body.title.text,
            "较新内容"
        );
    }
}
