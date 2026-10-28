import { api, isTauri, normalizeError, onEvent } from "./api";
import type {
  AppError,
  BootstrapPayload,
  ConfigInfo,
  DiagnosticReport,
  DisplayList,
  DisplaySnapshot,
  DisplaySummary,
  FeatureValueState,
  InputOption,
} from "./types";

function el<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) {
    throw new Error(`Missing element #${id}`);
  }
  return element as T;
}

const ui = {
  panel: el<HTMLElement>("panel"),
  subtitle: el<HTMLElement>("display-subtitle"),
  refresh: el<HTMLButtonElement>("refresh-button"),
  settings: el<HTMLButtonElement>("settings-button"),
  configBanner: el<HTMLElement>("config-banner"),
  configMessage: el<HTMLElement>("config-message"),
  configOpen: el<HTMLButtonElement>("config-open"),
  displaySwitcher: el<HTMLElement>("display-switcher"),
  displayButton: el<HTMLButtonElement>("display-button"),
  displayMenu: el<HTMLElement>("display-menu"),
  displayName: el<HTMLElement>("display-name"),
  displayMeta: el<HTMLElement>("display-meta"),
  inputCard: el<HTMLElement>("input-card"),
  inputGrid: el<HTMLElement>("input-grid"),
  inputNote: el<HTMLElement>("input-note"),
  brightnessCard: el<HTMLElement>("brightness-card"),
  brightnessSlider: el<HTMLInputElement>("brightness-slider"),
  brightnessValue: el<HTMLOutputElement>("brightness-value"),
  brightnessPresets: el<HTMLElement>("brightness-presets"),
  brightnessHint: el<HTMLElement>("brightness-hint"),
  audioCard: el<HTMLElement>("audio-card"),
  volumeSlider: el<HTMLInputElement>("volume-slider"),
  volumeValue: el<HTMLOutputElement>("volume-value"),
  volumeHint: el<HTMLElement>("volume-hint"),
  muteButton: el<HTMLButtonElement>("mute-button"),
  emptyState: el<HTMLElement>("empty-state"),
  emptyMessage: el<HTMLElement>("empty-message"),
  emptyRetry: el<HTMLButtonElement>("empty-retry"),
  errorPanel: el<HTMLElement>("error-panel"),
  errorTitle: el<HTMLElement>("error-title"),
  errorMessage: el<HTMLElement>("error-message"),
  errorHint: el<HTMLElement>("error-hint"),
  errorClose: el<HTMLButtonElement>("error-close"),
  status: el<HTMLElement>("status-line"),
  diagnose: el<HTMLButtonElement>("diagnose-button"),
  toast: el<HTMLElement>("toast"),
};

interface State {
  config: ConfigInfo | null;
  displays: DisplaySummary[];
  snapshot: DisplaySnapshot | null;
  selectedId: string | null;
  busyInputId: string | null;
  loading: boolean;
  brightnessDragging: boolean;
  volumeDragging: boolean;
  brightnessSeq: number;
  volumeSeq: number;
  lastProbeAt: number;
}

const state: State = {
  config: null,
  displays: [],
  snapshot: null,
  selectedId: null,
  busyInputId: null,
  loading: true,
  brightnessDragging: false,
  volumeDragging: false,
  brightnessSeq: 0,
  volumeSeq: 0,
  lastProbeAt: 0,
};

let toastTimer: number | undefined;
let brightnessTimer: number | undefined;
let volumeTimer: number | undefined;

const CHECK_ICON = "✓";

function showToast(message: string): void {
  ui.toast.textContent = message;
  ui.toast.hidden = false;
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    ui.toast.hidden = true;
  }, 2600);
}

function setStatus(text: string, tone: "ok" | "warning" | "error" | "idle" = "idle"): void {
  ui.status.textContent = text;
  if (tone === "idle") {
    ui.status.removeAttribute("data-state");
  } else {
    ui.status.dataset.state = tone;
  }
}

function showError(title: string, error: AppError): void {
  ui.errorTitle.textContent = title;
  ui.errorMessage.textContent = error.message;
  if (error.hint) {
    ui.errorHint.textContent = error.hint;
    ui.errorHint.hidden = false;
  } else {
    ui.errorHint.hidden = true;
  }
  ui.errorPanel.hidden = false;
  ui.errorPanel.scrollIntoView({ block: "nearest" });
}

