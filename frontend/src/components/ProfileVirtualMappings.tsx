import { invoke } from "@tauri-apps/api/core";
import {
  Gamepad2,
  LoaderCircle,
  Radio,
  Square,
  Trash2,
  WandSparkles,
} from "lucide-react";
import { useEffect, useState } from "react";
import type {
  Profile,
  VirtualAction,
  VirtualMappingImport,
  VirtualProfileMapping,
  VirtualRoutingStatus,
} from "../models";

export function ProfileVirtualMappings({
  profile,
  busy,
  onNotice,
}: {
  profile: Profile;
  busy: boolean;
  onNotice: (message: string) => void;
}) {
  const [actions, setActions] = useState<VirtualAction[]>([]);
  const [mappings, setMappings] = useState<VirtualProfileMapping[]>([]);
  const [loading, setLoading] = useState(true);
  const [migrating, setMigrating] = useState(false);
  const [learningAction, setLearningAction] = useState<string | null>(null);
  const [routingBusy, setRoutingBusy] = useState(false);
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

  const learn = async (action: string) => {
    if (routingActive) return;
    setLearningAction(action);
    setError(null);
    onNotice(`Press the physical button for ${action}.`);
    try {
      const next = await invoke<VirtualProfileMapping[]>(
        "learn_virtual_profile_button",
        { profileId: profile.id, logicalAction: action },
      );
      setMappings(next);
      onNotice(`${action} mapped to the pressed physical button.`);
    } catch (reason) {
      const message = String(reason);
      if (!message.toLowerCase().includes("cancelled")) {
        setError(message);
        onNotice(message);
      }
    } finally {
      setLearningAction(null);
    }
  };

  const cancelLearning = async () => {
    await invoke("cancel_binding_lookup");
    setLearningAction(null);
    onNotice("Button learning cancelled.");
  };

  const remove = async (mapping: VirtualProfileMapping) => {
    if (routingActive) return;
    try {
      setMappings(
        await invoke<VirtualProfileMapping[]>("remove_virtual_profile_button", {
          profileId: profile.id,
          deviceId: mapping.deviceId,
          physicalButton: mapping.physicalButton,
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
            Map this profile&apos;s physical controls to the actions already
            bound through vJoy in LMU.
          </p>
        </div>
        <span>{mappings.length} assigned</span>
      </header>

      {routingActive && (
        <div className="profile-mapping-notice">
          Disable wheel remapping on the Virtual Input page before editing
          mappings.
        </div>
      )}
      {error && <div className="virtual-error">{error}</div>}

      {!loading && actions.length > 0 && (
        <button
          type="button"
          className="secondary compact profile-mapping-import"
          disabled={
            busy || routingActive || migrating || learningAction !== null
          }
          onClick={() => void migrate()}
        >
          {migrating ? <LoaderCircle className="spin" /> : <WandSparkles />}
          Reuse this profile&apos;s existing bindings
        </button>
      )}

      <div className="profile-mapping-list">
        {actions.map((action) => {
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
                  <span key={`${mapping.deviceId}-${mapping.physicalButton}`}>
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
              <button
                type="button"
                className="secondary compact"
                disabled={
                  busy || routingActive || learningAction !== null || migrating
                }
                onClick={() => void learn(action.action)}
              >
                {learningAction === action.action ? (
                  <>
                    <Radio className="spin" /> Press a button…
                  </>
                ) : assigned.length ? (
                  "Add another"
                ) : (
                  "Assign"
                )}
              </button>
            </div>
          );
        })}
      </div>

      {loading && <p className="virtual-empty">Loading button mappings…</p>}
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
      {learningAction && (
        <button
          type="button"
          className="ghost compact profile-mapping-cancel"
          onClick={() => void cancelLearning()}
        >
          <Square /> Cancel button capture
        </button>
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
    </section>
  );
}
