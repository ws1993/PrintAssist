//! Fast PDF page rasterization via IPdfRendererNative + Direct2D.
//!
//! Avoids the expensive WinRT "render to encoded stream → BitmapDecoder"
//! round-trip. Pages are drawn into a D2D target bitmap and read back as BGRA.

use windows::core::{Interface, IUnknown};
use windows::Data::Pdf::PdfPage;
use windows::Foundation::Size;
use windows::Win32::Foundation::BOOLEAN;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_COLOR_F, D2D_RECT_F,
    D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Bitmap1, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1,
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_CPU_READ, D2D1_BITMAP_OPTIONS_TARGET,
    D2D1_BITMAP_PROPERTIES1, D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_MAP_OPTIONS_READ,
};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Pdf::{
    PdfCreateRenderer, IPdfRendererNative, PDF_RENDER_PARAMS,
};

use super::image_print::DecodedImage;

/// Holds a reusable D2D/D3D context for multi-page PDF rasterization.
pub struct NativePdfRasterContext {
    _d3d_device: ID3D11Device,
    _dxgi_device: IDXGIDevice,
    _d2d_factory: ID2D1Factory1,
    _d2d_device: ID2D1Device,
    d2d_context: ID2D1DeviceContext,
    pdf_renderer: IPdfRendererNative,
}

impl NativePdfRasterContext {
    pub fn new() -> Result<Self, String> {
        let mut d3d_device: Option<ID3D11Device> = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d_device),
                None,
                None,
            )
            .map_err(|error| format!("创建 D3D11 设备失败：{error}"))?;
        }
        let d3d_device = d3d_device.ok_or_else(|| "D3D11 设备为空".to_string())?;
        let dxgi_device: IDXGIDevice = d3d_device
            .cast()
            .map_err(|error| format!("获取 DXGI 设备失败：{error}"))?;

        let d2d_factory: ID2D1Factory1 = unsafe {
            D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)
                .map_err(|error| format!("创建 D2D 工厂失败：{error}"))?
        };
        let d2d_device = unsafe {
            d2d_factory
                .CreateDevice(&dxgi_device)
                .map_err(|error| format!("创建 D2D 设备失败：{error}"))?
        };
        let d2d_context = unsafe {
            d2d_device
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)
                .map_err(|error| format!("创建 D2D 设备上下文失败：{error}"))?
        };
        let pdf_renderer = unsafe {
            PdfCreateRenderer(&dxgi_device)
                .map_err(|error| format!("创建 PDF 原生渲染器失败：{error}"))?
        };

        Ok(Self {
            _d3d_device: d3d_device,
            _dxgi_device: dxgi_device,
            _d2d_factory: d2d_factory,
            _d2d_device: d2d_device,
            d2d_context,
            pdf_renderer,
        })
    }

    pub fn render_page(
        &self,
        page: &PdfPage,
        destination_width: u32,
        destination_height: u32,
    ) -> Result<DecodedImage, String> {
        let width = destination_width.max(1);
        let height = destination_height.max(1);

        let pixel_format = D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        };
        let target_properties = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: pixel_format,
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET,
            colorContext: std::mem::ManuallyDrop::new(None),
        };
        let target_bitmap: ID2D1Bitmap1 = unsafe {
            self.d2d_context
                .CreateBitmap(
                    D2D_SIZE_U { width, height },
                    None,
                    0,
                    &target_properties,
                )
                .map_err(|error| format!("创建 D2D 目标位图失败：{error}"))?
        };

        let white = D2D1_COLOR_F {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        };
        let white_pdf = D2D_COLOR_F {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        };
        unsafe {
            self.d2d_context.SetTarget(&target_bitmap);
            self.d2d_context.BeginDraw();
            self.d2d_context.Clear(Some(&white));
        }

        let page_unknown: IUnknown = page
            .cast()
            .map_err(|error| format!("PdfPage 转 IUnknown 失败：{error}"))?;
        let render_params = PDF_RENDER_PARAMS {
            SourceRect: D2D_RECT_F {
                left: 0.0,
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
            },
            DestinationWidth: width,
            DestinationHeight: height,
            BackgroundColor: white_pdf,
            IgnoreHighContrast: BOOLEAN(1),
        };
        unsafe {
            self.pdf_renderer
                .RenderPageToDeviceContext(&page_unknown, &self.d2d_context, Some(&render_params))
                .map_err(|error| format!("RenderPageToDeviceContext 失败：{error}"))?;
            self.d2d_context
                .EndDraw(None, None)
                .map_err(|error| format!("D2D EndDraw 失败：{error}"))?;
        }

        let cpu_properties = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: pixel_format,
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            colorContext: std::mem::ManuallyDrop::new(None),
        };
        let cpu_bitmap: ID2D1Bitmap1 = unsafe {
            self.d2d_context
                .CreateBitmap(
                    D2D_SIZE_U { width, height },
                    None,
                    0,
                    &cpu_properties,
                )
                .map_err(|error| format!("创建 D2D CPU 位图失败：{error}"))?
        };
        unsafe {
            cpu_bitmap
                .CopyFromBitmap(None, &target_bitmap, None)
                .map_err(|error| format!("复制 D2D 位图失败：{error}"))?;
        }

        let mapped = unsafe {
            cpu_bitmap
                .Map(D2D1_MAP_OPTIONS_READ)
                .map_err(|error| format!("映射 D2D 位图失败：{error}"))?
        };
        let pitch = mapped.pitch as usize;
        let expected_row = (width as usize)
            .checked_mul(4)
            .ok_or_else(|| "页面宽度过大".to_string())?;
        let total = expected_row
            .checked_mul(height as usize)
            .ok_or_else(|| "页面尺寸过大".to_string())?;
        let mut pixels = vec![0_u8; total];
        unsafe {
            for row in 0..height as usize {
                let source = mapped.bits.add(row * pitch);
                let destination = pixels.as_mut_ptr().add(row * expected_row);
                std::ptr::copy_nonoverlapping(source, destination, expected_row);
            }
            cpu_bitmap
                .Unmap()
                .map_err(|error| format!("Unmap D2D 位图失败：{error}"))?;
        }

        Ok(DecodedImage {
            width,
            height,
            pixels,
        })
    }
}

/// Convenience: page DIP size → destination pixels for a target DPI.
pub fn destination_pixels_for_page_size(
    size: Size,
    target_dpi: u32,
    max_edge: u32,
    pdf_dip_dpi: f64,
) -> (u32, u32) {
    let scale = f64::from(target_dpi) / pdf_dip_dpi;
    let mut width = (f64::from(size.Width) * scale).max(1.0);
    let mut height = (f64::from(size.Height) * scale).max(1.0);
    let max_side = width.max(height);
    if max_side > f64::from(max_edge) {
        let fit = f64::from(max_edge) / max_side;
        width *= fit;
        height *= fit;
    }
    (
        width.round().clamp(1.0, f64::from(u32::MAX)) as u32,
        height.round().clamp(1.0, f64::from(u32::MAX)) as u32,
    )
}