function clearError(): void {
  ui.errorPanel.hidden = true;
}

function percentText(value: number | null): string {
  return value === null ? "—" : `${value}%`;
}

function setFill(slider: HTMLInputElement, value: number): void {
  slider.style.setProperty("--fill", `${value}%`);
  slider.setAttribute("aria-valuetext", `${value}%`);
}

function formatTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return "刚刚";
  }
  return date.toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function applyConfig(config: ConfigInfo): void {
  state.config = config;
  const message = config.configError ?? config.configWarnings[0] ?? null;
  if (message) {
    ui.configMessage.textContent = message;
    ui.configBanner.hidden = false;
  } else {
    ui.configBanner.hidden = true;
  }
}

function applyDisplayList(list: DisplayList): void {
  state.displays = list.displays;
  const stored = localStorage.getItem("displaypilot.display");
  if (!list.displays.some((display) => display.id === state.selectedId)) {
    state.selectedId =
      list.displays.find((display) => display.id === stored)?.id ??
      list.displays[0]?.id ??
      null;
  }
}

function renderDisplaySwitcher(): void {
  const selected = state.displays.find((display) => display.id === state.selectedId);
  if (!selected || state.displays.length < 2) {
    ui.displaySwitcher.hidden = true;
    closeDisplayMenu();
    return;
  }

  ui.displaySwitcher.hidden = false;
  ui.displayName.textContent = selected.name;
  ui.displayMeta.textContent = `${selected.profileLabel} · ${selected.backend}${
    selected.profileMatched ? "" : " · 通用配置"
  }`;
}

function buildDisplayMenu(): void {
  ui.displayMenu.replaceChildren();
  for (const display of state.displays) {
    const option = document.createElement("button");
    option.type = "button";
    option.className = "display-option";
    option.setAttribute("role", "option");
    option.setAttribute("aria-selected", String(display.id === state.selectedId));

    const text = document.createElement("span");
    text.className = "display-option-text";
    const name = document.createElement("span");
    name.className = "display-option-name";
    name.textContent = display.name;
    const meta = document.createElement("span");
    meta.className = "display-option-meta";
    meta.textContent = `${display.profileLabel} · ${display.backend}`;
    text.append(name, meta);

    const check = document.createElement("span");
    check.className = "check";
    check.setAttribute("aria-hidden", "true");
    check.textContent = CHECK_ICON;

    option.append(text, check);
    option.addEventListener("click", () => {
      closeDisplayMenu();
      void selectDisplay(display.id);
    });
    ui.displayMenu.append(option);
  }
}

function openDisplayMenu(): void {
  buildDisplayMenu();
  ui.displayMenu.hidden = false;
  ui.displayButton.setAttribute("aria-expanded", "true");
  const selected = ui.displayMenu.querySelector<HTMLButtonElement>(
    '.display-option[aria-selected="true"]',
  );
  (selected ?? ui.displayMenu.querySelector<HTMLButtonElement>(".display-option"))?.focus();
}

function closeDisplayMenu(): void {
  if (ui.displayMenu.hidden) {
    return;
  }
  ui.displayMenu.hidden = true;
  ui.displayButton.setAttribute("aria-expanded", "false");
}

function toggleDisplayMenu(): void {
  if (ui.displayMenu.hidden) {
    openDisplayMenu();
  } else {
    closeDisplayMenu();
  }
}

