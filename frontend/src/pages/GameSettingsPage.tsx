import {
  AlertTriangle,
  RefreshCw,
  Save,
  Search,
  SlidersHorizontal,
} from "lucide-react";
import { useMemo, useState } from "react";
import type { JsonObject, JsonPrimitive, JsonValue } from "../models";

type Entry = {
  path: string[];
  name: string;
  description: string | null;
  group: string;
  value: JsonPrimitive;
};

export function GameSettingsPage({
  document,
  baseline,
  loading,
  busy,
  onChange,
  onReload,
  onSave,
}: {
  document: JsonObject | null;
  baseline: JsonObject | null;
  loading: boolean;
  busy: boolean;
  onChange: (document: JsonObject) => void;
  onReload: () => void;
  onSave: () => void;
}) {
  const [query, setQuery] = useState("");
  const entries = useMemo(
    () => (document ? settingsEntries(document) : []),
    [document],
  );
  const filtered = entries.filter((entry) =>
    `${entry.name} ${entry.description ?? ""} ${entry.group}`
      .toLowerCase()
      .includes(query.trim().toLowerCase()),
  );
  const groups = filtered.reduce((grouped, entry) => {
    const group = grouped.get(entry.group) ?? [];
    group.push(entry);
    grouped.set(entry.group, group);
    return grouped;
  }, new Map<string, Entry[]>());
  const dirty = Boolean(
    document &&
      baseline &&
      JSON.stringify(document) !== JSON.stringify(baseline),
  );

  if (loading) {
    return (
      <section className="empty large">
        <RefreshCw className="spin" />
        <strong>Loading Settings.JSON…</strong>
      </section>
    );
  }
  if (!document) {
    return (
      <section className="empty large">
        <AlertTriangle />
        <strong>Settings.JSON could not be loaded</strong>
        <span>Confirm its path in Settings, then try again.</span>
        <button className="secondary" onClick={onReload}>
          Try again
        </button>
      </section>
    );
  }

  return (
    <section className="game-settings-page">
      <div className="game-settings-toolbar">
        <div className="search-box">
          <Search />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search names and descriptions"
          />
        </div>
        <div>
          <button
            className="secondary"
            disabled={busy}
            onClick={() => {
              if (
                !dirty ||
                window.confirm("Discard your unsaved game-setting changes?")
              )
                onReload();
            }}
          >
            <RefreshCw /> Reload
          </button>
          <button
            className="primary"
            disabled={busy || !dirty}
            onClick={onSave}
          >
            <Save /> Save changes
          </button>
        </div>
      </div>
      <div className="game-settings-warning">
        <AlertTriangle /> Close LMU before saving; the game may overwrite this
        file while running.
      </div>
      <div className="settings-groups">
        {[...groups.entries()].map(([group, groupEntries]) => (
          <section className="settings-group panel" key={group}>
            <header>
              <SlidersHorizontal />
              <div>
                <span className="eyebrow">Settings group</span>
                <h3>{group}</h3>
              </div>
              <em>{groupEntries.length} options</em>
            </header>
            <div>
              {groupEntries.map((entry) => (
                <SettingControl
                  entry={entry}
                  key={entry.path.join(".")}
                  onChange={(value) =>
                    onChange(setAtPath(document, entry.path, value))
                  }
                />
              ))}
            </div>
          </section>
        ))}
        {!filtered.length && (
          <div className="empty large">
            <Search />
            <strong>No settings match your search.</strong>
          </div>
        )}
      </div>
    </section>
  );
}

function SettingControl({
  entry,
  onChange,
}: {
  entry: Entry;
  onChange: (value: JsonPrimitive) => void;
}) {
  return (
    <label className="game-setting-row">
      <span>
        <strong>{entry.name}</strong>
        {entry.description && <small>{entry.description}</small>}
      </span>
      {typeof entry.value === "boolean" ? (
        <input
          type="checkbox"
          checked={entry.value}
          onChange={(event) => onChange(event.target.checked)}
        />
      ) : typeof entry.value === "number" ? (
        <input
          type="number"
          value={entry.value}
          onChange={(event) => {
            const value = Number(event.target.value);
            if (Number.isFinite(value)) onChange(value);
          }}
        />
      ) : (
        <input
          value={entry.value ?? ""}
          onChange={(event) => onChange(event.target.value)}
        />
      )}
    </label>
  );
}

function settingsEntries(document: JsonObject): Entry[] {
  const entries: Entry[] = [];
  const visit = (object: JsonObject, parentPath: string[]) => {
    for (const [name, value] of Object.entries(object)) {
      if (name.endsWith("#")) continue;
      const path = [...parentPath, name];
      if (isObject(value)) visit(value, path);
      else if (
        !Array.isArray(value) &&
        value !== null &&
        ["string", "number", "boolean"].includes(typeof value)
      ) {
        const description = object[`${name}#`];
        entries.push({
          path,
          name,
          description: typeof description === "string" ? description : null,
          group: parentPath.length ? parentPath.join(" › ") : "General",
          value: value as JsonPrimitive,
        });
      }
    }
  };
  visit(document, []);
  return entries;
}

function setAtPath(
  document: JsonObject,
  path: string[],
  value: JsonPrimitive,
): JsonObject {
  const next = structuredClone(document);
  let cursor: JsonObject = next;
  path.slice(0, -1).forEach((key) => {
    cursor = cursor[key] as JsonObject;
  });
  cursor[path[path.length - 1]] = value;
  return next;
}

function isObject(value: JsonValue): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
