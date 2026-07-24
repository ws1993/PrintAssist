use std::path::PathBuf;

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::contracts::{PrintBatchRequest, PrintBatchResult, ProxyConfig, SystemPrinter, UpdateCheckResult};
use crate::ingress::{collect_path_argument, is_supported_file};
use crate::printers;
use crate::printing::run_print_batch_sync;

const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/ws1993/PrintAssist/releases/latest";

#[tauri::command]
pub async fn list_system_printers() -> Result<Vec<SystemPrinter>, String> {
    tauri::async_runtime::spawn_blocking(printers::list_system_printers_sync)
        .await
        .map_err(|error| format!("printer discovery task failed: {error}"))?
}

#[tauri::command]
pub async fn pick_files(app: AppHandle) -> Result<Vec<String>, String> {
    let files = app
        .dialog()
        .file()
        .add_filter(
            "可打印文件",
            &[
                "pdf", // images
                "png", "jpg", "jpeg", "jpe", "jfif", "bmp", "dib", "tif", "tiff", "gif", "webp",
                "ico", "heic", "heif", "avif", "emf", "wmf", // text / office
                "txt", "log", "md", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
            ],
        )
        .blocking_pick_files();

    Ok(files
        .unwrap_or_default()
        .into_iter()
        .filter_map(|file_path| file_path.into_path().ok())
        .map(|path| path.to_string_lossy().to_string())
        .collect())
}

#[tauri::command]
pub async fn pick_folder_files(app: AppHandle) -> Result<Vec<String>, String> {
    let folder = app.dialog().file().blocking_pick_folder();
    let Some(folder_path) = folder.and_then(|path| path.into_path().ok()) else {
        return Ok(Vec::new());
    };

    let mut paths = Vec::new();
    collect_path_argument(&folder_path.to_string_lossy(), &mut paths);
    Ok(paths)
}

/// Expand dropped file/folder paths into supported printable file paths.
#[tauri::command]
pub async fn expand_file_paths(paths: Vec<String>) -> Result<Vec<String>, String> {
    let mut expanded = Vec::new();
    for path in paths {
        collect_path_argument(&path, &mut expanded);
    }
    Ok(expanded)
}

#[tauri::command]
pub async fn run_print_batch(request: PrintBatchRequest) -> Result<PrintBatchResult, String> {
    tauri::async_runtime::spawn_blocking(move || run_print_batch_sync(request))
        .await
        .map_err(|error| format!("print batch task failed: {error}"))
}

#[tauri::command]
pub async fn check_for_app_update(
    proxy: Option<ProxyConfig>,
) -> Result<UpdateCheckResult, String> {
    let mut client_builder = reqwest::Client::builder()
        .user_agent("PrintAssist-Updater");

    if let Some(ref proxy_config) = proxy {
        if !proxy_config.use_system_proxy {
            if let Some(ref custom_url) = proxy_config.custom_proxy_url {
                let mut proxy = reqwest::Proxy::all(custom_url)
                    .map_err(|error| format!("创建自定义代理失败：{error}"))?;
                match (&proxy_config.username, &proxy_config.password) {
                    (Some(user), Some(pass)) => {
                        proxy = proxy.basic_auth(user.as_str(), pass.as_str());
                    }
                    _ => {}
                }
                client_builder = client_builder.proxy(proxy);
            } else {
                // 无代理模式：不使用任何代理
                client_builder = client_builder.no_proxy();
            }
        }
        // use_system_proxy = true 时，reqwest 默认使用系统代理，无需额外配置
    }

    let client = client_builder
        .build()
        .map_err(|error| format!("创建 HTTP 客户端失败：{error}"))?;

    let response = client
        .get(GITHUB_LATEST_RELEASE_URL)
        .send()
        .await
        .map_err(|error| format!("请求 GitHub Release 失败：{error}"))?;

    if response.status().as_u16() == 404 {
        return Ok(UpdateCheckResult {
            available: false,
            version: None,
            body: Some("尚未发布 GitHub Release".to_string()),
        });
    }

    if !response.status().is_success() {
        return Err(format!("GitHub Release 返回状态 {}", response.status()));
    }

    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("解析 Release JSON 失败：{error}"))?;

    let remote_tag = payload
        .get("tag_name")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim_start_matches('v')
        .to_string();
    let body = payload
        .get("body")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let available = !remote_tag.is_empty() && remote_tag != current_version;

    Ok(UpdateCheckResult {
        available,
        version: if remote_tag.is_empty() {
            None
        } else {
            Some(remote_tag)
        },
        body,
    })
}

#[tauri::command]
pub async fn install_app_update() -> Result<(), String> {
    // First version opens the release page for user-confirmed download/install.
    // Signed in-app installer can replace this after CI signing secrets are configured.
    open::that("https://github.com/ws1993/PrintAssist/releases/latest")
        .map_err(|error| format!("打开下载页失败：{error}"))
}

#[tauri::command]
pub fn validate_supported_path(path: String) -> bool {
    is_supported_file(PathBuf::from(path).as_path())
}
