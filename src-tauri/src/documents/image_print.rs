//! Native image printing that preserves source orientation.
//!
//! Shell `printto` often routes through Photos/Paint, which may force portrait
//! paper and draw a small centered preview. This path:
//! 1. Decodes the image with WIC (applies EXIF orientation for visual layout)
//! 2. Sets printer DEVMODE orientation from the resulting aspect ratio
//! 3. Scales the bitmap to fill the printable area as much as possible

use std::ffi::OsStr;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use windows::core::{Interface, PCWSTR, PROPVARIANT};
use windows::Win32::Foundation::{GENERIC_READ, HANDLE, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, CreateDCW, DeleteDC, DeleteObject, GetDeviceCaps,
    ResetDCW, SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt, StretchDIBits,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DEVMODEW, DIB_RGB_COLORS, DMORIENT_LANDSCAPE,
    DMORIENT_PORTRAIT, DM_IN_BUFFER, DM_ORIENTATION, DM_OUT_BUFFER, HALFTONE, HBITMAP, HDC,
    HORZRES, LOGPIXELSX, SRCCOPY, VERTRES,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICBitmapFrameDecode, IWICBitmapSource,
    IWICImagingFactory, WICBitmapTransformFlipHorizontal, WICBitmapTransformFlipVertical,
    WICBitmapTransformOptions, WICBitmapTransformRotate180, WICBitmapTransformRotate270,
    WICBitmapTransformRotate90, WICConvertBitmapSource, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::Graphics::Printing::{ClosePrinter, DocumentPropertiesW, OpenPrinterW};
use windows::Win32::Storage::Xps::{AbortDoc, EndDoc, EndPage, StartDocW, StartPage, DOCINFOW};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};

/// Prints an image while preserving its orientation and maximizing page coverage.
pub fn print_image_to_printer(
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

    let decoded = decode_image_bgra(file_path)?;
    print_decoded_pages(std::slice::from_ref(&decoded), printer_name, copies)
}

/// Query the printer's logical DPI used for device coordinates.
///
/// Used by PDF rendering so intermediate bitmaps match print resolution
/// instead of a fixed low preview DPI that blurs when stretched.
pub fn query_printer_logical_dpi(printer_name: &str) -> Result<u32, String> {
    if printer_name.trim().is_empty() {
        return Err("打印机名称不能为空".to_string());
    }

    let printer_wide = os_str_to_wide(OsStr::new(printer_name));
    let hdc = unsafe {
        CreateDCW(
            windows::core::w!("WINSPOOL"),
            PCWSTR(printer_wide.as_ptr()),
            PCWSTR::null(),
            None,
        )
    };
    if hdc.is_invalid() {
        return Err("创建打印机设备上下文失败".to_string());
    }
    let _dc_guard = DcGuard(hdc);

    let dpi = unsafe { GetDeviceCaps(hdc, LOGPIXELSX) };
    if dpi <= 0 {
        return Err("无法读取打印机 DPI".to_string());
    }
    Ok(dpi as u32)
}

