use serde::Serialize;

use crate::config::AppPaths;
use crate::error::AppError;
use crate::models::DisplaySummary;
use crate::service::DisplayService;
use crate::transport::HardwareTransport;

const HELP: &str = "\
displaypilot — 多主机共享显示器的 DDC/CI 控制器

用法:
  displaypilot                       启动图形界面（托盘 + 面板）
  displaypilot list [--json]         列出 DDC/CI 显示器与输入源
  displaypilot switch <input>        切换输入源（如 hdmi1、hdmi2、dp、typec）
  displaypilot brightness [0-100]    读取或设置亮度（支持 +5 / -5）
  displaypilot volume [0-100]        读取或设置音量（支持 +5 / -5）
  displaypilot mute [on|off|toggle]  设置或切换静音
  displaypilot diagnose [--json]     输出诊断报告
  displaypilot config [--json]       显示配置、配置档与快捷键

公共选项:
  --display <id|序号|名称>           指定显示器，默认使用第一台
  --json                             以 JSON 输出，便于脚本处理
  -h, --help                         显示帮助
  -V, --version                      显示版本

退出码: 0 成功 · 1 操作失败 · 2 未发现显示器 · 64 参数错误
配置文件使用 TOML，路径可通过 `displaypilot config` 查看。
";

pub fn should_run(args: &[String]) -> bool {
    !args.is_empty() && args.first().map(String::as_str) != Some("--gui")
}

pub fn run(args: Vec<String>, paths: AppPaths) -> i32 {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{HELP}");
        return 0;
    }
    if args.iter().any(|arg| arg == "-V" || arg == "--version") {
        println!("displaypilot {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }

    let options = match Options::parse(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("参数错误：{message}\n");
            print!("{HELP}");
            return 64;
        }
    };

    let mut service = match DisplayService::new(HardwareTransport, &paths) {
        Ok(service) => service,
        Err(error) => return fail(&options, &error, 1),
    };

    match execute(&mut service, &options) {
        Ok(code) => code,
        Err(error) => {
            let code = match error.code.as_str() {
                "no_displays" => 2,
                "usage" => 64,
                _ => 1,
            };
            fail(&options, &error, code)
        }
    }
}

#[derive(Debug)]
struct Options {
    json: bool,
    display: Option<String>,
    positionals: Vec<String>,
}

impl Options {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut json = false;
        let mut display = None;
        let mut positionals = Vec::new();
        let mut iter = args.into_iter();

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--json" => json = true,
                "--display" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| "--display 需要一个值".to_string())?;
                    display = Some(value);
                }
                value if value.starts_with("--display=") => {
                    display = Some(value["--display=".len()..].to_string());
                }
                value if value.starts_with('-') && value != "-" && !is_negative_level(value) => {
                    return Err(format!("未知选项 {value}"));
                }
                value => positionals.push(value.to_string()),
            }
        }

        if positionals.is_empty() {
            positionals.push("list".into());
        }
        Ok(Self {
            json,
            display,
            positionals,
        })
    }
}

fn execute(
    service: &mut DisplayService<HardwareTransport>,
    options: &Options,
) -> Result<i32, AppError> {
    let command = options.positionals[0].as_str();
    match command {
        "list" | "displays" => {
            let list = service.list_displays(true);
            if options.json {
                print_json(&list);
            } else {
                print_display_list(&list);
            }
            if list.displays.is_empty() {
                if let Some(error) = &list.discovery_error {
                    eprintln_err(error);
                }
                return Ok(2);
            }
            Ok(0)
        }
        "switch" => {
            let input = options
                .positionals
                .get(1)
                .ok_or_else(|| usage("switch 需要输入源 id，例如 displaypilot switch dp"))?;
            let display = resolve_display(service, options.display.as_deref())?;
            let snapshot = service.switch_input(&display.id, input)?;
            if options.json {
                print_json(&snapshot);
            } else {
                println!(
                    "已切换到 {}：{}",
                    display.name,
                    snapshot
                        .current_input
                        .label
                        .clone()
                        .unwrap_or_else(|| input.clone())
                );
            }
            Ok(0)
        }
        "brightness" => {
            let display = resolve_display(service, options.display.as_deref())?;
            let target = match options.positionals.get(1) {
                Some(value) => {
                    let current = service.snapshot(&display.id, false)?.brightness.percent;
                    Some(parse_level(value, current, "亮度")?)
                }
                None => None,
            };
            let snapshot = match target {
                Some(percent) => service.set_brightness(&display.id, percent)?,
                None => service.snapshot(&display.id, false)?,
            };
            if options.json {
                print_json(&snapshot);
            } else {
                println!(
                    "{} 亮度：{}%",
                    display.name,
                    snapshot
                        .brightness
                        .percent
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "未知".into())
                );
            }
            Ok(0)
        }
        "volume" => {
            let display = resolve_display(service, options.display.as_deref())?;
            let target = match options.positionals.get(1) {
                Some(value) => {
                    let current = service.snapshot(&display.id, false)?.volume.percent;
                    Some(parse_level(value, current, "音量")?)
                }
                None => None,
            };
            let snapshot = match target {
                Some(percent) => service.set_volume(&display.id, percent)?,
                None => service.snapshot(&display.id, false)?,
            };
            if options.json {
                print_json(&snapshot);
            } else {
                println!(
                    "{} 音量：{}%{}",
                    display.name,
                    snapshot
                        .volume
                        .percent
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "未知".into()),
                    match snapshot.mute.muted {
                        Some(true) => "（已静音）",
                        _ => "",
                    }
                );
            }
            Ok(0)
        }
        "mute" => {
            let display = resolve_display(service, options.display.as_deref())?;
            let action = options
                .positionals
                .get(1)
                .map(String::as_str)
                .unwrap_or("toggle");
            let snapshot = match action {
                "on" => service.set_mute(&display.id, true)?,
                "off" => service.set_mute(&display.id, false)?,
                "toggle" => service.toggle_mute(&display.id)?,
                value => {
                    return Err(usage(&format!(
                        "mute 只接受 on、off 或 toggle，收到 {value}"
                    )))
                }
            };
            if options.json {
                print_json(&snapshot);
            } else {
                println!(
                    "{}：{}",
                    display.name,
                    match snapshot.mute.muted {
                        Some(true) => "已静音",
                        Some(false) => "未静音",
                        None => "静音状态未知",
                    }
                );
            }
            Ok(0)
        }
        "diagnose" | "diagnostics" => {
            let display = resolve_display(service, options.display.as_deref())?;
            let report = service.diagnostics(&display.id)?;
            if options.json {
                print_json(&report);
            } else {
                print_diagnostics(&report);
            }
            Ok(0)
        }
        "config" => {
            let info = service.config_info();
            if options.json {
                print_json(&info);
            } else {
                print_config(&info);
            }
            Ok(0)
        }
        other => Err(usage(&format!("未知子命令 {other}"))),
    }
}