function inputButton(input: InputOption): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "input-button";
  const isCurrent = state.snapshot?.currentInput.inputId === input.id;
  const isBusy = state.busyInputId === input.id;
  const unavailable = !input.supported;
  button.setAttribute("aria-pressed", String(isCurrent));
  button.setAttribute(
    "aria-label",
    unavailable
      ? `${input.label}（不支持切换：${input.reason ?? "未知原因"}）`
      : isCurrent
        ? `${input.label}（当前输入）`
        : `切换到 ${input.label}`,
  );
  button.dataset.unsupported = String(unavailable);
  if (unavailable) {
    const reason = input.reason ?? "该端口在当前 GPU 与平台上没有可用的切换通道。";
    button.title = input.hint ? `${reason}\n${input.hint}` : reason;
  }

  const switchSupported = state.snapshot?.display.inputSwitchSupported ?? true;
  button.disabled = !switchSupported || state.busyInputId !== null || unavailable;

  if (isBusy) {
    const spinner = document.createElement("span");
    spinner.className = "spinner";
    spinner.setAttribute("aria-hidden", "true");
    button.append(spinner);
  } else if (isCurrent) {
    const check = document.createElement("span");
    check.className = "check";
    check.setAttribute("aria-hidden", "true");
    check.textContent = CHECK_ICON;
    button.append(check);
  }

  const label = document.createElement("span");
  label.textContent = isBusy ? "切换中…" : input.label;
  button.append(label);

  button.addEventListener("click", () => {
    void switchInput(input.id, input.label);
  });
  return button;
}

function renderInputs(snapshot: DisplaySnapshot): void {
  ui.inputGrid.replaceChildren();
  const inputs = snapshot.display.inputs;
  if (inputs.length === 0) {
    const note = document.createElement("p");
    note.className = "field-hint";
    note.textContent = "当前显示器配置没有定义输入源。";
    ui.inputGrid.append(note);
    ui.inputNote.textContent = "";
    return;
  }

  for (const input of inputs) {
    ui.inputGrid.append(inputButton(input));
  }

  if (!snapshot.display.inputSwitchSupported) {
    ui.inputNote.textContent = "未启用";
    ui.inputNote.classList.add("warning");
  } else if (state.busyInputId) {
    ui.inputNote.textContent = "正在切换…";
    ui.inputNote.classList.remove("warning");
  } else if (snapshot.currentInput.inputId) {
    ui.inputNote.textContent = `当前 ${snapshot.currentInput.label ?? ""}`;
    ui.inputNote.classList.remove("warning");
  } else if (snapshot.currentInput.readError) {
    ui.inputNote.textContent = "状态未知";
    ui.inputNote.classList.add("warning");
  } else {
    ui.inputNote.textContent = "";
    ui.inputNote.classList.remove("warning");
  }
}

function renderRange(
  feature: FeatureValueState,
  slider: HTMLInputElement,
  value: HTMLOutputElement,
  hint: HTMLElement,
  presets?: HTMLElement,
): void {
  const percent = feature.percent;
  const editable = feature.supported && feature.editable;
  slider.disabled = !editable;
  slider.setAttribute("aria-disabled", String(!editable));
  if (!state.brightnessDragging && slider === ui.brightnessSlider) {
    slider.value = String(percent ?? 0);
  }
  if (!state.volumeDragging && slider === ui.volumeSlider) {
    slider.value = String(percent ?? 0);
  }
  setFill(slider, percent ?? 0);
  value.textContent = percentText(percent);

  if (hint) {
    const message = feature.hint ?? feature.readError;
    if (message) {
      hint.textContent = message;
      hint.hidden = false;
      hint.dataset.tone = feature.supported ? "warning" : "idle";
    } else {
      hint.hidden = true;
    }
  }

  if (presets) {
    for (const button of presets.querySelectorAll<HTMLButtonElement>("button")) {
      button.disabled = !editable;
    }
  }
}

