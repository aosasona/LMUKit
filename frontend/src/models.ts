export type Device = { key: string; name: string; bindingCount: number };

export type Profile = {
  id: string;
  name: string;
  hotkeySlot: number | null;
  bindingCount: number;
  devices: Device[];
  hasWheelImage: boolean;
  wheelTags: string[];
  wheelBrand: string | null;
  wheelName: string | null;
  classTags: string[];
  customTags: string[];
  differsFromLive: boolean | null;
  wheelId: string | null;
};

export type Wheel = {
  id: string;
  brand: string | null;
  name: string;
  hasImage: boolean;
  profileCount: number;
};

export type Binding = {
  action: string;
  deviceKey: string;
  deviceName: string;
  inputId: number;
  alternate: boolean;
};

export type BindingLookup = {
  control: string;
  inputId: number;
  matches: { profileName: string; action: string; alternate: boolean }[];
};

export type BindingAssignmentCandidate = {
  control: string;
  inputId: number;
  deviceKey: string;
  deviceName: string;
  conflicts: { action: string; alternate: boolean }[];
};

export type Snapshot = {
  profiles: Profile[];
  wheels: Wheel[];
  activeProfile: string | null;
  lmuConfigPath: string;
  lmuSettingsPath: string;
  activeProfileDirty: boolean | null;
  uiFontScale: number;
  inputMode: "legacy" | "virtual";
  autoStartVirtualProfile: boolean;
  companionApps: CompanionApp[];
  lmuLaunchMode: "desktop" | "vr";
};

export type CompanionApp = { name: string; path: string; enabled: boolean };
export type VirtualBindingDevice = {
  key: string;
  name: string;
  buttonBindingCount: number;
  likelyVirtual: boolean;
};
export type VirtualAction = { action: string; virtualButton: number };
export type VirtualProfileMapping = {
  deviceId: string;
  deviceName: string;
  physicalButton: number;
  modifiers: VirtualButtonModifier[];
  logicalAction: string;
};
export type VirtualButtonModifier = {
  deviceId: string;
  deviceName: string;
  physicalButton: number;
};
export type VirtualButtonCapture = {
  mapping: VirtualProfileMapping;
  conflictingAction: string | null;
};
export type VirtualControlInspection = {
  control: string;
  matches: { logicalAction: string; virtualButton: number }[];
};
export type VirtualMappingImport = {
  mappings: VirtualProfileMapping[];
  added: number;
  alreadyMapped: number;
  skippedWithoutVirtualAction: number;
  skippedNonButton: number;
  skippedUnknownDevice: number;
  conflictingControls: number;
};
export type VirtualProfileDefault = {
  classTag: string;
  profileId: string;
  profileName: string;
};
export type VirtualEncoderControl = {
  deviceId: string;
  deviceName: string;
  physicalButton: number;
};
export type VirtualEncoderMode = {
  name: string;
  clockwiseAction: string;
  counterclockwiseAction: string;
};
export type VirtualEncoderGroup = {
  name: string;
  clockwise: VirtualEncoderControl;
  counterclockwise: VirtualEncoderControl;
  cycle: VirtualEncoderControl | null;
  modes: VirtualEncoderMode[];
};
export type VirtualRoutingStatus = {
  active: boolean;
  profileId: string | null;
  profileName: string | null;
  identity: string | null;
  routedEventCount: number;
  lastEvent: string | null;
  error: string | null;
  selectionReason: string | null;
  activeEncoderModes: { encoderName: string; modeName: string }[];
};
export type LmuCarContextStatus = {
  connected: boolean;
  gameVersion: number | null;
  vehicleId: string | null;
  vehicleName: string | null;
  classRaw: string | null;
  classId: string | null;
  classLabel: string | null;
  sessionState:
    | "menu"
    | "loading"
    | "session"
    | "realtime"
    | "replay"
    | "unknown";
  source: "rest" | "shared_memory" | "manual" | "unknown";
  observedAt: string | null;
  status: "confirmed" | "stale" | "unknown";
  message: string;
};
export type AutoProfileSelectionStatus = {
  enabled: boolean;
  paused: boolean;
  candidateClass: string | null;
  confirmedObservations: number;
  requiredObservations: number;
  message: string;
};
export type VirtualSetupStatus = {
  identity: string;
  buttonCount: number;
  usedButtonCount: number;
  nextVirtualButton: number | null;
};
export type VirtualButtonPulse = {
  virtualButton: number;
  identity: string;
  nextVirtualButton: number | null;
};
export type VirtualSetupEvent = {
  pulse: VirtualButtonPulse | null;
  error: string | null;
};
export type DiagnosticDevice = {
  name: string;
  productName: string;
  buttonCount: number;
};
export type DiagnosticStatus = {
  appVersion: string;
  logDirectory: string;
  currentLogSize: number;
  retainedLogFiles: number;
  logRetentionDays: number;
  recentErrors: string[];
  activeProfile: string | null;
  inputMode: string;
  virtualHealth: string;
  routing: VirtualRoutingStatus;
  devices: DiagnosticDevice[];
};
export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonObject | JsonValue[];
export type JsonObject = { [key: string]: JsonValue };

export type Page =
  | "home"
  | "profiles"
  | "wheels"
  | "bindings"
  | "virtual"
  | "game"
  | "settings";

export const emptySnapshot: Snapshot = {
  profiles: [],
  wheels: [],
  activeProfile: null,
  lmuConfigPath: "",
  lmuSettingsPath: "",
  activeProfileDirty: null,
  uiFontScale: 1,
  inputMode: "legacy",
  autoStartVirtualProfile: false,
  companionApps: [],
  lmuLaunchMode: "desktop",
};

export const emptyCarContextStatus: LmuCarContextStatus = {
  connected: false,
  gameVersion: null,
  vehicleId: null,
  vehicleName: null,
  classRaw: null,
  classId: null,
  classLabel: null,
  sessionState: "unknown",
  source: "unknown",
  observedAt: null,
  status: "unknown",
  message: "Waiting for LMU shared memory.",
};

export const emptyAutoProfileSelectionStatus: AutoProfileSelectionStatus = {
  enabled: false,
  paused: false,
  candidateClass: null,
  confirmedObservations: 0,
  requiredObservations: 2,
  message: "Automatic profile selection is off.",
};
