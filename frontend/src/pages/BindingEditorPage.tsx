import { invoke } from "@tauri-apps/api/core";
import {
  Crosshair,
  Gamepad2,
  Plus,
  Search,
  SlidersHorizontal,
  X,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type {
  Binding,
  BindingAssignmentCandidate,
  BindingLookup,
  Profile,
} from "../models";
import { ActionMenu } from "../components/ActionMenu";
import { ConfirmDialog, type Confirmation } from "../components/ConfirmDialog";
import { AddBindingDialog } from "../components/AddBindingDialog";

export function BindingEditorPage({
  profile,
  bindings,
  busy,
  onClear,
  onAssign,
}: {
  profile?: Profile;
  bindings: Binding[];
  busy: boolean;
  onClear: (profile: Profile, binding: Binding) => void;
  onAssign: (
    profile: Profile,
    action: string,
    alternate: boolean,
    candidate: BindingAssignmentCandidate,
  ) => Promise<void>;
}) {
  const [query, setQuery] = useState("");
  const [lookup, setLookup] = useState<BindingLookup | null>(null);
  const [lookupError, setLookupError] = useState<string | null>(null);
  const [listening, setListening] = useState(false);
  const [assignmentTarget, setAssignmentTarget] = useState<{
    action: string;
    alternate: boolean;
  } | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [showAddBinding, setShowAddBinding] = useState(false);
  const [actionCatalogue, setActionCatalogue] = useState<string[]>([]);
  const cancelRequested = useRef(false);

  useEffect(() => {
    if (!profile) return;
    void invoke<string[]>("binding_action_catalogue")
      .then(setActionCatalogue)
      .catch((error) => setLookupError(String(error)));
  }, [profile?.id]);

  async function findControl() {
    if (listening || busy) return;
    setListening(true);
    setLookup(null);
    setLookupError(null);
    cancelRequested.current = false;
    try {
      setLookup(await invoke<BindingLookup>("find_binding_matches"));
    } catch (error) {
      if (!cancelRequested.current) setLookupError(String(error));
    } finally {
      setListening(false);
    }
  }

  async function listenForAssignment(action: string, alternate: boolean) {
    if (!profile || listening || busy) return;
    const target = { action, alternate };
    setListening(true);
    setAssignmentTarget(target);
    setLookup(null);
    setLookupError(null);
    cancelRequested.current = false;
    try {
      const candidate = await invoke<BindingAssignmentCandidate>(
        "listen_for_profile_binding",
        { profileId: profile.id, action, alternate },
      );
      if (candidate.conflicts.length) {
        const conflicts = candidate.conflicts
          .map(
            (conflict) =>
              `${conflict.action}${conflict.alternate ? " (alternate)" : ""}`,
          )
          .join(", ");
        setConfirmation({
          title: `Reuse ${candidate.control}?`,
          description: `${candidate.control} on ${candidate.deviceName} is already assigned to ${conflicts}. Assigning it to ${action} will keep those existing mappings.`,
          confirmLabel: "Assign anyway",
          onConfirm: () => {
            setConfirmation(null);
            void onAssign(profile, action, alternate, candidate);
          },
        });
      } else {
        await onAssign(profile, action, alternate, candidate);
      }
    } catch (error) {
      if (!cancelRequested.current) setLookupError(String(error));
    } finally {
      setListening(false);
      setAssignmentTarget(null);
    }
  }

  async function cancelLookup() {
    cancelRequested.current = true;
    await invoke("cancel_binding_lookup");
  }
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
          <span className="eyebrow">Control mappings</span>
          <h2>Bindings</h2>
        </div>
        <div className="binding-toolbar-actions">
          <button
            className="secondary"
            disabled={busy || listening}
            onClick={() => setShowAddBinding(true)}
          >
            <Plus /> Add binding
          </button>
          {listening ? (
            <button
              className="secondary cancel-listening"
              onClick={() => void cancelLookup()}
            >
              <X /> Cancel lookup
            </button>
          ) : (
            <button
              className="primary"
              disabled={busy}
              onClick={() => void findControl()}
            >
              <Crosshair /> Find a wheel control
            </button>
          )}
          <div className="search-box">
            <Search />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search actions or devices"
            />
          </div>
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
              {entries.map((binding) => {
                const hasCounterpart = bindings.some(
                  (entry) =>
                    entry.action === binding.action &&
                    entry.alternate !== binding.alternate,
                );
                return (
                  <div
                    className="binding-row"
                    key={`${binding.action}-${binding.alternate}`}
                  >
                    <div>
                      <strong>{binding.action}</strong>
                      <small>
                        {binding.alternate ? "Alternate" : "Primary"} binding
                      </small>
                    </div>
                    <span className="control-pill">
                      {controlLabel(binding.inputId)}
                    </span>
                    <code>ID {binding.inputId}</code>
                    <ActionMenu
                      label={`Actions for ${binding.action}`}
                      items={[
                        {
                          label: `Replace ${binding.alternate ? "alternate" : "primary"}`,
                          onSelect: () =>
                            void listenForAssignment(
                              binding.action,
                              binding.alternate,
                            ),
                        },
                        {
                          label: binding.alternate
                            ? "Add primary binding"
                            : "Add alternate binding",
                          hidden: hasCounterpart,
                          onSelect: () =>
                            void listenForAssignment(
                              binding.action,
                              !binding.alternate,
                            ),
                        },
                        {
                          label: "Clear binding",
                          danger: true,
                          onSelect: () =>
                            setConfirmation({
                              title: `Clear ${binding.action}?`,
                              description: `This removes the ${binding.alternate ? "alternate" : "primary"} binding from ${profile.name}. A recovery backup will be created.`,
                              confirmLabel: "Clear binding",
                              danger: true,
                              onConfirm: () => {
                                onClear(profile, binding);
                                setConfirmation(null);
                              },
                            }),
                        },
                      ]}
                    />
                  </div>
                );
              })}
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
      {(listening || lookup || lookupError) && (
        <aside className="lookup-dock">
          <div className="lookup-control">
            <Crosshair />
            <div>
              <span>
                {listening
                  ? assignmentTarget
                    ? "Choose a new binding"
                    : "Listening for input"
                  : lookupError
                    ? "Input operation failed"
                    : "Detected control"}
              </span>
              <strong>
                {listening
                  ? assignmentTarget
                    ? `${assignmentTarget.alternate ? "Alternate" : "Primary"} ${assignmentTarget.action} · press, turn, or move a control`
                    : "Press a button, move a POV, or turn an axis"
                  : (lookupError ??
                    `${lookup?.control} · ID ${lookup?.inputId}`)}
              </strong>
            </div>
          </div>
          {lookup && (
            <div className="lookup-matches">
              {lookup.matches.length ? (
                lookup.matches.map((match, index) => (
                  <div key={`${match.profileName}-${match.action}-${index}`}>
                    <strong>{match.profileName}</strong>
                    <span>
                      {match.action}
                      {match.alternate ? " · alternate" : ""}
                    </span>
                  </div>
                ))
              ) : (
                <span className="no-match">
                  This control is not bound in any saved profile.
                </span>
              )}
            </div>
          )}
        </aside>
      )}
      <ConfirmDialog
        confirmation={confirmation}
        busy={busy}
        onClose={() => setConfirmation(null)}
      />
      {showAddBinding && (
        <AddBindingDialog
          actions={actionCatalogue}
          onClose={() => setShowAddBinding(false)}
          onChoose={(action, alternate) => {
            setShowAddBinding(false);
            void listenForAssignment(action, alternate);
          }}
        />
      )}
    </section>
  );
}

function controlLabel(inputId: number) {
  if (inputId >= 32) return `Button ${inputId - 31}`;
  if (inputId >= 16) return `POV direction ${inputId - 15}`;
  return `Axis ${Math.floor(inputId / 2) + 1} ${inputId % 2 === 0 ? "+" : "−"}`;
}
