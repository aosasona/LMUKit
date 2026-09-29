import { Gauge, Search } from "lucide-react";
import { useMemo, useState, type CSSProperties } from "react";
import type { JsonObject, JsonPrimitive, JsonValue } from "../models";

type FfbSetting = {
  path: string[];
  label: string;
  group: string;
  value: number | boolean;
};

export function ForceFeedbackEditor({
  document,
  onChange,
}: {
  document: JsonObject;
  onChange: (document: JsonObject) => void;
}) {
  const [query, setQuery] = useState("");
  const settings = useMemo(() => collectFfbSettings(document), [document]);
  const filtered = settings.filter((setting) =>
    `${setting.label} ${setting.group}`
      .toLowerCase()
      .includes(query.trim().toLowerCase()),
  );
  const groups = filtered.reduce((result, setting) => {
    const entries = result.get(setting.group) ?? [];
    entries.push(setting);
    result.set(setting.group, entries);
    return result;
  }, new Map<string, FfbSetting[]>());

  return (
    <section className="ffb-editor">
      <div className="ffb-intro">
        <div>
          <span className="eyebrow">Profile tuning</span>
          <h2>Force feedback</h2>
          <p>
            Settings are grouped by device. Change only the wheel base that
            produces force feedback.
          </p>
        </div>
        <div className="search-box">
          <Search />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search FFB settings"
          />
        </div>
      </div>
      <div className="ffb-groups">
        {[...groups.entries()].map(([group, entries]) => (
          <section className="settings-group panel" key={group}>
            <header>
              <Gauge />
              <div>
                <span className="eyebrow">Input device</span>
                <h3>{group}</h3>
              </div>
              <em>{entries.length} options</em>
            </header>
            <div>
              {entries.map((setting) => (
                <FfbControl
                  setting={setting}
                  key={setting.path.join(".")}
                  onChange={(value) =>
                    onChange(setAtPath(document, setting.path, value))
                  }
                />
              ))}
            </div>
          </section>
        ))}
        {!filtered.length && (
          <div className="empty large">
            <Gauge />
            <strong>No recognised FFB settings found</strong>
            <span>
              Capture a current LMU wheel profile; LMUKit will not invent
              undocumented fields.
            </span>
          </div>
        )}
      </div>
    </section>
  );
}

function FfbControl({
  setting,
  onChange,
}: {
  setting: FfbSetting;
  onChange: (value: number | boolean) => void;
}) {
  const isGain = setting.label.toLowerCase() === "steering effects strength";
  const value =
    isGain && typeof setting.value === "number"
      ? setting.value / 100
      : setting.value;
  return (
    <label className="game-setting-row ffb-setting-row">
      <span>
        <strong>{isGain ? "FFB gain" : setting.label}</strong>
        {isGain && (
          <small>Steering effects strength · raw {setting.value}</small>
        )}
      </span>
      {typeof value === "boolean" ? (
        <input
          type="checkbox"
          checked={value}
          onChange={(event) => onChange(event.target.checked)}
        />
      ) : isGain ? (
        <span className="ffb-gain-control">
          <input
            type="range"
            min="0"
            max="100"
            step="1"
            value={value}
            style={{ "--range-progress": `${value}%` } as CSSProperties}
            onChange={(event) => onChange(Number(event.target.value) * 100)}
          />
          <strong>{Math.round(value)}%</strong>
        </span>
      ) : (
        <input
          type="number"
          step="any"
          value={value}
          onChange={(event) => {
            const next = Number(event.target.value);
            if (Number.isFinite(next)) onChange(next);
          }}
        />
      )}
    </label>
  );
}

function collectFfbSettings(document: JsonObject) {
  const settings: FfbSetting[] = [];
  const visit = (object: JsonObject, path: string[], inFfbSection: boolean) => {
    for (const [key, value] of Object.entries(object)) {
      if (key === "Input" || key === "Alternative Input") continue;
      const nextPath = [...path, key];
      const childIsFfb = inFfbSection || isFfbContainerName(key);
      if (isObject(value)) visit(value, nextPath, childIsFfb);
      else if (
        (typeof value === "number" || typeof value === "boolean") &&
        (childIsFfb || isFfbSettingName(key))
      ) {
        settings.push({
          path: nextPath,
          label: key,
          group: ffbGroupName(document, nextPath),
          value,
        });
      }
    }
  };
  visit(document, [], false);
  return settings.sort((left, right) =>
    `${left.group}\0${left.label}`.localeCompare(
      `${right.group}\0${right.label}`,
    ),
  );
}

function ffbGroupName(document: JsonObject, path: string[]) {
  if (path[0] === "Devices" && path.length >= 3) {
    const device = document.Devices as JsonObject | undefined;
    const metadata = device?.[path[1]] as JsonObject | undefined;
    const name = metadata?.name ?? metadata?.Name;
    if (typeof name === "string") return name;
    return path[1];
  }
  if (path[0] && isFfbContainerName(path[0])) return "Profile force feedback";
  return path[0] ?? "Profile";
}

function isFfbContainerName(name: string) {
  const normalized = name.toLowerCase();
  return normalized.includes("force feedback") || normalized === "ffb";
}

function isFfbSettingName(name: string) {
  const normalized = name.toLowerCase();
  return [
    "force feedback",
    "ffb",
    "steering effects strength",
    "steering torque",
    "minimum torque",
    "collision strength",
    "smoothing",
    "constant steering",
    "vendor ffb",
    "haptic",
    "vibrotactile",
    "rumble",
    "jolt",
    "steering resistance",
    "steering spring",
  ].some((term) => normalized.includes(term));
}

function setAtPath(document: JsonObject, path: string[], value: JsonPrimitive) {
  const next = structuredClone(document);
  let cursor = next;
  for (const key of path.slice(0, -1)) cursor = cursor[key] as JsonObject;
  cursor[path[path.length - 1]] = value;
  return next;
}

function isObject(value: JsonValue): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
