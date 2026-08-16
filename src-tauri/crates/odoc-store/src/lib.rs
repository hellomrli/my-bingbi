mod config;
mod crash;
mod document;
mod error;
mod kb;
mod provider;
mod secret;
mod session;

use directories::ProjectDirs;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use config::{AppConfig, AppTheme};
pub use crash::LocalCrashReporter;
pub use document::{DocumentSummary, DocumentVersionSummary, SaveDocumentResult};
pub use error::{SecretError, StoreError};
pub use kb::{CostSummary, CrashEvent, KbCacheHit, KbIndexState, PackInstallRecord, UpdateState};
pub use odoc_kb::KbConfig;
pub use secret::{KeyringSecretStore, MemorySecretStore, SecretStore};
pub use session::{SessionSummary, StageRunRecord};

pub const STORE_SCHEMA_VERSION: u32 = 2;
pub const DEFAULT_SECRET_SERVICE: &str = "cn.bingbi.workbench";

/// Emits the frontend declarations owned by this crate.
pub fn typescript_bindings() -> String {
    use ts_rs::TS;

    let declarations = [
        AppTheme::decl(),
        AppConfig::decl(),
        SaveDocumentResult::decl(),
        DocumentSummary::decl(),
        DocumentVersionSummary::decl(),
        SessionSummary::decl(),
        StageRunRecord::decl(),
        KbCacheHit::decl(),
        KbIndexState::decl(),
        CrashEvent::decl(),
        UpdateState::decl(),
        PackInstallRecord::decl(),
        CostSummary::decl(),
    ];
    format!(
        "// This file is generated from src-tauri/crates/odoc-store. Do not edit by hand.\n\nimport type {{ DocumentStatus, DocumentType }} from \"./bindings\";\n\nexport {}\n",
        declarations.join("\n\nexport ")
    )
}

pub struct DocumentStore {
    connection: Connection,
}

