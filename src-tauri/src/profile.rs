use std::collections::HashSet;
use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::config::UserConfig;
use crate::util::wildcard_match;

const BUILTIN_PROFILES: &str = include_str!("../profiles/builtin.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SupportSetting {
    #[default]
    Auto,
    Yes,
    No,
}

impl SupportSetting {
    pub fn is_disabled(self) -> bool {
        matches!(self, Self::No)
    }

    pub fn is_forced(self) -> bool {
        matches!(self, Self::Yes)
    }
}

impl Serialize for SupportSetting {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Auto => serializer.serialize_str("auto"),
            Self::Yes => serializer.serialize_bool(true),
            Self::No => serializer.serialize_bool(false),
        }
    }
}

struct SupportSettingVisitor;

impl<'de> Visitor<'de> for SupportSettingVisitor {
    type Value = SupportSetting;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("true, false or \"auto\"")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(if value {
            SupportSetting::Yes
        } else {
            SupportSetting::No
        })
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        if value.eq_ignore_ascii_case("auto") {
            Ok(SupportSetting::Auto)
        } else if value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes") {
            Ok(SupportSetting::Yes)
        } else if value.eq_ignore_ascii_case("false") || value.eq_ignore_ascii_case("no") {
            Ok(SupportSetting::No)
        } else {
            Err(E::custom(
                "supported must be true, false, or the string \"auto\"",
            ))
        }
    }
}

impl<'de> Deserialize<'de> for SupportSetting {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(SupportSettingVisitor)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct MatchRule {
    pub vendor_id: Option<String>,
    pub model_id: Option<u16>,
    pub model_name: Option<String>,
    pub model_name_any: Vec<String>,
    pub serial_pattern: Option<String>,
    pub backend: Option<String>,
}

impl MatchRule {
    pub fn is_empty(&self) -> bool {
        self.vendor_id.is_none()
            && self.model_id.is_none()
            && self.model_name.is_none()
            && self.model_name_any.is_empty()
            && self.serial_pattern.is_none()
            && self.backend.is_none()
    }

