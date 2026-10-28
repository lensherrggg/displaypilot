use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::{self, AppPaths, UserConfig};
use crate::error::AppError;
use crate::models::{
    AppInfo, ConfigInfo, CurrentInputState, DiagnosticDisplay, DiagnosticInput, DiagnosticReport,
    DisplayList, DisplaySnapshot, DisplaySummary, FeatureValueState, InputOption, MuteValueState,
    ProfileSummary, VcpRead,
};
use crate::profile::{
    Capabilities, DisplayIdentity, DisplayProfile, InputSelector, InputSource, ProfileCatalog,
    RangeCapability, RawSelector, VendorWriteMode,
};
use crate::transport::{DdcTransport, RawSupport, TransportError, VcpReading};
use crate::util::{
    clamp_percent, now_rfc3339, percent_to_raw, platform_info, raw_to_percent, stable_display_id,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickAction {
    Input(String),
    BrightnessUp,
    BrightnessDown,
    VolumeUp,
    VolumeDown,
    ToggleMute,
}

impl QuickAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "brightness_up" => Some(Self::BrightnessUp),
            "brightness_down" => Some(Self::BrightnessDown),
            "volume_up" => Some(Self::VolumeUp),
            "volume_down" => Some(Self::VolumeDown),
            "mute" => Some(Self::ToggleMute),
            value if value.starts_with("input:") && value.len() > "input:".len() => {
                Some(Self::Input(value["input:".len()..].to_string()))
            }
            _ => None,
        }
    }
}

struct ManagedDisplay<H> {
    id: String,
    identity: DisplayIdentity,
    summary_name: String,
    profile: DisplayProfile,
    profile_matched: bool,
    handle: H,
    capabilities_error: Option<String>,
    raw_support: RawSupport,
    unsupported_inputs: HashMap<String, String>,
    snapshot: Option<DisplaySnapshot>,
}

#[derive(Debug, Clone)]
struct InputAvailability {
    supported: bool,
    transport: String,
    reason: Option<String>,
    hint: Option<String>,
}

impl<H> ManagedDisplay<H> {
    fn input_availability(&self, input: &InputSource) -> InputAvailability {
        let raw_backend = self
            .raw_support
            .backend
            .clone()
            .unwrap_or_else(|| "raw".into());
        let runtime_reason = self.unsupported_inputs.get(&input.id).cloned();
        let raw_reason = self
            .raw_support
            .reason
            .clone()
            .unwrap_or_else(|| "原始 I2C 后端不可用。".into());

        match &input.selector {
            InputSelector::Standard { raw_fallback, .. }
            | InputSelector::Vendor { raw_fallback, .. } => {
                let has_fallback = raw_fallback.is_some();
                let transport = if has_fallback && self.raw_support.available {
                    format!("standard+raw:{raw_backend}")
                } else {
                    "standard".into()
                };
                let hint = if has_fallback && !self.raw_support.available {
                    Some(format!(
                        "标准 DDC/CI 被显示器忽略时需要原始 I2C，当前不可用：{raw_reason}"
                    ))
                } else {
                    None
                };
                InputAvailability {
                    supported: runtime_reason.is_none(),
                    transport,
                    reason: runtime_reason,
                    hint,
                }
            }
            InputSelector::Raw { source_address, .. } => {
                let transport = format!("raw:{raw_backend}@0x{source_address:02X}");
                let reason = if !self.raw_support.available {
                    Some(raw_reason)
                } else {
                    runtime_reason
                };
                InputAvailability {
                    supported: reason.is_none(),
                    transport,
                    reason,
                    hint: None,
                }
            }
        }
    }

    fn summary(&self) -> DisplaySummary {
        DisplaySummary {
            id: self.id.clone(),
            name: self.summary_name.clone(),
            profile_id: self.profile.id.clone(),
            profile_label: self.profile.label.clone(),
            profile_matched: self.profile_matched,
            backend: self.identity.backend.clone(),
            vendor_id: self.identity.vendor_id.clone(),
            model_id: self.identity.model_id,
            model_name: self.identity.model_name.clone(),
            serial_number: self.identity.serial_number.clone(),
            inputs: self
                .profile
                .inputs
                .iter()
                .map(|input| {
                    let availability = self.input_availability(input);
                    InputOption {
                        id: input.id.clone(),
                        label: input.label.clone(),
                        supported: availability.supported,
                        transport: availability.transport,
                        reason: availability.reason,
                        hint: availability.hint,
                    }
                })
                .collect(),
            input_switch_supported: self.profile.capabilities.input_switch
                && !self.profile.inputs.is_empty(),
            current_input_vcp_code: self.profile.current_input_vcp_code,
            raw_i2c_available: self.raw_support.available,
            raw_i2c_backend: self.raw_support.backend.clone(),
            raw_i2c_reason: self.raw_support.reason.clone(),
        }
    }
}

pub struct DisplayService<T: DdcTransport> {
    transport: T,
    user: UserConfig,
    catalog: ProfileCatalog,
    config_path: PathBuf,
    config_dir: PathBuf,
    log_path: PathBuf,
    config_error: Option<String>,
    config_warnings: Vec<String>,
    displays: Vec<ManagedDisplay<T::Handle>>,
    discovery_error: Option<AppError>,
    discovered: bool,
    last_used: Option<String>,
}

impl<T: DdcTransport> DisplayService<T> {
    pub fn new(transport: T, paths: &AppPaths) -> Result<Self, AppError> {
        paths.ensure_directories().map_err(AppError::config)?;

        let loaded = config::load(&paths.config_file);
        let mut config_error = loaded.error.clone();
        let mut config_warnings = Vec::new();
        if let Some(warning) = loaded.warning.clone() {
            config_warnings.push(warning);
        }

        let catalog = match ProfileCatalog::load(&loaded.user) {
            Ok(catalog) => catalog,
            Err(error) => {
                let fallback_error = format!("用户显示器配置无效：{error}");
                log::error!("{fallback_error}");
                config_error = Some(match config_error {
                    Some(existing) => format!("{existing} {fallback_error}"),
                    None => fallback_error,
                });
                ProfileCatalog::load(&UserConfig::default())
                    .map_err(|builtin| AppError::config(format!("内置配置无法加载：{builtin}")))?
            }
        };
        config_warnings.extend(catalog.warnings().iter().cloned());

        log::info!(
            "Config loaded path={} error={:?} warnings={:?}",
            paths.config_file.display(),
            config_error,
            config_warnings
        );

        Ok(Self {
            transport,
            user: loaded.user,
            catalog,
            config_path: paths.config_file.clone(),
            config_dir: paths.config_dir.clone(),
            log_path: paths.log_file.clone(),
            config_error,
            config_warnings,
            displays: Vec::new(),
            discovery_error: None,
            discovered: false,
            last_used: None,
        })
    }

    pub fn app_info(&self) -> AppInfo {
        let platform = platform_info();
        AppInfo {
            version: env!("CARGO_PKG_VERSION").into(),
            os: platform.os.into(),
            arch: platform.arch.into(),
            config_path: path_string(&self.config_path),
            log_path: path_string(&self.log_path),
        }
    }

    pub fn list_displays(&mut self, force: bool) -> DisplayList {
        if force || !self.discovered {
            let _ = self.discover(force);
        }

        DisplayList {
            displays: self.displays.iter().map(ManagedDisplay::summary).collect(),
            discovery_error: self.discovery_error.clone(),
            config_error: self.config_error.clone(),
            config_warnings: self.config_warnings.clone(),
        }
    }

    pub fn rediscover(&mut self) -> DisplayList {
        self.list_displays(true)
    }

    pub fn select_display(&mut self, display_id: &str) -> Result<(), AppError> {
        if self.find_index(display_id).is_none() {
            return Err(AppError::display_not_found(display_id));
        }
        self.last_used = Some(display_id.to_string());
        Ok(())
    }

