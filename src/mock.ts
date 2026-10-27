import type {
  AppError,
  AppInfo,
  BootstrapPayload,
  ConfigInfo,
  DiagnosticReport,
  DisplayList,
  DisplaySnapshot,
  DisplaySummary,
  GeneralConfig,
  ShortcutStatus,
} from "./types";

const display: DisplaySummary = {
  id: "dsp-demo27ul850",
  name: "LG 27UL850-W",
  profileId: "lg-27ul850",
  profileLabel: "LG 27UL850",
  profileMatched: true,
  backend: "winapi",
  vendorId: "GSM",
  modelId: 0x5b3f,
  modelName: "LG 27UL850-W",
  serialNumber: "DEMO1234",
  inputs: [
    { id: "hdmi1", label: "HDMI1" },
    { id: "hdmi2", label: "HDMI2" },
    { id: "dp", label: "DP" },
    { id: "typec", label: "TypeC" },
  ],
  inputSwitchSupported: true,
  currentInputVcpCode: 0x60,
};

const inputValues: Record<string, number> = {
  hdmi1: 0x90,
  hdmi2: 0x91,
  dp: 0xd0,
  typec: 0xd1,
};

let general: GeneralConfig = {
  brightnessStep: 5,
  volumeStep: 5,
  autoHide: true,
  showUnsupportedAudio: true,
};

let currentInputId = "hdmi1";
let brightnessPercent = 62;
let volumePercent = 34;
let muted = false;

function currentInput() {
  const input = display.inputs.find((item) => item.id === currentInputId);
  return {
    inputId: input?.id ?? null,
    label: input?.label ?? null,
    rawValue: input ? inputValues[input.id] ?? null : null,
    readError: null,
  };
}

function snapshot(): DisplaySnapshot {
  return {
    display: { ...display, inputs: display.inputs.map((input) => ({ ...input })) },
    currentInput: currentInput(),
    brightness: {
      supported: true,
      editable: true,
      percent: brightnessPercent,
      rawValue: Math.round((brightnessPercent / 100) * 255),
      min: 0,
      max: 255,
      step: 5,
      readError: null,
      hint: null,
    },
    volume: {
      supported: true,
      editable: true,
      percent: volumePercent,
      rawValue: Math.round((volumePercent / 100) * 255),
      min: 0,
      max: 255,
      step: 5,
      readError: null,
      hint: null,
    },
    mute: {
      supported: true,
      editable: true,
      muted,
      rawValue: muted ? 1 : 2,
      readError: null,
      hint: null,
    },
    probedAt: new Date().toISOString(),
  };
}

function displayList(): DisplayList {
  return {
    displays: [snapshot().display],
    discoveryError: null,
    configError: null,
    configWarnings: [],
  };
}

function configInfo(): ConfigInfo {
  return {
    version: "0.1.0",
    os: "windows",
    arch: "x86_64",
    configPath: "C:\\Users\\demo\\AppData\\Roaming\\DisplayPilot\\config.toml",
    configDir: "C:\\Users\\demo\\AppData\\Roaming\\DisplayPilot",
    logPath: "C:\\Users\\demo\\AppData\\Local\\DisplayPilot\\logs\\displaypilot.log",
    configError: null,
    configWarnings: [],
    general: { ...general },
    profiles: [
      {
        id: "lg-27ul850",
        label: "LG 27UL850",
        priority: 100,
        inputs: display.inputs,
        matchedDisplayIds: [display.id],
      },
      {
        id: "generic-ddc",
        label: "通用 DDC/CI 显示器",
        priority: -100,
        inputs: display.inputs,
        matchedDisplayIds: [],
      },
    ],
    shortcuts: [
      { action: "toggle_panel", label: "打开/关闭面板", accelerator: "" },
      { action: "brightness_up", label: "亮度增加", accelerator: "" },
      { action: "brightness_down", label: "亮度降低", accelerator: "" },
      { action: "volume_up", label: "音量增加", accelerator: "" },
      { action: "volume_down", label: "音量降低", accelerator: "" },
      { action: "mute", label: "静音/取消静音", accelerator: "" },
      { action: "input:hdmi1", label: "切换到 HDMI1", accelerator: "" },
      { action: "input:hdmi2", label: "切换到 HDMI2", accelerator: "" },
      { action: "input:dp", label: "切换到 DP", accelerator: "" },
      { action: "input:typec", label: "切换到 TYPEC", accelerator: "" },
    ],
  };
}

function appInfo(): AppInfo {
  const config = configInfo();
  return {
    version: config.version,
    os: config.os,
    arch: config.arch,
    configPath: config.configPath,
    logPath: config.logPath,
  };
}