function renderSnapshot(snapshot: DisplaySnapshot): void {
  if (state.selectedId && snapshot.display.id !== state.selectedId) {
    return;
  }
  state.snapshot = snapshot;
  state.lastProbeAt = Date.now();
  ui.panel.setAttribute("aria-busy", "false");
  state.loading = false;
  ui.emptyState.hidden = true;
  ui.inputCard.hidden = false;
  ui.brightnessCard.hidden = false;

  ui.subtitle.textContent = `${snapshot.display.name} · ${snapshot.display.profileLabel}`;
  renderDisplaySwitcher();
  renderInputs(snapshot);

  renderRange(
    snapshot.brightness,
    ui.brightnessSlider,
    ui.brightnessValue,
    ui.brightnessHint,
    ui.brightnessPresets,
  );

  const audioSupported = snapshot.volume.supported || snapshot.mute.supported;
  const showAudio = audioSupported || (state.config?.general.showUnsupportedAudio ?? true);
  ui.audioCard.hidden = !showAudio;
  renderRange(snapshot.volume, ui.volumeSlider, ui.volumeValue, ui.volumeHint);
  ui.muteButton.disabled = !snapshot.mute.editable;
  ui.muteButton.setAttribute("aria-checked", String(snapshot.mute.muted === true));
  ui.muteButton.setAttribute("aria-label", snapshot.mute.muted ? "取消静音" : "静音");
  if (!snapshot.mute.supported && snapshot.mute.hint) {
    ui.volumeHint.textContent = snapshot.mute.hint;
    ui.volumeHint.hidden = false;
    ui.volumeHint.dataset.tone = "idle";
  }

  const warnings =
    snapshot.brightness.readError ??
    snapshot.volume.readError ??
    snapshot.mute.readError ??
    snapshot.currentInput.readError;
  if (warnings) {
    setStatus(`更新于 ${formatTime(snapshot.probedAt)} · 部分状态不可用`, "warning");
  } else {
    const raw =
      snapshot.display.rawI2cAvailable && snapshot.display.rawI2cBackend
        ? ` · 原始 I2C ${snapshot.display.rawI2cBackend}`
        : "";
    setStatus(
      `更新于 ${formatTime(snapshot.probedAt)} · ${snapshot.display.backend}${raw}`,
      "ok",
    );
  }
}

function renderEmpty(list: DisplayList | null, error?: AppError): void {
  state.snapshot = null;
  state.loading = false;
  ui.panel.setAttribute("aria-busy", "false");
  ui.inputCard.hidden = true;
  ui.brightnessCard.hidden = true;
  ui.audioCard.hidden = true;
  ui.displaySwitcher.hidden = true;
  closeDisplayMenu();
  ui.emptyState.hidden = false;
  ui.subtitle.textContent = "未发现显示器";

  const discovery = error ?? list?.discoveryError ?? null;
  ui.subtitle.textContent = discovery?.code === "timeout" ? "显示器响应超时" : "未发现显示器";
  if (discovery) {
    ui.emptyMessage.textContent = discovery.hint
      ? `${discovery.message} ${discovery.hint}`
      : discovery.message;
    setStatus(discovery.code === "timeout" ? "显示器响应超时" : "未发现可控制的显示器", "error");
  } else {
    ui.emptyMessage.textContent =
      "请确认显示器已唤醒，并在 OSD 中启用 DDC/CI。优先使用直连 HDMI、DP 或 Type-C。";
    setStatus("未发现可控制的显示器", "warning");
  }
  ui.emptyRetry.disabled = false;
}

async function selectDisplay(displayId: string): Promise<void> {
  if (state.selectedId === displayId && state.snapshot) {
    return;
  }
  state.selectedId = displayId;
  localStorage.setItem("displaypilot.display", displayId);
  renderDisplaySwitcher();
  await loadSnapshot(displayId);
}

async function loadSnapshot(displayId: string, force = false): Promise<void> {
  try {
    await api("select_display", { displayId });
    const snapshot = await api<DisplaySnapshot>("get_snapshot", {
      displayId,
      force,
    });
    if (displayId === state.selectedId) {
      renderSnapshot(snapshot);
    }
  } catch (error) {
    const appError = normalizeError(error);
    if (displayId !== state.selectedId) {
      return;
    }
    showError("无法读取显示器状态", appError);
    setStatus("读取状态失败", "error");
    ui.panel.setAttribute("aria-busy", "false");
    state.loading = false;
  }
}

async function load(): Promise<void> {
  state.loading = true;
  ui.panel.setAttribute("aria-busy", "true");
  ui.refresh.disabled = true;
  try {
    const payload = await api<BootstrapPayload>("bootstrap");
    applyConfig(payload.config);
    applyDisplayList(payload.displays);

    if (payload.displays.displays.length === 0) {
      renderEmpty(payload.displays);
      return;
    }
    if (state.selectedId) {
      await loadSnapshot(state.selectedId, true);
    }
  } catch (error) {
    const appError = normalizeError(error);
    applyDisplayList({ displays: [], discoveryError: appError, configError: null, configWarnings: [] });
    renderEmpty(null, appError);
  } finally {
    state.loading = false;
    ui.refresh.disabled = false;
  }
}