    pub fn snapshot(&mut self, display_id: &str, force: bool) -> Result<DisplaySnapshot, AppError> {
        let index = self.require_index(display_id)?;
        self.ensure_snapshot(index, force)
    }

    pub fn switch_input(
        &mut self,
        display_id: &str,
        input_id: &str,
    ) -> Result<DisplaySnapshot, AppError> {
        let index = self.require_index(display_id)?;
        self.ensure_snapshot(index, false)?;

        let (selector, already_current, input_label, verify_code, raw_support) = {
            let display = &self.displays[index];
            if !display.profile.capabilities.input_switch {
                return Err(
                    AppError::new("unsupported", "当前显示器配置未启用输入源切换。")
                        .with_hint("在显示器配置中将 capabilities.input_switch 设为 true。"),
                );
            }
            let input = display.profile.input(input_id).ok_or_else(|| {
                AppError::new(
                    "invalid_input",
                    format!("输入源 {input_id} 不在当前显示器配置中。"),
                )
            })?;
            let already_current = display
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.current_input.input_id.as_deref())
                == Some(input_id);
            (
                input.selector.clone(),
                already_current,
                input.label.clone(),
                display.profile.current_input_vcp_code,
                display.raw_support.clone(),
            )
        };

        if already_current {
            return Ok(self.displays[index]
                .snapshot
                .as_ref()
                .expect("snapshot was ensured")
                .clone());
        }

        log::info!("Switch input: display={display_id} input={input_id} ({input_label})");

        if selector.is_raw() && !raw_support.available {
            let reason = raw_support
                .reason
                .clone()
                .unwrap_or_else(|| "原始 I2C 后端不可用。".into());
            return Err(self.mark_input_unsupported(index, input_id, &input_label, reason));
        }

        let mut used_transport = if selector.is_raw() {
            format!(
                "raw:{}",
                raw_support.backend.as_deref().unwrap_or("unknown")
            )
        } else {
            "standard".to_string()
        };

        let primary_result = {
            let transport = &self.transport;
            let display = &mut self.displays[index];
            write_selector(transport, &mut display.handle, &selector)
        };

        let mut primary_error = None;
        let mut readback = None;
        match primary_result {
            Ok(()) => {
                // DDC input writes are fire-and-forget on Windows: the call can
                // report success even when the monitor ignores the value.
                std::thread::sleep(std::time::Duration::from_millis(120));
                let transport = &self.transport;
                let display = &mut self.displays[index];
                readback = Some(read_input_value(
                    transport,
                    &mut display.handle,
                    verify_code,
                ));
            }
            Err(error) => {
                log::warn!("Input switch write failed for {display_id}: {error}");
                primary_error = Some(error);
            }
        }

        if let Some(Ok(value)) = &readback {
            if input_accepted(&self.displays[index].profile, input_id, *value) {
                return Ok(self.finish_switch(
                    index,
                    input_id,
                    &input_label,
                    &selector,
                    &used_transport,
                ));
            }
            log::warn!(
                "Input switch ignored for {display_id}: requested 0x{:04X}, still {}",
                selector.value(),
                describe_input(&self.displays[index].profile, *value)
            );
        } else if let Some(Err(error)) = &readback {
            // The monitor may have switched away and stopped answering DDC.
            log::info!(
                "Post-switch input readback unavailable: {error}; assuming switch succeeded"
            );
            return Ok(self.finish_switch(
                index,
                input_id,
                &input_label,
                &selector,
                &used_transport,
            ));
        }

        let fallback = selector.raw_fallback().cloned();
        let Some(fallback) = fallback else {
            if let Some(error) = primary_error {
                self.displays[index].snapshot = None;
                self.last_used = Some(display_id.to_string());
                return Err(AppError::switch_failed(&error));
            }
            let current = readback
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .map(|value| describe_input(&self.displays[index].profile, *value))
                .unwrap_or_else(|| "未知输入".into());
            let reason = format!("显示器没有接受切换到 {input_label}，当前仍为 {current}。");
            return Err(self.mark_input_unsupported(index, input_id, &input_label, reason));
        };

        if !raw_support.available {
            let reason = format!(
                "标准 DDC/CI 已被显示器忽略，该端口需要原始 I2C 回退（VCP 0x{:02X} @0x{:02X}），但原始 I2C 后端不可用：{}",
                fallback.vcp_code,
                fallback.source_address,
                raw_support
                    .reason
                    .as_deref()
                    .unwrap_or("未知原因")
            );
            return Err(self.mark_input_unsupported(index, input_id, &input_label, reason));
        }

        log::info!(
            "Falling back to raw I2C for {display_id}: VCP 0x{:02X} = 0x{:04X} from source 0x{:02X}",
            fallback.vcp_code,
            fallback.value,
            fallback.source_address
        );
        let fallback_result = {
            let transport = &self.transport;
            let display = &mut self.displays[index];
            write_raw_selector(transport, &mut display.handle, &fallback)
        };
        if let Err(error) = fallback_result {
            log::warn!("Raw I2C fallback failed for {display_id}: {error}");
            self.displays[index].snapshot = None;
            self.last_used = Some(display_id.to_string());
            return Err(AppError::switch_failed(&error).with_hint(
                "标准 DDC/CI 与原始 I2C 均写入失败。确认目标接口已连接信号源，并检查显卡驱动是否正常。",
            ));
        }

