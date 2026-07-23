//! In-process WinRT PDF rendering + shared GDI print path.
//!
//! Performance notes:
//! - Previous path spawned PowerShell per job (1-3s cold start) and wrote
//!   temporary PNGs before re-decoding in Rust.
//! - Current path prefers IPdfRendererNative + D2D (no encoded stream round-trip).
//! - Falls back to WinRT RenderWithOptionsToStream + BitmapDecoder if D2D fails.
//!
//! Orientation:
//! - PdfPage.Size already reflects /Rotate.
//! - GDI only selects DMORIENT_* from the final bitmap aspect ratio.
//! - Pixels are never rotated again after render.

use std::path::{Path, PathBuf};

use windows::core::HSTRING;
use windows::Data::Pdf::{PdfDocument, PdfPage, PdfPageRenderOptions};
use windows::Foundation::Size;
use windows::Graphics::Imaging::{BitmapDecoder, BitmapPixelFormat, PixelDataProvider};
use windows::Storage::StorageFile;
use windows::Storage::Streams::InMemoryRandomAccessStream;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

use super::image_print::{print_decoded_pages, query_printer_logical_dpi, DecodedImage};
use super::pdf_raster_d2d::{destination_pixels_for_page_size, NativePdfRasterContext};

/// PDF page DIPs (PdfPage.Size) are defined at 96 units/inch.
const PDF_DIP_DPI: f64 = 96.0;
/// Floor for sharp text on virtual/physical printers that report low DPI.
const MIN_RENDER_DPI: u32 = 240;
/// Cap so multi-page jobs stay responsive while remaining print-quality.
const MAX_RENDER_DPI: u32 = 300;
/// Hard max edge length to avoid multi-hundred-MB page buffers.
const MAX_RENDER_EDGE_PX: u32 = 5000;

/// Prints a PDF while preserving per-page orientation and maximizing coverage.
pub fn print_pdf_to_printer(
    file_path: &Path,
    printer_name: &str,
    copies: u32,
) -> Result<(), String> {
    if !file_path.exists() {
        return Err(format!("文件不存在：{}", file_path.display()));
    }
    if printer_name.trim().is_empty() {
        return Err("打印机名称不能为空".to_string());
    }

    let target_dpi =
        choose_render_dpi(query_printer_logical_dpi(printer_name).unwrap_or(MAX_RENDER_DPI));
    let pages = render_pdf_pages_in_process(file_path, target_dpi)?;
    if pages.is_empty() {
        return Err("PDF 渲染后没有页面".to_string());
    }
    print_decoded_pages(&pages, printer_name, copies)
}

/// Render each PDF page in-process to a print-quality BGRA bitmap.
pub fn render_pdf_pages_in_process(
    file_path: &Path,
    target_dpi: u32,
) -> Result<Vec<DecodedImage>, String> {
    let absolute_path = canonicalize_existing_path(file_path)?;
    let path_text = absolute_path
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();

    let com_initialized = initialize_com_apartment()?;
    let render_result = (|| {
        let storage_file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path_text.as_str()))
            .map_err(|error| format!("打开 PDF 文件失败：{error}"))?
            .get()
            .map_err(|error| format!("打开 PDF 文件失败：{error}"))?;

        let pdf_document = PdfDocument::LoadFromFileAsync(&storage_file)
            .map_err(|error| format!("加载 PDF 失败：{error}"))?
            .get()
            .map_err(|error| format!("加载 PDF 失败：{error}"))?;

        let page_count = pdf_document
            .PageCount()
            .map_err(|error| format!("读取 PDF 页数失败：{error}"))?;
        if page_count == 0 {
            return Err("PDF 不含任何页面".to_string());
        }

        let mut pages = Vec::with_capacity(page_count as usize);
        let d2d_context = NativePdfRasterContext::new().ok();
        for page_index in 0..page_count {
            let page = pdf_document
                .GetPage(page_index)
                .map_err(|error| format!("读取第 {} 页失败：{error}", page_index + 1))?;
            let decoded = render_single_page(&page, target_dpi, d2d_context.as_ref())
                .map_err(|error| format!("渲染第 {} 页失败：{error}", page_index + 1))?;
            let _ = page.Close();
            pages.push(decoded);
        }
        Ok(pages)
    })();

    if com_initialized {
        unsafe {
            CoUninitialize();
        }
    }
    render_result
}

