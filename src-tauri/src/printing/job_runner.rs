use std::fs;
use std::path::Path;

use crate::contracts::{
    PrintBatchRequest, PrintBatchResult, PrintBatchResultItem, PrintQueueItemPayload,
};
use crate::documents::office::convert_office_to_pdf;
#[cfg(windows)]
use crate::documents::print_shell::print_file_to_printer;
use crate::documents::{detect_document_kind, DocumentKind};
use crate::printers;

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

    let mut temporary_pdf: Option<std::path::PathBuf> = None;
    let print_path = match kind {
        DocumentKind::Word | DocumentKind::Excel | DocumentKind::PowerPoint => {
            match convert_office_to_pdf(path, kind) {
                Ok(pdf_path) => {
                    temporary_pdf = Some(pdf_path.clone());
                    pdf_path
                }
                Err(office_error) => {
                    if !item.allow_association_fallback {
                        return failed_item(item, &office_error);
                    }
                    // Association fallback cannot prove full setting fidelity.
                    path.to_path_buf()
                }
            }
        }
        _ => path.to_path_buf(),
    };

    if item.settings.page_range_mode == "custom" {
        // Shell printto cannot guarantee custom page-range fidelity for all associations.
        // Surface an explicit limitation instead of pretending full control.
        if matches!(
            kind,
            DocumentKind::Pdf | DocumentKind::Word | DocumentKind::Excel | DocumentKind::PowerPoint
        ) {
            if let Some(temporary_path) = temporary_pdf {
                let _ = fs::remove_file(temporary_path);
            }
            return failed_item(
                item,
                "当前引擎暂不能可靠保证 PDF/Office 的自定义页码范围；请使用全部页或后续版本的内置渲染路径",
            );
        }
    }

    #[cfg(windows)]
    let print_result = print_file_to_printer(
        &print_path,
        &item.settings.printer_name,
        item.settings.copies.max(1),
    );
    #[cfg(not(windows))]
    let print_result: Result<(), String> = Err("当前平台不支持打印".to_string());

    if let Some(temporary_path) = temporary_pdf {
        let _ = fs::remove_file(temporary_path);
    }

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

fn validate_printer_capabilities(item: &PrintQueueItemPayload) -> Result<(), String> {
    let printers = printers::list_system_printers_sync()?;
    let printer = printers
        .into_iter()
        .find(|candidate| candidate.name.eq_ignore_ascii_case(&item.settings.printer_name))
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
