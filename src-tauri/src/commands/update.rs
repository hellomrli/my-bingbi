//! Self-update check and install commands.

use crate::dto::UpdateCheckResult;
use crate::error::IpcError;
use tauri_plugin_updater::Error as UpdaterError;
use tauri_plugin_updater::UpdaterExt;

/// Checks the configured update endpoint for a newer version.
///
/// The updater plugin is registered when the build embeds an updater public key in
/// `tauri.conf.json`; the key is public, so every build (including local builds) can check the
/// same signed release channel.
#[tauri::command]
pub(crate) async fn check_for_update<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<UpdateCheckResult, IpcError> {
    let current_version = app.package_info().version.to_string();
    let updater = app.updater().map_err(|_| IpcError {
        code: "update_unavailable",
        message: "更新服务未初始化。",
    })?;
    let update = updater.check().await.map_err(map_update_error)?;
    Ok(match update {
        Some(update) => UpdateCheckResult {
            available: true,
            version: Some(update.version.to_string()),
            current_version,
            installed: false,
        },
        None => UpdateCheckResult {
            available: false,
            version: None,
            current_version,
            installed: false,
        },
    })
}

/// Downloads and installs the pending update, then restarts the application.
#[tauri::command]
pub(crate) async fn apply_update<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), IpcError> {
    let updater = app.updater().map_err(|_| IpcError {
        code: "update_unavailable",
        message: "更新服务未初始化。",
    })?;
    let update = updater
        .check()
        .await
        .map_err(map_update_error)?
        .ok_or(IpcError {
            code: "update_unavailable",
            message: "当前已是最新版本。",
        })?;
    update
        .download_and_install(|_bytes, _total| {}, || {})
        .await
        .map_err(map_install_error)?;
    app.request_restart();
    Ok(())
}

fn map_update_error(error: UpdaterError) -> IpcError {
    match error {
        UpdaterError::EmptyEndpoints => IpcError {
            code: "update_unconfigured",
            message: "更新通道未配置，请使用带更新公钥的正式构建。",
        },
        UpdaterError::ReleaseNotFound => IpcError {
            code: "update_manifest_missing",
            message: "更新清单不存在；发布新版本后再试。",
        },
        UpdaterError::TargetNotFound(_) | UpdaterError::TargetsNotFound(_) => IpcError {
            code: "update_platform_missing",
            message: "新版本没有提供当前平台的安装包。",
        },
        UpdaterError::Reqwest(_) | UpdaterError::Network(_) => IpcError {
            code: "update_network",
            message: "无法连接更新服务器，请检查网络或代理设置。",
        },
        UpdaterError::Minisign(_) | UpdaterError::Base64(_) | UpdaterError::SignatureUtf8(_) => {
            IpcError {
                code: "update_signature_invalid",
                message: "更新包签名校验失败，已拒绝安装。",
            }
        }
        UpdaterError::UnsupportedOs | UpdaterError::UnsupportedArch => IpcError {
            code: "update_unsupported",
            message: "当前系统或架构不支持在线更新。",
        },
        UpdaterError::Semver(_) | UpdaterError::Serialization(_) | UpdaterError::UrlParse(_) => {
            IpcError {
                code: "update_manifest_invalid",
                message: "更新清单格式不正确。",
            }
        }
        _ => IpcError {
            code: "update_check_failed",
            message: "检查更新失败，请稍后重试。",
        },
    }
}

fn map_install_error(error: UpdaterError) -> IpcError {
    match error {
        UpdaterError::Minisign(_) | UpdaterError::Base64(_) | UpdaterError::SignatureUtf8(_) => {
            IpcError {
                code: "update_signature_invalid",
                message: "更新包签名校验失败，已拒绝安装。",
            }
        }
        UpdaterError::Reqwest(_) | UpdaterError::Network(_) => IpcError {
            code: "update_network",
            message: "更新包下载失败，请检查网络后重试。",
        },
        _ => IpcError {
            code: "update_install_failed",
            message: "更新下载或安装失败，请稍后重试。",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{map_install_error, map_update_error};
    use tauri_plugin_updater::Error as UpdaterError;

    #[test]
    fn update_errors_become_actionable_codes() {
        assert_eq!(
            map_update_error(UpdaterError::EmptyEndpoints).code,
            "update_unconfigured"
        );
        assert_eq!(
            map_update_error(UpdaterError::ReleaseNotFound).code,
            "update_manifest_missing"
        );
        assert_eq!(
            map_update_error(UpdaterError::TargetNotFound("windows-x86_64".to_owned())).code,
            "update_platform_missing"
        );
        assert_eq!(
            map_update_error(UpdaterError::Network("offline".to_owned())).code,
            "update_network"
        );
        assert_eq!(
            map_update_error(UpdaterError::UnsupportedOs).code,
            "update_unsupported"
        );
    }

    #[test]
    fn install_errors_keep_signature_failures_distinct() {
        assert_eq!(
            map_install_error(UpdaterError::Network("offline".to_owned())).code,
            "update_network"
        );
        assert_eq!(
            map_install_error(UpdaterError::SignatureUtf8("bad".to_owned())).code,
            "update_signature_invalid"
        );
    }
}
