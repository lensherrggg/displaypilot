use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::profile::DisplayProfile;

const DEFAULT_CONFIG: &str = include_str!("../profiles/config.toml");

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub log_dir: PathBuf,
    pub log_file: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Self {
        let base = dirs::config_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let config_dir = base.join("DisplayPilot");

        #[cfg(target_os = "macos")]
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| config_dir.clone())
            .join("Library")
            .join("Logs")
            .join("DisplayPilot");

        #[cfg(not(target_os = "macos"))]
        let log_dir = dirs::data_local_dir()
            .unwrap_or_else(|| config_dir.clone())
            .join("DisplayPilot")
            .join("logs");

        Self::at(config_dir, log_dir)
    }

    pub fn at(config_dir: PathBuf, log_dir: PathBuf) -> Self {
        let config_file = config_dir.join("config.toml");
        let log_file = log_dir.join("displaypilot.log");
        Self {
            config_dir,
            config_file,
            log_dir,
            log_file,
        }
    }

    pub fn ensure_directories(&self) -> Result<(), String> {
        fs::create_dir_all(&self.config_dir)
            .map_err(|error| format!("无法创建配置目录 {}: {error}", self.config_dir.display()))?;
        fs::create_dir_all(&self.log_dir)
            .map_err(|error| format!("无法创建日志目录 {}: {error}", self.log_dir.display()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct GeneralConfig {
    #[serde(alias = "brightness_step")]
    pub brightness_step: u8,
    #[serde(alias = "volume_step")]
    pub volume_step: u8,
    #[serde(alias = "auto_hide")]
    pub auto_hide: bool,
    #[serde(alias = "show_unsupported_audio")]
    pub show_unsupported_audio: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            brightness_step: 5,
            volume_step: 5,
            auto_hide: true,
            show_unsupported_audio: true,
        }
    }
}

impl GeneralConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.brightness_step == 0 || self.brightness_step > 50 {
            return Err("general.brightness_step 必须在 1–50 之间".into());
        }
        if self.volume_step == 0 || self.volume_step > 50 {
            return Err("general.volume_step 必须在 1–50 之间".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputShortcut {
    pub input: String,
    pub accelerator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ShortcutConfig {
    pub toggle_panel: String,
    pub brightness_up: String,
    pub brightness_down: String,
    pub volume_up: String,
    pub volume_down: String,
    pub mute: String,
    pub input_switch: Vec<InputShortcut>,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            toggle_panel: String::new(),
            brightness_up: String::new(),
            brightness_down: String::new(),
            volume_up: String::new(),
            volume_down: String::new(),
            mute: String::new(),
            input_switch: ["hdmi1", "hdmi2", "dp", "typec"]
                .into_iter()
                .map(|input| InputShortcut {
                    input: input.into(),
                    accelerator: String::new(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UserConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub general: GeneralConfig,
    pub shortcuts: ShortcutConfig,
    pub profile_overrides: BTreeMap<String, toml::Value>,
    pub custom_profiles: Vec<DisplayProfile>,
}

fn default_schema_version() -> u32 {
    1
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            general: GeneralConfig::default(),
            shortcuts: ShortcutConfig::default(),
            profile_overrides: BTreeMap::new(),
            custom_profiles: Vec::new(),
        }
    }
}

impl UserConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "不支持的配置 schema_version {}（当前支持 1）",
                self.schema_version
            ));
        }
        self.general.validate()?;
        for shortcut in &self.shortcuts.input_switch {
            if shortcut.input.trim().is_empty() {
                return Err("shortcuts.input_switch.input 不能为空".into());
            }
        }
        Ok(())
    }

    pub fn shortcut_bindings(&self) -> Vec<ShortcutBinding> {
        let mut bindings = vec![
            ShortcutBinding::new(
                "toggle_panel",
                "打开/关闭面板",
                &self.shortcuts.toggle_panel,
            ),
            ShortcutBinding::new("brightness_up", "亮度增加", &self.shortcuts.brightness_up),
            ShortcutBinding::new(
                "brightness_down",
                "亮度降低",
                &self.shortcuts.brightness_down,
            ),
            ShortcutBinding::new("volume_up", "音量增加", &self.shortcuts.volume_up),
            ShortcutBinding::new("volume_down", "音量降低", &self.shortcuts.volume_down),
            ShortcutBinding::new("mute", "静音/取消静音", &self.shortcuts.mute),
        ];

        for shortcut in &self.shortcuts.input_switch {
            bindings.push(ShortcutBinding::new(
                format!("input:{}", shortcut.input),
                format!("切换到 {}", shortcut.input.to_uppercase()),
                &shortcut.accelerator,
            ));
        }

        bindings
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutBinding {
    pub action: String,
    pub label: String,
    pub accelerator: String,
}

impl ShortcutBinding {
    fn new(action: impl Into<String>, label: impl Into<String>, accelerator: &str) -> Self {
        Self {
            action: action.into(),
            label: label.into(),
            accelerator: accelerator.trim().to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub user: UserConfig,
    pub path: PathBuf,
    pub error: Option<String>,
    pub warning: Option<String>,
}

pub fn load(path: &Path) -> LoadedConfig {
    let warning = ensure_default_config(path).err();
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => {
            return LoadedConfig {
                user: UserConfig::default(),
                path: path.to_path_buf(),
                error: Some(format!(
                    "无法读取用户配置 {}：{error}。正在使用内置默认设置。",
                    path.display()
                )),
                warning,
            };
        }
    };

    match toml::from_str::<UserConfig>(&contents) {
        Ok(user) => match user.validate() {
            Ok(()) => LoadedConfig {
                user,
                path: path.to_path_buf(),
                error: None,
                warning,
            },
            Err(error) => LoadedConfig {
                user: UserConfig::default(),
                path: path.to_path_buf(),
                error: Some(format!("用户配置无效：{error}。正在使用内置默认设置。")),
                warning,
            },
        },
        Err(error) => LoadedConfig {
            user: UserConfig::default(),
            path: path.to_path_buf(),
            error: Some(format!("用户配置无法解析：{error}。正在使用内置默认设置。")),
            warning,
        },
    }
}

pub fn ensure_default_config(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建配置目录 {}：{error}", parent.display()))?;
    }
    fs::write(path, DEFAULT_CONFIG)
        .map_err(|error| format!("无法写入默认配置 {}：{error}", path.display()))
}

/// Updates only the `[general]` table so user comments and profile overrides survive.
pub fn update_general(path: &Path, general: &GeneralConfig) -> Result<(), String> {
    general.validate()?;
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("无法读取配置文件 {}：{error}", path.display()))?;
    let mut document = contents
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("配置不是有效的 TOML：{error}"))?;

    if !document.contains_key("general") {
        document["general"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let table = document["general"]
        .as_table_mut()
        .ok_or_else(|| "配置中的 general 必须是一个表。".to_string())?;
    table["brightness_step"] = toml_edit::value(i64::from(general.brightness_step));
    table["volume_step"] = toml_edit::value(i64::from(general.volume_step));
    table["auto_hide"] = toml_edit::value(general.auto_hide);
    table["show_unsupported_audio"] = toml_edit::value(general.show_unsupported_audio);
    document["schema_version"] = toml_edit::value(1_i64);

    fs::write(path, document.to_string())
        .map_err(|error| format!("无法写入配置文件 {}：{error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_created_and_parsed() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.toml");
        let loaded = load(&path);
        assert!(loaded.error.is_none());
        assert!(path.exists());
        assert_eq!(loaded.user.general.brightness_step, 5);
        assert_eq!(loaded.user.shortcuts.input_switch.len(), 4);
    }

    #[test]
    fn invalid_config_falls_back_to_defaults() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.toml");
        fs::write(&path, "schema_version = 99\n").unwrap();
        let loaded = load(&path);
        assert!(loaded.error.is_some());
        assert_eq!(loaded.user.schema_version, 1);
    }

    #[test]
    fn empty_accelerators_are_skipped_from_registration() {
        let bindings = UserConfig::default().shortcut_bindings();
        assert!(bindings
            .iter()
            .all(|binding| binding.accelerator.is_empty()));
        assert_eq!(bindings.len(), 10);
    }

    #[test]
    fn general_accepts_snake_case_toml_and_serializes_camel_case() {
        let snake: UserConfig = toml::from_str(
            "[general]\nbrightness_step = 12\nvolume_step = 8\nauto_hide = false\nshow_unsupported_audio = false\n",
        )
        .unwrap();
        assert_eq!(snake.general.brightness_step, 12);
        assert_eq!(snake.general.volume_step, 8);
        assert!(!snake.general.auto_hide);

        let json = serde_json::to_value(&snake.general).unwrap();
        assert_eq!(json["brightnessStep"], 12);
        assert_eq!(json["showUnsupportedAudio"], false);

        let from_camel: GeneralConfig =
            serde_json::from_value(json).expect("camelCase IPC payload should deserialize");
        assert_eq!(from_camel, snake.general);
    }

    #[test]
    fn update_general_rejects_out_of_range_steps_without_touching_the_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.toml");
        let loaded = load(&path);
        assert!(loaded.error.is_none());
        let before = fs::read_to_string(&path).unwrap();

        let invalid = GeneralConfig {
            brightness_step: 0,
            ..GeneralConfig::default()
        };
        let error = update_general(&path, &invalid).unwrap_err();
        assert!(error.contains("brightness_step"));
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
    }
}