/// Prints one or more already-decoded pages in a single print job.
///
/// The rendered pixels already have their final visual orientation. The
/// DEVMODE only selects the matching printer coordinate system; paper size and
/// dimensions remain owned by the printer driver.
pub fn print_decoded_pages(
    pages: &[DecodedImage],
    printer_name: &str,
    copies: u32,
) -> Result<(), String> {
    if pages.is_empty() {
        return Err("没有可打印的页面".to_string());
    }
    if printer_name.trim().is_empty() {
        return Err("打印机名称不能为空".to_string());
    }

    let copy_count = copies.max(1);
    for _ in 0..copy_count {
        print_decoded_pages_once(pages, printer_name)?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    /// BGRA8 pixels, row-major, top-down.
    pub pixels: Vec<u8>,
}

pub fn decode_image_bgra(file_path: &Path) -> Result<DecodedImage, String> {
    // S_OK / S_FALSE succeed; RPC_E_CHANGED_MODE means another mode is already active.
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let should_uninitialize = hr.is_ok();
    if hr.is_err() {
        let code = hr.0 as u32;
        // RPC_E_CHANGED_MODE — COM usable in the existing apartment.
        if code != 0x8001_0106 {
            return Err(format!("初始化 COM 失败：0x{code:08X}"));
        }
    }

    let result = (|| {
        let factory: IWICImagingFactory = unsafe {
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|error| format!("创建 WIC 工厂失败：{error}"))?
        };

        let path_wide = path_to_wide(file_path);
        let decoder = unsafe {
            factory
                .CreateDecoderFromFilename(
                    PCWSTR(path_wide.as_ptr()),
                    None,
                    GENERIC_READ,
                    WICDecodeMetadataCacheOnDemand,
                )
                .map_err(|error| format!("无法解码图片（系统可能缺少该格式编解码器）：{error}"))?
        };

        let frame: IWICBitmapFrameDecode = unsafe {
            decoder
                .GetFrame(0)
                .map_err(|error| format!("读取图片帧失败：{error}"))?
        };

        let converted: IWICBitmapSource = unsafe {
            WICConvertBitmapSource(&GUID_WICPixelFormat32bppBGRA, &frame)
                .map_err(|error| format!("转换图片像素格式失败：{error}"))?
        };

        let oriented = apply_exif_orientation_if_present(&factory, &converted, &frame)?;

        let mut width = 0_u32;
        let mut height = 0_u32;
        unsafe {
            oriented
                .GetSize(&mut width, &mut height)
                .map_err(|error| format!("读取图片尺寸失败：{error}"))?;
        }
        if width == 0 || height == 0 {
            return Err("图片尺寸无效".to_string());
        }

        let stride = width
            .checked_mul(4)
            .ok_or_else(|| "图片宽度过大".to_string())? as usize;
        let buffer_size = stride
            .checked_mul(height as usize)
            .ok_or_else(|| "图片尺寸过大".to_string())?;
        let mut pixels = vec![0_u8; buffer_size];

        unsafe {
            oriented
                .CopyPixels(ptr::null(), stride as u32, &mut pixels)
                .map_err(|error| format!("读取图片像素失败：{error}"))?;
        }

        Ok(DecodedImage {
            width,
            height,
            pixels,
        })
    })();

    if should_uninitialize {
        unsafe {
            CoUninitialize();
        }
    }
    result
}

fn apply_exif_orientation_if_present(
    factory: &IWICImagingFactory,
    source: &IWICBitmapSource,
    frame: &IWICBitmapFrameDecode,
) -> Result<IWICBitmapSource, String> {
    let orientation = read_exif_orientation(frame).unwrap_or(1);
    if orientation <= 1 {
        return Ok(source.clone());
    }

    let transform = unsafe {
        factory
            .CreateBitmapFlipRotator()
            .map_err(|error| format!("创建图片方向变换失败：{error}"))?
    };

    let options: WICBitmapTransformOptions = match orientation {
        2 => WICBitmapTransformFlipHorizontal,
        3 => WICBitmapTransformRotate180,
        4 => WICBitmapTransformFlipVertical,
        5 => WICBitmapTransformOptions(
            WICBitmapTransformRotate90.0 | WICBitmapTransformFlipHorizontal.0,
        ),
        6 => WICBitmapTransformRotate90,
        7 => WICBitmapTransformOptions(
            WICBitmapTransformRotate270.0 | WICBitmapTransformFlipHorizontal.0,
        ),
        8 => WICBitmapTransformRotate270,
        _ => return Ok(source.clone()),
    };

    unsafe {
        transform
            .Initialize(source, options)
            .map_err(|error| format!("应用图片 EXIF 方向失败：{error}"))?;
    }
    transform
        .cast::<IWICBitmapSource>()
        .map_err(|error| format!("图片方向变换类型转换失败：{error}"))
}

