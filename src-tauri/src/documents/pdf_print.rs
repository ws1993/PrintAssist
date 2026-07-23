//! Native PDF printing that preserves each page's landscape/portrait orientation.
//!
//! History of orientation bugs this path avoids:
//! - Shell `printto` / `PrintDocument.Landscape` can force landscape paper while
//!   also rotating already-correct page bitmaps → content appears 90° wrong.
//!
//! Current path:
//! 1. Render each PDF page with WinRT to a temporary PNG (visual orientation only)
//! 2. Decode PNGs with WIC
//! 3. Print via GDI with content-matched `dmOrientation` only
//!    (paper size stays with the driver; pixels are never rotated again)

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::image_print::{
    decode_image_bgra, print_decoded_pages, query_printer_logical_dpi, DecodedImage,
};

/// PDF page DIPs (PdfPage.Size) are defined at 96 units/inch.
const PDF_DIP_DPI: f64 = 96.0;
/// Floor for sharp text on virtual/physical printers that report low DPI.
const MIN_RENDER_DPI: u32 = 240;
/// Cap so multi-page jobs stay responsive while remaining print-quality.
const MAX_RENDER_DPI: u32 = 300;
/// Hard max edge length to avoid multi-hundred-MB page buffers.
const MAX_RENDER_EDGE_PX: u32 = 5000;

