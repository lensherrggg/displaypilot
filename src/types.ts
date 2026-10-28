export interface AppError {
  code: string;
  message: string;
  hint?: string | null;
}

export interface InputOption {
  id: string;
  label: string;
  supported: boolean;
  transport: string;
  reason: string | null;
  hint: string | null;
}

export interface DisplaySummary {
  id: string;
  name: string;
  profileId: string;
  profileLabel: string;
  profileMatched: boolean;
  backend: string;
  vendorId: string | null;
  modelId: number | null;
  modelName: string | null;
  serialNumber: string | null;
  inputs: InputOption[];
  inputSwitchSupported: boolean;
  currentInputVcpCode: number;
  rawI2cAvailable: boolean;
  rawI2cBackend: string | null;
  rawI2cReason: string | null;
}

export interface DisplayList {
  displays: DisplaySummary[];
  discoveryError: AppError | null;
  configError: string | null;
  configWarnings: string[];
}

export interface CurrentInputState {
  inputId: string | null;
  label: string | null;
  rawValue: number | null;
  transport: string | null;
  readError: string | null;
}

export interface FeatureValueState {
  supported: boolean;
  editable: boolean;
  percent: number | null;
  rawValue: number | null;
  min: number;
  max: number;
  step: number;
  readError: string | null;
  hint: string | null;
}

export interface MuteValueState {
  supported: boolean;
  editable: boolean;
  muted: boolean | null;
  rawValue: number | null;
  readError: string | null;
  hint: string | null;
}

export interface DisplaySnapshot {
  display: DisplaySummary;
  currentInput: CurrentInputState;
  brightness: FeatureValueState;
  volume: FeatureValueState;
  mute: MuteValueState;
  probedAt: string;
}

export interface GeneralConfig {
  brightnessStep: number;
  volumeStep: number;
  autoHide: boolean;
  showUnsupportedAudio: boolean;
}

export interface ShortcutBinding {
  action: string;
  label: string;
  accelerator: string;
}

export interface ShortcutStatus extends ShortcutBinding {
  registered: boolean;
  error: string | null;
}

export interface ProfileSummary {
  id: string;
  label: string;
  priority: number;
  inputs: InputOption[];
  matchedDisplayIds: string[];
}

export interface ConfigInfo {
  version: string;
  os: string;
  arch: string;
  configPath: string;
  configDir: string;
  logPath: string;
  configError: string | null;
  configWarnings: string[];
  general: GeneralConfig;
  profiles: ProfileSummary[];
  shortcuts: ShortcutBinding[];
}

export interface AppInfo {
  version: string;
  os: string;
  arch: string;
  configPath: string;
  logPath: string;
}

export interface BootstrapPayload {
  app: AppInfo;
  config: ConfigInfo;
  displays: DisplayList;
}

export interface DiagnosticInput {
  id: string;
  label: string;
  selector: string;
  supported: boolean;
  transport: string;
  reason: string | null;
  hint: string | null;
}

export interface DiagnosticDisplay {
  id: string;
  name: string;
  backend: string;
  nativeId: string;
  vendorId: string | null;
  modelId: number | null;
  modelName: string | null;
  serialNumber: string | null;
  profileId: string;
  profileLabel: string;
  profileMatched: boolean;
  capabilitiesError: string | null;
  rawI2cAvailable: boolean;
  rawI2cBackend: string | null;
  rawI2cReason: string | null;
  inputs: DiagnosticInput[];
}

export interface VcpRead {
  code: string;
  name: string;
  ok: boolean;
  value: number | null;
  maximum: number | null;
  ty: number | null;
  sh: number | null;
  sl: number | null;
  error: string | null;
}

export interface DiagnosticReport {
  generatedAt: string;
  appVersion: string;
  os: string;
  arch: string;
  display: DiagnosticDisplay;
  currentInput: CurrentInputState;
  brightness: FeatureValueState;
  volume: FeatureValueState;
  mute: MuteValueState;
  capabilities: string | null;
  capabilitiesError: string | null;
  vcpReads: VcpRead[];
  notes: string[];
}

export interface Ack {
  ok: boolean;
}
