use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

/// Prints a file to a named printer using the Shell "printto" verb.
/// This path relies on the file association or Office automation result.
pub fn print_file_to_printer(
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

    let copy_count = copies.max(1);
    for _ in 0..copy_count {
        shell_print_to(file_path, printer_name)?;
    }
    Ok(())
}

fn shell_print_to(file_path: &Path, printer_name: &str) -> Result<(), String> {
    let file_wide = path_to_wide(file_path);
    let printer_argument = format!("\"{}\"", printer_name.replace('"', ""));
    let printer_wide = os_str_to_wide(OsStr::new(&printer_argument));
    let operation = os_str_to_wide(OsStr::new("printto"));

    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file_wide.as_ptr()),
            PCWSTR(printer_wide.as_ptr()),
            PCWSTR::null(),
            SW_HIDE,
        )
    };

    // ShellExecute returns value > 32 on success.
    if result.0 as usize <= 32 {
        return Err(format!(
            "Shell printto 失败，返回码 {}（请确认文件关联或 Office 可用）",
            result.0 as usize
        ));
    }
    Ok(())
}

fn path_to_wide(path: &Path) -> Vec<u16> {
    os_str_to_wide(path.as_os_str())
}

fn os_str_to_wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_file() {
        let result = print_file_to_printer(
            Path::new("C:\\\\this-file-should-not-exist-printassist.pdf"),
            "Microsoft Print to PDF",
            1,
        );
        assert!(result.is_err());
    }
}
