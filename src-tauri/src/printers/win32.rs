use std::mem::{align_of, size_of};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER};
use windows::Win32::Graphics::Printing::{
    EnumPrintersW, GetDefaultPrinterW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL,
    PRINTER_INFO_2W, PRINTER_STATUS_DOOR_OPEN, PRINTER_STATUS_ERROR, PRINTER_STATUS_NOT_AVAILABLE,
    PRINTER_STATUS_NO_TONER, PRINTER_STATUS_OFFLINE, PRINTER_STATUS_OUTPUT_BIN_FULL,
    PRINTER_STATUS_OUT_OF_MEMORY, PRINTER_STATUS_PAGE_PUNT, PRINTER_STATUS_PAPER_JAM,
    PRINTER_STATUS_PAPER_OUT, PRINTER_STATUS_PAPER_PROBLEM, PRINTER_STATUS_SERVER_OFFLINE,
    PRINTER_STATUS_SERVER_UNKNOWN, PRINTER_STATUS_USER_INTERVENTION,
};
use windows::Win32::Storage::Xps::{DeviceCapabilitiesW, DC_COLORDEVICE, DC_DUPLEX};

use crate::contracts::{
    CapabilitySource, CapabilitySupport, PrinterCapability, PrinterOperationalState, SystemPrinter,
};

const ENUMERATION_FLAGS: u32 = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
const ERROR_STATUS_FLAGS: u32 = PRINTER_STATUS_ERROR
    | PRINTER_STATUS_DOOR_OPEN
    | PRINTER_STATUS_NO_TONER
    | PRINTER_STATUS_OUTPUT_BIN_FULL
    | PRINTER_STATUS_OUT_OF_MEMORY
    | PRINTER_STATUS_PAGE_PUNT
    | PRINTER_STATUS_PAPER_JAM
    | PRINTER_STATUS_PAPER_OUT
    | PRINTER_STATUS_PAPER_PROBLEM
    | PRINTER_STATUS_USER_INTERVENTION;
const OFFLINE_STATUS_FLAGS: u32 =
    PRINTER_STATUS_OFFLINE | PRINTER_STATUS_NOT_AVAILABLE | PRINTER_STATUS_SERVER_OFFLINE;

pub fn list_system_printers_sync() -> Result<Vec<SystemPrinter>, String> {
    let default_printer_name = get_default_printer_name()?;
    let printer_records = enumerate_printer_records()?;

    Ok(printer_records
        .into_iter()
        .map(|printer_record| build_system_printer(printer_record, default_printer_name.as_deref()))
        .collect())
}

fn enumerate_printer_records() -> Result<Vec<PrinterRecord>, String> {
    let mut required_bytes = 0_u32;
    let mut returned_printers = 0_u32;

    let first_result = unsafe {
        EnumPrintersW(
            ENUMERATION_FLAGS,
            PCWSTR::null(),
            2,
            None,
            &mut required_bytes,
            &mut returned_printers,
        )
    };

    if required_bytes == 0 {
        return match first_result {
            Ok(()) => Ok(Vec::new()),
            Err(error) => Err(format!("EnumPrintersW size query failed: {error}")),
        };
    }

    let mut aligned_buffer = AlignedPrinterBuffer::new(required_bytes as usize);
    let enumeration_result = unsafe {
        EnumPrintersW(
            ENUMERATION_FLAGS,
            PCWSTR::null(),
            2,
            Some(aligned_buffer.as_bytes_mut()),
            &mut required_bytes,
            &mut returned_printers,
        )
    };
    enumeration_result.map_err(|error| format!("EnumPrintersW enumeration failed: {error}"))?;

    let printer_information = unsafe {
        std::slice::from_raw_parts(
            aligned_buffer.as_ptr().cast::<PRINTER_INFO_2W>(),
            returned_printers as usize,
        )
    };

    Ok(printer_information
        .iter()
        .map(|information| PrinterRecord {
            name: wide_pointer_to_string(information.pPrinterName),
            port_name: wide_pointer_to_optional_string(information.pPortName),
            status_code: information.Status,
        })
        .collect())
}

fn get_default_printer_name() -> Result<Option<String>, String> {
    let mut required_characters = 0_u32;
    let size_result = unsafe { GetDefaultPrinterW(PWSTR::null(), &mut required_characters) };

    if size_result.as_bool() {
        return Ok(None);
    }

    let size_error = unsafe { GetLastError() };
    if required_characters == 0 {
        // ERROR_FILE_NOT_FOUND means this user has no default printer configured.
        return if size_error.0 == 2 {
            Ok(None)
        } else {
            Err(format!(
                "GetDefaultPrinterW size query failed with Win32 error {}",
                size_error.0
            ))
        };
    }

    if size_error != ERROR_INSUFFICIENT_BUFFER {
        return Err(format!(
            "GetDefaultPrinterW size query failed with Win32 error {}",
            size_error.0
        ));
    }

    let mut name_buffer = vec![0_u16; required_characters as usize];
    let query_result =
        unsafe { GetDefaultPrinterW(PWSTR(name_buffer.as_mut_ptr()), &mut required_characters) };
    if !query_result.as_bool() {
        let query_error = unsafe { GetLastError() };
        return Err(format!(
            "GetDefaultPrinterW query failed with Win32 error {}",
            query_error.0
        ));
    }

    Ok(Some(wide_slice_to_string(&name_buffer)))
}

