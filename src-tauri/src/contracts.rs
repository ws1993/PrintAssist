use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilitySupport {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilitySource {
    Driver,
    System,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrinterOperationalState {
    Ready,
    Offline,
    Error,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterCapability {
    pub support: CapabilitySupport,
    pub source: CapabilitySource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPrinter {
    pub name: String,
    pub port_name: Option<String>,
    pub is_default: bool,
    pub state: PrinterOperationalState,
    pub status_code: u32,
    pub color: PrinterCapability,
    pub duplex: PrinterCapability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedPrintSettingsPayload {
    pub printer_name: String,
    pub color_mode: String,
    pub sides_mode: String,
    pub flip_mode: String,
    pub copies: u32,
    pub page_range_mode: String,
    pub page_range_expression: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintQueueItemPayload {
    pub queue_item_id: String,
    pub path: String,
    pub file_name: String,
    pub settings: ResolvedPrintSettingsPayload,
    pub allow_association_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintBatchRequest {
    pub items: Vec<PrintQueueItemPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintBatchResultItem {
    pub queue_item_id: String,
    pub path: String,
    pub file_name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintBatchResult {
    pub succeeded: u32,
    pub failed: u32,
    pub skipped: u32,
    pub results: Vec<PrintBatchResultItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    pub use_system_proxy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_proxy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_contract_using_frontend_field_names() {
        let printer = SystemPrinter {
            name: "Office Printer".to_string(),
            port_name: Some("USB001".to_string()),
            is_default: true,
            state: PrinterOperationalState::Ready,
            status_code: 0,
            color: PrinterCapability {
                support: CapabilitySupport::Supported,
                source: CapabilitySource::Driver,
                detail: None,
            },
            duplex: PrinterCapability {
                support: CapabilitySupport::Unknown,
                source: CapabilitySource::Unavailable,
                detail: Some("driver query failed".to_string()),
            },
            error: None,
        };

        let value = serde_json::to_value(printer).expect("contract should serialize");

        assert_eq!(value["portName"], "USB001");
        assert_eq!(value["isDefault"], true);
        assert_eq!(value["color"]["support"], "supported");
        assert_eq!(value["duplex"]["support"], "unknown");
        assert!(value.get("error").is_none());
    }
}