fn read_exif_orientation(frame: &IWICBitmapFrameDecode) -> Option<u16> {
    let reader = unsafe { frame.GetMetadataQueryReader().ok()? };
    let mut value = PROPVARIANT::new();
    unsafe {
        reader
            .GetMetadataByName(windows::core::w!("/app1/ifd/{ushort=274}"), &mut value)
            .ok()?;
    }
    let raw = value.as_raw();
    unsafe {
        let vt = raw.Anonymous.Anonymous.vt;
        if vt == 18 {
            Some(raw.Anonymous.Anonymous.Anonymous.uiVal)
        } else if vt == 3 {
            Some(raw.Anonymous.Anonymous.Anonymous.lVal as u16)
        } else {
            None
        }
    }
}

fn print_decoded_pages_once(pages: &[DecodedImage], printer_name: &str) -> Result<(), String> {
    let printer_wide = os_str_to_wide(OsStr::new(printer_name));
    let mut printer_handle = HANDLE::default();
    unsafe {
        OpenPrinterW(PCWSTR(printer_wide.as_ptr()), &mut printer_handle, None)
            .map_err(|error| format!("打开打印机失败：{error}"))?;
    }
    let _printer_guard = PrinterGuard(printer_handle);

    let mut base_devmode = query_devmode(printer_handle, &printer_wide)?;
    let mut page_devmode = prepare_page_devmode(
        printer_handle,
        &printer_wide,
        &base_devmode,
        pages[0].width,
        pages[0].height,
    )?;

    let hdc = unsafe {
        CreateDCW(
            windows::core::w!("WINSPOOL"),
            PCWSTR(printer_wide.as_ptr()),
            PCWSTR::null(),
            Some(page_devmode.as_ptr() as *const DEVMODEW),
        )
    };
    if hdc.is_invalid() {
        return Err("创建打印机设备上下文失败".to_string());
    }
    let _dc_guard = DcGuard(hdc);

    let doc_name = os_str_to_wide(OsStr::new("PrintAssist"));
    let doc_info = DOCINFOW {
        cbSize: size_of::<DOCINFOW>() as i32,
        lpszDocName: PCWSTR(doc_name.as_ptr()),
        lpszOutput: PCWSTR::null(),
        lpszDatatype: PCWSTR::null(),
        fwType: 0,
    };

    let job_id = unsafe { StartDocW(hdc, &doc_info) };
    if job_id <= 0 {
        return Err("StartDoc 失败".to_string());
    }

    let page_result = (|| {
        for page in pages {
            // Select the matching printer coordinate system. Pixels are
            // already visually correct and are never rotated here.
            page_devmode = prepare_page_devmode(
                printer_handle,
                &printer_wide,
                &base_devmode,
                page.width,
                page.height,
            )?;
            let reset = unsafe { ResetDCW(hdc, page_devmode.as_ptr() as *const DEVMODEW) };
            if reset.is_invalid() {
                return Err("ResetDC 设置页面方向失败".to_string());
            }

            if unsafe { StartPage(hdc) } <= 0 {
                return Err("StartPage 失败".to_string());
            }
            draw_image_on_page(hdc, page)?;
            if unsafe { EndPage(hdc) } <= 0 {
                return Err("EndPage 失败".to_string());
            }
        }
        Ok(())
    })();

    // Silence unused mutation if compiler folds base_devmode oddly on some builds.
    let _ = &mut base_devmode;

    match page_result {
        Ok(()) => {
            if unsafe { EndDoc(hdc) } <= 0 {
                return Err("EndDoc 失败".to_string());
            }
            Ok(())
        }
        Err(error) => {
            unsafe {
                let _ = AbortDoc(hdc);
            }
            Err(error)
        }
    }
}

fn query_devmode(printer_handle: HANDLE, printer_wide: &[u16]) -> Result<Vec<u8>, String> {
    let needed = unsafe {
        DocumentPropertiesW(
            HWND::default(),
            printer_handle,
            PCWSTR(printer_wide.as_ptr()),
            None,
            None,
            0,
        )
    };
    if needed <= 0 {
        return Err("查询打印机 DEVMODE 大小失败".to_string());
    }

    let mut buffer = vec![0_u8; needed as usize];
    let filled = unsafe {
        DocumentPropertiesW(
            HWND::default(),
            printer_handle,
            PCWSTR(printer_wide.as_ptr()),
            Some(buffer.as_mut_ptr() as *mut DEVMODEW),
            None,
            DM_OUT_BUFFER.0,
        )
    };
    if filled < 0 {
        return Err("读取打印机 DEVMODE 失败".to_string());
    }
    Ok(buffer)
}

