import { api, isMac, isTauri, normalizeError, onEvent } from "./api";
import type {
  AppError,
  AppInfo,
  BootstrapPayload,
  ConfigInfo,
  DisplayList,
  DisplaySummary,
  GeneralConfig,
  ProfileSummary,
  ShortcutBinding,
  ShortcutStatus,
} from "./types";

function el<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) {
    throw new Error(`Missing element #${id}`);
  }
  return element as T;
}

const ui = {
  shell: el<HTMLElement>("settings-shell"),
  close: el<HTMLButtonElement>("close-settings"),
  aboutLine: el<HTMLElement>("about-line"),
  banner: el<HTMLElement>("config-banner"),
  bannerMessage: el<HTMLElement>("config-message"),
  bannerOpen: el<HTMLButtonElement>("banner-open-config"),
  brightnessStep: el<HTMLInputElement>("brightness-step"),
  volumeStep: el<HTMLInputElement>("volume-step"),
  autoHide: el<HTMLInputElement>("auto-hide"),
  showUnsupported: el<HTMLInputElement>("show-unsupported-audio"),
  generalNote: el<HTMLElement>("general-note"),
  save: el<HTMLButtonElement>("save-general"),
  reset: el<HTMLButtonElement>("reset-general"),
  shortcutList: el<HTMLUListElement>("shortcut-list"),
  shortcutNote: el<HTMLElement>("shortcut-note"),
  reload: el<HTMLButtonElement>("reload-config"),
  openConfig: el<HTMLButtonElement>("open-config"),
  displayList: el<HTMLUListElement>("display-list"),
  discoveryNote: el<HTMLElement>("discovery-note"),
  refresh: el<HTMLButtonElement>("refresh-displays"),
  profileList: el<HTMLUListElement>("profile-list"),
  configPath: el<HTMLElement>("config-path"),
  logPath: el<HTMLElement>("log-path"),
  openConfigDir: el<HTMLButtonElement>("open-config-dir"),
  openLogDir: el<HTMLButtonElement>("open-log-dir"),
  appVersion: el<HTMLElement>("app-version"),
  platform: el<HTMLElement>("platform-line"),
  quit: el<HTMLButtonElement>("quit-app"),
  status: el<HTMLElement>("status-line"),
  preview: el<HTMLElement>("preview-note"),
  toast: el<HTMLElement>("toast"),
};

interface State {
  config: ConfigInfo | null;
  app: AppInfo | null;
  displays: DisplaySummary[];
  discoveryError: AppError | null;
  shortcuts: ShortcutStatus[];
  dirty: boolean;
  busy: boolean;
}

const state: State = {
  config: null,
  app: null,
  displays: [],
  discoveryError: null,
  shortcuts: [],
  dirty: false,
  busy: false,
};

let toastTimer: number | undefined;

function showToast(message: string): void {
  ui.toast.textContent = message;
  ui.toast.hidden = false;
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    ui.toast.hidden = true;
  }, 2400);
}

function setStatus(text: string, tone: "ok" | "warning" | "error" | "idle" = "idle"): void {
  ui.status.textContent = text;
  if (tone === "idle") {
    ui.status.removeAttribute("data-state");
  } else {
    ui.status.dataset.state = tone;
  }
}

function showBanner(message: string): void {
  ui.bannerMessage.textContent = message;
  ui.banner.hidden = false;
}

function hideBanner(): void {
  ui.banner.hidden = true;
}

function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) {
    node.className = className;
  }
  if (text !== undefined) {
    node.textContent = text;
  }
  return node;
}

function applyConfig(config: ConfigInfo): void {
  state.config = config;
  ui.configPath.textContent = config.configPath;
  ui.logPath.textContent = config.logPath;
  ui.appVersion.textContent = `v${config.version}`;
  ui.platform.textContent = `${config.os} · ${config.arch}`;
  ui.aboutLine.textContent = `DisplayPilot ${config.version}`;

  const message = config.configError ?? config.configWarnings[0] ?? null;
  if (message) {
    showBanner(message);
  } else {
    hideBanner();
  }
}

function populateGeneral(general: GeneralConfig): void {
  ui.brightnessStep.value = String(general.brightnessStep);
  ui.volumeStep.value = String(general.volumeStep);
  ui.autoHide.checked = general.autoHide;
  ui.showUnsupported.checked = general.showUnsupportedAudio;
  ui.brightnessStep.removeAttribute("aria-invalid");
  ui.volumeStep.removeAttribute("aria-invalid");
  updateDirtyState();
}

