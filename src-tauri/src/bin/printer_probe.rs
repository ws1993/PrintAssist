use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    #[cfg(windows)]
    if args.first().map(|arg| arg.as_str()) == Some("duplex") {
        return run_duplex_probe(&args[1..]);
    }

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

/// Usage: printer_probe duplex ["printer name"] [simplex|long|short]
///
/// Reports whether the printer driver keeps the requested `dmDuplex` value
/// after validating the job DEVMODE. Omit the printer name to use the
/// system default printer.
#[cfg(windows)]
fn run_duplex_probe(args: &[String]) -> ExitCode {
    use print_assist_lib::documents::image_print::probe_duplex_devmode;
    use print_assist_lib::documents::PrintSides;

    let sides = match args.get(1).map(|value| value.as_str()) {
        Some("simplex") => PrintSides::Simplex,
        Some("short") => PrintSides::DuplexShortEdge,
        Some("long") | None => PrintSides::DuplexLongEdge,
        Some(other) => {
            eprintln!("未知双面模式：{other}（可用：simplex | long | short）");
            return ExitCode::FAILURE;
        }
    };

    let printer = match args.first() {
        Some(name) => name.clone(),
        None => match print_assist_lib::printers::list_system_printers_sync() {
            Ok(printers) => match printers.into_iter().find(|printer| printer.is_default) {
                Some(printer) => printer.name,
                None => {
                    eprintln!("未找到系统默认打印机，请显式指定打印机名称");
                    return ExitCode::FAILURE;
                }
            },
            Err(error) => {
                eprintln!("枚举打印机失败：{error}");
                return ExitCode::FAILURE;
            }
        },
    };

    match probe_duplex_devmode(&printer, sides) {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(json) => {
                println!("{json}");
                if report.driver_honored_request {
                    println!("✅ 驱动接受双面设置，打印后应输出双面。");
                    ExitCode::SUCCESS
                } else {
                    println!(
                        "❌ 驱动把 dmDuplex 从 {} 改回了 {}，会静默降级为单面。",
                        report.requested_duplex, report.validated_duplex
                    );
                    ExitCode::FAILURE
                }
            }
            Err(error) => {
                eprintln!("序列化探针结果失败：{error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("双面 DEVMODE 探针失败：{error}");
            ExitCode::FAILURE
        }
    }
}