/// Clone base DEVMODE, apply content-matched orientation, then let the driver
/// validate/merge via DocumentPropertiesW(DM_IN_BUFFER | DM_OUT_BUFFER).
fn prepare_page_devmode(
    printer_handle: HANDLE,
    printer_wide: &[u16],
    base_devmode: &[u8],
    content_width: u32,
    content_height: u32,
) -> Result<Vec<u8>, String> {
    let mut modified = base_devmode.to_vec();
    set_page_orientation_from_content(&mut modified, content_width, content_height);

    let mut validated = vec![0_u8; modified.len()];
    let result = unsafe {
        DocumentPropertiesW(
            HWND::default(),
            printer_handle,
            PCWSTR(printer_wide.as_ptr()),
            Some(validated.as_mut_ptr() as *mut DEVMODEW),
            Some(modified.as_ptr() as *const DEVMODEW),
            DM_IN_BUFFER.0 | DM_OUT_BUFFER.0,
        )
    };
    if result < 0 {
        // Some virtual printers reject custom sizes; fall back to our modified DEVMODE.
        return Ok(modified);
    }
    // Keep validated buffer if driver wrote a full DEVMODE; otherwise use modified.
    if validated.len() >= size_of::<DEVMODEW>() {
        let size = unsafe { (*(validated.as_ptr() as *const DEVMODEW)).dmSize as usize };
        if size >= size_of::<DEVMODEW>() {
            return Ok(validated);
        }
    }
    Ok(modified)
}

/// Select the printer coordinate system that matches the rendered page.
///
/// Paper size and dimensions must remain unchanged. Combining portrait
/// orientation with landscape custom dimensions gives drivers conflicting
/// signals and can make them rotate already-oriented content by 90 degrees.
fn set_page_orientation_from_content(
    devmode_bytes: &mut [u8],
    content_width: u32,
    content_height: u32,
) {
    if devmode_bytes.len() < size_of::<DEVMODEW>() {
        return;
    }

    let devmode = unsafe { &mut *(devmode_bytes.as_mut_ptr() as *mut DEVMODEW) };
    let requested_orientation = if content_width > content_height {
        DMORIENT_LANDSCAPE
    } else {
        DMORIENT_PORTRAIT
    };

    devmode.dmFields |= DM_ORIENTATION;
    devmode.Anonymous1.Anonymous1.dmOrientation = requested_orientation as i16;
}

fn draw_image_on_page(hdc: HDC, image: &DecodedImage) -> Result<(), String> {
    let page_width = unsafe { GetDeviceCaps(hdc, HORZRES) };
    let page_height = unsafe { GetDeviceCaps(hdc, VERTRES) };
    if page_width <= 0 || page_height <= 0 {
        return Err("无法读取打印机可打印区域".to_string());
    }

    let dest = compute_destination_rect(
        page_width as u32,
        page_height as u32,
        image.width,
        image.height,
    );

    let hbitmap = create_dib_bitmap(hdc, image)?;
    let _bitmap_guard = BitmapGuard(hbitmap);

    let mem_dc = unsafe { CreateCompatibleDC(hdc) };
    if mem_dc.is_invalid() {
        return Err("创建兼容 DC 失败".to_string());
    }
    let _mem_guard = DcGuard(mem_dc);

    let old = unsafe { SelectObject(mem_dc, hbitmap) };
    // HALFTONE improves quality when printer resolution exceeds source pixels.
    unsafe {
        let _ = SetStretchBltMode(hdc, HALFTONE);
        let _ = SetBrushOrgEx(hdc, 0, 0, None);
    }
    let ok = unsafe {
        StretchBlt(
            hdc,
            dest.left,
            dest.top,
            dest.right - dest.left,
            dest.bottom - dest.top,
            mem_dc,
            0,
            0,
            image.width as i32,
            image.height as i32,
            SRCCOPY,
        )
    };
    unsafe {
        let _ = SelectObject(mem_dc, old);
    }

    if !ok.as_bool() {
        return Err("绘制图片到打印机失败".to_string());
    }
    Ok(())
}