fn build_system_printer(
    printer_record: PrinterRecord,
    default_printer_name: Option<&str>,
) -> SystemPrinter {
    let color = query_boolean_capability(
        &printer_record.name,
        printer_record.port_name.as_deref(),
        DC_COLORDEVICE,
        "color",
    );
    let duplex = query_boolean_capability(
        &printer_record.name,
        printer_record.port_name.as_deref(),
        DC_DUPLEX,
        "duplex",
    );

    let mut capability_errors = Vec::new();
    if let Some(detail) = color.detail.clone() {
        capability_errors.push(detail);
    }
    if let Some(detail) = duplex.detail.clone() {
        capability_errors.push(detail);
    }

    SystemPrinter {
        is_default: default_printer_name
            .map(|default_name| default_name.eq_ignore_ascii_case(&printer_record.name))
            .unwrap_or(false),
        state: operational_state_from_status(printer_record.status_code),
        status_code: printer_record.status_code,
        name: printer_record.name,
        port_name: printer_record.port_name,
        color,
        duplex,
        error: (!capability_errors.is_empty()).then(|| capability_errors.join("; ")),
    }
}

fn query_boolean_capability(
    printer_name: &str,
    port_name: Option<&str>,
    capability: windows::Win32::Storage::Xps::PRINTER_DEVICE_CAPABILITIES,
    capability_name: &str,
) -> PrinterCapability {
    let printer_name_wide = null_terminated_wide(printer_name);
    let port_name_wide = port_name.map(null_terminated_wide);
    let port_pointer = port_name_wide
        .as_ref()
        .map(|wide_name| PCWSTR(wide_name.as_ptr()))
        .unwrap_or(PCWSTR::null());

    let query_result = unsafe {
        DeviceCapabilitiesW(
            PCWSTR(printer_name_wide.as_ptr()),
            port_pointer,
            capability,
            PWSTR::null(),
            None,
        )
    };

    match query_result {
        1 => PrinterCapability {
            support: CapabilitySupport::Supported,
            source: CapabilitySource::Driver,
            detail: None,
        },
        0 => PrinterCapability {
            support: CapabilitySupport::Unsupported,
            source: CapabilitySource::Driver,
            detail: None,
        },
        _ => {
            let win32_error = unsafe { GetLastError() };
            PrinterCapability {
                support: CapabilitySupport::Unknown,
                source: CapabilitySource::Unavailable,
                detail: Some(format!(
                    "{capability_name} capability query failed with Win32 error {}",
                    win32_error.0
                )),
            }
        }
    }
}

fn operational_state_from_status(status_code: u32) -> PrinterOperationalState {
    if status_code == 0 {
        PrinterOperationalState::Ready
    } else if status_code & OFFLINE_STATUS_FLAGS != 0 {
        PrinterOperationalState::Offline
    } else if status_code & ERROR_STATUS_FLAGS != 0 {
        PrinterOperationalState::Error
    } else if status_code & PRINTER_STATUS_SERVER_UNKNOWN != 0 {
        PrinterOperationalState::Unknown
    } else {
        PrinterOperationalState::Ready
    }
}

fn null_terminated_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_pointer_to_optional_string(pointer: PWSTR) -> Option<String> {
    if pointer.is_null() {
        None
    } else {
        Some(wide_pointer_to_string(pointer))
    }
}

fn wide_pointer_to_string(pointer: PWSTR) -> String {
    if pointer.is_null() {
        return String::new();
    }

    let mut character_count = 0_usize;
    unsafe {
        while *pointer.0.add(character_count) != 0 {
            character_count += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(pointer.0, character_count))
    }
}

fn wide_slice_to_string(value: &[u16]) -> String {
    let character_count = value
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..character_count])
}

#[derive(Debug)]
struct PrinterRecord {
    name: String,
    port_name: Option<String>,
    status_code: u32,
}

struct AlignedPrinterBuffer {
    storage: Vec<usize>,
    byte_length: usize,
}

impl AlignedPrinterBuffer {
    fn new(byte_length: usize) -> Self {
        debug_assert!(align_of::<usize>() >= align_of::<PRINTER_INFO_2W>());
        let word_count = (byte_length + size_of::<usize>() - 1) / size_of::<usize>();
        Self {
            storage: vec![0; word_count],
            byte_length,
        }
    }

    fn as_ptr(&self) -> *const u8 {
        self.storage.as_ptr().cast()
    }

    fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(self.storage.as_mut_ptr().cast(), self.byte_length)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_zero_status_to_ready() {
        assert_eq!(
            operational_state_from_status(0),
            PrinterOperationalState::Ready
        );
    }

    #[test]
    fn gives_offline_status_precedence_over_error_flags() {
        assert_eq!(
            operational_state_from_status(PRINTER_STATUS_OFFLINE | PRINTER_STATUS_PAPER_JAM),
            PrinterOperationalState::Offline
        );
    }

    #[test]
    fn maps_actionable_faults_to_error() {
        assert_eq!(
            operational_state_from_status(PRINTER_STATUS_PAPER_OUT),
            PrinterOperationalState::Error
        );
    }

    #[test]
    fn maps_server_unknown_to_unknown() {
        assert_eq!(
            operational_state_from_status(PRINTER_STATUS_SERVER_UNKNOWN),
            PrinterOperationalState::Unknown
        );
    }
}
