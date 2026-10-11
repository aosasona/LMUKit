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
  logicalAction: string;
};
export type VirtualRoutingStatus = {
  active: boolean;
  profileId: string | null;
  profileName: string | null;
  identity: string | null;
  routedEventCount: number;
  lastEvent: string | null;
  error: string | null;
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
  companionApps: [],
  lmuLaunchMode: "desktop",
};