fn resolve_display(
    service: &mut DisplayService<HardwareTransport>,
    requested: Option<&str>,
) -> Result<DisplaySummary, AppError> {
    let list = service.list_displays(false);
    if list.displays.is_empty() {
        return Err(list
            .discovery_error
            .unwrap_or_else(|| AppError::no_displays(None)));
    }

    let Some(requested) = requested else {
        return Ok(list.displays[0].clone());
    };

    if let Some(display) = list.displays.iter().find(|display| display.id == requested) {
        return Ok(display.clone());
    }
    if let Ok(index) = requested.parse::<usize>() {
        if index >= 1 && index <= list.displays.len() {
            return Ok(list.displays[index - 1].clone());
        }
    }
    let lowered = requested.to_lowercase();
    if let Some(display) = list
        .displays
        .iter()
        .find(|display| display.name.to_lowercase() == lowered)
    {
        return Ok(display.clone());
    }
    let matches: Vec<_> = list
        .displays
        .iter()
        .filter(|display| display.name.to_lowercase().contains(&lowered))
        .collect();
    if matches.len() == 1 {
        return Ok(matches[0].clone());
    }

    Err(AppError::display_not_found(requested))
}

fn is_negative_level(value: &str) -> bool {
    value.strip_prefix('-').is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn parse_level(value: &str, current: Option<u8>, label: &str) -> Result<u8, AppError> {
    if let Some(step) = value.strip_prefix('+').or_else(|| value.strip_prefix('-')) {
        let delta: u8 = step
            .parse()
            .map_err(|_| usage(&format!("{label}增减量无效：{value}")))?;
        let current = current.ok_or_else(|| {
            AppError::new(
                "unsupported",
                format!("无法读取当前{label}，不能使用相对调整。"),
            )
            .with_hint("直接提供 0–100 的绝对值。")
        })?;
        return Ok(if value.starts_with('-') {
            current.saturating_sub(delta)
        } else {
            current.saturating_add(delta).min(100)
        });
    }

    let percent: u8 = value
        .parse()
        .map_err(|_| usage(&format!("{label}必须是 0–100 的整数，收到 {value}")))?;
    if percent > 100 {
        return Err(usage(&format!("{label}必须在 0–100 之间")));
    }
    Ok(percent)
}

fn print_display_list(list: &crate::models::DisplayList) {
    println!(
        "DisplayPilot {} · {} {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    if let Some(error) = &list.config_error {
        eprintln!("配置警告：{error}");
    }
    for warning in &list.config_warnings {
        eprintln!("配置提示：{warning}");
    }
    if list.displays.is_empty() {
        println!("未发现可通过 DDC/CI 控制的显示器。");
        return;
    }

    println!("发现 {} 台显示器：", list.displays.len());
    for (index, display) in list.displays.iter().enumerate() {
        println!(
            "  {}. {}  {}  [{}]{}",
            index + 1,
            display.id,
            display.name,
            display.profile_id,
            if display.profile_matched {
                ""
            } else {
                "（通用配置）"
            }
        );
        println!(
            "     后端 {} · 厂商 {} · 型号 {}",
            display.backend,
            display.vendor_id.as_deref().unwrap_or("—"),
            display
                .model_id
                .map(|model| format!("0x{model:04X}"))
                .unwrap_or_else(|| "—".into())
        );
        if display.inputs.is_empty() {
            println!("     输入源：未配置");
        } else {
            let inputs = display
                .inputs
                .iter()
                .map(|input| format!("{}={}", input.id, input.label))
                .collect::<Vec<_>>()
                .join(" · ");
            println!("     输入源：{inputs}");
        }
    }
    println!("提示：使用 --display <序号|id|名称> 指定显示器。");
}

fn print_diagnostics(report: &crate::models::DiagnosticReport) {
    println!(
        "DisplayPilot {} · {} · 生成于 {}",
        report.app_version, report.os, report.generated_at
    );
    println!(
        "显示器：{} ({}) 后端={} 配置档={}",
        report.display.name, report.display.id, report.display.backend, report.display.profile_id
    );
    println!(
        "当前输入：{}",
        report.current_input.label.as_deref().unwrap_or("未知")
    );
    println!(
        "亮度：{} · 音量：{} · 静音：{}",
        percent_label(report.brightness.percent),
        percent_label(report.volume.percent),
        match report.mute.muted {
            Some(true) => "是",
            Some(false) => "否",
            None => "未知",
        }
    );
    if let Some(error) = &report.display.capabilities_error {
        println!("能力字符串读取失败：{error}");
    }
    if let Some(capabilities) = &report.capabilities {
        println!("能力字符串：\n{capabilities}");
    }
    println!("VCP 读取：");
    for read in &report.vcp_reads {
        match (read.value, &read.error) {
            (Some(value), _) => println!(
                "  {} {} = 0x{:04X}（最大 0x{:04X}）",
                read.code,
                read.name,
                value,
                read.maximum.unwrap_or(0)
            ),
            (None, Some(error)) => println!("  {} {} 读取失败：{}", read.code, read.name, error),
            (None, None) => println!("  {} {} 无数据", read.code, read.name),
        }
    }
    for note in &report.notes {
        println!("• {note}");
    }
}

fn print_config(info: &crate::models::ConfigInfo) {
    println!("DisplayPilot {} · {} {}", info.version, info.os, info.arch);
    println!("配置文件：{}", info.config_path);
    println!("配置目录：{}", info.config_dir);
    println!("日志文件：{}", info.log_path);
    if let Some(error) = &info.config_error {
        println!("配置错误：{error}");
    }
    for warning in &info.config_warnings {
        println!("配置提示：{warning}");
    }
    println!(
        "步进：亮度 {} · 音量 {}",
        info.general.brightness_step, info.general.volume_step
    );
    println!("配置档：");
    for profile in &info.profiles {
        println!(
            "  {}  {}  priority={}  输入源={}",
            profile.id,
            profile.label,
            profile.priority,
            profile.inputs.len()
        );
    }
    println!("快捷键：");
    for shortcut in &info.shortcuts {
        let accelerator = if shortcut.accelerator.is_empty() {
            "未设置".to_string()
        } else {
            shortcut.accelerator.clone()
        };
        println!("  {} = {}", shortcut.label, accelerator);
    }
}

fn percent_label(value: Option<u8>) -> String {
    value
        .map(|value| format!("{value}%"))
        .unwrap_or_else(|| "未知".into())
}

fn print_json(value: &impl Serialize) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(error) => eprintln!("JSON 序列化失败：{error}"),
    }
}