    pub fn matches(&self, identity: &DisplayIdentity) -> bool {
        if self.is_empty() {
            return false;
        }

        if let Some(vendor_id) = &self.vendor_id {
            let Some(actual) = &identity.vendor_id else {
                return false;
            };
            if !actual.eq_ignore_ascii_case(vendor_id) {
                return false;
            }
        }

        if let Some(model_id) = self.model_id {
            if identity.model_id != Some(model_id) {
                return false;
            }
        }

        if let Some(pattern) = &self.model_name {
            let Some(model_name) = &identity.model_name else {
                return false;
            };
            if !wildcard_match(pattern, model_name)
                && !model_name.to_lowercase().contains(&pattern.to_lowercase())
            {
                return false;
            }
        }

        if !self.model_name_any.is_empty() {
            let Some(model_name) = &identity.model_name else {
                return false;
            };
            if !self.model_name_any.iter().any(|pattern| {
                wildcard_match(pattern, model_name)
                    || model_name.to_lowercase().contains(&pattern.to_lowercase())
            }) {
                return false;
            }
        }

        if let Some(pattern) = &self.serial_pattern {
            let Some(serial) = &identity.serial_number else {
                return false;
            };
            if !wildcard_match(pattern, serial) {
                return false;
            }
        }

        if let Some(backend) = &self.backend {
            if !identity.backend.eq_ignore_ascii_case(backend) {
                return false;
            }
        }

        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayIdentity {
    pub backend: String,
    pub native_id: String,
    pub vendor_id: Option<String>,
    pub model_id: Option<u16>,
    pub model_name: Option<String>,
    pub serial_number: Option<String>,
    pub edid_serial: Option<u32>,
}

impl DisplayIdentity {
    pub fn display_name(&self) -> String {
        if let Some(model_name) = self
            .model_name
            .as_ref()
            .filter(|name| !name.trim().is_empty())
        {
            return model_name.trim().to_string();
        }
        match (&self.vendor_id, self.model_id) {
            (Some(vendor), Some(model)) => format!("{vendor} {model:04X}"),
            (None, Some(model)) => format!("显示器 {model:04X}"),
            _ => format!("显示器 {}", self.native_id),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RangeCapability {
    pub supported: SupportSetting,
    pub vcp_code: u8,
    pub min: u16,
    pub max: u16,
    pub step: u8,
}

impl RangeCapability {
    pub fn brightness_default() -> Self {
        Self {
            supported: SupportSetting::Yes,
            vcp_code: 0x10,
            min: 0,
            max: 255,
            step: 5,
        }
    }

    pub fn volume_default() -> Self {
        Self {
            supported: SupportSetting::Auto,
            vcp_code: 0x62,
            min: 0,
            max: 255,
            step: 5,
        }
    }
}

impl Default for RangeCapability {
    fn default() -> Self {
        Self::brightness_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MuteCapability {
    pub supported: SupportSetting,
    pub vcp_code: u8,
    pub on_value: u16,
    pub off_value: u16,
}

impl Default for MuteCapability {
    fn default() -> Self {
        Self {
            supported: SupportSetting::Auto,
            vcp_code: 0x8D,
            on_value: 1,
            off_value: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Capabilities {
    pub input_switch: bool,
    pub brightness: RangeCapability,
    pub volume: RangeCapability,
    pub mute: MuteCapability,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            input_switch: true,
            brightness: RangeCapability::brightness_default(),
            volume: RangeCapability::volume_default(),
            mute: MuteCapability::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VendorWriteMode {
    #[default]
    Table,
    Vcp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputSelector {
    Standard {
        vcp_code: u8,
        value: u16,
    },
    Vendor {
        vcp_code: u8,
        value: u16,
        #[serde(default)]
        data_address: u16,
        #[serde(default)]
        write_mode: VendorWriteMode,
    },
}

impl InputSelector {
    pub fn vcp_code(&self) -> u8 {
        match self {
            Self::Standard { vcp_code, .. } | Self::Vendor { vcp_code, .. } => *vcp_code,
        }
    }

    pub fn value(&self) -> u16 {
        match self {
            Self::Standard { value, .. } | Self::Vendor { value, .. } => *value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputSource {
    pub id: String,
    pub label: String,
    pub selector: InputSelector,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DisplayProfile {
    pub id: String,
    pub label: String,
    pub priority: i32,
    #[serde(rename = "match")]
    pub match_rule: MatchRule,
    pub current_input_vcp_code: u8,
    pub capabilities: Capabilities,
    pub inputs: Vec<InputSource>,
}

impl Default for DisplayProfile {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            priority: 0,
            match_rule: MatchRule::default(),
            current_input_vcp_code: 0x60,
            capabilities: Capabilities::default(),
            inputs: Vec::new(),
        }
    }
}

impl DisplayProfile {
    pub fn input(&self, id: &str) -> Option<&InputSource> {
        self.inputs.iter().find(|input| input.id == id)
    }

    pub fn input_for_value(&self, value: u16) -> Option<&InputSource> {
        self.inputs
            .iter()
            .find(|input| input.selector.value() == value)
    }

    fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("显示器配置缺少 id".into());
        }
        if !self
            .id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            return Err(format!(
                "配置 id '{}' 只能包含字母、数字、'-' 和 '_'",
                self.id
            ));
        }
        if self.label.trim().is_empty() {
            return Err(format!("配置 '{}' 缺少 label", self.id));
        }
        if self.capabilities.brightness.max < self.capabilities.brightness.min {
            return Err(format!("配置 '{}' 的亮度 max 小于 min", self.id));
        }
        if self.capabilities.volume.max < self.capabilities.volume.min {
            return Err(format!("配置 '{}' 的音量 max 小于 min", self.id));
        }
        if self.capabilities.brightness.step == 0 || self.capabilities.volume.step == 0 {
            return Err(format!("配置 '{}' 的 step 必须大于 0", self.id));
        }

        let mut ids = HashSet::new();
        for input in &self.inputs {
            if input.id.trim().is_empty() || input.label.trim().is_empty() {
                return Err(format!("配置 '{}' 包含缺少 id/label 的输入源", self.id));
            }
            if !ids.insert(input.id.as_str()) {
                return Err(format!(
                    "配置 '{}' 包含重复的输入源 id '{}'",
                    self.id, input.id
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct ProfilesFile {
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    profiles: Vec<DisplayProfile>,
}

#[derive(Debug, Clone)]
pub struct ProfileCatalog {
    profiles: Vec<DisplayProfile>,
    warnings: Vec<String>,
}

impl ProfileCatalog {
    pub fn load(user: &UserConfig) -> Result<Self, String> {
        let mut root: toml::Value = toml::from_str(BUILTIN_PROFILES)
            .map_err(|error| format!("内置显示器配置无效：{error}"))?;
        let mut warnings = Vec::new();

        let builtin_ids: HashSet<String> = root
            .get("profiles")
            .and_then(toml::Value::as_array)
            .map(|profiles| {
                profiles
                    .iter()
                    .filter_map(|profile| {
                        profile
                            .get("id")
                            .and_then(toml::Value::as_str)
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();

        if let Some(profiles) = root.get_mut("profiles").and_then(toml::Value::as_array_mut) {
            for (id, patch) in &user.profile_overrides {
                if !builtin_ids.contains(id) {
                    warnings.push(format!(
                        "profile_overrides.{id} 未匹配任何内置配置，已忽略。"
                    ));
                    continue;
                }
                if let Some(profile) = profiles.iter_mut().find(|profile| {
                    profile.get("id").and_then(toml::Value::as_str) == Some(id.as_str())
                }) {
                    merge_toml(profile, patch.clone());
                }
            }
        }

        let mut file: ProfilesFile = root
            .try_into()
            .map_err(|error| format!("显示器配置解析失败：{error}"))?;

        if file.schema_version != 0 && file.schema_version != 1 {
            return Err(format!(
                "不支持的显示器配置 schema_version {}",
                file.schema_version
            ));
        }

        file.profiles.extend(user.custom_profiles.iter().cloned());
        if file.profiles.is_empty() {
            return Err("显示器配置列表为空".into());
        }

        file.profiles
            .sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));

        let mut ids = HashSet::new();
        for profile in &file.profiles {
            profile.validate()?;
            if !ids.insert(profile.id.clone()) {
                return Err(format!("显示器配置 id '{}' 重复", profile.id));
            }
        }

        if !file
            .profiles
            .iter()
            .any(|profile| profile.match_rule.is_empty())
        {
            warnings.push("没有通用回退配置，未识别显示器将使用第一条配置。".into());
        }

        Ok(Self {
            profiles: file.profiles,
            warnings,
        })
    }

    pub fn profiles(&self) -> &[DisplayProfile] {
        &self.profiles
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn resolve(&self, identity: &DisplayIdentity) -> (&DisplayProfile, bool) {
        if let Some(profile) = self
            .profiles
            .iter()
            .find(|profile| !profile.match_rule.is_empty() && profile.match_rule.matches(identity))
        {
            return (profile, true);
        }

        if let Some(profile) = self
            .profiles
            .iter()
            .find(|profile| profile.id == "generic-ddc")
            .or_else(|| {
                self.profiles
                    .iter()
                    .find(|profile| profile.match_rule.is_empty())
            })
        {
            return (profile, false);
        }

        (&self.profiles[0], false)
    }
}

fn merge_toml(base: &mut toml::Value, patch: toml::Value) {
    let toml::Value::Table(patch_table) = patch else {
        *base = patch;
        return;
    };
    let toml::Value::Table(base_table) = base else {
        *base = toml::Value::Table(patch_table);
        return;
    };

    for (key, value) in patch_table {
        match base_table.get_mut(&key) {
            Some(existing) if existing.is_table() && value.is_table() => {
                merge_toml(existing, value);
            }
            _ => {
                base_table.insert(key, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(model: &str) -> DisplayIdentity {
        DisplayIdentity {
            backend: "winapi".into(),
            native_id: "DISPLAY1".into(),
            vendor_id: Some("GSM".into()),
            model_id: Some(0x1234),
            model_name: Some(model.into()),
            serial_number: Some("ABC123".into()),
            edid_serial: Some(1),
        }
    }

    #[test]
    fn builtin_profiles_parse_and_match_lg() {
        let catalog = ProfileCatalog::load(&UserConfig::default()).unwrap();
        let (profile, matched) = catalog.resolve(&identity("LG 27UL850-W"));
        assert!(matched);
        assert_eq!(profile.id, "lg-27ul850");
        assert_eq!(profile.inputs.len(), 4);
        assert_eq!(profile.input("dp").unwrap().selector.value(), 0xD0);
    }

    #[test]
    fn macos_lg_profile_matches_numeric_identity() {
        let catalog = ProfileCatalog::load(&UserConfig::default()).unwrap();
        let identity = DisplayIdentity {
            backend: "macos".into(),
            native_id: "1".into(),
            vendor_id: Some("1E6D".into()),
            model_id: Some(30470),
            model_name: Some("LG HDR 4K".into()),
            serial_number: Some("44077".into()),
            edid_serial: Some(44077),
        };
        let (profile, matched) = catalog.resolve(&identity);
        assert!(matched);
        assert_eq!(profile.id, "lg-27ul850-macos");
        assert_eq!(profile.capabilities.brightness.max, 100);
        assert_eq!(profile.input("dp").unwrap().selector.value(), 0x0F);
        assert_eq!(profile.input("typec").unwrap().selector.value(), 0x10);
    }

    #[test]
    fn unknown_display_uses_generic_profile() {
        let catalog = ProfileCatalog::load(&UserConfig::default()).unwrap();
        let (profile, matched) = catalog.resolve(&identity("Dell U2723QE"));
        assert!(!matched);
        assert_eq!(profile.id, "generic-ddc");
        assert_eq!(profile.input("hdmi1").unwrap().selector.value(), 0x11);
    }

    #[test]
    fn user_override_deep_merges_and_replaces_arrays() {
        let mut user = UserConfig::default();
        let patch: toml::Value = toml::from_str(
            r#"
label = "LG 27UL850 (书房)"
[capabilities.brightness]
max = 100
[[inputs]]
id = "dp"
label = "DisplayPort"
[inputs.selector]
type = "standard"
vcp_code = 0x60
value = 0xD0
"#,
        )
        .unwrap();
        user.profile_overrides.insert("lg-27ul850".into(), patch);

        let catalog = ProfileCatalog::load(&user).unwrap();
        let profile = catalog
            .profiles()
            .iter()
            .find(|profile| profile.id == "lg-27ul850")
            .unwrap();
        assert_eq!(profile.label, "LG 27UL850 (书房)");
        assert_eq!(profile.capabilities.brightness.max, 100);
        assert_eq!(profile.capabilities.brightness.vcp_code, 0x10);
        assert_eq!(profile.inputs.len(), 1);
        assert_eq!(profile.inputs[0].label, "DisplayPort");
        assert!(profile.input("hdmi1").is_none());
    }

    #[test]
    fn custom_profile_is_matched_by_priority() {
        let mut user = UserConfig::default();
        user.custom_profiles.push(DisplayProfile {
            id: "custom-lg".into(),
            label: "Custom LG".into(),
            priority: 500,
            match_rule: MatchRule {
                vendor_id: Some("GSM".into()),
                ..MatchRule::default()
            },
            current_input_vcp_code: 0x60,
            capabilities: Capabilities::default(),
            inputs: vec![InputSource {
                id: "dp".into(),
                label: "DP".into(),
                selector: InputSelector::Standard {
                    vcp_code: 0x60,
                    value: 0x0F,
                },
            }],
        });

        let catalog = ProfileCatalog::load(&user).unwrap();
        let (profile, matched) = catalog.resolve(&identity("LG 27UL850-W"));
        assert!(matched);
        assert_eq!(profile.id, "custom-lg");
    }

    #[test]
    fn duplicate_custom_profile_id_is_rejected() {
        let mut user = UserConfig::default();
        user.custom_profiles.push(DisplayProfile {
            id: "lg-27ul850".into(),
            label: "Duplicate".into(),
            ..DisplayProfile::default()
        });
        assert!(ProfileCatalog::load(&user).is_err());
    }

    #[test]
    fn support_setting_accepts_bool_and_auto() {
        #[derive(Deserialize)]
        struct Wrapper {
            supported: SupportSetting,
        }
        let yes: Wrapper = toml::from_str("supported = true").unwrap();
        let auto: Wrapper = toml::from_str("supported = \"auto\"").unwrap();
        let no: Wrapper = toml::from_str("supported = false").unwrap();
        assert_eq!(yes.supported, SupportSetting::Yes);
        assert_eq!(auto.supported, SupportSetting::Auto);
        assert_eq!(no.supported, SupportSetting::No);
    }

    #[test]
    fn match_rule_supports_any_model_name_and_backend() {
        let rule = MatchRule {
            vendor_id: Some("gsm".into()),
            model_name_any: vec!["*27ul850*".into(), "*lg hdr 4k*".into()],
            backend: Some("WINAPI".into()),
            ..MatchRule::default()
        };
        assert!(rule.matches(&identity("LG HDR 4K")));
        assert!(!rule.matches(&identity("LG 27GL850")));
    }
}