/// Fit image into the page while preserving aspect ratio and maximizing size.
pub fn compute_destination_rect(
    page_width: u32,
    page_height: u32,
    image_width: u32,
    image_height: u32,
) -> RECT {
    let page_w = page_width.max(1) as f64;
    let page_h = page_height.max(1) as f64;
    let image_w = image_width.max(1) as f64;
    let image_h = image_height.max(1) as f64;

    let scale = (page_w / image_w).min(page_h / image_h);
    let draw_w = (image_w * scale).round().max(1.0);
    let draw_h = (image_h * scale).round().max(1.0);
    let left = ((page_w - draw_w) / 2.0).round() as i32;
    let top = ((page_h - draw_h) / 2.0).round() as i32;

    RECT {
        left,
        top,
        right: left + draw_w as i32,
        bottom: top + draw_h as i32,
    }
}

fn create_dib_bitmap(hdc: HDC, image: &DecodedImage) -> Result<HBITMAP, String> {
    let hbitmap = unsafe { CreateCompatibleBitmap(hdc, image.width as i32, image.height as i32) };
    if hbitmap.is_invalid() {
        return Err("创建位图失败".to_string());
    }

    let mut info: BITMAPINFO = unsafe { zeroed() };
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: image.width as i32,
        // Negative height = top-down DIB.
        biHeight: -(image.height as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0 as u32,
        biSizeImage: 0,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };

    let mem_dc = unsafe { CreateCompatibleDC(hdc) };
    if mem_dc.is_invalid() {
        unsafe {
            let _ = DeleteObject(hbitmap);
        }
        return Err("创建位图 DC 失败".to_string());
    }
    let old = unsafe { SelectObject(mem_dc, hbitmap) };

    let lines = unsafe {
        StretchDIBits(
            mem_dc,
            0,
            0,
            image.width as i32,
            image.height as i32,
            0,
            0,
            image.width as i32,
            image.height as i32,
            Some(image.pixels.as_ptr() as *const _),
            &info,
            DIB_RGB_COLORS,
            SRCCOPY,
        )
    };

    unsafe {
        let _ = SelectObject(mem_dc, old);
        let _ = DeleteDC(mem_dc);
    }

    if lines == 0 {
        unsafe {
            let _ = DeleteObject(hbitmap);
        }
        return Err("写入位图像素失败".to_string());
    }

    Ok(hbitmap)
}

fn path_to_wide(path: &Path) -> Vec<u16> {
    os_str_to_wide(path.as_os_str())
}

fn os_str_to_wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

struct PrinterGuard(HANDLE);
impl Drop for PrinterGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = ClosePrinter(self.0);
        }
    }
}

struct DcGuard(HDC);
impl Drop for DcGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