        used_transport = format!(
            "raw:{}",
            raw_support.backend.as_deref().unwrap_or("unknown")
        );
        std::thread::sleep(std::time::Duration::from_millis(220));
        let fallback_readback = {
            let transport = &self.transport;
            let display = &mut self.displays[index];
            read_input_value(transport, &mut display.handle, verify_code)
        };
        match fallback_readback {
            Ok(value) if input_accepted(&self.displays[index].profile, input_id, value) => {
                Ok(self.finish_switch(index, input_id, &input_label, &selector, &used_transport))
            }
            Ok(value) => {
                let current = describe_input(&self.displays[index].profile, value);
                let reason = format!(
                    "已尝试标准 DDC/CI 与原始 I2C（VCP 0x{:02X} @0x{:02X}），显示器仍停留在 {current}。",
                    fallback.vcp_code, fallback.source_address
                );
                log::warn!("Input switch ignored after raw fallback: {reason}");
                Err(self.mark_input_unsupported(index, input_id, &input_label, reason))
            }
            Err(error) => {
                // Switching to another host usually removes DDC from this host.
                log::info!(
                    "Post-raw-fallback readback unavailable: {error}; assuming switch succeeded"
                );
                Ok(self.finish_switch(index, input_id, &input_label, &selector, &used_transport))
            }
        }
    }

    fn finish_switch(
        &mut self,
        index: usize,
        input_id: &str,
        input_label: &str,
        selector: &InputSelector,
        transport: &str,
    ) -> DisplaySnapshot {
        self.displays[index].unsupported_inputs.remove(input_id);
        self.last_used = Some(self.displays[index].id.clone());
        let snapshot = self.displays[index]
            .snapshot
            .as_mut()
            .expect("snapshot was ensured");
        snapshot.current_input = CurrentInputState {
            input_id: Some(input_id.to_string()),
            label: Some(input_label.to_string()),
            raw_value: selector.verify_value(),
            transport: Some(transport.to_string()),
            read_error: None,
        };
        snapshot.probed_at = now_rfc3339();
        snapshot.clone()
    }

    fn mark_input_unsupported(
        &mut self,
        index: usize,
        input_id: &str,
        input_label: &str,
        reason: String,
    ) -> AppError {
        log::warn!("Input '{input_id}' marked unsupported: {reason}");
        self.displays[index]
            .unsupported_inputs
            .insert(input_id.to_string(), reason.clone());
        self.displays[index].snapshot = None;
        self.last_used = Some(self.displays[index].id.clone());
        AppError::new(
            "input_unsupported",
            format!("无法切换到 {input_label}：{reason}"),
        )
        .with_hint(
            "该端口在当前 GPU 与平台上没有可用的切换通道。可改用其他接口，\
             或在显示器 OSD 中手动切换；更换连接方式后刷新显示器列表再试。",
        )
    }

    pub fn set_brightness(
        &mut self,
        display_id: &str,
        percent: u8,
    ) -> Result<DisplaySnapshot, AppError> {
        let index = self.require_index(display_id)?;
        self.ensure_snapshot(index, false)?;
        let capability = self.displays[index].profile.capabilities.brightness.clone();
        if capability.supported.is_disabled() {
            return Err(AppError::new(
                "unsupported",
                "亮度控制已在显示器配置中禁用。",
            ));
        }

        let percent = clamp_percent(percent);
        let raw = percent_to_raw(percent, capability.min, capability.max);
        let result = {
            let transport = &self.transport;
            let display = &mut self.displays[index];
            transport.write_vcp(&mut display.handle, capability.vcp_code, raw)
        };

        if let Err(error) = result {
            log::warn!("Brightness write failed for {display_id}: {error}");
            if let Some(snapshot) = self.displays[index].snapshot.as_mut() {
                snapshot.brightness.percent = None;
                snapshot.brightness.raw_value = None;
                snapshot.brightness.read_error = Some(error.message.clone());
            }
            return Err(AppError::from_transport("调节亮度", &error)
                .with_hint("确认显示器支持 VCP 0x10 亮度写入，并检查当前主机是否为活动输入源。"));
        }

        self.last_used = Some(display_id.to_string());
        let snapshot = self.displays[index]
            .snapshot
            .as_mut()
            .expect("snapshot was ensured");
        snapshot.brightness =
            feature_state_from_value(&capability, percent, raw, None, true, true, None);
        snapshot.probed_at = now_rfc3339();
        Ok(snapshot.clone())
    }

    pub fn set_volume(
        &mut self,
        display_id: &str,
        percent: u8,
    ) -> Result<DisplaySnapshot, AppError> {
        let index = self.require_index(display_id)?;
        self.ensure_snapshot(index, false)?;
        let capability = self.displays[index].profile.capabilities.volume.clone();
        if capability.supported.is_disabled() {
            return Err(AppError::new(
                "unsupported",
                "音量控制已在显示器配置中禁用。",
            ));
        }
        if !self.displays[index]
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.volume.editable)
        {
            return Err(
                AppError::new("unsupported", "当前显示器未报告可用的音量控制。")
                    .with_hint("可在配置中将 capabilities.volume.supported 设为 true 后重试。"),
            );
        }

        let percent = clamp_percent(percent);
        let raw = percent_to_raw(percent, capability.min, capability.max);
        let result = {
            let transport = &self.transport;
            let display = &mut self.displays[index];
            transport.write_vcp(&mut display.handle, capability.vcp_code, raw)
        };

        if let Err(error) = result {
            log::warn!("Volume write failed for {display_id}: {error}");
            if let Some(snapshot) = self.displays[index].snapshot.as_mut() {
                snapshot.volume.percent = None;
                snapshot.volume.raw_value = None;
                snapshot.volume.read_error = Some(error.message.clone());
            }
            return Err(AppError::from_transport("调节音量", &error));
        }

        self.last_used = Some(display_id.to_string());
        let snapshot = self.displays[index]
            .snapshot
            .as_mut()
            .expect("snapshot was ensured");
        snapshot.volume =
            feature_state_from_value(&capability, percent, raw, None, true, true, None);
        snapshot.probed_at = now_rfc3339();
        Ok(snapshot.clone())
    }

    pub fn toggle_mute(&mut self, display_id: &str) -> Result<DisplaySnapshot, AppError> {
        let index = self.require_index(display_id)?;
        self.ensure_snapshot(index, false)?;
        let capability = self.displays[index].profile.capabilities.mute.clone();
        if capability.supported.is_disabled() {
            return Err(AppError::new(
                "unsupported",
                "静音控制已在显示器配置中禁用。",
            ));
        }
        let editable = self.displays[index]
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.mute.editable);
        if !editable {
            return Err(
                AppError::new("unsupported", "当前显示器未报告可用的静音控制。")
                    .with_hint("可在配置中将 capabilities.mute.supported 设为 true 后重试。"),
            );
        }

        let cached_muted = self.displays[index]
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.mute.muted);
        let current_raw = {
            let transport = &self.transport;
            let display = &mut self.displays[index];
            transport.read_vcp(&mut display.handle, capability.vcp_code)
        };

        let muted = match current_raw {
            Ok(reading) if reading.value == capability.on_value => Some(true),
            Ok(reading) if reading.value == capability.off_value => Some(false),
            Ok(reading) => {
                return Err(AppError::new(
                    "unsupported",
                    format!(
                        "无法识别静音状态：VCP 0x{:02X} 返回 0x{:04X}。",
                        capability.vcp_code, reading.value
                    ),
                )
                .with_hint("在配置中设置 capabilities.mute.on_value/off_value。"));
            }
            Err(_error) if cached_muted.is_some() => cached_muted,
            Err(error) => {
                return Err(AppError::from_transport("读取静音状态", &error)
                    .with_hint("当前显示器不支持读取静音状态；无法安全地执行切换。"));
            }
        };

        self.set_mute(display_id, !muted.unwrap_or(false))
    }

    pub fn set_mute(&mut self, display_id: &str, muted: bool) -> Result<DisplaySnapshot, AppError> {
        let index = self.require_index(display_id)?;
        self.ensure_snapshot(index, false)?;
        let capability = self.displays[index].profile.capabilities.mute.clone();
        if capability.supported.is_disabled() {
            return Err(AppError::new(
                "unsupported",
                "静音控制已在显示器配置中禁用。",
            ));
        }
        let editable = self.displays[index]
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.mute.editable);
        if !editable {
            return Err(
                AppError::new("unsupported", "当前显示器未报告可用的静音控制。")
                    .with_hint("可在配置中将 capabilities.mute.supported 设为 true 后重试。"),
            );
        }

        let raw = if muted {
            capability.on_value
        } else {
            capability.off_value
        };
        let result = {
            let transport = &self.transport;
            let display = &mut self.displays[index];
            transport.write_vcp(&mut display.handle, capability.vcp_code, raw)
        };
        if let Err(error) = result {
            log::warn!("Mute write failed for {display_id}: {error}");
            return Err(AppError::from_transport("切换静音", &error));
        }

        self.last_used = Some(display_id.to_string());
        let snapshot = self.displays[index]
            .snapshot
            .as_mut()
            .expect("snapshot was ensured");
        snapshot.mute = MuteValueState {
            supported: true,
            editable: true,
            muted: Some(muted),
            raw_value: Some(raw),
            read_error: None,
            hint: None,
        };
        snapshot.probed_at = now_rfc3339();
        Ok(snapshot.clone())
    }

    pub fn quick_action(&mut self, action: &QuickAction) -> Result<DisplaySnapshot, AppError> {
        let index = self.target_index()?;
        let display_id = self.displays[index].id.clone();
        let brightness_step = self.user.general.brightness_step;
        let volume_step = self.user.general.volume_step;

        match action {
            QuickAction::Input(input_id) => self.switch_input(&display_id, input_id),
            QuickAction::BrightnessUp => {
                let current = self.current_percent(index, Feature::Brightness)?;
                self.set_brightness(
                    &display_id,
                    current.saturating_add(brightness_step).min(100),
                )
            }
            QuickAction::BrightnessDown => {
                let current = self.current_percent(index, Feature::Brightness)?;
                self.set_brightness(&display_id, current.saturating_sub(brightness_step))
            }
            QuickAction::VolumeUp => {
                let current = self.current_percent(index, Feature::Volume)?;
                self.set_volume(&display_id, current.saturating_add(volume_step).min(100))
            }
            QuickAction::VolumeDown => {
                let current = self.current_percent(index, Feature::Volume)?;
                self.set_volume(&display_id, current.saturating_sub(volume_step))
            }
            QuickAction::ToggleMute => self.toggle_mute(&display_id),
        }
    }

    pub fn config_info(&self) -> ConfigInfo {
        let platform = platform_info();
        let profiles = self
            .catalog
            .profiles()
            .iter()
            .map(|profile| ProfileSummary {
                id: profile.id.clone(),
                label: profile.label.clone(),
                priority: profile.priority,
                inputs: profile
                    .inputs
                    .iter()
                    .map(|input| InputOption {
                        id: input.id.clone(),
                        label: input.label.clone(),
                        supported: profile.capabilities.input_switch,
                        transport: "configured".into(),
                        reason: None,
                        hint: None,
                    })
                    .collect(),
                matched_display_ids: self
                    .displays
                    .iter()
                    .filter(|display| display.profile.id == profile.id)
                    .map(|display| display.id.clone())
                    .collect(),
            })
            .collect();

        ConfigInfo {
            version: env!("CARGO_PKG_VERSION").into(),
            os: platform.os.into(),
            arch: platform.arch.into(),
            config_path: path_string(&self.config_path),
            config_dir: path_string(&self.config_dir),
            log_path: path_string(&self.log_path),
            config_error: self.config_error.clone(),
            config_warnings: self.config_warnings.clone(),
            general: self.user.general.clone(),
            profiles,
            shortcuts: self.user.shortcut_bindings(),
        }
    }

    pub fn reload_config(&mut self) -> Result<ConfigInfo, AppError> {
        let loaded = config::load(&self.config_path);
        if let Some(error) = loaded.error {
            return Err(AppError::config(error));
        }
        let catalog = ProfileCatalog::load(&loaded.user).map_err(AppError::config)?;
        let mut warnings = Vec::new();
        if let Some(warning) = loaded.warning {
            warnings.push(warning);
        }
        warnings.extend(catalog.warnings().iter().cloned());

        self.user = loaded.user;
        self.catalog = catalog;
        self.config_error = None;
        self.config_warnings = warnings;

        let catalog = &self.catalog;
        for display in &mut self.displays {
            let (profile, matched) = catalog.resolve(&display.identity);
            display.profile = profile.clone();
            display.profile_matched = matched;
            display.snapshot = None;
        }

        log::info!("User config reloaded from {}", self.config_path.display());
        Ok(self.config_info())
    }

    pub fn diagnostics(&mut self, display_id: &str) -> Result<DiagnosticReport, AppError> {
        let index = self.require_index(display_id)?;
        let snapshot = self.ensure_snapshot(index, true)?;

        let (display_info, profile, raw_support) = {
            let display = &self.displays[index];
            let inputs = display
                .profile
                .inputs
                .iter()
                .map(|input| {
                    let availability = display.input_availability(input);
                    DiagnosticInput {
                        id: input.id.clone(),
                        label: input.label.clone(),
                        selector: describe_selector(&input.selector),
                        supported: availability.supported,
                        transport: availability.transport,
                        reason: availability.reason,
                        hint: availability.hint,
                    }
                })
                .collect();
            (
                DiagnosticDisplay {
                    id: display.id.clone(),
                    name: display.summary_name.clone(),
                    backend: display.identity.backend.clone(),
                    native_id: display.identity.native_id.clone(),
                    vendor_id: display.identity.vendor_id.clone(),
                    model_id: display.identity.model_id,
                    model_name: display.identity.model_name.clone(),
                    serial_number: display.identity.serial_number.clone(),
                    profile_id: display.profile.id.clone(),
                    profile_label: display.profile.label.clone(),
                    profile_matched: display.profile_matched,
                    capabilities_error: display.capabilities_error.clone(),
                    raw_i2c_available: display.raw_support.available,
                    raw_i2c_backend: display.raw_support.backend.clone(),
                    raw_i2c_reason: display.raw_support.reason.clone(),
                    inputs,
                },
                display.profile.clone(),
                display.raw_support.clone(),
            )
        };

        let mut codes = Vec::new();
        push_unique(&mut codes, profile.current_input_vcp_code);
        push_unique(&mut codes, profile.capabilities.brightness.vcp_code);
        push_unique(&mut codes, profile.capabilities.volume.vcp_code);
        push_unique(&mut codes, profile.capabilities.mute.vcp_code);
        push_unique(&mut codes, 0xDF);

        let mut vcp_reads = Vec::new();
        for code in codes {
            let result = {
                let transport = &self.transport;
                let display = &mut self.displays[index];
                transport.read_vcp(&mut display.handle, code)
            };
            vcp_reads.push(vcp_read(&code, result));
        }

        let capabilities_result = {
            let transport = &self.transport;
            let display = &mut self.displays[index];
            transport.capabilities_string(&mut display.handle)
        };
        let (capabilities, capabilities_error) = match capabilities_result {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error.message)),
        };

        let mut notes = vec![
            "DDC/CI 指令只应在显示器当前活动输入源所在的主机上发起。".into(),
            "音量/静音的 auto 能力以实际读取结果为准；不支持时控件会禁用。".into(),
            "输入源 VCP 值来自内置配置或用户配置，请用实际切换结果核验。".into(),
        ];
        if raw_support.available {
            notes.push(format!(
                "原始 I2C 后端可用（{}）：标准 DDC/CI 被忽略时可回退到 VCP 0xF4 @0x50。",
                raw_support.backend.as_deref().unwrap_or("raw")
            ));
        } else {
            notes.push(format!(
                "原始 I2C 后端不可用：{}",
                raw_support
                    .reason
                    .as_deref()
                    .unwrap_or("当前平台或 GPU 不提供原始 I2C 访问。")
            ));
        }

        Ok(DiagnosticReport {
            generated_at: now_rfc3339(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            os: platform_info().os.into(),
            arch: platform_info().arch.into(),
            display: display_info,
            current_input: snapshot.current_input,
            brightness: snapshot.brightness,
            volume: snapshot.volume,
            mute: snapshot.mute,
            capabilities,
            capabilities_error,
            vcp_reads,
            notes,
        })
    }

    fn discover(&mut self, force: bool) -> Result<(), AppError> {
        if self.discovered && !force {
            return Ok(());
        }

        self.discovered = true;
        match self.transport.enumerate() {
            Ok(raw_displays) => {
                if raw_displays.is_empty() {
                    self.displays.clear();
                    let error = AppError::no_displays(None);
                    log::warn!("No DDC/CI displays found");
                    self.discovery_error = Some(error.clone());
                    return Err(error);
                }

                let mut displays = Vec::with_capacity(raw_displays.len());
                for raw in raw_displays {
                    let identity = DisplayIdentity {
                        backend: raw.backend.clone(),
                        native_id: raw.native_id.clone(),
                        vendor_id: raw.vendor_id.clone(),
                        model_id: raw.model_id,
                        model_name: raw.model_name.clone(),
                        serial_number: raw.serial_number.clone(),
                        edid_serial: raw.edid_serial,
                    };
                    let id = stable_display_id(
                        raw.vendor_id.as_deref(),
                        raw.model_id,
                        raw.model_name.as_deref(),
                        raw.serial_number.as_deref(),
                        raw.edid_serial,
                        &raw.backend,
                        &raw.native_id,
                    );
                    let (profile, profile_matched) = self.catalog.resolve(&identity);
                    let profile = profile.clone();
                    log::info!(
                        "Display {} '{}' matched profile '{}' ({}) backend={} serial={:?}",
                        id,
                        identity.display_name(),
                        profile.id,
                        if profile_matched { "exact" } else { "fallback" },
                        identity.backend,
                        identity.serial_number
                    );
                    displays.push(ManagedDisplay {
                        id: id.clone(),
                        summary_name: if identity.model_name.is_some() {
                            identity.display_name()
                        } else {
                            profile.label.clone()
                        },
                        identity,
                        profile,
                        profile_matched,
                        handle: raw.handle,
                        capabilities_error: raw.capabilities_error,
                        raw_support: raw.raw_support,
                        unsupported_inputs: HashMap::new(),
                        snapshot: None,
                    });
                }

                displays.sort_by(|a, b| {
                    a.summary_name
                        .to_lowercase()
                        .cmp(&b.summary_name.to_lowercase())
                        .then_with(|| a.id.cmp(&b.id))
                });

                if self
                    .last_used
                    .as_ref()
                    .is_none_or(|id| !displays.iter().any(|display| &display.id == id))
                {
                    self.last_used = displays.first().map(|display| display.id.clone());
                }

                self.displays = displays;
                self.discovery_error = None;
                log::info!(
                    "DDC discovery complete: {} usable display(s)",
                    self.displays.len()
                );
                Ok(())
            }
            Err(error) => {
                self.displays.clear();
                let app_error = AppError::no_displays(Some(error.message.clone()));
                log::error!("Display enumeration failed: {error}");
                self.discovery_error = Some(app_error.clone());
                Err(app_error)
            }
        }
    }

    fn require_index(&mut self, display_id: &str) -> Result<usize, AppError> {
        if let Some(index) = self.find_index(display_id) {
            return Ok(index);
        }
        let _ = self.discover(true);
        self.find_index(display_id)
            .ok_or_else(|| AppError::display_not_found(display_id))
    }

    fn find_index(&self, display_id: &str) -> Option<usize> {
        self.displays
            .iter()
            .position(|display| display.id == display_id)
    }

    fn target_index(&mut self) -> Result<usize, AppError> {
        if self.displays.is_empty() {
            let _ = self.discover(true);
        }
        if let Some(last_used) = &self.last_used {
            if let Some(index) = self.find_index(last_used) {
                return Ok(index);
            }
        }
        if self.displays.is_empty() {
            return Err(AppError::no_displays(
                self.discovery_error
                    .as_ref()
                    .map(|error| error.message.clone()),
            ));
        }
        Ok(0)
    }

    fn ensure_snapshot(&mut self, index: usize, force: bool) -> Result<DisplaySnapshot, AppError> {
        if !force {
            if let Some(snapshot) = &self.displays[index].snapshot {
                return Ok(snapshot.clone());
            }
        }
        self.build_snapshot(index)
    }

    fn build_snapshot(&mut self, index: usize) -> Result<DisplaySnapshot, AppError> {
        let summary = self.displays[index].summary();
        let profile = self.displays[index].profile.clone();

        let current_input = {
            let transport = &self.transport;
            let display = &mut self.displays[index];
            match transport.read_vcp(&mut display.handle, profile.current_input_vcp_code) {
                Ok(reading) => {
                    log::debug!(
                        "Read current input VCP 0x{:02X}: 0x{:04X}",
                        profile.current_input_vcp_code,
                        reading.value
                    );
                    match profile.input_for_value(reading.value) {
                        Some(input) => CurrentInputState {
                            input_id: Some(input.id.clone()),
                            label: Some(input.label.clone()),
                            raw_value: Some(reading.value),
                            transport: Some("standard".into()),
                            read_error: None,
                        },
                        None => CurrentInputState {
                            input_id: None,
                            label: None,
                            raw_value: Some(reading.value),
                            transport: Some("standard".into()),
                            read_error: Some(format!(
                                "当前输入源值 0x{:04X} 未在配置中定义。",
                                reading.value
                            )),
                        },
                    }
                }
                Err(error) => {
                    log::warn!("Failed to read current input: {error}");
                    CurrentInputState {
                        raw_value: None,
                        read_error: Some(error.message),
                        ..CurrentInputState::default()
                    }
                }
            }
        };

        let brightness = self.read_brightness_state(index, &profile.capabilities);
        let volume = self.read_volume_state(index, &profile.capabilities);
        let mute = self.read_mute_state(index, &profile.capabilities);

        let snapshot = DisplaySnapshot {
            display: summary,
            current_input,
            brightness,
            volume,
            mute,
            probed_at: now_rfc3339(),
        };
        self.displays[index].snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn read_brightness_state(
        &mut self,
        index: usize,
        capabilities: &Capabilities,
    ) -> FeatureValueState {
        let capability = capabilities.brightness.clone();
        if capability.supported.is_disabled() {
            return feature_state_unsupported(&capability, "亮度控制已在显示器配置中禁用。");
        }
        let result = {
            let transport = &self.transport;
            let display = &mut self.displays[index];
            transport.read_vcp(&mut display.handle, capability.vcp_code)
        };

        match result {
            Ok(reading) => feature_state_from_value(
                &capability,
                raw_to_percent(reading.value, capability.min, capability.max),
                reading.value,
                None,
                true,
                true,
                None,
            ),
            Err(error) => FeatureValueState {
                supported: true,
                editable: true,
                percent: None,
                raw_value: None,
                min: capability.min,
                max: capability.max,
                step: capability.step,
                read_error: Some(error.message),
                hint: Some("无法读取亮度；滑块仍可尝试写入最终值。".into()),
            },
        }
    }

    fn read_volume_state(
        &mut self,
        index: usize,
        capabilities: &Capabilities,
    ) -> FeatureValueState {
        let capability = capabilities.volume.clone();
        if capability.supported.is_disabled() {
            return feature_state_unsupported(&capability, "音量控制已在显示器配置中禁用。");
        }
        let result = {
            let transport = &self.transport;
            let display = &mut self.displays[index];
            transport.read_vcp(&mut display.handle, capability.vcp_code)
        };

        match result {
            Ok(reading) => feature_state_from_value(
                &capability,
                raw_to_percent(reading.value, capability.min, capability.max),
                reading.value,
                None,
                true,
                true,
                None,
            ),
            Err(error) => {
                if capability.supported.is_forced() {
                    FeatureValueState {
                        supported: true,
                        editable: true,
                        percent: None,
                        raw_value: None,
                        min: capability.min,
                        max: capability.max,
                        step: capability.step,
                        read_error: Some(error.message),
                        hint: Some("配置已强制启用音量控制，但当前读取失败；写入仍会尝试。".into()),
                    }
                } else {
                    feature_state_unsupported(
                        &capability,
                        "当前显示器未报告音量能力。可在配置中将 supported 设为 true 后重试。",
                    )
                    .with_read_error(error.message)
                }
            }
        }
    }

    fn read_mute_state(&mut self, index: usize, capabilities: &Capabilities) -> MuteValueState {
        let capability = capabilities.mute.clone();
        if capability.supported.is_disabled() {
            return mute_state_unsupported("静音控制已在显示器配置中禁用。");
        }
        let result = {
            let transport = &self.transport;
            let display = &mut self.displays[index];
            transport.read_vcp(&mut display.handle, capability.vcp_code)
        };

        match result {
            Ok(reading) => {
                let muted = if reading.value == capability.on_value {
                    Some(true)
                } else if reading.value == capability.off_value {
                    Some(false)
                } else {
                    None
                };
                MuteValueState {
                    supported: true,
                    editable: true,
                    muted,
                    raw_value: Some(reading.value),
                    read_error: if muted.is_none() {
                        Some(format!("无法识别静音值 0x{:04X}。", reading.value))
                    } else {
                        None
                    },
                    hint: if muted.is_none() {
                        Some("请在配置中设置 mute.on_value/off_value。".into())
                    } else {
                        None
                    },
                }
            }
            Err(error) => {
                if capability.supported.is_forced() {
                    MuteValueState {
                        supported: true,
                        editable: true,
                        muted: None,
                        raw_value: None,
                        read_error: Some(error.message),
                        hint: Some("配置已强制启用静音控制，但当前读取失败。".into()),
                    }
                } else {
                    mute_state_unsupported(
                        "当前显示器未报告静音能力。可在配置中将 supported 设为 true 后重试。",
                    )
                    .with_read_error(error.message)
                }
            }
        }
    }

    fn current_percent(&mut self, index: usize, feature: Feature) -> Result<u8, AppError> {
        self.ensure_snapshot(index, false)?;
        let value = match feature {
            Feature::Brightness => self.displays[index]
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.brightness.percent),
            Feature::Volume => self.displays[index]
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.volume.percent),
        };

        value.ok_or_else(|| {
            AppError::new(
                "unsupported",
                match feature {
                    Feature::Brightness => "无法读取当前亮度。",
                    Feature::Volume => "无法读取当前音量。",
                },
            )
            .with_hint("打开面板并点击刷新，或检查显示器 DDC/CI 支持。")
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum Feature {
    Brightness,
    Volume,
}

fn feature_state_from_value(
    capability: &RangeCapability,
    percent: u8,
    raw: u16,
    read_error: Option<String>,
    supported: bool,
    editable: bool,
    hint: Option<String>,
) -> FeatureValueState {
    FeatureValueState {
        supported,
        editable,
        percent: Some(percent),
        raw_value: Some(raw),
        min: capability.min,
        max: capability.max,
        step: capability.step,
        read_error,
        hint,
    }
}

fn feature_state_unsupported(capability: &RangeCapability, hint: &str) -> FeatureValueState {
    FeatureValueState {
        supported: false,
        editable: false,
        percent: None,
        raw_value: None,
        min: capability.min,
        max: capability.max,
        step: capability.step,
        read_error: None,
        hint: Some(hint.into()),
    }
}

fn mute_state_unsupported(hint: &str) -> MuteValueState {
    MuteValueState {
        supported: false,
        editable: false,
        muted: None,
        raw_value: None,
        read_error: None,
        hint: Some(hint.into()),
    }
}

trait FeatureStateExt {
    fn with_read_error(self, error: String) -> Self;
}

impl FeatureStateExt for FeatureValueState {
    fn with_read_error(mut self, error: String) -> Self {
        self.read_error = Some(error);
        self
    }
}

trait MuteStateExt {
    fn with_read_error(self, error: String) -> Self;
}

impl MuteStateExt for MuteValueState {
    fn with_read_error(mut self, error: String) -> Self {
        self.read_error = Some(error);
        self
    }
}

fn vcp_read(code: &u8, result: Result<VcpReading, TransportError>) -> VcpRead {
    let name = match *code {
        0x10 => "Brightness",
        0x60 => "Input Source",
        0x62 => "Audio Volume",
        0x8D => "Audio Mute",
        0xDF => "VCP Version",
        _ => "VCP Feature",
    };
    match result {
        Ok(reading) => VcpRead {
            code: format!("0x{code:02X}"),
            name: name.into(),
            ok: true,
            value: Some(reading.value),
            maximum: Some(reading.maximum),
            ty: Some(reading.ty),
            sh: Some(reading.sh),
            sl: Some(reading.sl),
            error: None,
        },
        Err(error) => VcpRead {
            code: format!("0x{code:02X}"),
            name: name.into(),
            ok: false,
            value: None,
            maximum: None,
            ty: None,
            sh: None,
            sl: None,
            error: Some(error.message),
        },
    }
}

fn write_selector<T: DdcTransport>(
    transport: &T,
    handle: &mut T::Handle,
    selector: &InputSelector,
) -> Result<(), TransportError> {
    match selector {
        InputSelector::Standard {
            vcp_code, value, ..
        } => transport.write_vcp(handle, *vcp_code, *value),
        InputSelector::Vendor {
            vcp_code,
            value,
            data_address,
            write_mode,
            ..
        } => match write_mode {
            VendorWriteMode::Table => {
                transport.table_write(handle, *vcp_code, *data_address, &value.to_be_bytes())
            }
            VendorWriteMode::Vcp => transport.write_vcp(handle, *vcp_code, *value),
        },
        InputSelector::Raw {
            vcp_code,
            value,
            source_address,
            ..
        } => transport.raw_write(handle, *source_address, *vcp_code, *value),
    }
}

fn write_raw_selector<T: DdcTransport>(
    transport: &T,
    handle: &mut T::Handle,
    raw: &RawSelector,
) -> Result<(), TransportError> {
    transport.raw_write(handle, raw.source_address, raw.vcp_code, raw.value)
}

fn read_input_value<T: DdcTransport>(
    transport: &T,
    handle: &mut T::Handle,
    code: u8,
) -> Result<u16, TransportError> {
    transport
        .read_vcp(handle, code)
        .map(|reading| reading.value)
}

fn input_accepted(profile: &DisplayProfile, input_id: &str, value: u16) -> bool {
    profile
        .input(input_id)
        .and_then(|input| input.selector.verify_value())
        == Some(value)
}

fn describe_input(profile: &DisplayProfile, value: u16) -> String {
    profile
        .input_for_value(value)
        .map(|input| format!("{} (0x{value:04X})", input.label))
        .unwrap_or_else(|| format!("未知输入 (0x{value:04X})"))
}

fn describe_selector(selector: &InputSelector) -> String {
    match selector {
        InputSelector::Standard {
            vcp_code,
            value,
            raw_fallback,
        } => {
            let mut text = format!("standard: VCP 0x{vcp_code:02X} = 0x{value:04X}");
            if let Some(raw) = raw_fallback {
                text.push_str(&format!(
                    "; raw fallback: VCP 0x{:02X} = 0x{:04X} @0x{:02X}",
                    raw.vcp_code, raw.value, raw.source_address
                ));
            }
            text
        }
        InputSelector::Vendor {
            vcp_code,
            value,
            data_address,
            write_mode,
            raw_fallback,
        } => {
            let mut text = format!(
                "vendor: {:?}, VCP 0x{vcp_code:02X} = 0x{value:04X}, addr 0x{data_address:04X}",
                write_mode
            );
            if let Some(raw) = raw_fallback {
                text.push_str(&format!(
                    "; raw fallback: VCP 0x{:02X} = 0x{:04X} @0x{:02X}",
                    raw.vcp_code, raw.value, raw.source_address
                ));
            }
            text
        }
        InputSelector::Raw {
            vcp_code,
            value,
            source_address,
            verify_value,
        } => {
            let mut text =
                format!("raw: VCP 0x{vcp_code:02X} = 0x{value:04X} @0x{source_address:02X}");
            if let Some(verify) = verify_value {
                text.push_str(&format!("; verify 0x{verify:04X}"));
            }
            text
        }
    }
}

fn push_unique(codes: &mut Vec<u8>, code: u8) {
    if !codes.contains(&code) {
        codes.push(code);
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    use super::*;
    use crate::transport::RawDisplay;

    #[derive(Default)]
    struct FakeState {
        values: BTreeMap<u8, VcpReading>,
        fail_reads: std::collections::HashSet<u8>,
        fail_writes: std::collections::HashSet<u8>,
        ignored_writes: std::collections::HashSet<u8>,
        raw_available: bool,
        raw_fail_writes: bool,
        raw_ignored: bool,
        writes: Vec<(u8, u16)>,
        raw_writes: Vec<(u8, u8, u16)>,
        table_writes: Vec<(u8, u16, Vec<u8>)>,
        capabilities: String,
        capabilities_fail: bool,
    }

    impl FakeState {
        fn lg() -> Self {
            let mut values = BTreeMap::new();
            values.insert(0x10, reading(128, 255));
            values.insert(0x60, reading(0x0F, 0x12));
            values.insert(0x62, reading(64, 255));
            values.insert(0x8D, reading(1, 2));
            values.insert(0xDF, reading(0x0202, 0));
            Self {
                values,
                raw_available: true,
                capabilities: "(prot(monitor)type(lcd)model(LG 27UL850)vcp(10 60 62 8D DF))".into(),
                ..Self::default()
            }
        }
    }

    fn reading(value: u16, maximum: u16) -> VcpReading {
        VcpReading {
            value,
            maximum,
            ty: 0,
            sh: (value >> 8) as u8,
            sl: value as u8,
        }
    }

    struct FakeHandle {
        state: Rc<RefCell<FakeState>>,
    }

    struct FakeTransport {
        state: Rc<RefCell<FakeState>>,
        vendor_id: Option<&'static str>,
        model_id: Option<u16>,
        model_name: Option<&'static str>,
        serial_number: Option<&'static str>,
        backend: &'static str,
        native_id: &'static str,
        no_displays: bool,
    }

    impl FakeTransport {
        fn new(state: Rc<RefCell<FakeState>>) -> Self {
            Self {
                state,
                vendor_id: Some("GSM"),
                model_id: Some(0x5B7C),
                model_name: Some("LG 27UL850-W"),
                serial_number: Some("TEST-SERIAL-1"),
                backend: "winapi",
                native_id: "test-path-a",
                no_displays: false,
            }
        }

        fn lg() -> Self {
            Self::new(Rc::new(RefCell::new(FakeState::lg())))
        }

        fn generic() -> Self {
            let mut state = FakeState::lg();
            state.values.insert(0x60, reading(0x0F, 0x12));
            Self {
                vendor_id: Some("DEL"),
                model_id: Some(0x1234),
                model_name: Some("DELL U2720Q"),
                serial_number: None,
                backend: "i2c",
                native_id: "test-path-b",
                no_displays: false,
                state: Rc::new(RefCell::new(state)),
            }
        }

        fn none() -> Self {
            let mut transport = Self::lg();
            transport.no_displays = true;
            transport
        }
    }

    impl DdcTransport for FakeTransport {
        type Handle = FakeHandle;

        fn enumerate(&self) -> Result<Vec<RawDisplay<Self::Handle>>, TransportError> {
            if self.no_displays {
                return Ok(Vec::new());
            }
            let capabilities_fail = self.state.borrow().capabilities_fail;
            Ok(vec![RawDisplay {
                handle: FakeHandle {
                    state: self.state.clone(),
                },
                backend: self.backend.into(),
                native_id: self.native_id.into(),
                vendor_id: self.vendor_id.map(Into::into),
                model_id: self.model_id,
                model_name: self.model_name.map(Into::into),
                serial_number: self.serial_number.map(Into::into),
                edid_serial: self.serial_number.map(|_| 4242),
                edid: None,
                capabilities_error: capabilities_fail.then(|| "simulated caps failure".into()),
                raw_support: if self.state.borrow().raw_available {
                    RawSupport::available("nvapi")
                } else {
                    RawSupport::unavailable(Some("nvapi"), "simulated raw backend unavailable")
                },
            }])
        }

        fn read_vcp(
            &self,
            display: &mut Self::Handle,
            code: u8,
        ) -> Result<VcpReading, TransportError> {
            let state = display.state.borrow();
            if state.fail_reads.contains(&code) {
                return Err(TransportError::communication(format!(
                    "simulated read failure for 0x{code:02X}"
                )));
            }
            state.values.get(&code).copied().ok_or_else(|| {
                TransportError::unsupported(format!("VCP 0x{code:02X} is not supported"))
            })
        }

        fn write_vcp(
            &self,
            display: &mut Self::Handle,
            code: u8,
            value: u16,
        ) -> Result<(), TransportError> {
            let mut state = display.state.borrow_mut();
            if state.fail_writes.contains(&code) {
                return Err(TransportError::communication(format!(
                    "simulated write failure for 0x{code:02X}"
                )));
            }
            state.writes.push((code, value));
            if state.ignored_writes.contains(&code) {
                return Ok(());
            }
            let maximum = state.values.get(&code).map_or(0, |entry| entry.maximum);
            state.values.insert(code, reading(value, maximum));
            Ok(())
        }

        fn table_write(
            &self,
            display: &mut Self::Handle,
            code: u8,
            offset: u16,
            data: &[u8],
        ) -> Result<(), TransportError> {
            let mut state = display.state.borrow_mut();
            if state.fail_writes.contains(&code) {
                return Err(TransportError::communication(format!(
                    "simulated table write failure for 0x{code:02X}"
                )));
            }
            state.table_writes.push((code, offset, data.to_vec()));
            Ok(())
        }

        fn capabilities_string(
            &self,
            display: &mut Self::Handle,
        ) -> Result<String, TransportError> {
            let state = display.state.borrow();
            if state.capabilities_fail {
                return Err(TransportError::communication("simulated caps failure"));
            }
            Ok(state.capabilities.clone())
        }

        fn raw_read(
            &self,
            display: &mut Self::Handle,
            _source: u8,
            code: u8,
        ) -> Result<VcpReading, TransportError> {
            let state = display.state.borrow();
            if state.fail_reads.contains(&code) {
                return Err(TransportError::communication(format!(
                    "simulated raw read failure for 0x{code:02X}"
                )));
            }
            state.values.get(&code).copied().ok_or_else(|| {
                TransportError::unsupported(format!("VCP 0x{code:02X} is not supported"))
            })
        }

        fn raw_write(
            &self,
            display: &mut Self::Handle,
            source: u8,
            code: u8,
            value: u16,
        ) -> Result<(), TransportError> {
            let mut state = display.state.borrow_mut();
            if state.raw_fail_writes {
                return Err(TransportError::communication(format!(
                    "simulated raw write failure for 0x{code:02X}"
                )));
            }
            state.raw_writes.push((source, code, value));
            if state.raw_ignored {
                return Ok(());
            }
            let mapped = if code == 0xF4 {
                match value {
                    0x90 => 0x11,
                    0x91 => 0x12,
                    0xD0 | 0xD1 => 0x0F,
                    0xD2 => 0x10,
                    other => other,
                }
            } else {
                value
            };
            let target = if code == 0xF4 { 0x60 } else { code };
            let maximum = state.values.get(&target).map_or(0, |entry| entry.maximum);
            state.values.insert(target, reading(mapped, maximum));
            Ok(())
        }
    }

    fn service(transport: FakeTransport) -> (DisplayService<FakeTransport>, tempfile::TempDir) {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(temp.path().join("config"), temp.path().join("logs"));
        let service = DisplayService::new(transport, &paths).expect("service should initialize");
        (service, temp)
    }

    fn only_display_id(service: &mut DisplayService<FakeTransport>) -> String {
        let list = service.list_displays(false);
        assert_eq!(list.displays.len(), 1);
        list.displays[0].id.clone()
    }

    #[test]
    fn lg_display_matches_profile_and_reads_state() {
        let (mut service, _temp) = service(FakeTransport::lg());
        let id = only_display_id(&mut service);
        let list = service.list_displays(false);
        assert_eq!(list.displays[0].profile_id, "lg-27ul850");
        assert!(list.displays[0].profile_matched);
        assert_eq!(list.displays[0].name, "LG 27UL850-W");

        let snapshot = service.snapshot(&id, false).unwrap();
        assert_eq!(snapshot.current_input.input_id.as_deref(), Some("dp"));
        assert_eq!(snapshot.current_input.raw_value, Some(0x0F));
        assert_eq!(snapshot.brightness.percent, Some(50));
        assert_eq!(snapshot.brightness.raw_value, Some(128));
        assert!(snapshot.volume.editable);
        assert_eq!(snapshot.volume.percent, Some(25));
        assert_eq!(snapshot.mute.muted, Some(true));
        assert!(snapshot.brightness.read_error.is_none());
    }

    #[test]
    fn switch_input_writes_configured_value_and_updates_snapshot() {
        let transport = FakeTransport::lg();
        let state = transport.state.clone();
        let (mut service, _temp) = service(transport);
        let id = only_display_id(&mut service);

        let snapshot = service.switch_input(&id, "hdmi1").unwrap();
        assert_eq!(snapshot.current_input.input_id.as_deref(), Some("hdmi1"));
        assert_eq!(snapshot.current_input.raw_value, Some(0x11));

        let state = state.borrow();
        assert_eq!(state.writes, vec![(0x60, 0x11)]);
        assert_eq!(state.values[&0x60].value, 0x11);
    }

    #[test]
    fn ignored_input_write_falls_back_to_raw_i2c() {
        let transport = FakeTransport::lg();
        let state = transport.state.clone();
        state.borrow_mut().ignored_writes.insert(0x60);
        let (mut service, _temp) = service(transport);
        let id = only_display_id(&mut service);

        let snapshot = service.switch_input(&id, "typec").unwrap();
        assert_eq!(snapshot.current_input.input_id.as_deref(), Some("typec"));
        assert_eq!(snapshot.current_input.raw_value, Some(0x10));
        assert_eq!(
            snapshot.current_input.transport.as_deref(),
            Some("raw:nvapi")
        );

        let state = state.borrow();
        assert_eq!(state.writes, vec![(0x60, 0x10)]);
        assert_eq!(state.raw_writes, vec![(0x50, 0xF4, 0xD2)]);
        assert_eq!(state.values[&0x60].value, 0x10);
    }

    #[test]
    fn ignored_input_without_raw_backend_is_marked_unsupported() {
        let transport = FakeTransport::lg();
        let state = transport.state.clone();
        {
            let mut state = state.borrow_mut();
            state.ignored_writes.insert(0x60);
            state.raw_available = false;
        }
        let (mut service, _temp) = service(transport);
        let id = only_display_id(&mut service);

        let error = service.switch_input(&id, "typec").unwrap_err();
        assert_eq!(error.code, "input_unsupported");
        assert!(error.message.contains("TypeC"));
        assert!(error.hint.is_some());
        assert!(state.borrow().raw_writes.is_empty());

        let list = service.list_displays(false);
        let typec = list.displays[0]
            .inputs
            .iter()
            .find(|input| input.id == "typec")
            .unwrap();
        assert!(!typec.supported);
        assert!(typec
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("原始 I2C"));
        assert!(!list.displays[0].raw_i2c_available);

        let snapshot = service.snapshot(&id, false).unwrap();
        assert_eq!(snapshot.current_input.input_id.as_deref(), Some("dp"));
        assert_eq!(snapshot.current_input.raw_value, Some(0x0F));
    }

    #[test]
    fn switch_failure_is_actionable_and_invalidates_cache() {
        let transport = FakeTransport::lg();
        let state = transport.state.clone();
        {
            let mut state = state.borrow_mut();
            state.fail_writes.insert(0x60);
            state.raw_fail_writes = true;
        }
        let (mut service, _temp) = service(transport);
        let id = only_display_id(&mut service);

        let error = service.switch_input(&id, "hdmi2").unwrap_err();
        assert_eq!(error.code, "ddc_error");
        assert!(error.hint.is_some());
        assert!(state.borrow().writes.is_empty());
        assert!(state.borrow().raw_writes.is_empty());
    }

    #[test]
    fn both_transports_ignored_marks_port_unsupported() {
        let transport = FakeTransport::lg();
        let state = transport.state.clone();
        {
            let mut state = state.borrow_mut();
            state.ignored_writes.insert(0x60);
            state.raw_ignored = true;
        }
        let (mut service, _temp) = service(transport);
        let id = only_display_id(&mut service);

        let error = service.switch_input(&id, "hdmi1").unwrap_err();
        assert_eq!(error.code, "input_unsupported");
        assert!(error.message.contains("HDMI1"));
        assert_eq!(state.borrow().raw_writes.last(), Some(&(0x50, 0xF4, 0x90)));

        let list = service.list_displays(false);
        let hdmi1 = list.displays[0]
            .inputs
            .iter()
            .find(|input| input.id == "hdmi1")
            .unwrap();
        assert!(!hdmi1.supported);
        assert!(hdmi1
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("原始 I2C"));

        // A successful switch clears the runtime unsupported mark.
        state.borrow_mut().ignored_writes.clear();
        state.borrow_mut().raw_ignored = false;
        service.switch_input(&id, "hdmi1").unwrap();
        let list = service.list_displays(false);
        let hdmi1 = list.displays[0]
            .inputs
            .iter()
            .find(|input| input.id == "hdmi1")
            .unwrap();
        assert!(hdmi1.supported);
    }

    #[test]
    fn brightness_write_scales_percent_to_vcp_range() {
        let transport = FakeTransport::lg();
        let state = transport.state.clone();
        let (mut service, _temp) = service(transport);
        let id = only_display_id(&mut service);

        let snapshot = service.set_brightness(&id, 55).unwrap();
        assert_eq!(snapshot.brightness.percent, Some(55));
        let writes = &state.borrow().writes;
        assert_eq!(writes.last(), Some(&(0x10, 140)));
    }

    #[test]
    fn volume_is_hidden_when_probe_read_fails() {
        let transport = FakeTransport::lg();
        let state = transport.state.clone();
        state.borrow_mut().fail_reads.insert(0x62);
        let (mut service, _temp) = service(transport);
        let id = only_display_id(&mut service);

        let snapshot = service.snapshot(&id, false).unwrap();
        assert!(!snapshot.volume.supported);
        assert!(!snapshot.volume.editable);
        assert!(snapshot.volume.read_error.is_some());
        assert_eq!(service.set_volume(&id, 50).unwrap_err().code, "unsupported");
    }

    #[test]
    fn mute_toggle_reads_current_state_and_writes_opposite() {
        let transport = FakeTransport::lg();
        let state = transport.state.clone();
        let (mut service, _temp) = service(transport);
        let id = only_display_id(&mut service);

        let snapshot = service.toggle_mute(&id).unwrap();
        assert_eq!(snapshot.mute.muted, Some(false));
        assert_eq!(state.borrow().writes.last(), Some(&(0x8D, 2)));

        let snapshot = service.set_mute(&id, true).unwrap();
        assert_eq!(snapshot.mute.muted, Some(true));
    }

    #[test]
    fn generic_display_falls_back_to_standard_input_values() {
        let (mut service, _temp) = service(FakeTransport::generic());
        let id = only_display_id(&mut service);
        let list = service.list_displays(false);
        assert_eq!(list.displays[0].profile_id, "generic-ddc");
        assert!(!list.displays[0].profile_matched);

        let snapshot = service.snapshot(&id, false).unwrap();
        assert_eq!(snapshot.current_input.label.as_deref(), Some("DP"));
        service.switch_input(&id, "typec").unwrap();
    }

    #[test]
    fn quick_action_brightness_uses_configured_step() {
        let (mut service, _temp) = service(FakeTransport::lg());
        let id = only_display_id(&mut service);
        let before = service
            .snapshot(&id, false)
            .unwrap()
            .brightness
            .percent
            .unwrap();

        service.quick_action(&QuickAction::BrightnessUp).unwrap();
        let after = service
            .snapshot(&id, true)
            .unwrap()
            .brightness
            .percent
            .unwrap();
        assert_eq!(after, before + 5);
    }

    #[test]
    fn diagnostics_include_reads_and_capabilities() {
        let (mut service, _temp) = service(FakeTransport::lg());
        let id = only_display_id(&mut service);

        let report = service.diagnostics(&id).unwrap();
        assert_eq!(report.display.profile_id, "lg-27ul850");
        assert!(report.capabilities.unwrap().contains("LG 27UL850"));
        assert!(report.capabilities_error.is_none());
        assert!(report.vcp_reads.iter().any(|read| read.code == "0x60"));
        assert!(report.vcp_reads.iter().all(|read| read.ok));
        assert!(!report.notes.is_empty());
    }

    #[test]
    fn no_displays_surfaces_actionable_error() {
        let (mut service, _temp) = service(FakeTransport::none());
        let list = service.list_displays(false);
        assert!(list.displays.is_empty());
        let error = list.discovery_error.expect("discovery error");
        assert_eq!(error.code, "no_displays");
        assert!(error.hint.is_some());
    }
}
