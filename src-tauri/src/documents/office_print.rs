//! Office COM direct printing that keeps each document's own page orientation.
//!
//! Uses in-process IDispatch automation (no PowerShell window). Word/Excel/
//! PowerPoint PrintOut honors document PageSetup / slide size.

use std::path::Path;

use windows::core::VARIANT;

use super::office_com::{
    absolute_path_text, set_active_printer, ComApartment, DispatchObject,
};
use super::DocumentKind;

/// Prints an Office document via desktop Office COM automation.
///
/// `page_range_expression` is only applied for Word custom ranges (e.g. `1,3-5`).
/// Excel/PowerPoint custom ranges should still go through the PDF path.
pub fn print_office_to_printer(
    path: &Path,
    kind: DocumentKind,
    printer_name: &str,
    copies: u32,
    page_range_mode: &str,
    page_range_expression: &str,
) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("文件不存在：{}", path.display()));
    }
    if printer_name.trim().is_empty() {
        return Err("打印机名称不能为空".to_string());
    }

    let copy_count = copies.max(1);
    let custom_pages = page_range_mode == "custom";
    if custom_pages && page_range_expression.trim().is_empty() {
        return Err("自定义页码表达式为空".to_string());
    }

    // Excel/PowerPoint page-range selection is more reliable after PDF extraction.
    if custom_pages && matches!(kind, DocumentKind::Excel | DocumentKind::PowerPoint) {
        return Err("Excel/PowerPoint 自定义页码请走 PDF 路径".to_string());
    }

    let _apartment = ComApartment::enter()?;
    let absolute_input = absolute_path_text(path)?;

    match kind {
        DocumentKind::Word => print_word(
            &absolute_input,
            printer_name,
            copy_count,
            custom_pages,
            page_range_expression,
        ),
        DocumentKind::Excel => print_excel(&absolute_input, printer_name, copy_count),
        DocumentKind::PowerPoint => print_powerpoint(&absolute_input, printer_name, copy_count),
        _ => Err("不是 Office 文档".to_string()),
    }
}

fn print_word(
    absolute_input: &str,
    printer_name: &str,
    copies: u32,
    custom_pages: bool,
    page_range_expression: &str,
) -> Result<(), String> {
    // wdPrintAllDocument = 0, wdPrintRangeOfPages = 4
    let range_code: i32 = if custom_pages { 4 } else { 0 };
    let pages_text = if custom_pages {
        page_range_expression.trim().to_string()
    } else {
        String::new()
    };

    let application = DispatchObject::create_application("Word.Application")?;
    let result = (|| {
        let _ = application.put_bool_property("Visible", false);
        let _ = application.put_i32_property("DisplayAlerts", 0);

        // Prefer ActivePrinter; WordBasic.FilePrintSetup is harder via IDispatch.
        set_active_printer(&application, printer_name)?;

        let documents = application.get_object_property("Documents")?;
        // Open(FileName, ConfirmConversions=false, ReadOnly=true)
        let document_variant = documents.call(
            "Open",
            vec![
                VARIANT::from(absolute_input),
                VARIANT::from(false),
                VARIANT::from(true),
            ],
        )?;
        let document = DispatchObject::from_variant(&document_variant)?;

        let print_result = (|| {
            // PrintOut(Background, Append, Range, OutputFileName, From, To, Item,
            //          Copies, Pages, PageType, PrintToFile, Collate)
            document.call_unit(
                "PrintOut",
                vec![
                    VARIANT::from(false),          // Background
                    VARIANT::from(false),          // Append
                    VARIANT::from(range_code),     // Range
                    VARIANT::from(""),             // OutputFileName
                    VARIANT::from(""),             // From
                    VARIANT::from(""),             // To
                    VARIANT::from(0_i32),          // Item
                    VARIANT::from(copies as i32),  // Copies
                    VARIANT::from(pages_text.as_str()), // Pages
                    VARIANT::from(0_i32),          // PageType
                    VARIANT::from(false),          // PrintToFile
                    VARIANT::from(true),           // Collate
                ],
            )
        })();

        // wdDoNotSaveChanges = 0
        let _ = document.call_unit("Close", vec![VARIANT::from(0_i32)]);
        print_result
    })();

    let _ = application.call_unit("Quit", vec![VARIANT::from(0_i32)]);
    result.map_err(|error| format!("Word 直接打印失败：{error}"))
}