async function refreshAll(showToastAfter = false): Promise<void> {
  if (state.loading) {
    return;
  }
  state.loading = true;
  ui.refresh.disabled = true;
  ui.refresh.classList.add("is-spinning");
  try {
    const list = await api<DisplayList>("refresh_displays");
    applyDisplayList(list);
    if (list.displays.length === 0) {
      renderEmpty(list);
    } else if (state.selectedId) {
      await loadSnapshot(state.selectedId, true);
      if (showToastAfter) {
        showToast("显示器状态已刷新");
      }
    }
    clearError();
  } catch (error) {
    const appError = normalizeError(error);
    showError("刷新失败", appError);
    setStatus("刷新失败", "error");
  } finally {
    state.loading = false;
    ui.refresh.disabled = false;
    ui.refresh.classList.remove("is-spinning");
  }
}

async function refreshSnapshot(force = true): Promise<void> {
  if (!state.selectedId || state.loading) {
    return;
  }
  try {
    const snapshot = await api<DisplaySnapshot>("get_snapshot", {
      displayId: state.selectedId,
      force,
    });
    renderSnapshot(snapshot);
  } catch (error) {
    showError("无法刷新状态", normalizeError(error));
  }
}

async function switchInput(inputId: string, inputLabel: string): Promise<void> {
  if (!state.selectedId || state.busyInputId) {
    return;
  }
  state.busyInputId = inputId;
  if (state.snapshot) {
    renderSnapshot(state.snapshot);
  }
  ui.inputCard.setAttribute("aria-busy", "true");
  try {
    const snapshot = await api<DisplaySnapshot>("switch_input", {
      displayId: state.selectedId,
      inputId,
    });
    renderSnapshot(snapshot);
    showToast(`已切换到 ${inputLabel}`);
    clearError();
  } catch (error) {
    const appError = normalizeError(error);
    if (appError.code === "input_unsupported") {
      try {
        const list = await api<DisplayList>("list_displays", { force: false });
        applyDisplayList(list);
        await refreshSnapshot(true);
      } catch {
        // Keep the original actionable error visible.
      }
    }
    showError("无法切换输入源", appError);
  } finally {
    state.busyInputId = null;
    ui.inputCard.setAttribute("aria-busy", "false");
    if (state.snapshot) {
      renderSnapshot(state.snapshot);
    }
  }
}

function restoreBrightness(): void {
  const percent = state.snapshot?.brightness.percent ?? 0;
  ui.brightnessSlider.value = String(percent);
  ui.brightnessValue.textContent = percentText(percent);
  setFill(ui.brightnessSlider, percent);
}

function restoreVolume(): void {
  const percent = state.snapshot?.volume.percent ?? 0;
  ui.volumeSlider.value = String(percent);
  ui.volumeValue.textContent = percentText(percent);
  setFill(ui.volumeSlider, percent);
}

async function commitBrightness(percent: number): Promise<void> {
  if (!state.snapshot) {
    return;
  }
  const displayId = state.snapshot.display.id;
  const seq = ++state.brightnessSeq;
  ui.brightnessCard.setAttribute("aria-busy", "true");
  try {
    const snapshot = await api<DisplaySnapshot>("set_brightness", {
      displayId,
      percent,
    });
    if (seq === state.brightnessSeq && snapshot.display.id === state.selectedId) {
      renderSnapshot(snapshot);
    }
  } catch (error) {
    if (seq === state.brightnessSeq) {
      showError("无法调节亮度", normalizeError(error));
      restoreBrightness();
    }
  } finally {
    if (seq === state.brightnessSeq) {
      ui.brightnessCard.setAttribute("aria-busy", "false");
    }
  }
}

async function commitVolume(percent: number): Promise<void> {
  if (!state.snapshot) {
    return;
  }
  const displayId = state.snapshot.display.id;
  const seq = ++state.volumeSeq;
  ui.audioCard.setAttribute("aria-busy", "true");
  try {
    const snapshot = await api<DisplaySnapshot>("set_volume", {
      displayId,
      percent,
    });
    if (seq === state.volumeSeq && snapshot.display.id === state.selectedId) {
      renderSnapshot(snapshot);
    }
  } catch (error) {
    if (seq === state.volumeSeq) {
      showError("无法调节音量", normalizeError(error));
      restoreVolume();
    }
  } finally {
    if (seq === state.volumeSeq) {
      ui.audioCard.setAttribute("aria-busy", "false");
    }
  }
}