/// Prints a PDF while preserving per-page orientation and maximizing coverage.
pub fn print_pdf_to_printer(file_path: &Path, printer_name: &str, copies: u32) -> Result<(), String> {
    if !file_path.exists() {
        return Err(format!("文件不存在：{}", file_path.display()));
    }
    if printer_name.trim().is_empty() {
        return Err("打印机名称不能为空".to_string());
    }

    let staging = staging_dir()?;
    let render_prefix = format!(
        "printassist-pdf-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    );
    let page_pattern = staging.join(format!("{render_prefix}-page-*.png"));

    let target_dpi = choose_render_dpi(
        query_printer_logical_dpi(printer_name).unwrap_or(MAX_RENDER_DPI),
    );
    let render_result =
        render_pdf_pages_to_pngs(file_path, &staging, &render_prefix, target_dpi);
    let page_paths = match render_result {
        Ok(paths) => paths,
        Err(error) => {
            cleanup_glob_prefix(&staging, &render_prefix);
            return Err(error);
        }
    };

    if page_paths.is_empty() {
        cleanup_glob_prefix(&staging, &render_prefix);
        return Err("PDF 渲染后没有页面".to_string());
    }

    let decode_result = (|| {
        let mut pages: Vec<DecodedImage> = Vec::with_capacity(page_paths.len());
        for path in &page_paths {
            pages.push(decode_image_bgra(path)?);
        }
        // GDI path: set paper orientation from bitmap size, draw pixels without
        // any additional 90° transform.
        print_decoded_pages(&pages, printer_name, copies)
    })();

    // Always clean rendered staging files.
    for path in &page_paths {
        let _ = fs::remove_file(path);
    }
    let _ = page_pattern;

    decode_result
}

fn render_pdf_pages_to_pngs(
    file_path: &Path,
    staging: &Path,
    render_prefix: &str,
    target_dpi: u32,
) -> Result<Vec<PathBuf>, String> {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'

$pdfPath = '{input}'
$outDir = '{outdir}'
$prefix = '{prefix}'
$targetDpi = {dpi}
$maxEdge = {max_edge}

$null = [Windows.Data.Pdf.PdfDocument, Windows.Data.Pdf, ContentType = WindowsRuntime]
$null = [Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]
$null = [Windows.Storage.Streams.InMemoryRandomAccessStream, Windows.Storage.Streams, ContentType = WindowsRuntime]
$null = [Windows.Storage.Streams.DataReader, Windows.Storage.Streams, ContentType = WindowsRuntime]
$null = [Windows.Graphics.Imaging.BitmapEncoder, Windows.Graphics.Imaging, ContentType = WindowsRuntime]

Add-Type -AssemblyName System.Runtime.WindowsRuntime

# PowerShell cannot cast many WinRT ops to IAsyncInfo. Use AsTask instead.
$asTaskGeneric = [System.WindowsRuntimeSystemExtensions].GetMethods() |
  Where-Object {{
    $_.Name -eq 'AsTask' -and
    $_.IsGenericMethod -and
    $_.GetParameters().Count -eq 1
  }} |
  Select-Object -First 1

$asTaskAction = [System.WindowsRuntimeSystemExtensions].GetMethods() |
  Where-Object {{
    $_.Name -eq 'AsTask' -and
    -not $_.IsGenericMethod -and
    $_.GetParameters().Count -eq 1 -and
    $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncAction'
  }} |
  Select-Object -First 1

if ($null -eq $asTaskGeneric -or $null -eq $asTaskAction) {{
  throw '无法加载 WinRT AsTask 扩展（System.Runtime.WindowsRuntime）'
}}

function Await-WinRtAction($operation) {{
  $netTask = $asTaskAction.Invoke($null, @($operation))
  if (-not $netTask.Wait(120000)) {{
    throw 'WinRT 操作超时（IAsyncAction）'
  }}
  if ($netTask.IsFaulted) {{
    throw $netTask.Exception.GetBaseException()
  }}
}}

function Await-WinRtOperation($operation, [Type]$resultType) {{
  $asTask = $asTaskGeneric.MakeGenericMethod($resultType)
  $netTask = $asTask.Invoke($null, @($operation))
  if (-not $netTask.Wait(120000)) {{
    throw "WinRT 操作超时（$($resultType.FullName)）"
  }}
  if ($netTask.IsFaulted) {{
    throw $netTask.Exception.GetBaseException()
  }}
  return $netTask.Result
}}

function Render-PageToPng([Windows.Data.Pdf.PdfPage]$page, [string]$pngPath) {{
  # PdfPage.Size already reflects /Rotate; Render* draws the visual orientation.
  # Render at printer-matched DPI so GDI does not enlarge a low-res preview.
  $size = $page.Size
  $scale = [double]$targetDpi / 96.0
  $destWidth = [Math]::Max(1.0, $size.Width * $scale)
  $destHeight = [Math]::Max(1.0, $size.Height * $scale)
  $maxSide = [Math]::Max($destWidth, $destHeight)
  if ($maxSide -gt $maxEdge) {{
    $fit = $maxEdge / $maxSide
    $destWidth = $destWidth * $fit
    $destHeight = $destHeight * $fit
  }}
  $destWidthPx = [Math]::Max(1, [int][Math]::Round($destWidth))
  $destHeightPx = [Math]::Max(1, [int][Math]::Round($destHeight))

  $stream = [Windows.Storage.Streams.InMemoryRandomAccessStream]::new()
  try {{
    $options = [Windows.Data.Pdf.PdfPageRenderOptions]::new()
    $options.DestinationWidth = [System.UInt32]$destWidthPx
    $options.DestinationHeight = [System.UInt32]$destHeightPx
    $options.BitmapEncoderId = [Windows.Graphics.Imaging.BitmapEncoder]::PngEncoderId

    # C#/PowerShell project the options API as RenderToStreamAsync(stream, options).
    # The WinRT ABI name RenderWithOptionsToStreamAsync is often not visible.
    $rendered = $false
    $renderErrors = New-Object System.Collections.Generic.List[string]
    try {{
      # 2-arg overload with PdfPageRenderOptions (high DPI).
      Await-WinRtAction ($page.RenderToStreamAsync($stream, $options))
      $rendered = $true
    }} catch {{
      $renderErrors.Add("RenderToStreamAsync(stream,options): $($_.Exception.Message)") | Out-Null
      try {{ $stream.Dispose() }} catch {{}}
      $stream = [Windows.Storage.Streams.InMemoryRandomAccessStream]::new()
      try {{
        # Reflection fallback for hosts that only expose the ABI method name.
        $method = [Windows.Data.Pdf.PdfPage].GetMethods() |
          Where-Object {{
            ($_.Name -eq 'RenderToStreamAsync' -or $_.Name -eq 'RenderWithOptionsToStreamAsync') -and
            $_.GetParameters().Count -eq 2
          }} |
          Select-Object -First 1
        if ($null -ne $method) {{
          $asyncOp = $method.Invoke($page, @($stream, $options))
          Await-WinRtAction $asyncOp
          $rendered = $true
        }} else {{
          $renderErrors.Add('no 2-arg render method found on PdfPage') | Out-Null
        }}
      }} catch {{
        $renderErrors.Add("reflection render: $($_.Exception.Message)") | Out-Null
        try {{ $stream.Dispose() }} catch {{}}
        $stream = [Windows.Storage.Streams.InMemoryRandomAccessStream]::new()
      }}
    }}
    if (-not $rendered) {{
      try {{
        Await-WinRtAction ($page.RenderToStreamAsync($stream))
        $rendered = $true
        $renderErrors.Add('fell back to 96 DPI RenderToStreamAsync(stream)') | Out-Null
      }} catch {{
        $renderErrors.Add("RenderToStreamAsync(stream): $($_.Exception.Message)") | Out-Null
      }}
    }}
    if (-not $rendered) {{
      throw ("PDF 页面渲染失败（目标 ${{destWidthPx}}x${{destHeightPx}} @ ${{targetDpi}} DPI）：" + ($renderErrors -join ' | '))
    }}

    # Stream already contains an encoded image (PNG). Write it directly —
    # skip BitmapDecoder + System.Drawing re-encode (major speed win).
    $stream.Seek(0)
    $byteLength = [uint32]$stream.Size
    if ($byteLength -lt 1) {{
      throw 'PDF 页面渲染结果为空'
    }}
    $inputStream = $stream.GetInputStreamAt(0)
    $reader = [Windows.Storage.Streams.DataReader]::new($inputStream)
    try {{
      $loaded = Await-WinRtOperation ($reader.LoadAsync([System.UInt32]$byteLength)) ([System.UInt32])
      if ($loaded -lt 1) {{
        throw '读取 PDF 渲染流失败'
      }}
      $bytes = New-Object byte[] $loaded
      $reader.ReadBytes($bytes)
      [System.IO.File]::WriteAllBytes($pngPath, $bytes)
    }} finally {{
      $reader.Dispose()
      if ($null -ne $inputStream) {{ $inputStream.Dispose() }}
    }}
  }} finally {{
    if ($null -ne $stream) {{ $stream.Dispose() }}
  }}
}}

if (-not (Test-Path -LiteralPath $outDir)) {{
  New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}}

$file = Await-WinRtOperation `
  ([Windows.Storage.StorageFile]::GetFileFromPathAsync($pdfPath)) `
  ([Windows.Storage.StorageFile])
$pdf = Await-WinRtOperation `
  ([Windows.Data.Pdf.PdfDocument]::LoadFromFileAsync($file)) `
  ([Windows.Data.Pdf.PdfDocument])
if ($pdf.PageCount -lt 1) {{
  throw 'PDF 不含任何页面'
}}

$written = New-Object System.Collections.Generic.List[string]
for ($i = 0; $i -lt $pdf.PageCount; $i++) {{
  $page = $pdf.GetPage([uint32]$i)
  try {{
    Await-WinRtAction ($page.PreparePageAsync())
    $pageName = $prefix + '-page-' + $i.ToString('0000') + '.png'
    $pngPath = Join-Path $outDir $pageName
    Render-PageToPng $page $pngPath
    $written.Add($pngPath) | Out-Null
  }} finally {{
    # PdfPage projects as IClosable; PowerShell exposes Dispose, not Close.
    if ($null -ne $page) {{
      try {{
        if ($page -is [System.IDisposable]) {{
          $page.Dispose()
        }}
      }} catch {{}}
    }}
  }}
}}

# Emit page list for the host (one path per line).
$written -join "`n"
exit 0
"#,
        input = escape_for_powershell(file_path),
        outdir = escape_for_powershell(staging),
        prefix = escape_for_powershell_literal(render_prefix),
        dpi = target_dpi,
        max_edge = MAX_RENDER_EDGE_PX,
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .output()
        .map_err(|error| format!("启动 PowerShell 失败：{error}"))?;

    // Collect rendered files from disk (more reliable than parsing stdout encoding).
    let mut paths: Vec<PathBuf> = fs::read_dir(staging)
        .map_err(|error| format!("读取 PDF 渲染目录失败：{error}"))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with(&format!("{render_prefix}-page-")) && name.ends_with(".png"))
                .unwrap_or(false)
        })
        .collect();

    paths.sort();
    if paths.is_empty() {
        let detail = decode_process_text(&output.stderr);
        let detail = if detail.trim().is_empty() {
            decode_process_text(&output.stdout)
        } else {
            detail
        };
        if !output.status.success() {
            return Err(format!("PDF 页面渲染失败：{detail}"));
        }
        let stdout = decode_process_text(&output.stdout);
        return Err(format!(
            "PDF 渲染未生成页面文件。输出：{}",
            stdout.trim()
        ));
    }
    // PowerShell may return non-zero after Dispose/COM cleanup even when pages
    // rendered successfully; disk artifacts are the source of truth.
    Ok(paths)
}