fn print_excel(absolute_input: &str, printer_name: &str, copies: u32) -> Result<(), String> {
    let application = DispatchObject::create_application("Excel.Application")?;
    let result = (|| {
        let _ = application.put_bool_property("Visible", false);
        let _ = application.put_bool_property("DisplayAlerts", false);

        set_active_printer(&application, printer_name)?;

        let workbooks = application.get_object_property("Workbooks")?;
        // Open(Filename, UpdateLinks=0, ReadOnly=true)
        let workbook_variant = workbooks.call(
            "Open",
            vec![
                VARIANT::from(absolute_input),
                VARIANT::from(0_i32),
                VARIANT::from(true),
            ],
        )?;
        let workbook = DispatchObject::from_variant(&workbook_variant)?;

        let print_result = (|| {
            // PrintOut(From, To, Copies, ...) — empty optional From/To.
            workbook.call_unit(
                "PrintOut",
                vec![
                    VARIANT::new(),
                    VARIANT::new(),
                    VARIANT::from(copies as i32),
                ],
            )
        })();

        let _ = workbook.call_unit("Close", vec![VARIANT::from(false)]);
        print_result
    })();

    let _ = application.call_unit("Quit", Vec::new());
    result.map_err(|error| format!("Excel 直接打印失败：{error}"))
}

fn print_powerpoint(absolute_input: &str, printer_name: &str, copies: u32) -> Result<(), String> {
    let application = DispatchObject::create_application("PowerPoint.Application")?;
    let result = (|| {
        // WithWindow=false keeps UI quiet; Visible is optional / version-dependent.
        let _ = application.put_i32_property("Visible", 0);

        let presentations = application.get_object_property("Presentations")?;
        // Open(FileName, ReadOnly=true, Untitled=false, WithWindow=false)
        let presentation_variant = presentations.call(
            "Open",
            vec![
                VARIANT::from(absolute_input),
                VARIANT::from(true),
                VARIANT::from(false),
                VARIANT::from(false),
            ],
        )?;
        let presentation = DispatchObject::from_variant(&presentation_variant)?;

        let print_result = (|| {
            let print_options = presentation.get_object_property("PrintOptions")?;
            print_options.put_string_property("ActivePrinter", printer_name)?;
            // FitToPage=false keeps slide aspect; OutputType=1 prints slides as authored.
            print_options.put_bool_property("FitToPage", false)?;
            print_options.put_i32_property("OutputType", 1)?;
            print_options.put_i32_property("NumberOfCopies", copies as i32)?;
            presentation.call_unit("PrintOut", Vec::new())
        })();

        let _ = presentation.call_unit("Close", Vec::new());
        print_result
    })();

    let _ = application.call_unit("Quit", Vec::new());
    result.map_err(|error| format!("PowerPoint 直接打印失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_office_file() {
        let result = print_office_to_printer(
            Path::new("C:\\\\this-file-should-not-exist-printassist.docx"),
            DocumentKind::Word,
            "Microsoft Print to PDF",
            1,
            "all",
            "",
        );
        assert!(result.is_err());
    }

    #[test]
    fn excel_custom_range_is_rejected_for_direct_path() {
        let path = std::env::temp_dir().join(format!(
            "printassist-excel-range-test-{}.xlsx",
            std::process::id()
        ));
        std::fs::write(&path, b"dummy").expect("write temp xlsx stub");
        let result = print_office_to_printer(
            &path,
            DocumentKind::Excel,
            "Microsoft Print to PDF",
            1,
            "custom",
            "1-2",
        );
        let _ = std::fs::remove_file(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("PDF"));
    }
}