async function toggleMute(): Promise<void> {
  if (!state.snapshot || !state.snapshot.mute.editable) {
    return;
  }
  const displayId = state.snapshot.display.id;
  const previous = state.snapshot.mute.muted;
  ui.muteButton.setAttribute("aria-checked", String(previous !== true));
  ui.audioCard.setAttribute("aria-busy", "true");
  try {
    const snapshot = await api<DisplaySnapshot>("toggle_mute", { displayId });
    renderSnapshot(snapshot);
    showToast(snapshot.mute.muted ? "已静音" : "已取消静音");
  } catch (error) {
    ui.muteButton.setAttribute("aria-checked", String(previous === true));
    showError("无法切换静音", normalizeError(error));
  } finally {
    ui.audioCard.setAttribute("aria-busy", "false");
  }
}

function diagnosticText(report: DiagnosticReport): string {
  const lines = [
    `DisplayPilot ${report.appVersion} 诊断`,
    `生成时间: ${report.generatedAt}`,
    `系统: ${report.os} ${report.arch}`,
    "",
    `显示器: ${report.display.name}`,
    `稳定 ID: ${report.display.id}`,
    `后端/native: ${report.display.backend} / ${report.display.nativeId}`,
    `厂商/型号: ${report.display.vendorId ?? "—"} / ${
      report.display.modelId === null
        ? "—"
        : `0x${report.display.modelId.toString(16).toUpperCase()}`
    }`,
    `序列号: ${report.display.serialNumber ?? "—"}`,
    `配置档: ${report.display.profileId} (${report.display.profileLabel}) matched=${
      report.display.profileMatched
    }`,
    `当前输入: ${report.currentInput.label ?? "未知"} (raw=${
      report.currentInput.rawValue ?? "—"
    })`,
    `亮度: ${percentText(report.brightness.percent)} raw=${report.brightness.rawValue ?? "—"}`,
    `音量: ${percentText(report.volume.percent)} raw=${report.volume.rawValue ?? "—"}`,
    `静音: ${report.mute.muted === null ? "未知" : report.mute.muted ? "是" : "否"}`,
    "",
    "VCP:",
    ...report.vcpReads.map(
      (read) =>
        `  ${read.code} ${read.name}: ${
          read.ok ? `0x${(read.value ?? 0).toString(16).toUpperCase()} max=0x${(
            read.maximum ?? 0
          )
            .toString(16)
            .toUpperCase()}` : `读取失败 ${read.error ?? ""}`
        }`,
    ),
    "",
    "输入源:",
    ...report.display.inputs.map((input) => `  ${input.id} ${input.label} — ${input.selector}`),
    "",
    `能力字符串: ${report.capabilities ?? report.capabilitiesError ?? "不可用"}`,
    "",
    "备注:",
    ...report.notes.map((note) => `  • ${note}`),
  ];
  return lines.join("\n");
}

async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.append(area);
    area.select();
    const copied = document.execCommand("copy");
    area.remove();
    return copied;
  }
}

async function copyDiagnostics(): Promise<void> {
  if (!state.selectedId) {
    showToast("没有可诊断的显示器");
    return;
  }
  ui.diagnose.disabled = true;
  try {
    const report = await api<DiagnosticReport>("diagnostics", {
      displayId: state.selectedId,
    });
    const copied = await copyText(diagnosticText(report));
    showToast(copied ? "诊断信息已复制" : "复制失败，请检查剪贴板权限");
  } catch (error) {
    const appError = normalizeError(error);
    await copyText(`${appError.message}\n${appError.hint ?? ""}`);
    showToast("诊断读取失败，已复制错误信息");
  } finally {
    ui.diagnose.disabled = false;
  }
}

function scheduleBrightness(value: number): void {
  ui.brightnessValue.textContent = `${value}%`;
  setFill(ui.brightnessSlider, value);
  window.clearTimeout(brightnessTimer);
  brightnessTimer = window.setTimeout(() => {
    void commitBrightness(value);
  }, 130);
}

