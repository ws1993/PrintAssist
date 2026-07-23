//! Office COM direct printing that keeps each document's own page orientation.
//!
//! Shell `printto` / PDF-viewer handlers often re-layout landscape pages onto
//! portrait paper. Word/Excel/PowerPoint PrintOut uses the document PageSetup
//! (or slide size) and therefore preserves landscape/portrait as authored.

use std::path::Path;
use std::process::Command;

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

    let script = match kind {
        DocumentKind::Word => word_print_script(
            path,
            printer_name,
            copy_count,
            custom_pages,
            page_range_expression,
        ),
        DocumentKind::Excel => excel_print_script(path, printer_name, copy_count),
        DocumentKind::PowerPoint => powerpoint_print_script(path, printer_name, copy_count),
        _ => return Err("不是 Office 文档".to_string()),
    };

    run_powershell(&script)
}

fn word_print_script(
    path: &Path,
    printer_name: &str,
    copies: u32,
    custom_pages: bool,
    page_range_expression: &str,
) -> String {
    // wdPrintAllDocument = 0, wdPrintRangeOfPages = 4
    let (range_code, pages_literal) = if custom_pages {
        (
            4,
            format!(
                "'{}'",
                escape_for_powershell_literal(page_range_expression.trim())
            ),
        )
    } else {
        (0, "''".to_string())
    };

    format!(
        r#"
$ErrorActionPreference = 'Stop'
$word = New-Object -ComObject Word.Application
$word.Visible = $false
$word.DisplayAlerts = 0
try {{
  $document = $word.Documents.Open('{input}', $false, $true)
  try {{
    # Keep section page orientation from the document; do not force portrait.
    # FilePrintSetup routes the job to the target printer without changing layout.
    $null = $word.WordBasic.FilePrintSetup([ref]'{printer}', [ref]$true)
    $background = $false
    $append = $false
    $range = {range}
    $outputFileName = ''
    $from = ''
    $to = ''
    $item = 0
    $copies = {copies}
    $pages = {pages}
    $pageType = 0
    $printToFile = $false
    $collate = $true
    $document.PrintOut(
      [ref]$background,
      [ref]$append,
      [ref]$range,
      [ref]$outputFileName,
      [ref]$from,
      [ref]$to,
      [ref]$item,
      [ref]$copies,
      [ref]$pages,
      [ref]$pageType,
      [ref]$printToFile,
      [ref]$collate
    )
  }} finally {{
    $document.Close([ref]0)
  }}
}} finally {{
  $word.Quit()
  [System.Runtime.InteropServices.Marshal]::ReleaseComObject($word) | Out-Null
}}
"#,
        input = escape_for_powershell(path),
        printer = escape_for_powershell_literal(printer_name),
        range = range_code,
        copies = copies,
        pages = pages_literal,
    )
}

fn excel_print_script(path: &Path, printer_name: &str, copies: u32) -> String {
    format!(
        r#"
$ErrorActionPreference = 'Stop'
$excel = New-Object -ComObject Excel.Application
$excel.Visible = $false
$excel.DisplayAlerts = $false
try {{
  $workbook = $excel.Workbooks.Open('{input}', 0, $true)
  try {{
    # ActivePrinter format varies by locale/port; try bare name then Ne00 fallbacks.
    $set = $false
    foreach ($candidate in @('{printer}', '{printer} on Ne00:', '{printer} on Ne01:')) {{
      try {{
        $excel.ActivePrinter = $candidate
        $set = $true
        break
      }} catch {{}}
    }}
    if (-not $set) {{
      throw "无法切换到打印机：{printer}"
    }}
    # Workbook/sheet PageSetup.Orientation is honored by PrintOut.
    $workbook.PrintOut([Type]::Missing, [Type]::Missing, {copies})
  }} finally {{
    $workbook.Close($false)
  }}
}} finally {{
  $excel.Quit()
  [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}}
"#,
        input = escape_for_powershell(path),
        printer = escape_for_powershell_literal(printer_name),
        copies = copies,
    )
}

fn powerpoint_print_script(path: &Path, printer_name: &str, copies: u32) -> String {
    format!(
        r#"
$ErrorActionPreference = 'Stop'
$powerpoint = New-Object -ComObject PowerPoint.Application
try {{
  $presentation = $powerpoint.Presentations.Open('{input}', $true, $false, $false)
  try {{
    $presentation.PrintOptions.ActivePrinter = '{printer}'
    # FitToPage=false keeps slide aspect; OutputType=1 prints slides as authored.
    $presentation.PrintOptions.FitToPage = $false
    $presentation.PrintOptions.OutputType = 1
    $presentation.PrintOptions.NumberOfCopies = {copies}
    $presentation.PrintOut()
  }} finally {{
    $presentation.Close()
  }}
}} finally {{
  $powerpoint.Quit()
  [System.Runtime.InteropServices.Marshal]::ReleaseComObject($powerpoint) | Out-Null
}}
"#,
        input = escape_for_powershell(path),
        printer = escape_for_powershell_literal(printer_name),
        copies = copies,
    )
}

fn run_powershell(script: &str) -> Result<(), String> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|error| format!("启动 PowerShell 失败：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if !stderr.trim().is_empty() {
            stderr
        } else {
            stdout
        };
        return Err(format!(
            "Office 直接打印失败，请确认已安装桌面版 Office。详情：{detail}"
        ));
    }
    Ok(())
}

fn escape_for_powershell(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

fn escape_for_powershell_literal(value: &str) -> String {
    value.replace('\'', "''")
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