function parseStep(input: HTMLInputElement): number | null {
  const value = Number(input.value);
  const valid = Number.isInteger(value) && value >= 1 && value <= 50;
  if (valid) {
    input.removeAttribute("aria-invalid");
    return value;
  }
  input.setAttribute("aria-invalid", "true");
  return null;
}

function readDraft(): GeneralConfig | null {
  const brightnessStep = parseStep(ui.brightnessStep);
  const volumeStep = parseStep(ui.volumeStep);
  if (brightnessStep === null || volumeStep === null) {
    return null;
  }
  return {
    brightnessStep,
    volumeStep,
    autoHide: ui.autoHide.checked,
    showUnsupportedAudio: ui.showUnsupported.checked,
  };
}

function sameGeneral(a: GeneralConfig, b: GeneralConfig): boolean {
  return (
    a.brightnessStep === b.brightnessStep &&
    a.volumeStep === b.volumeStep &&
    a.autoHide === b.autoHide &&
    a.showUnsupportedAudio === b.showUnsupportedAudio
  );
}

function updateDirtyState(): void {
  const saved = state.config?.general;
  const draft = readDraft();
  state.dirty = !saved || !draft || !sameGeneral(saved, draft);
  ui.save.disabled = !state.dirty || !draft || state.busy;
  ui.generalNote.textContent = !draft ? "请检查输入范围" : state.dirty ? "有未保存的更改" : "已保存";
  ui.generalNote.classList.toggle("warning", Boolean(draft) && state.dirty);
}

function acceleratorKeys(accelerator: string): string[] {
  return accelerator
    .split("+")
    .map((token) => token.trim())
    .filter(Boolean)
    .map((token) => {
      const lower = token.toLowerCase();
      if (lower === "cmdorctrl" || lower === "commandorcontrol" || lower === "cmd" || lower === "command") {
        return isMac ? "⌘" : "Ctrl";
      }
      if (lower === "ctrl" || lower === "control") {
        return isMac ? "⌃" : "Ctrl";
      }
      if (lower === "alt" || lower === "option") {
        return isMac ? "⌥" : "Alt";
      }
      if (lower === "shift") {
        return isMac ? "⇧" : "Shift";
      }
      if (lower === "super" || lower === "meta" || lower === "win") {
        return isMac ? "⌘" : "Win";
      }
      return token.length === 1 ? token.toUpperCase() : token;
    });
}

function badge(text: string, tone: "ok" | "error" | "muted" | "" = ""): HTMLSpanElement {
  const span = element("span", `badge${tone ? ` ${tone}` : ""}`, text);
  return span;
}

function shortcutBadge(binding: ShortcutBinding): HTMLSpanElement {
  if (!isTauri) {
    return badge("预览", "muted");
  }
  if (!binding.accelerator.trim()) {
    return badge("未设置", "muted");
  }
  const status = state.shortcuts.find((entry) => entry.action === binding.action);
  if (status?.registered) {
    return badge("已注册", "ok");
  }
  const result = badge(status?.error ? "注册失败" : "未注册", status?.error ? "error" : "muted");
  if (status?.error) {
    result.title = status.error;
  }
  return result;
}

function renderShortcuts(): void {
  const bindings = state.config?.shortcuts ?? [];
  ui.shortcutList.replaceChildren();
  for (const binding of bindings) {
    const row = element("li");
    row.append(element("span", "shortcut-label", binding.label));

    const keys = element("span", "shortcut-keys");
    const tokens = acceleratorKeys(binding.accelerator);
    if (tokens.length === 0) {
      keys.append(element("span", "shortcut-empty", "未设置"));
    } else {
      for (const token of tokens) {
        keys.append(element("kbd", "", token));
      }
    }
    row.append(keys, shortcutBadge(binding));
    ui.shortcutList.append(row);
  }

  ui.shortcutNote.textContent = isTauri
    ? "空快捷键不会注册。若组合键已被系统或其他应用占用，状态会显示为注册失败。"
    : "浏览器预览模式不会注册全局快捷键；在桌面应用中注册状态会显示在这里。";
}

