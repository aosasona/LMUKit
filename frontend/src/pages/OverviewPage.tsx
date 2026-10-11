import {
  Activity,
  CarFront,
  ChevronRight,
  CircleGauge,
  Gamepad2,
  SlidersHorizontal,
  Sparkles,
  Upload,
  Wrench,
  Zap,
} from "lucide-react";
import { useState } from "react";
import { ConfirmDialog, type Confirmation } from "../components/ConfirmDialog";
import type { InputMode } from "../components/InputModeSwitch";
import { WheelImage } from "../components/WheelImage";
import type {
  AutoProfileSelectionStatus,
  LmuCarContextStatus,
  Profile,
  VirtualRoutingStatus,
} from "../models";

function contextStateLabel(context: LmuCarContextStatus) {
  if (context.connected && context.status === "confirmed") return "Live";
  if (context.connected) return "Waiting";
  if (context.status === "stale") return "Stale";
  return "Offline";
}

function displaySession(value: LmuCarContextStatus["sessionState"]) {
  if (value === "realtime") return "Driving";
  return value[0].toUpperCase() + value.slice(1);
}

function displayObservedAt(value: string | null) {
  if (!value) return "No observation yet";
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return value;
  return `Observed ${date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}`;
}

type Props = {
  active?: Profile;
  activeDirty: boolean | null;
  selected?: Profile;
  wheelImageUrl: string | null;
  wheelImageRevision: number;
  profiles: Profile[];
  busy: boolean;
  inputMode: InputMode;
  routing: VirtualRoutingStatus;
  carContext: LmuCarContextStatus;
  autoProfileSelection: AutoProfileSelectionStatus;
  onNavigate: (page: "profiles" | "bindings" | "game") => void;
  onActivate: (profile: Profile) => void;
  onUpdate: (profile: Profile) => void;
  onDismissDirty: () => void;
  onSetAutoProfileSelection: (enabled: boolean) => void;
  onResumeAutoProfileSelection: () => void;
};