impl DocumentStore {
    /// Opens or creates a file-backed document store and applies pending migrations.
    ///
    /// # Errors
    ///
    /// Returns an error when the parent directory cannot be created, `SQLite` cannot open the
    /// database, a migration fails, or the database schema is newer than this application.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        let mut store = Self { connection };
        store.initialize(true)?;
        Ok(store)
    }

    /// Creates an initialized in-memory store for tests and transient workflows.
    ///
    /// # Errors
    ///
    /// Returns an error when `SQLite` initialization or the schema migration fails.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        let mut store = Self { connection };
        store.initialize(false)?;
        Ok(store)
    }

    /// Resolves the platform-specific default path for the application database.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ApplicationDirectoryUnavailable`] when the current platform does
    /// not expose a user configuration directory.
    pub fn default_database_path() -> Result<PathBuf, StoreError> {
        let directories = ProjectDirs::from("cn", "Bingbi", "my-wordediter")
            .ok_or(StoreError::ApplicationDirectoryUnavailable)?;
        Ok(directories.config_dir().join("bingbi.sqlite3"))
    }

    /// Resolves the platform-specific user configuration directory.
    ///
    /// User knowledge packs live under `config_dir/packs`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ApplicationDirectoryUnavailable`] when the current platform does
    /// not expose a user configuration directory.
    pub fn default_config_dir() -> Result<PathBuf, StoreError> {
        let directories = ProjectDirs::from("cn", "Bingbi", "my-wordediter")
            .ok_or(StoreError::ApplicationDirectoryUnavailable)?;
        Ok(directories.config_dir().to_path_buf())
    }

    fn initialize(&mut self, file_backed: bool) -> Result<(), StoreError> {
        self.connection.busy_timeout(Duration::from_secs(5))?;
        self.connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        if file_backed {
            self.connection
                .execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        }

        let current_version: u32 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if current_version > STORE_SCHEMA_VERSION {
            return Err(StoreError::UnsupportedSchema {
                found: current_version,
                supported: STORE_SCHEMA_VERSION,
            });
        }
        if current_version == 0 {
            self.migrate_to_v1()?;
        }
        let migrated_version: u32 =
            self.connection
                .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if migrated_version == 1 {
            self.migrate_to_v2()?;
        }
        Ok(())
    }

    fn migrate_to_v1(&mut self) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(SCHEMA_V1)?;
        transaction.pragma_update(None, "user_version", 1_u32)?;
        transaction.commit()?;
        Ok(())
    }

    fn migrate_to_v2(&mut self) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(
            r"
ALTER TABLE kb_notes ADD COLUMN content TEXT NOT NULL DEFAULT '';
ALTER TABLE kb_notes ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE kb_notes ADD COLUMN links_json TEXT NOT NULL DEFAULT '[]';

CREATE TABLE IF NOT EXISTS kb_index_state (
    vault          TEXT PRIMARY KEY,
    provider_kind  TEXT NOT NULL,
    last_cursor    INTEGER NOT NULL DEFAULT 0,
    note_count     INTEGER NOT NULL DEFAULT 0,
    chunk_count    INTEGER NOT NULL DEFAULT 0,
    status         TEXT NOT NULL DEFAULT 'idle',
    error_message  TEXT,
    updated_at     INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS crash_events (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    kind           TEXT NOT NULL,
    message        TEXT NOT NULL,
    context_json   TEXT NOT NULL DEFAULT '{}',
    created_at     INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS update_state (
    id             INTEGER PRIMARY KEY CHECK(id = 1),
    current_version TEXT NOT NULL,
    available_version TEXT,
    download_url   TEXT,
    checked_at     INTEGER,
    status         TEXT NOT NULL DEFAULT 'unknown'
);

CREATE TABLE IF NOT EXISTS pack_installations (
    pack_id        TEXT PRIMARY KEY,
    source_path    TEXT NOT NULL,
    enabled        INTEGER NOT NULL DEFAULT 1,
    installed_at   INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_crash_events_created_at ON crash_events(created_at DESC);
",
        )?;
        transaction.pragma_update(None, "user_version", STORE_SCHEMA_VERSION)?;
        transaction.commit()?;
        Ok(())
    }
}

const SCHEMA_V1: &str = r"
CREATE TABLE documents (
    id            TEXT PRIMARY KEY,
    title         TEXT NOT NULL,
    doc_type      TEXT NOT NULL,
    odoc_json     TEXT NOT NULL,
    revision      INTEGER NOT NULL,
    status        TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE INDEX idx_documents_updated_at ON documents(updated_at DESC);
CREATE INDEX idx_documents_doc_type ON documents(doc_type);

CREATE TABLE doc_versions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    doc_id        TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    seq           INTEGER NOT NULL,
    odoc_json     TEXT NOT NULL,
    odoc_revision INTEGER NOT NULL,
    stage         TEXT,
    created_at    INTEGER NOT NULL,
    UNIQUE(doc_id, seq)
);

CREATE INDEX idx_doc_versions_doc_seq ON doc_versions(doc_id, seq DESC);

CREATE TABLE app_settings (
    key           TEXT PRIMARY KEY,
    value_json    TEXT NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE sessions (
    id            TEXT PRIMARY KEY,
    doc_id        TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    pipeline      TEXT NOT NULL,
    state_json    TEXT NOT NULL,
    cost_tokens   INTEGER NOT NULL DEFAULT 0,
    cost_money    REAL NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE stage_runs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id    TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    stage_id      TEXT NOT NULL,
    input_json    TEXT NOT NULL,
    output_json   TEXT,
    provider      TEXT,
    tokens        INTEGER NOT NULL DEFAULT 0,
    duration_ms   INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL
);

CREATE TABLE kb_notes (
    path              TEXT NOT NULL,
    vault             TEXT NOT NULL,
    title             TEXT NOT NULL,
    frontmatter_json  TEXT NOT NULL DEFAULT '{}',
    mtime             INTEGER NOT NULL,
    hash              TEXT NOT NULL,
    PRIMARY KEY(vault, path)
);

CREATE TABLE kb_chunks (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    vault         TEXT NOT NULL,
    path          TEXT NOT NULL,
    seq           INTEGER NOT NULL,
    text          TEXT NOT NULL,
    embedding     BLOB,
    UNIQUE(vault, path, seq)
);

CREATE VIRTUAL TABLE kb_fts USING fts5(vault UNINDEXED, path UNINDEXED, title, content);

CREATE TABLE findings (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    doc_id        TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    rule_id       TEXT NOT NULL,
    severity      TEXT NOT NULL,
    node_id       TEXT NOT NULL,
    message       TEXT NOT NULL,
    created_at    INTEGER NOT NULL
);

CREATE TABLE providers (
    id            TEXT PRIMARY KEY,
    kind          TEXT NOT NULL,
    display_name  TEXT NOT NULL,
    base_url      TEXT,
    model         TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    is_local      INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);
";

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn initializes_the_full_schema() {
        let store = DocumentStore::open_in_memory().expect("store should open");
        let version: u32 = store
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("schema version should exist");
        assert_eq!(version, STORE_SCHEMA_VERSION);

        for table in [
            "documents",
            "doc_versions",
            "app_settings",
            "sessions",
            "stage_runs",
            "kb_notes",
            "kb_chunks",
            "kb_fts",
            "findings",
            "providers",
        ] {
            let count: u32 = store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1",
                    params![table],
                    |row| row.get(0),
                )
                .expect("schema object query should succeed");
            assert_eq!(count, 1, "missing schema object {table}");
        }
    }

    #[test]
    fn rejects_a_database_from_a_newer_schema() {
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let path = directory.path().join("future.sqlite3");
        let connection = Connection::open(&path).expect("database should open");
        connection
            .pragma_update(None, "user_version", STORE_SCHEMA_VERSION + 1)
            .expect("version should update");
        drop(connection);

        let error = DocumentStore::open(path)
            .err()
            .expect("newer schema should be rejected");
        assert!(matches!(error, StoreError::UnsupportedSchema { .. }));
    }
}
