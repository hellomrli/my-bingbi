//! Helpers used by more than one command module.

use crate::error::{IpcError, map_lint_error, map_llm_error, map_secret_error, map_store_error};
use crate::state::AppState;
use odoc_export::Gb9704Style;
use odoc_llm::{ProviderClient, ProviderKind};
use odoc_model::ODoc;
use odoc_pack::{PackRegistry, PackSummary};
use odoc_store::DocumentStore;
use std::path::PathBuf;
use tauri::Manager;

pub(crate) const MAX_DRAFT_MATERIAL_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const KB_FNS_TOKEN_ACCOUNT: &str = "knowledge-base:fns:token";
pub(crate) const KB_FNS_WS_TOKEN_ACCOUNT: &str = "knowledge-base:fns:ws-token";
pub(crate) const KB_MCP_TOKEN_ACCOUNT: &str = "knowledge-base:mcp:token";

pub(crate) fn lock_documents<'a>(
    state: &'a AppState,
) -> Result<std::sync::MutexGuard<'a, DocumentStore>, IpcError> {
    state.documents.lock().map_err(|_| IpcError {
        code: "store_unavailable",
        message: "本地文档库暂不可用。",
    })
}

pub(crate) fn provider_secret_account(provider_id: &str) -> Result<String, IpcError> {
    let valid = !provider_id.is_empty()
        && provider_id.len() <= 64
        && provider_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if valid {
        Ok(format!("provider:{provider_id}:api-key"))
    } else {
        Err(IpcError {
            code: "invalid_input",
            message: "模型服务标识格式无效。",
        })
    }
}

pub(crate) fn pack_summaries(packs: &PackRegistry) -> Vec<PackSummary> {
    packs
        .iter()
        .map(|pack| PackSummary::from(pack.manifest()))
        .collect()
}

pub(crate) fn builtin_pack_root<R: tauri::Runtime>(
    app: &tauri::App<R>,
) -> Result<PathBuf, tauri::Error> {
    if cfg!(debug_assertions) {
        Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../packs"))
    } else {
        Ok(app.path().resource_dir()?.join("packs"))
    }
}

fn development_pipeline_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../pipelines")
        .join(file_name)
}

pub(crate) fn builtin_pipeline_path<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    file_name: &str,
) -> Result<PathBuf, tauri::Error> {
    if cfg!(debug_assertions) {
        Ok(development_pipeline_path(file_name))
    } else {
        Ok(app.path().resource_dir()?.join("pipelines").join(file_name))
    }
}

pub(crate) fn ensure_exportable(document: &ODoc, packs: &PackRegistry) -> Result<(), IpcError> {
    let lint_report = odoc_lint::lint_document(document, packs).map_err(map_lint_error)?;
    let fact_check_report = odoc_lint::fact_check_document(document).map_err(map_lint_error)?;
    let fact_error_count: u32 = fact_check_report
        .findings
        .iter()
        .filter(|finding| finding.severity == odoc_lint::Severity::Error)
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    if lint_report.error_count.saturating_add(fact_error_count) > 0 {
        Err(IpcError {
            code: "export_blocked",
            message: "文档存在未解决的确定性错误，请修正后再导出。",
        })
    } else {
        Ok(())
    }
}

pub(crate) fn export_style(
    state: &AppState,
    packs: &PackRegistry,
) -> Result<Gb9704Style, IpcError> {
    let pack = packs.get("gb9704-format").ok_or(IpcError {
        code: "export_style_unavailable",
        message: "GB/T 9704 样式资源不可用。",
    })?;
    let resource = pack
        .resources_for_stage(odoc_pack::StageId::Export, None)
        .map_err(|_| IpcError {
            code: "export_style_unavailable",
            message: "GB/T 9704 样式资源不可用。",
        })?
        .into_iter()
        .find(|resource| resource.role == odoc_pack::ResourceRole::Style)
        .ok_or(IpcError {
            code: "export_style_unavailable",
            message: "GB/T 9704 样式资源不可用。",
        })?;
    let style_text = pack
        .read_text(&resource.relative_path)
        .map_err(|_| IpcError {
            code: "export_style_unavailable",
            message: "GB/T 9704 样式资源不可用。",
        })?;
    let mappings = lock_documents(state)?
        .app_config()
        .map_err(map_store_error)?
        .font_mappings;
    Gb9704Style::from_toml(&style_text)
        .map(|style| style.with_font_mappings(&mappings))
        .map_err(|_| IpcError {
            code: "export_style_invalid",
            message: "GB/T 9704 样式配置格式无效。",
        })
}

pub(crate) fn provider_client(
    state: &AppState,
    provider_id: &str,
) -> Result<ProviderClient, IpcError> {
    let profile = lock_documents(state)?
        .provider_profile(provider_id)
        .map_err(map_store_error)?;
    let api_key = if matches!(
        profile.kind,
        ProviderKind::OpenAiCompatible | ProviderKind::Anthropic
    ) {
        let account = provider_secret_account(provider_id)?;
        state
            .secrets
            .get_secret(&account)
            .map_err(map_secret_error)?
    } else {
        None
    };
    ProviderClient::new(profile, api_key).map_err(map_llm_error)
}

