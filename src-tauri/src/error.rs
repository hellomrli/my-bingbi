//! The IPC error type and error-mapping helpers.

use odoc_engine::EngineError;
use odoc_export::ExportError;
use odoc_kb::KbError;
use odoc_llm::LlmError;
use odoc_store::{SecretError, StoreError};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IpcError {
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
}

pub(crate) fn map_store_error(error: StoreError) -> IpcError {
    match error {
        StoreError::DocumentNotFound(_) | StoreError::VersionNotFound { .. } => IpcError {
            code: "document_not_found",
            message: "没有找到指定的公文或版本。",
        },
        StoreError::ProviderNotFound(_) => IpcError {
            code: "provider_not_found",
            message: "没有找到指定的模型服务。",
        },
        StoreError::InvalidDocument(_)
        | StoreError::InvalidSettingKey(_)
        | StoreError::InvalidSession(_)
        | StoreError::InvalidProviderConfiguration
        | StoreError::InvalidKnowledgeBaseConfiguration => IpcError {
            code: "invalid_input",
            message: "提交的数据格式无效。",
        },
        StoreError::StaleRevision { .. } => IpcError {
            code: "stale_revision",
            message: "检测到较新的已保存版本，请重新打开公文后再修改。",
        },
        StoreError::DocumentConflict(_) => IpcError {
            code: "document_conflict",
            message: "运行期间文档已变更，未覆盖当前内容；生成结果已保留，可从起草记录中另存。",
        },
        StoreError::Serialization(_) => IpcError {
            code: "serialization_failed",
            message: "公文数据无法序列化。",
        },
        StoreError::Sqlite(_)
        | StoreError::Io(_)
        | StoreError::ApplicationDirectoryUnavailable
        | StoreError::UnsupportedSchema { .. }
        | StoreError::SessionNotFound(_)
        | StoreError::InvalidSystemClock => IpcError {
            code: "store_unavailable",
            message: "本地文档库暂不可用。",
        },
    }
}

pub(crate) fn map_llm_error(error: LlmError) -> IpcError {
    IpcError {
        code: error.code(),
        message: error.user_message(),
    }
}

pub(crate) fn map_engine_error(error: EngineError) -> IpcError {
    IpcError {
        code: error.code(),
        message: error.user_message(),
    }
}

pub(crate) fn map_kb_error(error: KbError) -> IpcError {
    IpcError {
        code: error.code(),
        message: error.user_message(),
    }
}

pub(crate) fn map_lint_error(_error: odoc_lint::LintError) -> IpcError {
    IpcError {
        code: "lint_unavailable",
        message: "确定性质检暂不可用。",
    }
}

pub(crate) fn map_export_error(_error: odoc_export::ExportError) -> IpcError {
    IpcError {
        code: "export_failed",
        message: "DOCX 文件生成失败。",
    }
}

pub(crate) fn map_import_error(error: ExportError) -> IpcError {
    match error {
        ExportError::PackageTooLarge => IpcError {
            code: "import_too_large",
            message: "DOCX 文件超过 64 MiB，无法导入。",
        },
        _ => IpcError {
            code: "import_failed",
            message: "DOCX 文件结构无效或无法读取。",
        },
    }
}

pub(crate) fn map_reference_error(error: ExportError) -> IpcError {
    match error {
        ExportError::PackageTooLarge => IpcError {
            code: "reference_too_large",
            message: "参考文件超过 64 MiB，内部内容超过安全限制，或提取文字超过 2 MiB。",
        },
        ExportError::UnsupportedReferenceFormat => IpcError {
            code: "reference_unsupported",
            message: "暂不支持该参考文件格式。支持 PDF、OFD、DOC、DOCX、XLSX、XLS 和 WPS。",
        },
        _ => IpcError {
            code: "reference_invalid",
            message: "参考文件结构无效、已加密、为纯扫描件，或没有可提取的文字。",
        },
    }
}

pub(crate) fn map_template_error(error: ExportError) -> IpcError {
    match error {
        ExportError::TemplateMarkerMissing => IpcError {
            code: "template_marker_missing",
            message: "DOCX 模板须包含独立的 BINGBI_BODY 占位段落。",
        },
        ExportError::PackageTooLarge => IpcError {
            code: "template_too_large",
            message: "DOCX 模板超过 64 MiB，无法保存。",
        },
        _ => IpcError {
            code: "template_invalid",
            message: "DOCX 模板结构无效或无法读取。",
        },
    }
}

pub(crate) fn map_tauri_error(_error: tauri::Error) -> IpcError {
    IpcError {
        code: "pipeline_unavailable",
        message: "起草流程配置不可用。",
    }
}

pub(crate) fn map_secret_error(_error: SecretError) -> IpcError {
    IpcError {
        code: "secret_store_unavailable",
        message: "系统凭据库不可用或处于锁定状态。",
    }
}

pub(crate) fn map_pdf_error(error: ExportError) -> IpcError {
    let message = match error {
        ExportError::FontRead(..) | ExportError::InvalidFont(..) => {
            "PDF 生成失败：未找到可用的中文字体。"
        }
        ExportError::TypstCompile(..) | ExportError::TypstPdfExport(..) => {
            "PDF 生成失败：文档排版失败。"
        }
        _ => "PDF 生成失败。",
    };
    IpcError {
        code: "export_failed",
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_errors_are_redacted_for_ipc() {
        let error = map_store_error(StoreError::DocumentNotFound("secret-id".to_owned()));
        assert_eq!(error.code, "document_not_found");
        assert!(!error.message.contains("secret-id"));
    }

    #[test]
    fn llm_errors_are_stable_and_redacted_for_ipc() {
        let error = map_llm_error(LlmError::HttpStatus(401));
        assert_eq!(error.code, "llm_http_error");
        assert_eq!(error.message, "模型服务拒绝了凭据，请检查 API Key。");
    }

    #[test]
    fn knowledge_base_errors_are_stable_and_redacted_for_ipc() {
        let error = map_kb_error(KbError::HttpStatus(401));
        assert_eq!(error.code, "kb_http_error");
        assert!(!error.message.contains("401"));
    }
}