function renderDisplays(): void {
  ui.displayList.replaceChildren();

  if (state.displays.length === 0) {
    const empty = element("li", "display-item");
    empty.append(
      element(
        "p",
        "item-meta",
        "未发现可通过 DDC/CI 控制的显示器。请确认显示器已唤醒，并在 OSD 中启用 DDC/CI。",
      ),
    );
    ui.displayList.append(empty);
  }

  for (const display of state.displays) {
    const row = element("li", "display-item");
    const head = element("div", "display-head");
    const main = element("div", "item-main");
    main.append(element("span", "item-title", display.name));

    const modelId =
      display.modelId === null
        ? "—"
        : `0x${display.modelId.toString(16).toUpperCase().padStart(4, "0")}`;
    const parts = [display.vendorId ?? "—", modelId, display.backend];
    if (display.serialNumber) {
      parts.push(`SN ${display.serialNumber}`);
    }
    main.append(element("span", "item-meta", parts.join(" · ")));

    const profile = badge(display.profileLabel, display.profileMatched ? "ok" : "muted");
    if (!display.profileMatched) {
      profile.title = "没有匹配到专用配置档，正在使用通用回退值。";
    }
    head.append(main, profile);
    row.append(head);

    const inputs = display.inputs.map((input) => input.label).join(" · ");
    row.append(element("div", "item-inputs", `输入源：${inputs || "未配置"}`));
    ui.displayList.append(row);
  }

  if (state.discoveryError) {
    const error = state.discoveryError;
    ui.discoveryNote.textContent = error.hint
      ? `${error.message} ${error.hint}`
      : error.message;
    ui.discoveryNote.hidden = false;
  } else {
    ui.discoveryNote.hidden = true;
  }
}

function renderProfiles(): void {
  const profiles: ProfileSummary[] = state.config?.profiles ?? [];
  ui.profileList.replaceChildren();
  for (const profile of profiles) {
    const row = element("li", "profile-item");
    const head = element("div", "profile-head");
    head.append(element("span", "profile-name", profile.label));

    const matched =
      profile.matchedDisplayIds.length === 0
        ? "未匹配"
        : `匹配 ${profile.matchedDisplayIds.length} 台`;
    head.append(element("span", "badge", matched));
    row.append(head);
    row.append(
      element(
        "div",
        "profile-meta",
        `${profile.id} · 优先级 ${profile.priority}`,
      ),
    );
    const inputs = profile.inputs.map((input) => input.label).join(" · ");
    row.append(element("div", "profile-inputs", `输入源：${inputs || "未定义"}`));
    ui.profileList.append(row);
  }
}

async function refreshShortcutStatus(): Promise<void> {
  if (!isTauri) {
    state.shortcuts = [];
    renderShortcuts();
    return;
  }
  try {
    state.shortcuts = await api<ShortcutStatus[]>("shortcut_status");
  } catch {
    state.shortcuts = [];
  }
  renderShortcuts();
}

async function load(): Promise<void> {
  if (state.busy) {
    return;
  }
  state.busy = true;
  setStatus("正在加载配置…");
  try {
    const payload = await api<BootstrapPayload>("bootstrap");
    state.app = payload.app;
    state.displays = payload.displays.displays;
    state.discoveryError = payload.displays.discoveryError;
    applyConfig(payload.config);
    populateGeneral(payload.config.general);
    renderDisplays();
    renderProfiles();
    await refreshShortcutStatus();
    setStatus("配置已加载", "ok");
  } catch (error) {
    const appError = normalizeError(error);
    showBanner(appError.hint ? `${appError.message} ${appError.hint}` : appError.message);
    setStatus("加载失败", "error");
  } finally {
    state.busy = false;
    updateDirtyState();
  }
}

async function saveGeneral(): Promise<void> {
  const draft = readDraft();
  if (!draft) {
    setStatus("请输入 1–50 之间的整数", "warning");
    return;
  }
  state.busy = true;
  updateDirtyState();
  ui.save.disabled = true;
  setStatus("正在保存…");
  try {
    const config = await api<ConfigInfo>("update_general", {
      general: draft,
    });
    applyConfig(config);
    populateGeneral(config.general);
    renderProfiles();
    await refreshShortcutStatus();
    showToast("设置已保存");
    setStatus("设置已保存", "ok");
  } catch (error) {
    const appError = normalizeError(error);
    showBanner(appError.hint ? `${appError.message} ${appError.hint}` : appError.message);
    setStatus("保存失败", "error");
    showToast("设置保存失败");
  } finally {
    state.busy = false;
    updateDirtyState();
  }
}

