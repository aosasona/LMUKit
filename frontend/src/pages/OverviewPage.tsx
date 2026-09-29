import {
  Activity,
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
import type { Profile } from "../models";

type Props = {
  active?: Profile;
  selected?: Profile;
  wheelImageUrl: string | null;
  profiles: Profile[];
  busy: boolean;
  onNavigate: (page: "profiles" | "bindings" | "game") => void;
  onActivate: (profile: Profile) => void;
  onUpdate: (profile: Profile) => void;
};

export function OverviewPage({
  active,
  selected,
  wheelImageUrl,
  profiles,
  busy,
  onNavigate,
  onActivate,
  onUpdate,
}: Props) {
  const hero = selected ?? active;
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  return (
    <>
      <div className="dashboard-grid">
        <section className="hero-card">
          <div className="hero-copy">
            <span className="chip">
              <span className="pulse" /> Current setup
            </span>
            <h2>{active?.name ?? "Choose your race setup"}</h2>
            <p>
              {active
                ? `${active.bindingCount} controls across ${Math.max(active.devices.length, 1)} connected device${active.devices.length === 1 ? "" : "s"}.`
                : "Select a profile to prepare LMU for your next session."}
            </p>
            <div className="hero-actions">
              {hero && (
                <button
                  className="primary"
                  disabled={busy || hero.id === active?.id}
                  onClick={() => onActivate(hero)}
                >
                  <Zap size={17} fill="currentColor" />
                  {hero.id === active?.id ? "Prepared" : "Use this profile"}
                </button>
              )}
              {active?.differsFromLive && (
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
            <strong>{active?.bindingCount ?? 0}</strong>
            <small>{active?.devices.length ?? 0} input devices</small>
          </div>
        </section>
        <section className="metric-card">
          <div className="metric-icon violet">
            <Activity />
          </div>
          <div>
            <span>Profile health</span>
            <strong className="word">Synced</strong>
            <small>No pending changes</small>
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
                  profile.id === active?.id
                    ? "mini-profile active"
                    : "mini-profile"
                }
                onClick={() => onActivate(profile)}
              >
                <span className="profile-number">0{index + 1}</span>
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
