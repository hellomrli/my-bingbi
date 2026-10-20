//! Kernel wiring for the desktop application and its IPC handlers.

mod clinepass;
mod commands;
mod dto;
mod error;
mod state;
mod util;

use crate::commands::kb::kb_sync_supervisor;
use crate::state::AppState;
use crate::util::{builtin_pack_root, ensure_user_pack_root};
use odoc_pack::PackRegistry;
use odoc_store::{DEFAULT_SECRET_SERVICE, DocumentStore, KeyringSecretStore, LocalCrashReporter};
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let database_path = DocumentStore::default_database_path()?;
            let documents = DocumentStore::open(&database_path)?;
            let crash_reporter = LocalCrashReporter::new(
                &database_path,
                env!("CARGO_PKG_VERSION"),
                documents.app_config()?.crash_reporting_enabled,
            );
            crash_reporter.install();
            let secrets = KeyringSecretStore::new(DEFAULT_SECRET_SERVICE)?;
            // The updater needs a release signing key. Release builds embed the public key in
            // tauri.conf.json (`plugins.updater.pubkey`, injected by the release pipeline via
            // --config); register the plugin only when that key is present so dev/CI builds
            // stay signature-free.
            let updater_pubkey = app
                .config()
                .plugins
                .0
                .get("updater")
                .and_then(|config| config.get("pubkey"))
                .and_then(|pubkey| pubkey.as_str())
                .map(str::trim)
                .filter(|pubkey| !pubkey.is_empty());
            if let Some(pubkey) = updater_pubkey {
                app.handle().plugin(
                    tauri_plugin_updater::Builder::new()
                        .pubkey(pubkey.to_owned())
                        .build(),
                )?;
            }
            let user_pack_root = DocumentStore::default_config_dir()?.join("packs");
            ensure_user_pack_root(&user_pack_root)?;
            let (packs, pack_issues) =
                PackRegistry::load_with_user_root(builtin_pack_root(app)?, &user_pack_root)?;
            let (kb_sync_generation, kb_sync_receiver) = tokio::sync::watch::channel(0_u64);
            app.manage(AppState {
                documents: Mutex::new(documents),
                secrets: Box::new(secrets),
                crash_reporter,
                kb_sync_generation,
                user_pack_root,
                pack_issues: pack_issues
                    .into_iter()
                    .map(|issue| issue.to_string())
                    .collect(),
            });
            app.manage(packs);
            tauri::async_runtime::spawn(kb_sync_supervisor(app.handle().clone(), kb_sync_receiver));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::document::create_empty_document,
            commands::document::backend_version,
            commands::config::list_pack_summaries,
            commands::config::list_templates,
            clinepass::clinepass_overview,
            clinepass::clinepass_open_subscription,
            commands::config::user_pack_root,
            commands::config::pack_load_issues,
            commands::config::list_crash_events,
            commands::config::clear_crash_events,
            commands::update::check_for_update,
            commands::update::apply_update,
            commands::document::save_document,
            commands::document::load_document,
            commands::document::list_documents,
            commands::document::list_sessions,
            commands::document::list_stage_runs,
            commands::document::delete_document,
            commands::document::list_document_versions,
            commands::document::load_document_version,
            commands::document::restore_document_version,
            commands::document::diff_document_versions,
            commands::config::load_app_config,
            commands::config::save_app_config,
            commands::config::load_kb_config,
            commands::config::save_kb_config,
            commands::config::set_kb_fns_token,
            commands::config::has_kb_fns_token,
            commands::config::delete_kb_fns_token,
            commands::config::set_kb_fns_ws_token,
            commands::config::has_kb_fns_ws_token,
            commands::config::delete_kb_fns_ws_token,
            commands::config::set_kb_mcp_token,
            commands::config::has_kb_mcp_token,
            commands::config::delete_kb_mcp_token,
            commands::kb::kb_search,
            commands::kb::kb_get,
            commands::kb::kb_index_state,
            commands::kb::kb_rebuild_index,
            commands::kb::kb_clear_index,
            commands::kb::kb_create_local_vault,
            commands::kb::kb_create_note,
            commands::kb::kb_sync_once,
            commands::kb::archive_document_to_kb,
            commands::config::list_provider_profiles,
            commands::config::save_provider_profile,
            commands::config::set_provider_secret,
            commands::config::has_provider_secret,
            commands::config::delete_provider_secret,
            commands::model::test_provider_connection,
            commands::model::list_provider_models,
            commands::model::chat_with_provider,
            commands::model::chat_stream_with_provider,
            commands::engine::draft_start,
            commands::engine::rerun_stage,
            commands::engine::review_start,
            commands::lint::lint_document_now,
            commands::lint::fact_check_document_now,
            commands::export::export_docx,
            commands::document::import_docx,
            commands::document::extract_docx_text,
            commands::document::extract_reference_text,
            commands::document::read_dropped_reference,
            commands::export::export_pdf,
            commands::export::export_capabilities,
            commands::export::save_docx_template,
            commands::export::delete_docx_template,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run the Bingbi desktop application");
}