async function reloadConfig(): Promise<void> {
  if (state.busy) {
    return;
  }
  state.busy = true;
  setStatus("正在重新加载配置…");
  try {
    const config = await api<ConfigInfo>("reload_config");
    applyConfig(config);
    populateGeneral(config.general);
    renderProfiles();
    await refreshShortcutStatus();
    showToast("配置已重新加载");
    setStatus("配置已重新加载", "ok");
  } catch (error) {
    const appError = normalizeError(error);
    showBanner(appError.hint ? `${appError.message} ${appError.hint}` : appError.message);
    setStatus("重新加载失败", "error");
  } finally {
    state.busy = false;
    updateDirtyState();
  }
}

async function refreshDisplays(): Promise<void> {
  if (state.busy) {
    return;
  }
  state.busy = true;
  ui.refresh.disabled = true;
  setStatus("正在重新扫描显示器…");
  try {
    const list = await api<DisplayList>("refresh_displays");
    state.displays = list.displays;
    state.discoveryError = list.discoveryError;
    renderDisplays();
    const config = await api<ConfigInfo>("config_info");
    applyConfig(config);
    renderProfiles();
    showToast(`已发现 ${list.displays.length} 台显示器`);
    setStatus("显示器列表已刷新", "ok");
  } catch (error) {
    const appError = normalizeError(error);
    showBanner(appError.hint ? `${appError.message} ${appError.hint}` : appError.message);
    setStatus("重新扫描失败", "error");
  } finally {
    state.busy = false;
    ui.refresh.disabled = false;
    updateDirtyState();
  }
}

function openDirectory(command: "open_config_dir" | "open_log_dir"): void {
  void api(command).catch((error) => {
    const appError = normalizeError(error);
    showBanner(appError.hint ? `${appError.message} ${appError.hint}` : appError.message);
  });
}

function resetGeneral(): void {
  if (state.config) {
    populateGeneral(state.config.general);
    setStatus("已还原为已保存的设置", "idle");
  }
}

async function closeSettings(): Promise<void> {
  if (!isTauri) {
    showToast("浏览器预览模式：关闭设置");
    return;
  }
  try {
    await api("hide_settings");
  } catch {
    window.close();
  }
}

function quitApp(): void {
  if (!isTauri) {
    showToast("浏览器预览模式不会退出应用");
    return;
  }
  if (!window.confirm("退出 DisplayPilot？托盘图标将消失。")) {
    return;
  }
  void api("quit_app").catch((error) => {
    const appError = normalizeError(error);
    showBanner(appError.message);
  });
}

for (const input of [ui.brightnessStep, ui.volumeStep]) {
  input.addEventListener("input", updateDirtyState);
  input.addEventListener("change", updateDirtyState);
}

for (const toggle of [ui.autoHide, ui.showUnsupported]) {
  toggle.addEventListener("change", updateDirtyState);
}

ui.save.addEventListener("click", () => {
  void saveGeneral();
});

ui.reset.addEventListener("click", resetGeneral);

ui.reload.addEventListener("click", () => {
  void reloadConfig();
});

ui.openConfig.addEventListener("click", () => {
  openDirectory("open_config_dir");
});

ui.openConfigDir.addEventListener("click", () => {
  openDirectory("open_config_dir");
});

ui.openLogDir.addEventListener("click", () => {
  openDirectory("open_log_dir");
});

ui.bannerOpen.addEventListener("click", () => {
  openDirectory("open_config_dir");
});

ui.refresh.addEventListener("click", () => {
  void refreshDisplays();
});

ui.close.addEventListener("click", () => {
  void closeSettings();
});

ui.quit.addEventListener("click", quitApp);

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    void closeSettings();
  }
});

async function bindEvents(): Promise<void> {
  try {
    await onEvent("settings-shown", () => {
      if (!state.busy && !state.dirty) {
        void load();
      }
    });
  } catch {
    // Event wiring is optional; the page still works through direct commands.
  }
}

ui.preview.hidden = isTauri;
void bindEvents();
void load();
