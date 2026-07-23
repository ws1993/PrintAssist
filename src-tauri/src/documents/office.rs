use std::path::{Path, PathBuf};
use std::process::Command;

use super::DocumentKind;

/// Converts Office documents to a temporary PDF via installed desktop Office COM automation.
pub fn convert_office_to_pdf(path: &Path, kind: DocumentKind) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("文件不存在：{}", path.display()));
    }

    let output_path = std::env::temp_dir().join(format!(
        "printassist-{}-{}.pdf",
        std::process::id(),
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("document")
    ));

    let script = match kind {
        DocumentKind::Word => format!(
            r#"
$ErrorActionPreference = 'Stop'
$word = New-Object -ComObject Word.Application
$word.Visible = $false
try {{
  $document = $word.Documents.Open('{input}')
  $document.ExportAsFixedFormat('{output}', 17)
  $document.Close([ref]0)
}} finally {{
  $word.Quit()
  [System.Runtime.InteropServices.Marshal]::ReleaseComObject($word) | Out-Null
}}
"#,
            input = escape_for_powershell(path),
            output = escape_for_powershell(&output_path)
        ),
        DocumentKind::Excel => format!(
            r#"
$ErrorActionPreference = 'Stop'
$excel = New-Object -ComObject Excel.Application
$excel.Visible = $false
$excel.DisplayAlerts = $false
try {{
  $workbook = $excel.Workbooks.Open('{input}')
  $workbook.ExportAsFixedFormat(0, '{output}')
  $workbook.Close($false)
}} finally {{
  $excel.Quit()
  [System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) | Out-Null
}}
"#,
            input = escape_for_powershell(path),
            output = escape_for_powershell(&output_path)
        ),
        DocumentKind::PowerPoint => format!(
            r#"
$ErrorActionPreference = 'Stop'
$powerpoint = New-Object -ComObject PowerPoint.Application
try {{
  $presentation = $powerpoint.Presentations.Open('{input}', $true, $false, $false)
  $presentation.ExportAsFixedFormat('{output}', 2)
  $presentation.Close()
}} finally {{
  $powerpoint.Quit()
  [System.Runtime.InteropServices.Marshal]::ReleaseComObject($powerpoint) | Out-Null
}}
"#,
            input = escape_for_powershell(path),
            output = escape_for_powershell(&output_path)
        ),
        _ => return Err("不是 Office 文档".to_string()),
    };

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|error| format!("启动 PowerShell 失败：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Office 转换失败，请确认已安装桌面版 Office。详情：{stderr}"
        ));
    }

    if !output_path.exists() {
        return Err("Office 转换未生成 PDF 文件".to_string());
    }

    Ok(output_path)
}

fn escape_for_powershell(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}