fn render_single_page(
    page: &PdfPage,
    target_dpi: u32,
    d2d_context: Option<&NativePdfRasterContext>,
) -> Result<DecodedImage, String> {
    page.PreparePageAsync()
        .map_err(|error| format!("PreparePage 失败：{error}"))?
        .get()
        .map_err(|error| format!("PreparePage 失败：{error}"))?;

    let size: Size = page
        .Size()
        .map_err(|error| format!("读取页面尺寸失败：{error}"))?;
    let (destination_width, destination_height) =
        destination_pixels_for_page_size(size, target_dpi, MAX_RENDER_EDGE_PX, PDF_DIP_DPI);

    if let Some(context) = d2d_context {
        match context.render_page(page, destination_width, destination_height) {
            Ok(decoded) => return Ok(decoded),
            Err(_d2d_error) => {
                // Fall through to stream-based path.
            }
        }
    }

    let options =
        PdfPageRenderOptions::new().map_err(|error| format!("创建渲染选项失败：{error}"))?;
    options
        .SetDestinationWidth(destination_width)
        .map_err(|error| format!("设置渲染宽度失败：{error}"))?;
    options
        .SetDestinationHeight(destination_height)
        .map_err(|error| format!("设置渲染高度失败：{error}"))?;
    options
        .SetIsIgnoringHighContrast(true)
        .map_err(|error| format!("设置渲染对比度失败：{error}"))?;

    let stream =
        InMemoryRandomAccessStream::new().map_err(|error| format!("创建内存流失败：{error}"))?;
    page.RenderWithOptionsToStreamAsync(&stream, &options)
        .map_err(|error| format!("RenderWithOptions 失败：{error}"))?
        .get()
        .map_err(|error| format!("RenderWithOptions 失败：{error}"))?;

    stream
        .Seek(0)
        .map_err(|error| format!("定位渲染流失败：{error}"))?;

    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(|error| format!("创建位图解码器失败：{error}"))?
        .get()
        .map_err(|error| format!("创建位图解码器失败：{error}"))?;

    let width = decoder
        .PixelWidth()
        .map_err(|error| format!("读取渲染宽度失败：{error}"))?;
    let height = decoder
        .PixelHeight()
        .map_err(|error| format!("读取渲染高度失败：{error}"))?;
    if width == 0 || height == 0 {
        return Err("渲染结果尺寸无效".to_string());
    }

    let pixel_format = decoder
        .BitmapPixelFormat()
        .map_err(|error| format!("读取像素格式失败：{error}"))?;

    let provider: PixelDataProvider = decoder
        .GetPixelDataAsync()
        .map_err(|error| format!("读取像素失败：{error}"))?
        .get()
        .map_err(|error| format!("读取像素失败：{error}"))?;
    let pixel_array = provider
        .DetachPixelData()
        .map_err(|error| format!("提取像素失败：{error}"))?;
    let mut pixels = pixel_array.as_slice().to_vec();

    // Normalize RGBA -> BGRA if needed so GDI StretchDIBits stays correct.
    if pixel_format == BitmapPixelFormat::Rgba8 {
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }
    }

    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|value| value.checked_mul(4))
        .ok_or_else(|| "渲染尺寸过大".to_string())?;
    if pixels.len() < expected {
        return Err(format!(
            "像素缓冲区过短：got {}, expected at least {expected}",
            pixels.len()
        ));
    }
    pixels.truncate(expected);

    Ok(DecodedImage {
        width,
        height,
        pixels,
    })
}

fn destination_pixels_for_page(width_dip: f32, height_dip: f32, target_dpi: u32) -> (u32, u32) {
    destination_pixels_for_page_size(
        Size {
            Width: width_dip,
            Height: height_dip,
        },
        target_dpi,
        MAX_RENDER_EDGE_PX,
        PDF_DIP_DPI,
    )
}

/// Map printer-reported DPI into a practical PDF rasterization DPI.
fn choose_render_dpi(printer_dpi: u32) -> u32 {
    printer_dpi.clamp(MIN_RENDER_DPI, MAX_RENDER_DPI)
}

fn canonicalize_existing_path(file_path: &Path) -> Result<PathBuf, String> {
    if !file_path.exists() {
        return Err(format!("文件不存在：{}", file_path.display()));
    }
    match std::fs::canonicalize(file_path) {
        Ok(path) => Ok(path),
        Err(_) => Ok(file_path.to_path_buf()),
    }
}

fn initialize_com_apartment() -> Result<bool, String> {
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if hr.is_ok() {
        return Ok(true);
    }
    let code = hr.0 as u32;
    // RPC_E_CHANGED_MODE — COM already initialized in another apartment.
    if code == 0x8001_0106 {
        return Ok(false);
    }
    Err(format!("初始化 COM 失败：0x{code:08X}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};
    use std::fs;

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
        let content = Stream::new(
            dictionary! {},
            b"BT /F1 24 Tf 72 72 Td (Hello) Tj ET".to_vec(),
        );
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
    fn choose_render_dpi_clamps_to_print_quality_band() {
        assert_eq!(choose_render_dpi(96), MIN_RENDER_DPI);
        assert_eq!(choose_render_dpi(300), 300);
        assert_eq!(choose_render_dpi(600), MAX_RENDER_DPI);
        let _ = PDF_DIP_DPI;
    }

    #[test]
    fn destination_pixels_scale_with_dpi_and_cap_edge() {
        let (width, height) = destination_pixels_for_page(792.0, 612.0, 300);
        // 792 * 300/96 = 2475
        assert!(width >= 2400 && width <= 2600, "width={width}");
        assert!(height >= 1800 && height <= 2000, "height={height}");

        let (capped_width, capped_height) = destination_pixels_for_page(20000.0, 10000.0, 300);
        assert!(capped_width <= MAX_RENDER_EDGE_PX);
        assert!(capped_height <= MAX_RENDER_EDGE_PX);
    }

    #[test]
    fn renders_landscape_pdf_in_process_at_print_quality() {
        let unique_suffix = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after epoch")
                .as_millis()
        );
        let pdf_path =
            std::env::temp_dir().join(format!("printassist-render-inproc-{unique_suffix}.pdf"));
        write_minimal_pdf(&pdf_path, true);

        let pages = match render_pdf_pages_in_process(&pdf_path, MAX_RENDER_DPI) {
            Ok(pages) => pages,
            Err(error) => {
                let _ = fs::remove_file(&pdf_path);
                panic!("in-process PDF render failed: {error}");
            }
        };
        let _ = fs::remove_file(&pdf_path);

        assert_eq!(pages.len(), 1);
        let page = &pages[0];
        assert!(
            page.width > page.height,
            "expected landscape, got {}x{}",
            page.width,
            page.height
        );
        assert!(
            page.width >= 2000,
            "expected print-quality width, got {}",
            page.width
        );
        assert_eq!(page.pixels.len(), (page.width * page.height * 4) as usize);
    }
}