function scheduleVolume(value: number): void {
  ui.volumeValue.textContent = `${value}%`;
  setFill(ui.volumeSlider, value);
  window.clearTimeout(volumeTimer);
  volumeTimer = window.setTimeout(() => {
    void commitVolume(value);
  }, 130);
}

ui.brightnessSlider.addEventListener("input", () => {
  state.brightnessDragging = true;
  scheduleBrightness(Number(ui.brightnessSlider.value));
});

ui.brightnessSlider.addEventListener("change", () => {
  state.brightnessDragging = false;
  window.clearTimeout(brightnessTimer);
  const value = Number(ui.brightnessSlider.value);
  void commitBrightness(value);
});

ui.volumeSlider.addEventListener("input", () => {
  state.volumeDragging = true;
  scheduleVolume(Number(ui.volumeSlider.value));
});

ui.volumeSlider.addEventListener("change", () => {
  state.volumeDragging = false;
  window.clearTimeout(volumeTimer);
  const value = Number(ui.volumeSlider.value);
  void commitVolume(value);
});

for (const button of ui.brightnessPresets.querySelectorAll<HTMLButtonElement>("button")) {
  button.addEventListener("click", () => {
    const value = Number(button.dataset.brightness ?? 0);
    state.brightnessDragging = false;
    ui.brightnessSlider.value = String(value);
    scheduleBrightness(value);
  });
}

ui.muteButton.addEventListener("click", () => {
  void toggleMute();
});

ui.displayButton.addEventListener("click", () => {
  toggleDisplayMenu();
});

ui.displayMenu.addEventListener("keydown", (event) => {
  const options = Array.from(
    ui.displayMenu.querySelectorAll<HTMLButtonElement>(".display-option"),
  );
  const current = options.indexOf(document.activeElement as HTMLButtonElement);
  if (event.key === "ArrowDown") {
    event.preventDefault();
    options[(current + 1) % options.length]?.focus();
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    options[(current - 1 + options.length) % options.length]?.focus();
  }
});

document.addEventListener("pointerdown", (event) => {
  if (!ui.displayMenu.hidden && !ui.displaySwitcher.contains(event.target as Node)) {
    closeDisplayMenu();
  }
});

document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") {
    return;
  }
  if (!ui.displayMenu.hidden) {
    closeDisplayMenu();
    ui.displayButton.focus();
    event.preventDefault();
    return;
  }
  if (!ui.errorPanel.hidden) {
    clearError();
    event.preventDefault();
    return;
  }
  if (isTauri) {
    void api("hide_panel");
  }
});

ui.refresh.addEventListener("click", () => {
  void refreshAll(true);
});

ui.settings.addEventListener("click", () => {
  void api("open_settings").catch((error) => {
    showError("无法打开设置", normalizeError(error));
  });
});

ui.configOpen.addEventListener("click", () => {
  void api("open_config_dir").catch((error) => {
    showError("无法打开配置目录", normalizeError(error));
  });
});

ui.errorClose.addEventListener("click", () => {
  clearError();
});

ui.emptyRetry.addEventListener("click", () => {
  ui.emptyRetry.disabled = true;
  void refreshAll(true).finally(() => {
    ui.emptyRetry.disabled = false;
  });
});

ui.diagnose.addEventListener("click", () => {
  void copyDiagnostics();
});

window.addEventListener("focus", () => {
  if (state.snapshot && Date.now() - state.lastProbeAt > 4000) {
    void refreshSnapshot(true);
  }
});

document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible" && Date.now() - state.lastProbeAt > 4000) {
    void refreshSnapshot(true);
  }
});

async function bindEvents(): Promise<void> {
  try {
    await onEvent<DisplaySnapshot>("snapshot-updated", (snapshot) => {
      if (snapshot.display.id === state.selectedId) {
        renderSnapshot(snapshot);
      }
    });
    await onEvent<AppError>("operation-failed", (error) => {
      showError("快捷键操作未完成", normalizeError(error));
    });
    await onEvent("refresh-requested", () => {
      void refreshAll(true);
    });
  } catch {
    // Event wiring is optional; the panel still works through commands.
  }
}

void bindEvents();
void load();
