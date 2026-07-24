//! In-process Office COM automation via IDispatch (no PowerShell host process).
//!
//! Uses the already-linked `windows` crate only: CoCreateInstance + IDispatch
//! Invoke. Desktop Office must be installed; no commercial PDF/Office SDK is
//! bundled, so binary size stays essentially unchanged.

use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};

use windows::core::{Interface, BSTR, GUID, IUnknown, PCWSTR, VARIANT};
use windows::Win32::System::Com::{
    CLSIDFromProgID, CoCreateInstance, CoInitializeEx, CoUninitialize, IDispatch,
    CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, DISPATCH_FLAGS, DISPATCH_METHOD,
    DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT, DISPPARAMS, EXCEPINFO,
};
use windows::Win32::System::Ole::DISPID_PROPERTYPUT;

/// COM apartment scope for the calling thread.
pub struct ComApartment {
    should_uninitialize: bool,
}

impl ComApartment {
    pub fn enter() -> Result<Self, String> {
        let initialize_result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if initialize_result.is_ok() {
            return Ok(Self {
                should_uninitialize: true,
            });
        }
        let code = initialize_result.0 as u32;
        // RPC_E_CHANGED_MODE — already initialized in another apartment mode.
        if code == 0x8001_0106 {
            return Ok(Self {
                should_uninitialize: false,
            });
        }
        Err(format!("初始化 COM 失败：0x{code:08X}"))
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.should_uninitialize {
            unsafe { CoUninitialize() };
        }
    }
}

/// Thin wrapper around an Office `IDispatch` object.
pub struct DispatchObject {
    dispatch: IDispatch,
}

impl DispatchObject {
    pub fn create_application(prog_id: &str) -> Result<Self, String> {
        let prog_id_wide = windows::core::HSTRING::from(prog_id);
        let class_id = unsafe { CLSIDFromProgID(&prog_id_wide) }.map_err(|error| {
            format!("找不到 {prog_id}（请确认已安装桌面版 Office）：{error}")
        })?;

        let dispatch: IDispatch =
            unsafe { CoCreateInstance(&class_id, None, CLSCTX_LOCAL_SERVER) }.map_err(|error| {
                format!("创建 {prog_id} 失败（请确认已安装桌面版 Office）：{error}")
            })?;

        Ok(Self { dispatch })
    }

    pub fn from_variant(variant: &VARIANT) -> Result<Self, String> {
        let dispatch = variant_to_idispatch(variant)?;
        Ok(Self { dispatch })
    }

    pub fn put_bool_property(&self, name: &str, value: bool) -> Result<(), String> {
        let _ = self.invoke(name, DISPATCH_PROPERTYPUT, vec![VARIANT::from(value)])?;
        Ok(())
    }

    pub fn put_i32_property(&self, name: &str, value: i32) -> Result<(), String> {
        let _ = self.invoke(name, DISPATCH_PROPERTYPUT, vec![VARIANT::from(value)])?;
        Ok(())
    }

    pub fn put_string_property(&self, name: &str, value: &str) -> Result<(), String> {
        let _ = self.invoke(name, DISPATCH_PROPERTYPUT, vec![VARIANT::from(value)])?;
        Ok(())
    }

    pub fn get_property(&self, name: &str) -> Result<VARIANT, String> {
        self.invoke(name, DISPATCH_PROPERTYGET, Vec::new())
    }

    pub fn get_object_property(&self, name: &str) -> Result<Self, String> {
        let variant = self.get_property(name)?;
        Self::from_variant(&variant)
    }

    pub fn call(&self, name: &str, args: Vec<VARIANT>) -> Result<VARIANT, String> {
        self.invoke(name, DISPATCH_METHOD, args)
    }

    pub fn call_unit(&self, name: &str, args: Vec<VARIANT>) -> Result<(), String> {
        let _ = self.call(name, args)?;
        Ok(())
    }