/// Map printer-reported DPI into a practical PDF rasterization DPI.
fn choose_render_dpi(printer_dpi: u32) -> u32 {
    printer_dpi.clamp(MIN_RENDER_DPI, MAX_RENDER_DPI)
}

fn staging_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("PrintAssist").join("pdf-render");
    fs::create_dir_all(&dir).map_err(|error| format!("创建 PDF 渲染目录失败：{error}"))?;
    Ok(dir)
}

fn cleanup_glob_prefix(staging: &Path, render_prefix: &str) {
    if let Ok(entries) = fs::read_dir(staging) {
        for entry in entries.filter_map(|result| result.ok()) {
            let path = entry.path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with(render_prefix))
                .unwrap_or(false)
            {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn escape_for_powershell(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

fn escape_for_powershell_literal(value: &str) -> String {
    value.replace('\'', "''")
}

/// Decode PowerShell stdout/stderr. Chinese Windows often emits GBK, not UTF-8.
fn decode_process_text(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }

    #[cfg(windows)]
    {
        use windows::Win32::Globalization::{MultiByteToWideChar, CP_ACP};
        let needed = unsafe {
            MultiByteToWideChar(CP_ACP, Default::default(), bytes, None)
        };
        if needed > 0 {
            let mut wide = vec![0_u16; needed as usize];
            let written = unsafe {
                MultiByteToWideChar(CP_ACP, Default::default(), bytes, Some(&mut wide))
            };
            if written > 0 {
                return String::from_utf16_lossy(&wide[..written as usize]);
            }
        }
    }

    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};

    fn write_minimal_pdf(path: &Path, landscape: bool) {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = document.add_object(dictionary! {
            "Font" => dictionary! {
                "F1" => font_id,
            },
        });
        let (width, height) = if landscape { (792, 612) } else { (612, 792) };
        let content = Stream::new(dictionary! {}, b"BT /F1 24 Tf 72 72 Td (Hello) Tj ET".to_vec());
        let content_id = document.add_object(content);
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
            "Contents" => content_id,
            "Resources" => resources_id,
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Count" => 1_i64,
                "Kids" => vec![page_id.into()],
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document.save(path).expect("save pdf");
    }

    fn write_rotated_landscape_pdf(path: &Path) {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let resources_id = document.add_object(dictionary! {});
        let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Rotate" => 90_i64,
            "Contents" => content_id,
            "Resources" => resources_id,
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Count" => 1_i64,
                "Kids" => vec![page_id.into()],
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document.save(path).expect("save rotated pdf");
    }

    #[test]
    fn rejects_missing_pdf_file() {
        let result = print_pdf_to_printer(
            Path::new("C:\\\\this-file-should-not-exist-printassist.pdf"),
            "Microsoft Print to PDF",
            1,
        );
        assert!(result.is_err());
    }

    #[test]
    fn can_build_landscape_test_pdf() {
        let path = std::env::temp_dir().join(format!(
            "printassist-landscape-test-{}.pdf",
            std::process::id()
        ));
        write_minimal_pdf(&path, true);
        assert!(path.exists());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn can_build_rotated_landscape_pdf_fixture() {
        let path = std::env::temp_dir().join(format!(
            "printassist-rotated-landscape-fixture-{}.pdf",
            std::process::id()
        ));
        write_rotated_landscape_pdf(&path);
        assert!(path.exists());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn choose_render_dpi_clamps_to_print_quality_band() {
        assert_eq!(choose_render_dpi(96), MIN_RENDER_DPI);
        assert_eq!(choose_render_dpi(300), 300);
        assert_eq!(choose_render_dpi(600), MAX_RENDER_DPI);
        let _ = PDF_DIP_DPI;
    }

    #[test]
    fn renders_landscape_pdf_page_to_png() {
        let unique_suffix = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after epoch")
                .as_millis()
        );
        let pdf_path = std::env::temp_dir().join(format!(
            "printassist-render-smoke-{unique_suffix}.pdf"
        ));
        let render_directory = std::env::temp_dir().join("PrintAssist").join("pdf-smoke");
        let render_prefix = format!("render-smoke-{unique_suffix}");
        write_minimal_pdf(&pdf_path, true);

        let rendered_paths = match render_pdf_pages_to_pngs(
            &pdf_path,
            &render_directory,
            &render_prefix,
            MAX_RENDER_DPI,
        ) {
            Ok(paths) => paths,
            Err(error) => {
                let _ = fs::remove_file(&pdf_path);
                panic!("PDF render smoke failed: {error}");
            }
        };

        assert!(!rendered_paths.is_empty(), "expected at least one rendered page");
        let decoded_page =
            decode_image_bgra(&rendered_paths[0]).expect("decode rendered PNG");
        assert!(
            decoded_page.width > decoded_page.height,
            "expected landscape rendered page, got {}x{}",
            decoded_page.width,
            decoded_page.height
        );
        // 792 DIP * 300/96 ≈ 2475px; allow small rounding.
        assert!(
            decoded_page.width >= 2000,
            "expected print-quality width, got {}",
            decoded_page.width
        );

        let _ = fs::remove_file(pdf_path);
        for rendered_path in rendered_paths {
            let _ = fs::remove_file(rendered_path);
        }
    }
}
