use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("application data directory is unavailable")]
    ApplicationDirectoryUnavailable,
    #[error("database schema {found} is newer than supported schema {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("document '{0}' was not found")]
    DocumentNotFound(String),
    #[error("version {seq} of document '{doc_id}' was not found")]
    VersionNotFound { doc_id: String, seq: u32 },
    #[error("invalid document: {0}")]
    InvalidDocument(String),
    #[error("document '{id}' revision {incoming} is older than stored revision {stored}")]
    StaleRevision {
        id: String,
        incoming: u32,
        stored: u32,
    },
    #[error("invalid setting key '{0}'")]
    InvalidSettingKey(String),
    #[error("invalid drafting session: {0}")]
    InvalidSession(String),
    #[error("drafting session '{0}' was not found")]
    SessionNotFound(String),
    #[error("provider '{0}' was not found")]
    ProviderNotFound(String),
    #[error("provider configuration is invalid")]
    InvalidProviderConfiguration,
    #[error("knowledge-base configuration is invalid")]
    InvalidKnowledgeBaseConfiguration,
    #[error("system clock is before the Unix epoch")]
    InvalidSystemClock,
}

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("invalid secret identifier")]
    InvalidIdentifier,
    #[error("system credential store is unavailable: {0}")]
    Unavailable(String),
    #[error("secret store lock is poisoned")]
    LockPoisoned,
}