/// Creates the user pack root, seeding a commented example pack when the root is empty.
///
/// The example pack demonstrates custom deterministic rules: edit `rules/format.yaml` and
/// restart the app. Existing content is never touched.
pub(crate) fn ensure_user_pack_root(root: &std::path::Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(root)?;
    if root.read_dir()?.next().is_some() {
        return Ok(());
    }
    let pack_dir = root.join("local-corpus");
    std::fs::create_dir_all(pack_dir.join("rules"))?;
    std::fs::write(
        pack_dir.join("pack.toml"),
        r#"schema_version = "0.1"

[pack]
id = "local-corpus"
name = "本单位语料包"
version = "0.1.0"
description = "本单位规则与笔法（示例包，规则文件可直接编辑）"
license = "private"

[capabilities]
provides = ["format_rules", "language_rules"]

[[bindings]]
stage = "lint"
rules = ["rules/format.yaml"]
"#,
    )?;
    std::fs::write(
        pack_dir.join("rules/format.yaml"),
        r#"# 本单位自定义规则（示例）
# 每条规则是一个 `- id: ...` 条目，语法见内置包 packs/gb9704-format/rules/format.yaml。
# 取消注释即可启用示例规则，修改后重启应用生效。

# - id: unit-banned-word
#   name: 本单位禁用语
#   severity: error
#   type: lexicon_density
#   scope: body.blocks
#   lexicon: ["杜绝形式主义"]
#   max_per_100_chars: 0
#   message: 本单位禁用语，请更换表述

[]
"#,
    )?;
    std::fs::write(
        pack_dir.join("README.md"),
        "# 本单位语料包（示例）\n\n这是秉笔为用户自定义规则准备的示例知识包。修改\n`rules/format.yaml` 后重启应用生效；也可以复制本目录为一个新包\n（pack.toml 里的 id 必须与目录名一致）。\n",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use odoc_engine::PipelineDefinition;

    #[test]
    fn exposes_validated_builtin_pack_summaries() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../packs");
        let packs = PackRegistry::load(root).expect("built-in packs should load");
        let summaries = pack_summaries(&packs);
        assert_eq!(
            summaries
                .iter()
                .map(|summary| summary.id.as_str())
                .collect::<Vec<_>>(),
            ["gb9704-format", "renmin-style"]
        );
        assert!(summaries.iter().all(|summary| !summary.version.is_empty()));
    }

    #[test]
    fn provider_secret_accounts_are_namespaced_and_validated() {
        assert_eq!(
            provider_secret_account("deepseek").unwrap(),
            "provider:deepseek:api-key"
        );
        assert!(provider_secret_account("../deepseek").is_err());
        assert!(provider_secret_account("").is_err());
    }

    #[test]
    fn knowledge_base_token_account_is_separate_from_model_credentials() {
        assert_eq!(KB_FNS_TOKEN_ACCOUNT, "knowledge-base:fns:token");
        assert_ne!(
            KB_FNS_TOKEN_ACCOUNT,
            provider_secret_account("fns").expect("provider account should build")
        );
    }

    #[test]
    fn development_pipeline_paths_point_to_repository_resources() {
        for file_name in ["draft.yaml", "review.yaml", "imitate.yaml"] {
            let path = development_pipeline_path(file_name);
            assert!(path.ends_with(format!("pipelines/{file_name}")));
            assert!(
                path.is_file(),
                "{file_name} should ship with the repository"
            );
        }
    }

    #[test]
    fn builtin_pipelines_pass_kernel_validation() {
        let draft = PipelineDefinition::load(development_pipeline_path("draft.yaml"))
            .expect("draft pipeline should load");
        assert_eq!(draft.kind, odoc_engine::PipelineKind::Draft);
        let review = PipelineDefinition::load(development_pipeline_path("review.yaml"))
            .expect("review pipeline should load");
        assert_eq!(review.kind, odoc_engine::PipelineKind::Review);
        let imitate = PipelineDefinition::load(development_pipeline_path("imitate.yaml"))
            .expect("imitate pipeline should load");
        assert_eq!(imitate.kind, odoc_engine::PipelineKind::Imitate);
    }

    #[test]
    fn user_pack_root_seeds_an_example_pack_only_when_empty() {
        let directory = tempfile::tempdir().expect("temp directory");
        let root = directory.path().join("packs");
        ensure_user_pack_root(&root).expect("example pack should be seeded");
        let manifest = std::fs::read_to_string(root.join("local-corpus/pack.toml"))
            .expect("example manifest should exist");
        assert!(manifest.contains("id = \"local-corpus\""));
        assert!(
            std::fs::read_to_string(root.join("local-corpus/rules/format.yaml"))
                .expect("example rules should exist")
                .contains("本单位自定义规则")
        );

        // The seeded example is a loadable pack with no rules.
        let builtin = directory.path().join("builtin");
        std::fs::create_dir_all(&builtin).expect("builtin root");
        let (registry, issues) =
            PackRegistry::load_with_user_root(&builtin, &root).expect("packs should load");
        assert!(
            issues.is_empty(),
            "the seeded user root should load cleanly"
        );
        assert_eq!(
            registry
                .get("local-corpus")
                .expect("example pack should load")
                .manifest()
                .pack
                .id,
            "local-corpus"
        );

        // Existing content is never touched.
        std::fs::write(root.join("keep.txt"), "keep me").expect("marker should be written");
        ensure_user_pack_root(&root).expect("existing root should stay untouched");
        assert!(root.join("keep.txt").is_file());
        assert!(!root.join("another-pack").exists());
    }
}
