use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;

use crate::contracts::{PrintBatchRequest, PrintBatchResult, ProxyConfig, SystemPrinter, UpdateCheckResult};
use crate::ingress::{collect_path_argument, is_supported_file};
use crate::printers;
use crate::printing::run_print_batch_sync;

const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/ws1993/PrintAssist/releases/latest";
const GITHUB_RELEASES_PAGE: &str = "https://github.com/ws1993/PrintAssist/releases/latest";

/// Launch the downloaded NSIS installer so it survives app exit.
///
/// On Windows the installer must:
/// 1. start after PrintAssist.exe unlocks (wait for this process to exit),
/// 2. run outside the Tauri/WebView2 process tree so `app.exit` does not kill it.
fn launch_nsis_installer(installer_path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        if !installer_path.is_file() {
            return Err(format!("安装包不存在：{}", installer_path.display()));
        }

        let file_size = std::fs::metadata(installer_path)
            .map_err(|error| format!("读取安装包失败：{error}"))?
            .len();
        if file_size < 1024 {
            return Err("安装包文件过小，下载可能不完整".to_string());
        }

        // Normalize to a normal Win32 path (strip \\?\ prefix from canonicalize).
        let absolute_installer = installer_path
            .canonicalize()
            .unwrap_or_else(|_| installer_path.to_path_buf());
        let installer = absolute_installer
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('\'', "''")
            .replace('"', "")
            .to_string();

        let helper_dir = std::env::temp_dir().join("PrintAssist_Update");
        std::fs::create_dir_all(&helper_dir)
            .map_err(|error| format!("创建更新目录失败：{error}"))?;

        // Helper waits until THIS process exits (file unlock), then starts NSIS.
        // Launched via ShellExecute so it is owned by the shell, not Tauri's job object.
        // Installer runs with UI (not /S) so failures are visible instead of "nothing happens".
        let app_process_id = std::process::id();
        let helper_script_path = helper_dir.join("launch_installer.ps1");
        let log_path = helper_dir
            .join("install.log")
            .to_string_lossy()
            .replace('\'', "''")
            .to_string();
        let script_content = format!(
            "$ErrorActionPreference = 'Continue'\r\n\
             $log = '{log_path}'\r\n\
             function Write-UpdateLog([string]$message) {{\r\n\
               $line = (Get-Date -Format o) + ' ' + $message\r\n\
               Add-Content -LiteralPath $log -Value $line -Encoding UTF8\r\n\
             }}\r\n\
             Write-UpdateLog 'helper started; waiting for pid {app_process_id}'\r\n\
             $deadline = (Get-Date).AddSeconds(90)\r\n\
             while ((Get-Date) -lt $deadline) {{\r\n\
               if (-not (Get-Process -Id {app_process_id} -ErrorAction SilentlyContinue)) {{ break }}\r\n\
               Start-Sleep -Milliseconds 250\r\n\
             }}\r\n\
             if (Get-Process -Id {app_process_id} -ErrorAction SilentlyContinue) {{\r\n\
               Write-UpdateLog 'timeout waiting for app exit; launching installer anyway'\r\n\
             }} else {{\r\n\
               Write-UpdateLog 'app exited'\r\n\
             }}\r\n\
             Start-Sleep -Milliseconds 800\r\n\
             $installer = '{installer}'\r\n\
             if (-not (Test-Path -LiteralPath $installer)) {{\r\n\
               Write-UpdateLog \"installer missing: $installer\"\r\n\
               exit 2\r\n\
             }}\r\n\
             Write-UpdateLog \"starting installer: $installer\"\r\n\
             try {{\r\n\
               $process = Start-Process -FilePath $installer -PassThru\r\n\
               Write-UpdateLog (\"installer pid=\" + $process.Id)\r\n\
             }} catch {{\r\n\
               Write-UpdateLog (\"Start-Process failed: \" + $_.Exception.Message)\r\n\
               exit 3\r\n\
             }}\r\n"
        );
        std::fs::write(&helper_script_path, script_content)
            .map_err(|error| format!("写入安装启动脚本失败：{error}"))?;

        let script_path = helper_script_path
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('"', "")
            .to_string();

        shell_execute_open(
            "powershell.exe",
            &format!(
                "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File \"{script_path}\""
            ),
        )?;

        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = installer_path;
        Err("当前平台不支持应用内安装".to_string())
    }
}