export function OverviewPage({
  active,
  activeDirty,
  selected,
  wheelImageUrl,
  wheelImageRevision,
  profiles,
  busy,
  inputMode,
  routing,
  carContext,
  autoProfileSelection,
  onNavigate,
  onActivate,
  onUpdate,
  onDismissDirty,
  onSetAutoProfileSelection,
  onResumeAutoProfileSelection,
}: Props) {
  const legacyMode = inputMode === "legacy";
  const hero = legacyMode ? (active ?? selected) : (selected ?? active);
  const heroIsLive =
    !legacyMode && routing.active && routing.profileId === hero?.id;
  const highlightedProfileId = legacyMode
    ? active?.id
    : routing.active
      ? routing.profileId
      : selected?.id;
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  return (
    <>
      <div className="dashboard-grid">
        <section className="hero-card">
          <div className="hero-copy">
            <span className="chip">
              <span className="pulse" /> Current setup
            </span>
            <h2>{hero?.name ?? "Choose your race setup"}</h2>
            <p>
              {hero
                ? `${hero.bindingCount} controls across ${Math.max(hero.devices.length, 1)} connected device${hero.devices.length === 1 ? "" : "s"}.`
                : "Select a profile to prepare LMU for your next session."}
            </p>
            <div className="hero-actions">
              {hero && (
                <button
                  className="primary"
                  disabled={
                    busy || (legacyMode && hero.id === active?.id) || heroIsLive
                  }
                  onClick={() => onActivate(hero)}
                >
                  <Zap size={17} fill="currentColor" />
                  {legacyMode
                    ? hero.id === active?.id
                      ? "Prepared"
                      : "Use this profile"
                    : heroIsLive
                      ? "Live remapping active"
                      : "Use this profile live"}
                </button>
              )}
              {legacyMode && active?.differsFromLive && activeDirty && (
                <button
                  className="secondary"
                  onClick={() =>
                    setConfirmation({
                      title: `Update ${active.name} from LMU?`,
                      description:
                        "This replaces the saved profile with LMU's current bindings. LMUKit will create a recovery backup first.",
                      confirmLabel: "Update from LMU",
                      onConfirm: () => {
                        onUpdate(active);
                        setConfirmation(null);
                      },
                    })
                  }
                >
                  Update saved profile
                </button>
              )}
              <button
                className="secondary"
                onClick={() => onNavigate("profiles")}
              >
                View profiles <ChevronRight size={17} />
              </button>
            </div>
          </div>
          <div className="wheel-stage">
            <div className="halo" />
            <img
              src={wheelImageUrl ?? "/wheel-hero.png"}
              alt={
                wheelImageUrl
                  ? `Wheel assigned to ${hero?.name ?? "this profile"}`
                  : "A generic GT racing wheel and wheelbase"
              }
            />
          </div>
        </section>
        <section className="metric-card">
          <div className="metric-icon teal">
            <Gamepad2 />
          </div>
          <div>
            <span>Saved profiles</span>
            <strong>{profiles.length.toString().padStart(2, "0")}</strong>
            <small>Ready for LMU</small>
          </div>
        </section>
        <section className="metric-card">
          <div className="metric-icon amber">
            <CircleGauge />
          </div>
          <div>
            <span>Active bindings</span>
            <strong>{hero?.bindingCount ?? 0}</strong>
            <small>{hero?.devices.length ?? 0} input devices</small>
          </div>
        </section>
        <section className="metric-card">
          <div className="metric-icon violet">
            <Activity />
          </div>
          <div>
            <span>Profile health</span>
            <strong
              className={legacyMode && activeDirty ? "word warning" : "word"}
            >
              {legacyMode
                ? activeDirty
                  ? "Changed"
                  : "Synced"
                : routing.active
                  ? "Live"
                  : "Inactive"}
            </strong>
            <small>
              {!legacyMode
                ? "Legacy file comparison is inactive"
                : activeDirty
                  ? "LMU differs from the saved profile"
                  : "No pending changes"}
            </small>
            {legacyMode && activeDirty && (
              <button className="text-button" onClick={onDismissDirty}>
                Dismiss
              </button>
            )}
          </div>
        </section>
        <section
          className={`panel car-context-card context-${carContext.status}`}
        >
          <div className="car-context-heading">
            <div>
              <span className="eyebrow">Read-only game context</span>
              <h3>LMU car detection</h3>
            </div>
            <span
              className={`car-context-state ${carContext.connected ? "connected" : ""}`}
            >
              <i /> {contextStateLabel(carContext)}
            </span>
          </div>
          <div className="car-context-details">
            <CarFront />
            <span>
              <small>Player car</small>
              <strong>{carContext.vehicleName ?? "Not detected"}</strong>
              <em>{carContext.vehicleId ?? "Waiting for a player vehicle"}</em>
            </span>
            <span>
              <small>Class</small>
              <strong>
                {carContext.classLabel ?? carContext.classRaw ?? "Unknown"}
              </strong>
              <em>
                {carContext.classLabel && carContext.classRaw
                  ? `LMU reports ${carContext.classRaw}`
                  : "No matching profile tag"}
              </em>
            </span>
            <span>
              <small>Session</small>
              <strong>{displaySession(carContext.sessionState)}</strong>
              <em>
                {carContext.source === "shared_memory"
                  ? "LMU shared memory"
                  : "Source unavailable"}
              </em>
            </span>
            <span>
              <small>Freshness</small>
              <strong>{contextStateLabel(carContext)}</strong>
              <em title={carContext.observedAt ?? undefined}>
                {displayObservedAt(carContext.observedAt)}
              </em>
            </span>
          </div>
          <div className="car-context-footer">
            <div>
              <strong>Automatic class switching</strong>
              <span>
                {autoProfileSelection.enabled
                  ? autoProfileSelection.message
                  : carContext.message}
              </span>
            </div>
            <div className="car-context-actions">
              {autoProfileSelection.enabled && autoProfileSelection.paused && (
                <button
                  type="button"
                  className="secondary compact"
                  disabled={busy || inputMode !== "virtual"}
                  onClick={onResumeAutoProfileSelection}
                >
                  Resume Auto
                </button>
              )}
              <label className="inline-switch">
                <span>Auto-select</span>
                <input
                  type="checkbox"
                  checked={autoProfileSelection.enabled}
                  disabled={busy}
                  onChange={(event) =>
                    onSetAutoProfileSelection(event.target.checked)
                  }
                />
              </label>
            </div>
          </div>
        </section>
        <section className="panel recent">
          <div className="section-head">
            <div>
              <span className="eyebrow">Your garage</span>
              <h3>Race profiles</h3>
            </div>
            <button
              className="text-button"
              onClick={() => onNavigate("profiles")}
            >
              Manage all <ChevronRight size={16} />
            </button>
          </div>
          <div className="profile-strip">
            {profiles.slice(0, 4).map((profile, index) => (
              <button
                key={profile.id}
                className={
                  profile.id === highlightedProfileId
                    ? "mini-profile active"
                    : "mini-profile"
                }
                onClick={() => onActivate(profile)}
              >
                <WheelImage
                  wheelId={profile.wheelId}
                  hasImage={profile.hasWheelImage}
                  alt={`${profile.wheelName ?? profile.name} wheel`}
                  className="mini-profile-wheel"
                  revision={wheelImageRevision}
                />
                <div>
                  <strong>{profile.name}</strong>
                  <small>
                    {profile.bindingCount} bindings ·{" "}
                    {profile.devices[0]?.name ?? "Input profile"}
                  </small>
                </div>
                {profile.hotkeySlot && <kbd>Ctrl Alt {profile.hotkeySlot}</kbd>}
              </button>
            ))}
            {!profiles.length && (
              <div className="empty">
                <Sparkles size={22} />
                <span>Your saved profiles will appear here.</span>
              </div>
            )}
          </div>
        </section>
        <section className="panel quick-actions">
          <div className="section-head">
            <div>
              <span className="eyebrow">Get moving</span>
              <h3>Quick actions</h3>
            </div>
          </div>
          <button onClick={() => onNavigate("profiles")}>
            <Upload />
            <div>
              <strong>Import preset</strong>
              <small>Drop in an LMU JSON file</small>
            </div>
            <ChevronRight />
          </button>
          <button onClick={() => onNavigate("bindings")}>
            <SlidersHorizontal />
            <div>
              <strong>Edit bindings</strong>
              <small>Map controls outside LMU</small>
            </div>
            <ChevronRight />
          </button>
          <button onClick={() => onNavigate("game")}>
            <Wrench />
            <div>
              <strong>Tune game settings</strong>
              <small>FFB, display and more</small>
            </div>
            <ChevronRight />
          </button>
        </section>
      </div>
      <ConfirmDialog
        confirmation={confirmation}
        busy={busy}
        onClose={() => setConfirmation(null)}
      />
    </>
  );
}
