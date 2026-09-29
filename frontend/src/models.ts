export type Device = { key: string; name: string; bindingCount: number };

export type Profile = {
  id: string;
  name: string;
  hotkeySlot: number | null;
  bindingCount: number;
  devices: Device[];
  hasWheelImage: boolean;
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
  activeProfile: string | null;
  lmuConfigPath: string;
  lmuSettingsPath: string;
  activeProfileDirty: boolean | null;
};

export type Page = "home" | "profiles" | "bindings" | "game" | "settings";

export const emptySnapshot: Snapshot = {
  profiles: [],
  activeProfile: null,
  lmuConfigPath: "",
  lmuSettingsPath: "",
  activeProfileDirty: null,
};