function shortcutStatus(): ShortcutStatus[] {
  return configInfo().shortcuts.map((binding) => ({ ...binding, registered: false, error: null }));
}

function diagnosticReport(): DiagnosticReport {
  const state = snapshot();
  const config = configInfo();
  return {
    generatedAt: state.probedAt,
    appVersion: config.version,
    os: config.os,
    arch: config.arch,
    display: {
      id: display.id,
      name: display.name,
      backend: display.backend,
      nativeId: "\\\\?\\DISPLAY#GSM5B3F#5&demo",
      vendorId: display.vendorId,
      modelId: display.modelId,
      modelName: display.modelName,
      serialNumber: display.serialNumber,
      profileId: display.profileId,
      profileLabel: display.profileLabel,
      profileMatched: true,
      capabilitiesError: null,
      inputs: display.inputs.map((input) => ({
        id: input.id,
        label: input.label,
        selector: `standard: VCP 0x60 = 0x${(inputValues[input.id] ?? 0).toString(16).toUpperCase()}`,
      })),
    },
    currentInput: state.currentInput,
    brightness: state.brightness,
    volume: state.volume,
    mute: state.mute,
    capabilities:
      "(prot(monitor)type(lcd)model(27UL850)cmds(01 02 03 0C E3 F3)vcp(02 04 05 08 10 12 14 16 18 1A 52 60 62 6C 6E 70 86 8D AC AE B2 B6 C6 C8 C9 D6 DF E0 F0 F1 F2 FD)mswhql(1)asset_eep(40)mccs_ver(2.2))",
    capabilitiesError: null,
    notes: [
      "DDC/CI 指令只应在显示器当前活动输入源所在的主机上发起。",
      "音量/静音的 auto 能力以实际读取结果为准；不支持时控件会禁用。",
    ],
    vcpReads: [
      { code: "0x60", name: "Input Source", ok: true, value: inputValues[currentInputId] ?? 0x90, maximum: 0, ty: 0, sh: 0, sl: 0, error: null },
      { code: "0x10", name: "Brightness", ok: true, value: state.brightness.rawValue, maximum: 255, ty: 0, sh: 0, sl: 0, error: null },
      { code: "0x62", name: "Audio Volume", ok: true, value: state.volume.rawValue, maximum: 255, ty: 0, sh: 0, sl: 0, error: null },
      { code: "0x8D", name: "Audio Mute", ok: true, value: muted ? 1 : 2, maximum: 2, ty: 0, sh: 0, sl: 0, error: null },
      { code: "0xDF", name: "VCP Version", ok: true, value: 0x0202, maximum: 0, ty: 0, sh: 0, sl: 0, error: null },
    ],
  };
}

const delay = () => new Promise<void>((resolve) => setTimeout(resolve, 70));

function fail(code: string, message: string): never {
  const error: AppError = { code, message, hint: null };
  throw error;
}

export async function mockInvoke(command: string, args?: Record<string, unknown>): Promise<unknown> {
  await delay();
  switch (command) {
    case "bootstrap":
      return {
        app: appInfo(),
        config: configInfo(),
        displays: displayList(),
      } satisfies BootstrapPayload;
    case "app_info":
      return appInfo();
    case "list_displays":
    case "refresh_displays":
      return displayList();
    case "select_display":
    case "hide_panel":
    case "hide_settings":
    case "open_settings":
    case "open_config_dir":
    case "open_log_dir":
    case "quit_app":
      return { ok: true };
    case "main_window_visible":
      return true;
    case "get_snapshot":
      return snapshot();
    case "switch_input": {
      const inputId = String(args?.inputId ?? "");
      if (!display.inputs.some((input) => input.id === inputId)) {
        fail("invalid_input", `输入源 ${inputId} 不在当前显示器配置中。`);
      }
      currentInputId = inputId;
      return snapshot();
    }
    case "set_brightness": {
      brightnessPercent = Math.max(0, Math.min(100, Number(args?.percent ?? 0)));
      return snapshot();
    }
    case "set_volume": {
      volumePercent = Math.max(0, Math.min(100, Number(args?.percent ?? 0)));
      return snapshot();
    }
    case "toggle_mute":
      muted = !muted;
      return snapshot();
    case "set_mute":
      muted = Boolean(args?.muted);
      return snapshot();
    case "config_info":
      return configInfo();
    case "reload_config":
      return configInfo();
    case "update_general": {
      general = { ...(args?.general as GeneralConfig) };
      return configInfo();
    }
    case "shortcut_status":
      return shortcutStatus();
    case "diagnostics":
      return diagnosticReport();
    default:
      fail("mock", `浏览器预览模式不支持命令 ${command}。`);
  }
}