struct BitmapGuard(HBITMAP);
impl Drop for BitmapGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Gdi::{DM_PAPERLENGTH, DM_PAPERSIZE, DM_PAPERWIDTH};

    #[test]
    fn landscape_image_fills_landscape_page() {
        let rect = compute_destination_rect(3508, 2480, 2000, 1000);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        assert!(width > 3000, "expected wide draw width, got {width}");
        assert!(height > 1500, "expected proportional height, got {height}");
        let ratio = width as f64 / height as f64;
        assert!((ratio - 2.0).abs() < 0.05, "ratio={ratio}");
    }

    #[test]
    fn portrait_image_fills_portrait_page() {
        let rect = compute_destination_rect(2480, 3508, 1000, 2000);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        assert!(height > 3000, "expected tall draw height, got {height}");
        let ratio = height as f64 / width as f64;
        assert!((ratio - 2.0).abs() < 0.05, "ratio={ratio}");
    }

    #[test]
    fn square_image_is_centered_and_max_edge() {
        let rect = compute_destination_rect(2000, 1000, 500, 500);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        assert_eq!(width, height);
        assert_eq!(height, 1000);
        assert_eq!(rect.left, 500);
        assert_eq!(rect.top, 0);
    }

    #[test]
    fn rejects_missing_image_file() {
        let result = print_image_to_printer(
            Path::new("C:\\\\this-file-should-not-exist-printassist.png"),
            "Microsoft Print to PDF",
            1,
        );
        assert!(result.is_err());
    }

    #[test]
    fn landscape_content_uses_landscape_orientation_without_redefining_paper() {
        let mut devmode: DEVMODEW = unsafe { zeroed() };
        devmode.dmFields = DM_PAPERSIZE | DM_PAPERWIDTH | DM_PAPERLENGTH;
        devmode.Anonymous1.Anonymous1.dmPaperSize = 9; // DMPAPER_A4
        devmode.Anonymous1.Anonymous1.dmPaperWidth = 2100;
        devmode.Anonymous1.Anonymous1.dmPaperLength = 2970;

        let devmode_bytes = unsafe {
            std::slice::from_raw_parts_mut(
                (&mut devmode as *mut DEVMODEW).cast::<u8>(),
                size_of::<DEVMODEW>(),
            )
        };
        set_page_orientation_from_content(devmode_bytes, 2000, 1000);

        // Reading DEVMODE union fields requires unsafe.
        let (orientation, paper_size, paper_width, paper_length) = unsafe {
            (
                devmode.Anonymous1.Anonymous1.dmOrientation,
                devmode.Anonymous1.Anonymous1.dmPaperSize,
                devmode.Anonymous1.Anonymous1.dmPaperWidth,
                devmode.Anonymous1.Anonymous1.dmPaperLength,
            )
        };
        assert_eq!(
            orientation,
            windows::Win32::Graphics::Gdi::DMORIENT_LANDSCAPE as i16
        );
        assert_eq!(paper_size, 9);
        assert_eq!(paper_width, 2100);
        assert_eq!(paper_length, 2970);
        assert_eq!(devmode.dmFields & DM_PAPERSIZE, DM_PAPERSIZE);
        assert_eq!(devmode.dmFields & DM_PAPERWIDTH, DM_PAPERWIDTH);
        assert_eq!(devmode.dmFields & DM_PAPERLENGTH, DM_PAPERLENGTH);
    }

    #[test]
    fn portrait_content_uses_portrait_orientation_without_redefining_paper() {
        let mut devmode: DEVMODEW = unsafe { zeroed() };
        devmode.dmFields = DM_PAPERSIZE;
        devmode.Anonymous1.Anonymous1.dmPaperSize = 9; // DMPAPER_A4
        devmode.Anonymous1.Anonymous1.dmPaperWidth = 2100;
        devmode.Anonymous1.Anonymous1.dmPaperLength = 2970;

        let devmode_bytes = unsafe {
            std::slice::from_raw_parts_mut(
                (&mut devmode as *mut DEVMODEW).cast::<u8>(),
                size_of::<DEVMODEW>(),
            )
        };
        set_page_orientation_from_content(devmode_bytes, 1000, 2000);

        let (orientation, paper_size, paper_width, paper_length) = unsafe {
            (
                devmode.Anonymous1.Anonymous1.dmOrientation,
                devmode.Anonymous1.Anonymous1.dmPaperSize,
                devmode.Anonymous1.Anonymous1.dmPaperWidth,
                devmode.Anonymous1.Anonymous1.dmPaperLength,
            )
        };
        assert_eq!(orientation, DMORIENT_PORTRAIT as i16);
        assert_eq!(paper_size, 9);
        assert_eq!(paper_width, 2100);
        assert_eq!(paper_length, 2970);
        assert_eq!(devmode.dmFields & DM_PAPERSIZE, DM_PAPERSIZE);
    }
}
