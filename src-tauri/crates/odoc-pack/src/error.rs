use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PackError {
    #[error("filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("pack tree traversal failed: {0}")]
    Walk(#[from] walkdir::Error),
    #[error("pack manifest is invalid: {0}")]
    ManifestParse(#[from] toml::de::Error),
    #[error("unsupported pack schema version '{0}'")]
    UnsupportedSchema(String),
    #[error("invalid pack manifest: {0}")]
    InvalidManifest(String),
    #[error("pack directory name does not match manifest id '{0}'")]
    DirectoryIdMismatch(String),
    #[error("pack resource path is invalid: {0}")]
    InvalidResourcePath(String),
    #[error("pack resource does not exist: {0}")]
    ResourceNotFound(String),
    #[error("pack resource is not declared in the manifest: {0}")]
    UndeclaredResource(String),
    #[error("pack resource has an unsupported type: {0}")]
    UnsupportedResource(String),
    #[error("symbolic links are not allowed in packs: {0}")]
    SymlinkNotAllowed(PathBuf),
    #[error("pack file exceeds the {limit} byte limit: {path}")]
    FileTooLarge { path: PathBuf, limit: u64 },
    #[error("pack exceeds the {0} byte total-size limit")]
    PackTooLarge(u64),
    #[error("pack exceeds the {0} file-count limit")]
    TooManyFiles(usize),
    #[error("pack resource is not valid UTF-8: {0}")]
    InvalidUtf8(String),
    #[error("pack YAML resource is invalid: {0}")]
    InvalidYaml(String),
    #[error("pack TOML resource is invalid: {0}")]
    InvalidToml(String),
    #[error("pack rule '{rule_id}' is invalid: {reason}")]
    InvalidRule {
        rule_id: String,
        reason: &'static str,
    },
    #[error("duplicate rule id '{0}'")]
    DuplicateRuleId(String),
    #[error("duplicate pack id '{0}'")]
    DuplicatePackId(String),
}
