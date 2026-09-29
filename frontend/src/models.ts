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

export type Snapshot = {
  profiles: Profile[];
  wheels: Wheel[];
  activeProfile: string | null;
  lmuConfigPath: string;
  lmuSettingsPath: string;
  activeProfileDirty: boolean | null;
  uiFontScale: number;
};

export type Page = "home" | "profiles" | "bindings" | "game" | "settings";

export const emptySnapshot: Snapshot = {
  profiles: [],
  wheels: [],
  activeProfile: null,
  lmuConfigPath: "",
  lmuSettingsPath: "",
  activeProfileDirty: null,
  uiFontScale: 1,
};
