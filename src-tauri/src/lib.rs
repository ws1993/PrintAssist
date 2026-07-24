pub mod commands;
pub mod contracts;
pub mod documents;
pub mod ingress;
pub mod printers;
pub mod printing;

use crate::ingress::collect_launch_paths;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let paths = collect_launch_paths(&args);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
            if !paths.is_empty() {
                let _ = app.emit("files-added", paths);
            }
        }));
    }

    builder
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let launch_paths = collect_launch_paths(&std::env::args().collect::<Vec<_>>());
            if !launch_paths.is_empty() {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                    let _ = handle.emit("files-added", launch_paths);
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_system_printers,
            commands::pick_files,
            commands::pick_folder_files,
            commands::expand_file_paths,
            commands::run_print_batch,
            commands::check_for_app_update,
            commands::download_and_install_update,
            commands::open_release_page,
            commands::validate_supported_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running PrintAssist");
}
