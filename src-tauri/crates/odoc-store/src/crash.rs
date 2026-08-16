use crate::DocumentStore;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{panic, thread};

const MAX_MESSAGE_CHARS: usize = 2_048;
const MAX_CONTEXT_TEXT_CHARS: usize = 1_024;

/// Opt-in reporter for Rust panics that persists diagnostics only in the local `SQLite` store.
///
/// The hook deliberately records a small, bounded context (application version, target, thread
/// and source location). It never performs network I/O and can be switched at runtime without
/// reinstalling the process-wide panic hook.
#[derive(Clone)]
pub struct LocalCrashReporter {
    database_path: Arc<PathBuf>,
    app_version: Arc<str>,
    enabled: Arc<AtomicBool>,
}

impl LocalCrashReporter {
    pub fn new(
        database_path: impl AsRef<Path>,
        app_version: impl Into<Arc<str>>,
        enabled: bool,
    ) -> Self {
        Self {
            database_path: Arc::new(database_path.as_ref().to_path_buf()),
            app_version: app_version.into(),
            enabled: Arc::new(AtomicBool::new(enabled)),
        }
    }

    /// Installs the process-wide panic hook while preserving Rust's existing stderr reporter.
    pub fn install(&self) {
        let reporter = self.clone();
        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            reporter.capture(panic_info);
            previous_hook(panic_info);
        }));
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn capture(&self, panic_info: &panic::PanicHookInfo<'_>) {
        if !self.is_enabled() {
            return;
        }
        let current_thread = thread::current();
        let thread_name = bounded_text(
            current_thread.name().unwrap_or("unnamed"),
            MAX_CONTEXT_TEXT_CHARS,
        );
        let location = panic_info.location();
        let context = PanicContext {
            app_version: &self.app_version,
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            thread: &thread_name,
            file: location.map(panic::Location::file),
            line: location.map(panic::Location::line),
            column: location.map(panic::Location::column),
        };
        let Ok(context_json) = serde_json::to_string(&context) else {
            return;
        };
        let message = panic_message(panic_info);

        // Use a short-lived connection: the panic may have poisoned the main store mutex. Any
        // failure here is ignored so diagnostic recording can never replace the original panic.
        let _ = DocumentStore::record_crash_event_at(
            self.database_path.as_ref(),
            "rust_panic",
            &message,
            &context_json,
        );
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PanicContext<'a> {
    app_version: &'a str,
    os: &'static str,
    arch: &'static str,
    thread: &'a str,
    file: Option<&'a str>,
    line: Option<u32>,
    column: Option<u32>,
}

fn panic_message(panic_info: &panic::PanicHookInfo<'_>) -> String {
    let payload = panic_info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("未知 Rust panic");
    bounded_text(message, MAX_MESSAGE_CHARS)
}

fn bounded_text(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .take(max_chars)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reporter_can_be_switched_without_reinstalling_the_hook() {
        let reporter = LocalCrashReporter::new("/tmp/bingbi-test.sqlite3", "0.1.0", false);
        assert!(!reporter.is_enabled());
        reporter.set_enabled(true);
        assert!(reporter.is_enabled());
        reporter.set_enabled(false);
        assert!(!reporter.is_enabled());
    }

    #[test]
    fn diagnostic_text_is_bounded_and_drops_unsafe_controls() {
        assert_eq!(bounded_text("前\0文\n后文", 4), "前文\n后");
        assert_eq!(bounded_text("abcdef", 3), "abc");
    }
}