    fn invoke(
        &self,
        name: &str,
        flags: DISPATCH_FLAGS,
        args_in_normal_order: Vec<VARIANT>,
    ) -> Result<VARIANT, String> {
        let member_dispid = resolve_dispid(&self.dispatch, name)?;

        // IDispatch::Invoke expects arguments in reverse order.
        let mut arguments = args_in_normal_order;
        arguments.reverse();

        let mut named_dispid = DISPID_PROPERTYPUT;
        let parameters = if flags == DISPATCH_PROPERTYPUT {
            if arguments.is_empty() {
                return Err(format!("属性写入 {name} 缺少值"));
            }
            DISPPARAMS {
                rgvarg: arguments.as_mut_ptr(),
                rgdispidNamedArgs: &mut named_dispid,
                cArgs: arguments.len() as u32,
                cNamedArgs: 1,
            }
        } else {
            DISPPARAMS {
                rgvarg: if arguments.is_empty() {
                    std::ptr::null_mut()
                } else {
                    arguments.as_mut_ptr()
                },
                rgdispidNamedArgs: std::ptr::null_mut(),
                cArgs: arguments.len() as u32,
                cNamedArgs: 0,
            }
        };

        let mut result = VARIANT::new();
        let mut exception_info = EXCEPINFO::default();
        let mut argument_error = 0_u32;

        let invoke_result = unsafe {
            self.dispatch.Invoke(
                member_dispid,
                &GUID::zeroed(),
                0,
                flags,
                &parameters,
                Some(&mut result),
                Some(&mut exception_info),
                Some(&mut argument_error),
            )
        };

        // Keep argument VARIANTs alive for the duration of Invoke.
        drop(arguments);

        if let Err(error) = invoke_result {
            let detail = format_exception_info(&exception_info);
            free_exception_info(&mut exception_info);
            if detail.is_empty() {
                return Err(format!("调用 {name} 失败：{error}"));
            }
            return Err(format!("调用 {name} 失败：{error}；{detail}"));
        }

        free_exception_info(&mut exception_info);
        Ok(result)
    }
}

fn resolve_dispid(dispatch: &IDispatch, name: &str) -> Result<i32, String> {
    let mut wide_name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let name_pointer = PCWSTR(wide_name.as_mut_ptr());
    let mut dispid = 0_i32;
    unsafe {
        dispatch.GetIDsOfNames(&GUID::zeroed(), &name_pointer, 1, 0, &mut dispid)
    }
    .map_err(|error| format!("找不到成员 {name}：{error}"))?;
    Ok(dispid)
}

fn variant_to_idispatch(variant: &VARIANT) -> Result<IDispatch, String> {
    // VT_DISPATCH = 9, VT_UNKNOWN = 13
    const VT_DISPATCH: u16 = 9;
    const VT_UNKNOWN: u16 = 13;

    unsafe {
        let raw = variant.as_raw();
        let variant_type = raw.Anonymous.Anonymous.vt;
        if variant_type != VT_DISPATCH && variant_type != VT_UNKNOWN {
            return Err(format!("期望 COM 对象，实际 VARIANT 类型为 {variant_type}"));
        }

        let interface_pointer = raw.Anonymous.Anonymous.Anonymous.pdispVal;
        if interface_pointer.is_null() {
            return Err("COM 对象为空".to_string());
        }

        // VARIANT still owns the pointer; QueryInterface via cast AddRefs for our handle.
        let unknown = ManuallyDrop::new(IUnknown::from_raw(interface_pointer));
        unknown
            .cast::<IDispatch>()
            .map_err(|error| format!("无法取得 IDispatch：{error}"))
    }
}

fn format_exception_info(exception_info: &EXCEPINFO) -> String {
    let description = exception_info.bstrDescription.to_string();
    let source = exception_info.bstrSource.to_string();
    match (description.is_empty(), source.is_empty()) {
        (true, true) => String::new(),
        (false, true) => description,
        (true, false) => source,
        (false, false) => format!("{source}: {description}"),
    }
}

fn free_exception_info(exception_info: &mut EXCEPINFO) {
    unsafe {
        ManuallyDrop::drop(&mut exception_info.bstrSource);
        exception_info.bstrSource = ManuallyDrop::new(BSTR::new());
        ManuallyDrop::drop(&mut exception_info.bstrDescription);
        exception_info.bstrDescription = ManuallyDrop::new(BSTR::new());
        ManuallyDrop::drop(&mut exception_info.bstrHelpFile);
        exception_info.bstrHelpFile = ManuallyDrop::new(BSTR::new());
    }
}

/// Absolute path text suitable for Office Open/Export APIs.
pub fn absolute_path_text(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Err(format!("文件不存在：{}", path.display()));
    }
    let absolute = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Ok(absolute
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string())
}

pub fn temporary_pdf_output_path(source_path: &Path) -> PathBuf {
    std::env::temp_dir().join(format!(
        "printassist-{}-{}.pdf",
        std::process::id(),
        source_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("document")
    ))
}

/// Try bare printer name and common `on Ne0x:` ActivePrinter forms.
pub fn set_active_printer(application: &DispatchObject, printer_name: &str) -> Result<(), String> {
    let candidates = [
        printer_name.to_string(),
        format!("{printer_name} on Ne00:"),
        format!("{printer_name} on Ne01:"),
        format!("{printer_name} on Ne02:"),
    ];

    let mut last_error = String::new();
    for candidate in candidates {
        match application.put_string_property("ActivePrinter", &candidate) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = error,
        }
    }
    Err(format!(
        "无法切换到打印机：{printer_name}（最后错误：{last_error}）"
    ))
}
