import { invoke } from "@tauri-apps/api/core";
import {
  Gamepad2,
  LoaderCircle,
  Radio,
  Search,
  Square,
  Trash2,
  WandSparkles,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import type {
  Profile,
  VirtualAction,
  VirtualMappingImport,
  VirtualProfileMapping,
  VirtualRoutingStatus,
} from "../models";

function searchTokens(value: string) {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim()
    .split(/\s+/)
    .filter(Boolean);
}

export function ProfileVirtualMappings({
  profile,
  busy,
  onNotice,
  editorOpen,
  onEditorOpenChange,
}: {
  profile: Profile;
  busy: boolean;
  onNotice: (message: string) => void;
  editorOpen: boolean;
  onEditorOpenChange: (open: boolean) => void;
}) {
  const [actions, setActions] = useState<VirtualAction[]>([]);
  const [mappings, setMappings] = useState<VirtualProfileMapping[]>([]);
  const [loading, setLoading] = useState(true);
  const [migrating, setMigrating] = useState(false);
  const [learning, setLearning] = useState<{
    action: string;
    kind: "button" | "chord";
  } | null>(null);
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
    void Promise.all([
      invoke<VirtualAction[]>("saved_virtual_layout"),
      invoke<VirtualProfileMapping[]>("virtual_profile_mapping", {
        profileId: profile.id,
      }),
      invoke<VirtualRoutingStatus>("virtual_routing_status"),
    ])
      .then(([nextActions, nextMappings, routing]) => {
        if (cancelled) return;
        setActions(nextActions);
        setMappings(nextMappings);
        setRouting(routing);
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
  }, [profile.id]);

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
      void closeEditor();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  });

  const learn = async (action: string, kind: "button" | "chord") => {
    if (routingActive) return;
    setLearning({ action, kind });
    setError(null);
    onNotice(
      kind === "chord"
        ? `Hold the modifier, then press the trigger for ${action}.`
        : `Press the physical button for ${action}.`,
    );
    try {
      const next = await invoke<VirtualProfileMapping[]>(
        kind === "chord"
          ? "learn_virtual_profile_chord"
          : "learn_virtual_profile_button",
        { profileId: profile.id, logicalAction: action },
      );
      setMappings(next);
      onNotice(
        `${action} mapped to the pressed ${kind === "chord" ? "button chord" : "physical button"}.`,
      );
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
    onNotice("Button learning cancelled.");
  };

  const closeEditor = async () => {
    if (learning) await invoke("cancel_binding_lookup");
    setLearning(null);
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
        <span>{mappings.length} assigned</span>
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
            disabled={busy || routingBusy || !mappings.length}
            onClick={() => void startRouting()}
          >
            {routingBusy ? <LoaderCircle className="spin" /> : <Gamepad2 />}
            {routing.active
              ? "Switch to this profile"
              : "Use this profile live"}
          </button>
        )}
      </div>

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
                  busy || routingActive || migrating || learning !== null
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
            </div>

            <div className="profile-mapping-dialog-list">
              {visibleActions.map((action) => {
                const assigned = mappings.filter(
                  (mapping) => mapping.logicalAction === action.action,
                );
                return (
                  <div className="profile-mapping-row" key={action.action}>
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
                          migrating
                        }
                        onClick={() => void learn(action.action, "button")}
                      >
                        {learning?.action === action.action &&
                        learning.kind === "button" ? (
                          <>
                            <Radio className="spin" /> Press a button…
                          </>
                        ) : assigned.length ? (
                          "Add button"
                        ) : (
                          "Assign button"
                        )}
                      </button>
                      <button
                        type="button"
                        className="ghost compact"
                        disabled={
                          busy ||
                          routingActive ||
                          learning !== null ||
                          migrating
                        }
                        onClick={() => void learn(action.action, "chord")}
                      >
                        {learning?.action === action.action &&
                        learning.kind === "chord" ? (
                          <>
                            <Radio className="spin" /> Hold + press…
                          </>
                        ) : (
                          "Add chord"
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
              {learning ? (
                <button
                  type="button"
                  className="secondary"
                  onClick={() => void cancelLearning()}
                >
                  <Square /> Cancel button capture
                </button>
              ) : (
                <button
                  type="button"
                  className="primary"
                  onClick={() => void closeEditor()}
                >
                  Done
                </button>
              )}
            </footer>
          </section>
        </div>
      )}
    </section>
  );
}
