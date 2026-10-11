import { invoke } from "@tauri-apps/api/core";
import {
  Crosshair,
  Gamepad2,
  LoaderCircle,
  Radio,
  Search,
  SlidersHorizontal,
  Square,
  Star,
  Trash2,
  WandSparkles,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { displayClassTag } from "../lib/classTags";
import type {
  Profile,
  VirtualAction,
  VirtualButtonCapture,
  VirtualControlInspection,
  VirtualMappingImport,
  VirtualProfileDefault,
  VirtualProfileMapping,
  VirtualRoutingStatus,
} from "../models";
import { ConfirmDialog, type Confirmation } from "./ConfirmDialog";
import { ProfileEncoderMappings } from "./ProfileEncoderMappings";
import type { InputMode } from "./InputModeSwitch";

function searchTokens(value: string) {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim()
    .split(/\s+/)
    .filter(Boolean);
}

function describeControl(mapping: VirtualProfileMapping) {
  return [
    ...mapping.modifiers.map(
      (modifier) =>
        `${modifier.deviceName || "Physical controller"} · Button ${modifier.physicalButton}`,
    ),
    `${mapping.deviceName || "Physical controller"} · Button ${mapping.physicalButton}`,
  ].join(" + ");
}

export function ProfileVirtualMappings({
  profile,
  busy,
  inputMode,
  onNotice,
  editorOpen,
  onEditorOpenChange,
}: {
  profile: Profile;
  busy: boolean;
  inputMode: InputMode;
  onNotice: (message: string) => void;
  editorOpen: boolean;
  onEditorOpenChange: (open: boolean) => void;
}) {
  const [actions, setActions] = useState<VirtualAction[]>([]);
  const [mappings, setMappings] = useState<VirtualProfileMapping[]>([]);
  const [loading, setLoading] = useState(true);
  const [migrating, setMigrating] = useState(false);
  const [defaultsBusy, setDefaultsBusy] = useState<string | null>(null);
  const [defaults, setDefaults] = useState<VirtualProfileDefault[]>([]);
  const [learning, setLearning] = useState<string | null>(null);
  const [savingMapping, setSavingMapping] = useState(false);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [testing, setTesting] = useState(false);
  const [inspection, setInspection] = useState<VirtualControlInspection | null>(
    null,
  );
  const [routingBusy, setRoutingBusy] = useState(false);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<"all" | "assigned" | "unassigned">(
    "all",
  );
  const [routing, setRouting] = useState<VirtualRoutingStatus>({
    active: false,
    profileId: null,
    profileName: null,
    identity: null,
    routedEventCount: 0,
    lastEvent: null,
    error: null,
    selectionReason: null,
    activeEncoderModes: [],
  });
  const [error, setError] = useState<string | null>(null);
  const routingActive = routing.active;
  const assignedActions = new Set(
    mappings.map((mapping) => mapping.logicalAction),
  );
  const queryTokens = searchTokens(query);
  const visibleActions = actions.filter((action) => {
    const assigned = assignedActions.has(action.action);
    const actionMappings = mappings.filter(
      (mapping) => mapping.logicalAction === action.action,
    );
    const candidateTokens = searchTokens(
      [
        action.action,
        `vjoy b${action.virtualButton}`,
        ...actionMappings.flatMap((mapping) => [
          mapping.deviceName,
          mapping.deviceId,
          `button ${mapping.physicalButton}`,
          `b${mapping.physicalButton}`,
          ...mapping.modifiers.flatMap((modifier) => [
            modifier.deviceName,
            modifier.deviceId,
            `modifier button ${modifier.physicalButton}`,
            `modifier b${modifier.physicalButton}`,
          ]),
        ]),
      ].join(" "),
    );
    return (
      queryTokens.every((queryToken) =>
        candidateTokens.some((candidate) => candidate.includes(queryToken)),
      ) &&
      (filter === "all" ||
        (filter === "assigned" && assigned) ||
        (filter === "unassigned" && !assigned))
    );
  });

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setInspection(null);
    setTesting(false);
    setConfirmation(null);
    void Promise.all([
      invoke<VirtualAction[]>("saved_virtual_layout"),
      invoke<VirtualProfileMapping[]>("virtual_profile_mapping", {
        profileId: profile.id,
      }),
      invoke<VirtualRoutingStatus>("virtual_routing_status"),
      invoke<VirtualProfileDefault[]>("virtual_profile_defaults"),
    ])
      .then(([nextActions, nextMappings, routing, nextDefaults]) => {
        if (cancelled) return;
        setActions(nextActions);
        setMappings(nextMappings);
        setRouting(routing);
        setDefaults(nextDefaults);
      })
      .catch((reason) => {
        if (!cancelled) setError(String(reason));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
      void invoke("cancel_binding_lookup");
    };
  }, [profile.id, profile.wheelId, profile.classTags.join(",")]);

  useEffect(() => {
    const refresh = () => {
      void invoke<VirtualRoutingStatus>("virtual_routing_status")
        .then(setRouting)
        .catch(() => undefined);
    };
    const timer = window.setInterval(refresh, 500);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (!editorOpen) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      if (confirmation) {
        if (!savingMapping) setConfirmation(null);
        return;
      }
      void closeEditor();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  });

  const saveCapturedMapping = async (
    capture: VirtualButtonCapture,
    replaceConflict: boolean,
  ) => {
    setSavingMapping(true);
    setError(null);
    try {
      const next = await invoke<VirtualProfileMapping[]>(
        "save_virtual_profile_button",
        {
          profileId: profile.id,
          proposed: capture.mapping,
          replaceConflict,
        },
      );
      setMappings(next);
      setInspection(null);
      setConfirmation(null);
      onNotice(
        `${capture.mapping.logicalAction} mapped to ${describeControl(capture.mapping)}.`,
      );
    } catch (reason) {
      const message = String(reason);
      setError(message);
      onNotice(message);
    } finally {
      setSavingMapping(false);
    }
  };

  const learn = async (action: string) => {
    if (routingActive) return;
    setLearning(action);
    setError(null);
    onNotice(
      `Press and release one button for ${action}, or hold a modifier and press another button.`,
    );
    try {
      const capture = await invoke<VirtualButtonCapture>(
        "capture_virtual_profile_button",
        { profileId: profile.id, logicalAction: action },
      );
      if (capture.conflictingAction) {
        const control = describeControl(capture.mapping);
        setConfirmation({
          title: "Replace this button mapping?",
          description: `${control} currently triggers “${capture.conflictingAction}”. Replace it with “${action}”?`,
          confirmLabel: "Replace mapping",
          onConfirm: () => void saveCapturedMapping(capture, true),
        });
        onNotice(
          `${control} is already mapped to ${capture.conflictingAction}. Confirm before replacing it.`,
        );
      } else {
        await saveCapturedMapping(capture, false);
      }
    } catch (reason) {
      const message = String(reason);
      if (!message.toLowerCase().includes("cancelled")) {
        setError(message);
        onNotice(message);
      }
    } finally {
      setLearning(null);
    }
  };

  const cancelLearning = async () => {
    await invoke("cancel_binding_lookup");
    setLearning(null);
    setTesting(false);
    onNotice("Control capture cancelled.");
  };

  const testControl = async () => {
    if (routingActive) return;
    setTesting(true);
    setInspection(null);
    setError(null);
    setQuery("");
    setFilter("all");
    onNotice(
      "Press and release a control, or hold a modifier and press its control.",
    );
    try {
      const result = await invoke<VirtualControlInspection>(
        "inspect_virtual_profile_button",
        { profileId: profile.id },
      );
      setInspection(result);
      onNotice(
        result.matches.length
          ? `${result.control} maps to ${result.matches.map((match) => match.logicalAction).join(", ")}.`
          : `${result.control} is not mapped in this profile.`,
      );
    } catch (reason) {
      const message = String(reason);
      if (!message.toLowerCase().includes("cancelled")) {
        setError(message);
        onNotice(message);
      }
    } finally {
      setTesting(false);
    }
  };

  const closeEditor = async () => {
    if (learning || testing) await invoke("cancel_binding_lookup");
    setLearning(null);
    setTesting(false);
    setConfirmation(null);
    onEditorOpenChange(false);
  };

  const remove = async (mapping: VirtualProfileMapping) => {
    if (routingActive) return;
    try {
      setMappings(
        await invoke<VirtualProfileMapping[]>("remove_virtual_profile_button", {
          profileId: profile.id,
          deviceId: mapping.deviceId,
          physicalButton: mapping.physicalButton,
          modifiers: mapping.modifiers,
        }),
      );
      onNotice("Physical button mapping removed.");
    } catch (reason) {
      setError(String(reason));
      onNotice(String(reason));
    }
  };

  const migrate = async () => {
    if (routingActive) return;
    setMigrating(true);
    setError(null);
    try {
      const report = await invoke<VirtualMappingImport>(
        "import_profile_virtual_mapping",
        { profileId: profile.id },
      );
      setMappings(report.mappings);
      const warnings = [
        report.conflictingControls
          ? `${report.conflictingControls} conflicting control${report.conflictingControls === 1 ? "" : "s"} need manual assignment`
          : "",
        report.skippedUnknownDevice
          ? `${report.skippedUnknownDevice} binding${report.skippedUnknownDevice === 1 ? " has" : "s have"} no usable device ID`
          : "",
      ].filter(Boolean);
      onNotice(
        report.added
          ? `Reused ${report.added} existing LMU button${report.added === 1 ? "" : "s"}.${warnings.length ? ` ${warnings.join("; ")}.` : ""}`
          : warnings.length
            ? `No buttons were added. ${warnings.join("; ")}.`
            : "This profile's matching LMU buttons are already assigned.",
      );
    } catch (reason) {
      setError(String(reason));
      onNotice(String(reason));
    } finally {
      setMigrating(false);
    }
  };

  const changeDefault = async (classTag: string | null, remove: boolean) => {
    if (!profile.wheelId) return;
    const scope = classTag ?? "fallback";
    setDefaultsBusy(scope);
    setError(null);
    try {
      const next = await invoke<VirtualProfileDefault[]>(
        remove
          ? "clear_virtual_profile_default"
          : "set_virtual_profile_default",
        remove
          ? { wheelId: profile.wheelId, classTag }
          : { profileId: profile.id, classTag },
      );
      setDefaults(next);
      onNotice(
        remove
          ? `${classTag ? displayClassTag(classTag) : "Wheel fallback"} default removed.`
          : `${profile.name} is now the ${classTag ? displayClassTag(classTag) : "wheel fallback"} default.`,
      );
    } catch (reason) {
      setError(String(reason));
      onNotice(String(reason));
    } finally {
      setDefaultsBusy(null);
    }
  };

  const startRouting = async () => {
    if (!mappings.length) return;
    setRoutingBusy(true);
    setError(null);
    try {
      const next = await invoke<VirtualRoutingStatus>("start_virtual_routing", {
        profileId: profile.id,
      });
      setRouting(next);
      onNotice(`Wheel remapping enabled for ${profile.name}.`);
    } catch (reason) {
      setError(String(reason));
      onNotice(String(reason));
    } finally {
      setRoutingBusy(false);
    }
  };

  const stopRouting = async () => {
    setRoutingBusy(true);
    try {
      const next = await invoke<VirtualRoutingStatus>("stop_virtual_routing");
      setRouting(next);
      onNotice("Wheel remapping disabled and all buttons released.");
    } catch (reason) {
      setError(String(reason));
      onNotice(String(reason));
    } finally {
      setRoutingBusy(false);
    }
  };

  return (
    <section className="profile-virtual-mappings">
      <header>
        <div>
          <span className="eyebrow">Virtual input</span>
          <h4>Wheel button mappings</h4>
          <p>
            {loading
              ? "Loading this profile's mappings…"
              : `${mappings.length} physical mapping${mappings.length === 1 ? "" : "s"} across ${assignedActions.size} of ${actions.length} LMU actions.`}
          </p>
        </div>
        <div className="profile-virtual-mapping-actions">
          <span>{mappings.length} assigned</span>
          <ProfileEncoderMappings
            profile={profile}
            busy={busy}
            routingActive={routingActive}
            onNotice={onNotice}
          />
          <button
            type="button"
            className="secondary compact"
            disabled={busy || loading || !actions.length}
            onClick={() => onEditorOpenChange(true)}
          >
            <SlidersHorizontal /> Edit button mappings
          </button>
        </div>
      </header>

      {error && <div className="virtual-error">{error}</div>}
      {!loading && !actions.length && (
        <div className="profile-mapping-empty">
          <Gamepad2 />
          <span>
            <strong>No virtual actions configured</strong>
            <small>
              Complete the one-time vJoy setup under Virtual Input first.
            </small>
          </span>
        </div>
      )}
      <div className="profile-defaults">
        <div className="profile-defaults-heading">
          <span>
            <Star />
            <span>
              <strong>Default for this wheel</strong>
              <small>
                Choose which profile should be used for each car class.
              </small>
            </span>
          </span>
          {!profile.wheelId && <em>Assign a wheel first</em>}
        </div>
        {profile.wheelId && (
          <div className="profile-default-scopes">
            {[...profile.classTags, null].map((classTag) => {
              const assigned = defaults.find(
                (entry) =>
                  entry.wheelId === profile.wheelId &&
                  (classTag === null
                    ? entry.classTag === null
                    : entry.classTag?.toLowerCase() === classTag.toLowerCase()),
              );
              const isThisProfile = assigned?.profileId === profile.id;
              const scope = classTag ?? "fallback";
              return (
                <div className="profile-default-scope" key={scope}>
                  <span>
                    <strong>
                      {classTag ? displayClassTag(classTag) : "Other classes"}
                    </strong>
                    <small>
                      {assigned
                        ? assigned.profileName
                        : classTag
                          ? "No default selected"
                          : "No wheel fallback selected"}
                    </small>
                  </span>
                  <button
                    type="button"
                    className={
                      isThisProfile ? "ghost compact" : "secondary compact"
                    }
                    disabled={busy || defaultsBusy !== null}
                    onClick={() => void changeDefault(classTag, isThisProfile)}
                  >
                    {defaultsBusy === scope
                      ? "Saving…"
                      : isThisProfile
                        ? "Remove default"
                        : "Make default"}
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </div>
      <div className="profile-remapping-bar">
        <span>
          <i className={routing.active ? "active" : ""} />
          <span>
            <strong>
              {routing.active
                ? routing.profileId === profile.id
                  ? "Live remapping is on"
                  : `${routing.profileName ?? "Another profile"} is remapping`
                : "Live remapping is off"}
            </strong>
            <small>
              {routing.profileId === profile.id && routing.lastEvent
                ? routing.lastEvent
                : `${mappings.length} physical mapping${mappings.length === 1 ? "" : "s"}`}
            </small>
          </span>
        </span>
        {routing.active && routing.profileId === profile.id ? (
          <button
            type="button"
            className="secondary compact"
            disabled={routingBusy}
            onClick={() => void stopRouting()}
          >
            <Square /> Disable remapping
          </button>
        ) : (
          <button
            type="button"
            className="primary compact"
            disabled={
              busy || routingBusy || !mappings.length || inputMode !== "virtual"
            }
            onClick={() => void startRouting()}
          >
            {routingBusy ? <LoaderCircle className="spin" /> : <Gamepad2 />}
            {inputMode !== "virtual"
              ? "Select Virtual Input above"
              : routing.active
                ? "Switch to this profile"
                : "Use this profile live"}
          </button>
        )}
      </div>
      {routing.profileId === profile.id &&
        !!routing.activeEncoderModes.length && (
          <div className="active-encoder-modes profile-active-encoder-modes">
            {routing.activeEncoderModes.map((mode) => (
              <span key={mode.encoderName}>
                {mode.encoderName} <strong>{mode.modeName}</strong>
              </span>
            ))}
          </div>
        )}

      {editorOpen && (
        <div
          className="dialog-backdrop profile-mapping-backdrop"
          role="presentation"
          onPointerDown={(event) => {
            if (event.target === event.currentTarget) void closeEditor();
          }}
        >
          <section
            className="profile-mapping-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="profile-mapping-title"
            onPointerDown={(event) => event.stopPropagation()}
          >
            <header>
              <div>
                <span className="eyebrow">{profile.name}</span>
                <h2 id="profile-mapping-title">Wheel button mappings</h2>
                <p>
                  Pick an LMU action, then press the physical control that
                  should trigger it for this profile.
                </p>
              </div>
              <button
                type="button"
                className="dialog-close"
                aria-label="Close mapping editor"
                onClick={() => void closeEditor()}
              >
                <X />
              </button>
            </header>

            {routingActive && (
              <div className="profile-mapping-notice">
                Disable live remapping before changing button assignments.
              </div>
            )}
            {error && <div className="virtual-error">{error}</div>}

            <div className="profile-mapping-toolbar">
              <label className="profile-mapping-search">
                <Search />
                <input
                  autoFocus
                  value={query}
                  placeholder="Search actions, buttons, or devices"
                  onChange={(event) => setQuery(event.target.value)}
                />
              </label>
              <div className="profile-mapping-filters">
                {(["all", "unassigned", "assigned"] as const).map((option) => (
                  <button
                    type="button"
                    className={filter === option ? "active" : ""}
                    key={option}
                    onClick={() => setFilter(option)}
                  >
                    {option[0].toUpperCase() + option.slice(1)}
                  </button>
                ))}
              </div>
              <button
                type="button"
                className="secondary compact"
                disabled={
                  busy ||
                  routingActive ||
                  migrating ||
                  learning !== null ||
                  testing ||
                  savingMapping
                }
                onClick={() => void migrate()}
              >
                {migrating ? (
                  <LoaderCircle className="spin" />
                ) : (
                  <WandSparkles />
                )}
                Reuse existing bindings
              </button>
              <button
                type="button"
                className="secondary compact"
                disabled={
                  busy ||
                  routingActive ||
                  migrating ||
                  learning !== null ||
                  testing ||
                  savingMapping
                }
                onClick={() => void testControl()}
              >
                {testing ? <Radio className="spin" /> : <Crosshair />}
                {testing ? "Press a control…" : "Test a control"}
              </button>
            </div>

            {inspection && (
              <div className="profile-control-inspection">
                <Crosshair />
                <span>
                  <strong>{inspection.control}</strong>
                  <small>
                    {inspection.matches.length
                      ? inspection.matches
                          .map(
                            (match) =>
                              `${match.logicalAction} · vJoy B${match.virtualButton}`,
                          )
                          .join(" · ")
                      : "Not mapped in this profile"}
                  </small>
                </span>
                <button
                  type="button"
                  aria-label="Clear tested control"
                  onClick={() => setInspection(null)}
                >
                  <X />
                </button>
              </div>
            )}

            <div className="profile-mapping-dialog-list">
              {visibleActions.map((action) => {
                const assigned = mappings.filter(
                  (mapping) => mapping.logicalAction === action.action,
                );
                return (
                  <div
                    className={
                      inspection?.matches.some(
                        (match) => match.logicalAction === action.action,
                      )
                        ? "profile-mapping-row tested"
                        : "profile-mapping-row"
                    }
                    key={action.action}
                  >
                    <span>
                      <strong>{action.action}</strong>
                      <small>vJoy B{action.virtualButton}</small>
                    </span>
                    <div className="profile-mapping-sources">
                      {assigned.map((mapping) => (
                        <span
                          key={`${mapping.deviceId}-${mapping.physicalButton}-${mapping.modifiers
                            .map(
                              (modifier) =>
                                `${modifier.deviceId}-${modifier.physicalButton}`,
                            )
                            .join("+")}`}
                        >
                          {mapping.modifiers.map((modifier) => (
                            <span
                              className="profile-mapping-modifier"
                              key={`${modifier.deviceId}-${modifier.physicalButton}`}
                            >
                              {modifier.deviceName || "Physical controller"} ·
                              Button {modifier.physicalButton} +
                            </span>
                          ))}
                          {mapping.deviceName || "Physical controller"} · Button{" "}
                          {mapping.physicalButton}
                          <button
                            type="button"
                            aria-label={`Remove ${action.action} mapping`}
                            disabled={routingActive}
                            onClick={() => void remove(mapping)}
                          >
                            <Trash2 />
                          </button>
                        </span>
                      ))}
                    </div>
                    <div className="profile-mapping-assign-actions">
                      <button
                        type="button"
                        className="secondary compact"
                        disabled={
                          busy ||
                          routingActive ||
                          learning !== null ||
                          migrating ||
                          testing ||
                          savingMapping
                        }
                        onClick={() => void learn(action.action)}
                      >
                        {learning === action.action ? (
                          <>
                            <Radio className="spin" /> Press one, or hold +
                            press…
                          </>
                        ) : assigned.length ? (
                          "Add control"
                        ) : (
                          "Bind control"
                        )}
                      </button>
                    </div>
                  </div>
                );
              })}
              {!visibleActions.length && (
                <div className="profile-mapping-no-results">
                  No actions match this search and filter.
                </div>
              )}
            </div>

            <footer>
              <span>
                {visibleActions.length} of {actions.length} actions ·{" "}
                {mappings.length} physical mappings
              </span>
              {learning || testing ? (
                <button
                  type="button"
                  className="secondary"
                  onClick={() => void cancelLearning()}
                >
                  <Square /> Cancel control capture
                </button>
              ) : (
                <button
                  type="button"
                  className="primary"
                  disabled={savingMapping}
                  onClick={() => void closeEditor()}
                >
                  Done
                </button>
              )}
            </footer>
          </section>
        </div>
      )}
      <ConfirmDialog
        confirmation={confirmation}
        busy={savingMapping}
        onClose={() => setConfirmation(null)}
      />
    </section>
  );
}
