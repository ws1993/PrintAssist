#[cfg(windows)]
mod win32;

#[cfg(windows)]
pub use win32::list_system_printers_sync;

#[cfg(not(windows))]
use crate::contracts::SystemPrinter;

#[cfg(not(windows))]
pub fn list_system_printers_sync() -> Result<Vec<SystemPrinter>, String> {
    Err("system printer discovery is unsupported on this operating system".to_string())
}
