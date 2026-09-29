import { Gamepad2, Search, SlidersHorizontal, Trash2 } from "lucide-react";
import { useState } from "react";
import type { Binding, Profile } from "../models";

export function BindingEditorPage({
  profile,
  bindings,
  busy,
  onClear,
}: {
  profile?: Profile;
  bindings: Binding[];
  busy: boolean;
  onClear: (profile: Profile, binding: Binding) => void;
}) {
  const [query, setQuery] = useState("");
  const filtered = bindings.filter((binding) =>
    `${binding.action} ${binding.deviceName} ${controlLabel(binding.inputId)}`
      .toLowerCase()
      .includes(query.toLowerCase()),
  );
  const groups = filtered.reduce<Map<string, Binding[]>>((result, binding) => {
    const entries = result.get(binding.deviceName) ?? [];
    entries.push(binding);
    result.set(binding.deviceName, entries);
    return result;
  }, new Map());
  if (!profile)
    return (
      <section className="empty large">
        <SlidersHorizontal />
        <strong>Select a profile first</strong>
        <span>The binding editor works on your selected saved profile.</span>
      </section>
    );
  return (
    <section className="binding-editor">
      <div className="binding-toolbar">
        <div>
          <span className="eyebrow">Editing profile</span>
          <h2>{profile.name}</h2>
        </div>
        <div className="search-box">
          <Search />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search actions or devices"
          />
        </div>
      </div>
      <div className="binding-summary">
        <span>{bindings.length} mapped controls</span>
        <span>{groups.size} devices shown</span>
        <span>Every change creates a recovery backup</span>
      </div>
      <div className="binding-groups">
        {Array.from(groups.entries()).map(([device, entries]) => (
          <section className="binding-device" key={device}>
            <header>
              <div className="device-thumb">
                <Gamepad2 />
              </div>
              <div>
                <span className="eyebrow">Input device</span>
                <h3>{device}</h3>
              </div>
              <em>{entries.length} bindings</em>
            </header>
            <div className="binding-list">
              {entries.map((binding) => (
                <div
                  className="binding-row"
                  key={`${binding.action}-${binding.alternate}`}
                >
                  <div>
                    <strong>{binding.action}</strong>
                    {binding.alternate && <small>Alternate binding</small>}
                  </div>
                  <span className="control-pill">
                    {controlLabel(binding.inputId)}
                  </span>
                  <code>ID {binding.inputId}</code>
                  <button
                    disabled={busy}
                    aria-label={`Clear ${binding.action}`}
                    onClick={() => onClear(profile, binding)}
                  >
                    <Trash2 /> Clear
                  </button>
                </div>
              ))}
            </div>
          </section>
        ))}
      </div>
      {!filtered.length && (
        <div className="empty large">
          <Search />
          <strong>No matching bindings</strong>
          <span>Try another action or device name.</span>
        </div>
      )}
    </section>
  );
}

function controlLabel(inputId: number) {
  if (inputId >= 32) return `Button ${inputId - 31}`;
  if (inputId >= 16) return `POV direction ${inputId - 15}`;
  return `Axis ${Math.floor(inputId / 2) + 1} ${inputId % 2 === 0 ? "+" : "−"}`;
}
