//! State shared by the IPC commands and the background sync supervisor.

use odoc_store::{DocumentStore, LocalCrashReporter, SecretStore};
use std::path::PathBuf;
use std::sync::Mutex;

pub(crate) struct AppState {
    pub(crate) documents: Mutex<DocumentStore>,
    pub(crate) secrets: Box<dyn SecretStore>,
    pub(crate) crash_reporter: LocalCrashReporter,
    pub(crate) kb_sync_generation: tokio::sync::watch::Sender<u64>,
    pub(crate) user_pack_root: PathBuf,
    pub(crate) pack_issues: Vec<String>,
}
