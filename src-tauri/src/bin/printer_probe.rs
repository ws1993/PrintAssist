use std::process::ExitCode;

fn main() -> ExitCode {
    match print_assist_lib::printers::list_system_printers_sync() {
        Ok(printers) => match serde_json::to_string_pretty(&printers) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("failed to serialize printer probe result: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("printer probe failed: {error}");
            ExitCode::FAILURE
        }
    }
}