fn print_json_error(error: &AppError) {
    if let Ok(text) = serde_json::to_string_pretty(error) {
        eprintln!("{text}");
    }
}

fn fail(options: &Options, error: &AppError, code: i32) -> i32 {
    if options.json {
        print_json_error(error);
    } else {
        eprintln_err(error);
    }
    code
}

fn eprintln_err(error: &AppError) {
    eprintln!("错误：{}", error.message);
    if let Some(hint) = &error.hint {
        eprintln!("提示：{hint}");
    }
}

fn usage(message: &str) -> AppError {
    AppError::new("usage", message).with_hint("运行 displaypilot --help 查看用法。")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_relative_level_is_a_positional() {
        let options = Options::parse(vec!["brightness".into(), "-5".into()]).unwrap();
        assert_eq!(
            options.positionals,
            vec!["brightness".to_string(), "-5".to_string()]
        );
    }

    #[test]
    fn unknown_options_are_still_rejected() {
        let error = Options::parse(vec!["list".into(), "--verbose".into()]).unwrap_err();
        assert!(error.contains("--verbose"));
    }

    #[test]
    fn relative_levels_clamp_at_bounds() {
        assert_eq!(parse_level("-5", Some(30), "亮度").unwrap(), 25);
        assert_eq!(parse_level("+90", Some(30), "亮度").unwrap(), 100);
        assert_eq!(parse_level("-50", Some(30), "亮度").unwrap(), 0);
        assert!(parse_level("-", Some(30), "亮度").is_err());
        assert!(parse_level("101", None, "亮度").is_err());
    }
}