/// Start a process through the Windows shell so it is not tied to Tauri's process tree.
#[cfg(windows)]
fn shell_execute_open(file: &str, parameters: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    fn to_wide(value: &str) -> Vec<u16> {
        OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    let operation = to_wide("open");
    let file_wide = to_wide(file);
    let parameters_wide = to_wide(parameters);

    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file_wide.as_ptr()),
            PCWSTR(parameters_wide.as_ptr()),
            PCWSTR::null(),
            SW_HIDE,
        )
    };

    // ShellExecute returns > 32 on success.
    if result.0 as usize <= 32 {
        return Err(format!(
            "启动安装助手失败，ShellExecute 返回码 {}",
            result.0 as usize
        ));
    }

    Ok(())
}

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
            download_url: None,
            download_size: None,
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

    // Find the NSIS installer asset URL and size
    let mut download_url: Option<String> = None;
    let mut download_size: Option<u64> = None;
    if let Some(assets) = payload.get("assets").and_then(|v| v.as_array()) {
        for asset in assets {
            if let Some(name) = asset.get("name").and_then(|v| v.as_str()) {
                if name.ends_with("-setup.exe") || name.ends_with("_x64-setup.exe") {
                    download_url = asset
                        .get("browser_download_url")
                        .and_then(|v| v.as_str())
                        .map(|v| v.to_string());
                    download_size = asset.get("size").and_then(|v| v.as_u64());
                    break;
                }
            }
        }
    }

    Ok(UpdateCheckResult {
        available,
        version: if remote_tag.is_empty() {
            None
        } else {
            Some(remote_tag)
        },
        body,
        download_url,
        download_size,
    })
}

/// Download the update installer and execute it.
/// Emits progress events: "update-download-progress" with { percent, downloaded, total }
/// Emits completion: "update-download-complete" with { path }
/// Emits error: "update-download-error" with { message }
#[tauri::command]
pub async fn download_and_install_update(
    app: AppHandle,
    download_url: String,
    proxy: Option<ProxyConfig>,
) -> Result<String, String> {
    use futures_util::StreamExt;
    use std::fs::File;
    use std::io::Write;

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
                client_builder = client_builder.no_proxy();
            }
        }
    }

    let client = client_builder
        .build()
        .map_err(|error| format!("创建 HTTP 客户端失败：{error}"))?;

    let response = client
        .get(&download_url)
        .send()
        .await
        .map_err(|error| format!("下载更新失败：{error}"))?;

    if !response.status().is_success() {
        return Err(format!("下载返回状态 {}", response.status()));
    }

    let total_size = response.content_length().unwrap_or(0);

    // Determine filename from URL
    let filename = download_url
        .rsplit('/')
        .next()
        .unwrap_or("update.exe")
        .to_string();

    // Save to temp directory
    let temp_dir = std::env::temp_dir().join("PrintAssist_Update");
    let _ = std::fs::create_dir_all(&temp_dir);
    let file_path = temp_dir.join(&filename);

    let mut file = File::create(&file_path)
        .map_err(|error| format!("创建临时文件失败：{error}"))?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|error| format!("下载数据失败：{error}"))?;
        file.write_all(&chunk)
            .map_err(|error| format!("写入文件失败：{error}"))?;
        downloaded += chunk.len() as u64;

        let percent = if total_size > 0 {
            ((downloaded as f64 / total_size as f64) * 100.0) as u32
        } else {
            0
        };

        let _ = app.emit("update-download-progress", serde_json::json!({
            "percent": percent,
            "downloaded": downloaded,
            "total": total_size,
        }));
    }

    file.flush().map_err(|error| format!("刷新文件失败：{error}"))?;
    drop(file);

    let _ = app.emit("update-download-complete", serde_json::json!({
        "path": file_path.to_string_lossy().to_string(),
    }));

    if total_size > 0 && downloaded < total_size {
        return Err(format!(
            "下载不完整：已下载 {downloaded} / {total_size} 字节"
        ));
    }

    // Must succeed before exiting; otherwise the app would close with no installer.
    launch_nsis_installer(&file_path)?;

    // Helper is waiting for this process to exit, then starts the installer UI.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    app.exit(0);

    Ok(file_path.to_string_lossy().to_string())
}

/// Open the GitHub releases page in the default browser as a fallback.
#[tauri::command]
pub async fn open_release_page() -> Result<(), String> {
    open::that(GITHUB_RELEASES_PAGE)
        .map_err(|error| format!("打开下载页失败：{error}"))
}

#[tauri::command]
pub fn validate_supported_path(path: String) -> bool {
    is_supported_file(PathBuf::from(path).as_path())
}
