//! Office document conversion helpers.
//!
//! On Windows, conversion uses in-process Office COM (IDispatch) — no PowerShell
//! host window. Desktop Office must be installed for conversion fidelity.

use std::path::{Path, PathBuf};

use super::DocumentKind;

/// Converts Office documents to a temporary PDF via installed desktop Office.
pub fn convert_office_to_pdf(path: &Path, kind: DocumentKind) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("文件不存在：{}", path.display()));
    }

    match kind {
        DocumentKind::Word | DocumentKind::Excel | DocumentKind::PowerPoint => {}
        _ => return Err("不是 Office 文档".to_string()),
    }

    #[cfg(windows)]
    {
        convert_office_to_pdf_windows(path, kind)
    }

    #[cfg(not(windows))]
    {
        let _ = (path, kind);
        Err("当前平台不支持 Office 转 PDF".to_string())
    }
}

#[cfg(windows)]
fn convert_office_to_pdf_windows(path: &Path, kind: DocumentKind) -> Result<PathBuf, String> {
    use super::office_com::{absolute_path_text, temporary_pdf_output_path, ComApartment};

    let _apartment = ComApartment::enter()?;
    let absolute_input = absolute_path_text(path)?;
    let output_path = temporary_pdf_output_path(path);
    let output_text = output_path
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();

    // Remove a stale file with the same name so existence checks are trustworthy.
    let _ = std::fs::remove_file(&output_path);

    let convert_result = match kind {
        DocumentKind::Word => convert_word_to_pdf(&absolute_input, &output_text),
        DocumentKind::Excel => convert_excel_to_pdf(&absolute_input, &output_text),
        DocumentKind::PowerPoint => convert_powerpoint_to_pdf(&absolute_input, &output_text),
        _ => Err("不是 Office 文档".to_string()),
    };

    convert_result?;

    if !output_path.exists() {
        return Err("Office 转换未生成 PDF 文件".to_string());
    }

    Ok(output_path)
}

#[cfg(windows)]
fn convert_word_to_pdf(absolute_input: &str, output_text: &str) -> Result<(), String> {
    use windows::core::VARIANT;

    use super::office_com::DispatchObject;

    let application = DispatchObject::create_application("Word.Application")?;
    let result = (|| {
        let _ = application.put_bool_property("Visible", false);
        let _ = application.put_i32_property("DisplayAlerts", 0);

        let documents = application.get_object_property("Documents")?;
        let document_variant = documents.call(
            "Open",
            vec![
                VARIANT::from(absolute_input),
                VARIANT::from(false),
                VARIANT::from(true),
            ],
        )?;
        let document = DispatchObject::from_variant(&document_variant)?;

        // ExportAsFixedFormat(OutputFileName, ExportFormat=wdExportFormatPDF=17)
        let export_result = document.call_unit(
            "ExportAsFixedFormat",
            vec![VARIANT::from(output_text), VARIANT::from(17_i32)],
        );

        let _ = document.call_unit("Close", vec![VARIANT::from(0_i32)]);
        export_result
    })();

    let _ = application.call_unit("Quit", vec![VARIANT::from(0_i32)]);
    result.map_err(|error| format!("Word 转 PDF 失败：{error}"))
}

#[cfg(windows)]
fn convert_excel_to_pdf(absolute_input: &str, output_text: &str) -> Result<(), String> {
    use windows::core::VARIANT;

    use super::office_com::DispatchObject;

    let application = DispatchObject::create_application("Excel.Application")?;
    let result = (|| {
        let _ = application.put_bool_property("Visible", false);
        let _ = application.put_bool_property("DisplayAlerts", false);

        let workbooks = application.get_object_property("Workbooks")?;
        let workbook_variant = workbooks.call(
            "Open",
            vec![
                VARIANT::from(absolute_input),
                VARIANT::from(0_i32),
                VARIANT::from(true),
            ],
        )?;
        let workbook = DispatchObject::from_variant(&workbook_variant)?;

        // ExportAsFixedFormat(Type=xlTypePDF=0, Filename)
        let export_result = workbook.call_unit(
            "ExportAsFixedFormat",
            vec![VARIANT::from(0_i32), VARIANT::from(output_text)],
        );

        let _ = workbook.call_unit("Close", vec![VARIANT::from(false)]);
        export_result
    })();

    let _ = application.call_unit("Quit", Vec::new());
    result.map_err(|error| format!("Excel 转 PDF 失败：{error}"))
}

#[cfg(windows)]
fn convert_powerpoint_to_pdf(absolute_input: &str, output_text: &str) -> Result<(), String> {
    use windows::core::VARIANT;

    use super::office_com::DispatchObject;

    let application = DispatchObject::create_application("PowerPoint.Application")?;
    let result = (|| {
        let _ = application.put_i32_property("Visible", 0);

        let presentations = application.get_object_property("Presentations")?;
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

        // ExportAsFixedFormat(Path, FixedFormatType=ppFixedFormatTypePDF=2)
        let export_result = presentation.call_unit(
            "ExportAsFixedFormat",
            vec![VARIANT::from(output_text), VARIANT::from(2_i32)],
        );

        let _ = presentation.call_unit("Close", Vec::new());
        export_result
    })();

    let _ = application.call_unit("Quit", Vec::new());
    result.map_err(|error| format!("PowerPoint 转 PDF 失败：{error}"))
}
