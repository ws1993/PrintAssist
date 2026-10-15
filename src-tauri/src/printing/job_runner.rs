use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::contracts::{
    PrintBatchRequest, PrintBatchResult, PrintBatchResultItem, PrintQueueItemPayload,
};
use crate::documents::office::convert_office_to_pdf;
use crate::documents::pdf_pages::extract_pages_to_temp_pdf;
#[cfg(windows)]
use crate::documents::print_shell::print_file_to_printer;
use crate::documents::{detect_document_kind, DocumentKind, PrintSides};
use crate::printers;

/// Shell `printto` / staged PDF handlers may open files asynchronously.
/// Keep temp PDFs alive long enough for those fallbacks.
const TEMP_PRINT_FILE_RETENTION: Duration = Duration::from_secs(180);

pub fn run_print_batch_sync(request: PrintBatchRequest) -> PrintBatchResult {
    let mut results = Vec::new();
    let mut succeeded = 0_u32;
    let mut failed = 0_u32;
    let mut skipped = 0_u32;

    for item in request.items {
        let result_item = print_single_item(item);
        match result_item.status.as_str() {
            "succeeded" => succeeded += 1,
            "skipped" => skipped += 1,
            _ => failed += 1,
        }
        results.push(result_item);
    }

    PrintBatchResult {
        succeeded,
        failed,
        skipped,
        results,
    }
}

fn print_single_item(item: PrintQueueItemPayload) -> PrintBatchResultItem {
    let path = Path::new(&item.path);
    if !path.exists() {
        return failed_item(item, "文件不存在");
    }

    if item.settings.printer_name.trim().is_empty() {
        return failed_item(item, "未指定打印机");
    }

    if let Err(error) = validate_printer_capabilities(&item) {
        return failed_item(item, &error);
    }

    let kind = detect_document_kind(path);
    if kind == DocumentKind::Unknown {
        return failed_item(item, "不支持的文件类型");
    }

    if item.settings.page_range_mode == "custom"
        && item.settings.page_range_expression.trim().is_empty()
    {
        return failed_item(item, "自定义页码表达式为空");
    }

    let custom_range = item.settings.page_range_mode == "custom";

    // Image/text do not define multi-page custom ranges.
    if custom_range && matches!(kind, DocumentKind::Image | DocumentKind::Text) {
        return failed_item(item, "图片/文本不支持自定义页码范围，请使用全部页");
    }

    let mut temporary_paths: Vec<PathBuf> = Vec::new();

    #[cfg(windows)]
    let print_result = print_item_windows(&item, path, kind, custom_range, &mut temporary_paths);

    #[cfg(not(windows))]
    let print_result: Result<(), String> = {
        let _ = (path, kind, custom_range, &mut temporary_paths);
        Err("当前平台不支持打印".to_string())
    };

    // Keep staged files around for async association fallbacks.
    schedule_temporary_cleanup(temporary_paths);

    match print_result {
        Ok(()) => PrintBatchResultItem {
            queue_item_id: item.queue_item_id,
            path: item.path,
            file_name: item.file_name,
            status: "succeeded".to_string(),
            message: None,
        },
        Err(error) => failed_item(item, &error),
    }
}

/// Map UI sides/flip settings onto the DEVMODE duplex intent.
/// 双面长边 → long-edge flip (DMDUP_VERTICAL)，双面短边 → short-edge flip.
fn resolve_print_sides(sides_mode: &str, flip_mode: &str) -> PrintSides {
    if sides_mode == "duplex" {
        if flip_mode == "shortEdge" {
            PrintSides::DuplexShortEdge
        } else {
            PrintSides::DuplexLongEdge
        }
    } else {
        PrintSides::Simplex
    }
}

#[cfg(windows)]
fn print_item_windows(
    item: &PrintQueueItemPayload,
    path: &Path,
    kind: DocumentKind,
    custom_range: bool,
    temporary_paths: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let printer = item.settings.printer_name.as_str();
    let copies = item.settings.copies.max(1);
    let sides = resolve_print_sides(&item.settings.sides_mode, &item.settings.flip_mode);

    match kind {
        DocumentKind::Image => {
            crate::documents::image_print::print_image_to_printer(path, printer, copies, sides)
        }

        DocumentKind::Pdf => {
            let pdf_path = if custom_range {
                let ranged = extract_pages_to_temp_pdf(path, &item.settings.page_range_expression)?;
                temporary_paths.push(ranged.clone());
                ranged
            } else {
                path.to_path_buf()
            };
            print_pdf_preserving_orientation(&pdf_path, printer, copies, sides)
        }

        DocumentKind::Word => {
            // COM PrintOut cannot request duplex; duplex jobs must go through
            // the PDF path where the DEVMODE dmDuplex field is applied.
            if sides != PrintSides::Simplex {
                return fallback_office_via_pdf(
                    item,
                    path,
                    kind,
                    custom_range,
                    temporary_paths,
                    sides,
                    "Word 双面打印需要 PDF 路径",
                );
            }
            // Word COM PrintOut honors section PageSetup orientation.
            match crate::documents::office_print::print_office_to_printer(
                path,
                kind,
                printer,
                copies,
                item.settings.page_range_mode.as_str(),
                item.settings.page_range_expression.as_str(),
            ) {
                Ok(()) => Ok(()),
                Err(office_error) => {
                    // Fallback: convert to PDF then native-print (still orientation-safe).
                    fallback_office_via_pdf(
                        item,
                        path,
                        kind,
                        custom_range,
                        temporary_paths,
                        sides,
                        &office_error,
                    )
                }
            }
        }

        DocumentKind::Excel | DocumentKind::PowerPoint => {
            if custom_range || sides != PrintSides::Simplex {
                // Custom ranges and duplex both require the PDF path.
                let reason = if custom_range {
                    "自定义页码"
                } else {
                    "Excel/PPT 双面打印需要 PDF 路径"
                };
                return fallback_office_via_pdf(
                    item,
                    path,
                    kind,
                    custom_range,
                    temporary_paths,
                    sides,
                    reason,
                );
            }

            match crate::documents::office_print::print_office_to_printer(
                path, kind, printer, copies, "all", "",
            ) {
                Ok(()) => Ok(()),
                Err(office_error) => fallback_office_via_pdf(
                    item,
                    path,
                    kind,
                    false,
                    temporary_paths,
                    sides,
                    &office_error,
                ),
            }
        }

        DocumentKind::Text => {
            // Text still uses association printto; the DEVMODE is owned by the
            // association handler, so duplex cannot be enforced here.
            print_file_to_printer(path, printer, copies)
        }

        DocumentKind::Unknown => Err("不支持的文件类型".to_string()),
    }
}

