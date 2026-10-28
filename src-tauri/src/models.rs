use serde::Serialize;

use crate::config::{GeneralConfig, ShortcutBinding};
use crate::error::AppError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputOption {
    pub id: String,
    pub label: String,
    pub supported: bool,
    pub transport: String,
    pub reason: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplaySummary {
    pub id: String,
    pub name: String,
    pub profile_id: String,
    pub profile_label: String,
    pub profile_matched: bool,
    pub backend: String,
    pub vendor_id: Option<String>,
    pub model_id: Option<u16>,
    pub model_name: Option<String>,
    pub serial_number: Option<String>,
    pub inputs: Vec<InputOption>,
    pub input_switch_supported: bool,
    pub current_input_vcp_code: u8,
    pub raw_i2c_available: bool,
    pub raw_i2c_backend: Option<String>,
    pub raw_i2c_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayList {
    pub displays: Vec<DisplaySummary>,
    pub discovery_error: Option<AppError>,
    pub config_error: Option<String>,
    pub config_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CurrentInputState {
    pub input_id: Option<String>,
    pub label: Option<String>,
    pub raw_value: Option<u16>,
    pub transport: Option<String>,
    pub read_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureValueState {
    pub supported: bool,
    pub editable: bool,
    pub percent: Option<u8>,
    pub raw_value: Option<u16>,
    pub min: u16,
    pub max: u16,
    pub step: u8,
    pub read_error: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MuteValueState {
    pub supported: bool,
    pub editable: bool,
    pub muted: Option<bool>,
    pub raw_value: Option<u16>,
    pub read_error: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplaySnapshot {
    pub display: DisplaySummary,
    pub current_input: CurrentInputState,
    pub brightness: FeatureValueState,
    pub volume: FeatureValueState,
    pub mute: MuteValueState,
    pub probed_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub id: String,
    pub label: String,
    pub priority: i32,
    pub inputs: Vec<InputOption>,
    pub matched_display_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigInfo {
    pub version: String,
    pub os: String,
    pub arch: String,
    pub config_path: String,
    pub config_dir: String,
    pub log_path: String,
    pub config_error: Option<String>,
    pub config_warnings: Vec<String>,
    pub general: GeneralConfig,
    pub profiles: Vec<ProfileSummary>,
    pub shortcuts: Vec<ShortcutBinding>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticDisplay {
    pub id: String,
    pub name: String,
    pub backend: String,
    pub native_id: String,
    pub vendor_id: Option<String>,
    pub model_id: Option<u16>,
    pub model_name: Option<String>,
    pub serial_number: Option<String>,
    pub profile_id: String,
    pub profile_label: String,
    pub profile_matched: bool,
    pub capabilities_error: Option<String>,
    pub raw_i2c_available: bool,
    pub raw_i2c_backend: Option<String>,
    pub raw_i2c_reason: Option<String>,
    pub inputs: Vec<DiagnosticInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticInput {
    pub id: String,
    pub label: String,
    pub selector: String,
    pub supported: bool,
    pub transport: String,
    pub reason: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VcpRead {
    pub code: String,
    pub name: String,
    pub ok: bool,
    pub value: Option<u16>,
    pub maximum: Option<u16>,
    pub ty: Option<u8>,
    pub sh: Option<u8>,
    pub sl: Option<u8>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReport {
    pub generated_at: String,
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub display: DiagnosticDisplay,
    pub current_input: CurrentInputState,
    pub brightness: FeatureValueState,
    pub volume: FeatureValueState,
    pub mute: MuteValueState,
    pub capabilities: Option<String>,
    pub capabilities_error: Option<String>,
    pub vcp_reads: Vec<VcpRead>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    pub action: String,
    pub label: String,
    pub accelerator: String,
    pub registered: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub os: String,
    pub arch: String,
    pub config_path: String,
    pub log_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapPayload {
    pub app: AppInfo,
    pub config: ConfigInfo,
    pub displays: DisplayList,
}