/// Prefer WinRT/GDI-style PDF printing that keeps landscape pages landscape.
/// Do not silently fall back to shell `printto`: association handlers often
/// re-layout landscape pages and reintroduce the 90° content rotation bug.
#[cfg(windows)]
fn print_pdf_preserving_orientation(
    pdf_path: &Path,
    printer: &str,
    copies: u32,
    sides: PrintSides,
) -> Result<(), String> {
    crate::documents::pdf_print::print_pdf_to_printer(pdf_path, printer, copies, sides)
}

#[cfg(windows)]
fn fallback_office_via_pdf(
    item: &PrintQueueItemPayload,
    path: &Path,
    kind: DocumentKind,
    custom_range: bool,
    temporary_paths: &mut Vec<PathBuf>,
    sides: PrintSides,
    prior_error: &str,
) -> Result<(), String> {
    let printer = item.settings.printer_name.as_str();
    let copies = item.settings.copies.max(1);

    let pdf_path = match convert_office_to_pdf(path, kind) {
        Ok(pdf) => {
            temporary_paths.push(pdf.clone());
            pdf
        }
        Err(convert_error) => {
            if sides != PrintSides::Simplex {
                // Duplex cannot be honored through association handlers;
                // fail loudly instead of silently printing single-sided.
                return Err(format!(
                    "{prior_error}；Office 转 PDF 失败：{convert_error}；双面打印无法使用关联程序回退"
                ));
            }
            if !item.allow_association_fallback {
                return Err(format!(
                    "{prior_error}；Office 转 PDF 也失败：{convert_error}"
                ));
            }
            if custom_range {
                return Err(format!(
                    "{prior_error}；{convert_error}；自定义页码需要 Office 转 PDF，无法使用关联程序回退"
                ));
            }
            // Association fallback cannot prove orientation fidelity.
            return print_file_to_printer(path, printer, copies).map_err(|shell_error| {
                format!(
                    "{prior_error}；Office 转 PDF 失败：{convert_error}；关联程序回退：{shell_error}"
                )
            });
        }
    };

    let pdf_path = if custom_range {
        match extract_pages_to_temp_pdf(&pdf_path, &item.settings.page_range_expression) {
            Ok(ranged) => {
                temporary_paths.push(ranged.clone());
                ranged
            }
            Err(error) => return Err(error),
        }
    } else {
        pdf_path
    };

    print_pdf_preserving_orientation(&pdf_path, printer, copies, sides)
}

fn validate_printer_capabilities(item: &PrintQueueItemPayload) -> Result<(), String> {
    let printers = printers::list_system_printers_sync()?;
    let printer = printers
        .into_iter()
        .find(|candidate| {
            candidate
                .name
                .eq_ignore_ascii_case(&item.settings.printer_name)
        })
        .ok_or_else(|| format!("找不到打印机：{}", item.settings.printer_name))?;

    if printer.state == crate::contracts::PrinterOperationalState::Offline {
        return Err("打印机离线".to_string());
    }
    if printer.state == crate::contracts::PrinterOperationalState::Error {
        return Err("打印机处于错误状态".to_string());
    }

    if item.settings.color_mode == "color"
        && printer.color.support == crate::contracts::CapabilitySupport::Unsupported
    {
        return Err("当前打印机不支持彩色，已阻止静默降级".to_string());
    }

    if item.settings.sides_mode == "duplex"
        && printer.duplex.support == crate::contracts::CapabilitySupport::Unsupported
    {
        return Err("当前打印机不支持双面，已阻止静默降级".to_string());
    }

    Ok(())
}

fn failed_item(item: PrintQueueItemPayload, message: &str) -> PrintBatchResultItem {
    PrintBatchResultItem {
        queue_item_id: item.queue_item_id,
        path: item.path,
        file_name: item.file_name,
        status: "failed".to_string(),
        message: Some(message.to_string()),
    }
}

fn cleanup_temporary_paths(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

fn schedule_temporary_cleanup(paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    thread::spawn(move || {
        thread::sleep(TEMP_PRINT_FILE_RETENTION);
        cleanup_temporary_paths(&paths);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sides_and_flip_map_to_duplex_intent() {
        assert_eq!(
            resolve_print_sides("duplex", "longEdge"),
            PrintSides::DuplexLongEdge
        );
        assert_eq!(
            resolve_print_sides("duplex", "shortEdge"),
            PrintSides::DuplexShortEdge
        );
        assert_eq!(
            resolve_print_sides("simplex", "longEdge"),
            PrintSides::Simplex
        );
        assert_eq!(resolve_print_sides("", ""), PrintSides::Simplex);
    }
}
