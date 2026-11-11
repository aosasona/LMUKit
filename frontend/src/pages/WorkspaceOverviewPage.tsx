import { Disc3, Gamepad2, Workflow } from "lucide-react";
import type {
  AcCarContextStatus,
  AutoProfileSelectionStatus,
  Page,
  Profile,
  VirtualRoutingStatus,
  Workspace,
} from "../models";

export function WorkspaceOverviewPage({
  workspace,
  profiles,
  routing,
  carContext,
  autoProfileSelection,
  busy,
  onSetAutoProfileSelection,
  onResumeAutoProfileSelection,
  onNavigate,
}: {
  workspace: Workspace;
  profiles: Profile[];
  routing: VirtualRoutingStatus;
  carContext: AcCarContextStatus;
  autoProfileSelection: AutoProfileSelectionStatus;
  busy: boolean;
  onSetAutoProfileSelection: (enabled: boolean) => void;
  onResumeAutoProfileSelection: () => void;
  onNavigate: (page: Page) => void;
}) {
  return (
    <section className="workspace-overview">
      <header className="workspace-overview-hero panel">
        <span className="eyebrow">Active workspace</span>
        <h2>{workspace.name}</h2>
        <p>
          Profiles and physical mappings here are separate from Le Mans
          Ultimate. Your wheel library remains available across every workspace.
        </p>
        <div className="workspace-overview-status">
          <span>
            <strong>{profiles.length}</strong> profiles
          </span>
          <span className={routing.active ? "active" : ""}>
            <i />
            {routing.active
              ? `${routing.profileName ?? "A profile"} is live`
              : "Live remapping is off"}
          </span>
        </div>
      </header>

      <section className="panel car-context-card">
        <div className="car-context-heading">
          <div>
            <span className="eyebrow">Live car detection</span>
            <h3>{carContext.carName ?? "Waiting for Assetto Corsa"}</h3>
            <p>{carContext.message}</p>
          </div>
          <span className={carContext.connected ? "status-good" : "status-warn"}>
            {carContext.connected ? carContext.sessionState : "Offline"}
          </span>
        </div>
        <div className="profile-tags compact-tags">
          {carContext.tags.map((tag) => (
            <span className="custom-tag" key={tag}>
              {tag}
            </span>
          ))}
          {carContext.connected && !carContext.tags.length && (
            <span className="generic-tag">No car tags found</span>
          )}
        </div>
        <div className="car-context-footer">
          <div>
            <strong>Automatic car-tag switching</strong>
            <span>{autoProfileSelection.message}</span>
          </div>
          <div className="car-context-actions">
            {autoProfileSelection.enabled && autoProfileSelection.paused && (
              <button
                type="button"
                className="secondary compact"
                disabled={busy}
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

      <div className="workspace-overview-steps panel">
        <header>
          <span className="eyebrow">Assetto Corsa setup</span>
          <h3>Build this workspace in three steps</h3>
        </header>
        <button type="button" onClick={() => onNavigate("profiles")}>
          <Gamepad2 />
          <span>
            <b>1</b>
            <strong>Create a profile</strong>
            <small>Assign one of your shared wheels to it.</small>
          </span>
        </button>
        <button type="button" onClick={() => onNavigate("virtual")}>
          <Workflow />
          <span>
            <b>2</b>
            <strong>Register AC actions</strong>
            <small>
              Name a virtual action and send its vJoy button while AC listens.
            </small>
          </span>
        </button>
        <button type="button" onClick={() => onNavigate("bindings")}>
          <Disc3 />
          <span>
            <b>3</b>
            <strong>Map physical controls</strong>
            <small>Bind each wheel button to the named virtual action.</small>
          </span>
        </button>
      </div>

      <div className="workspace-overview-note panel">
        <strong>Car-tag switching is read-only</strong>
        <p>
          Pivot reads the active car ID from AC shared memory and its tags from
          ui_car.json. It does not edit Assetto Corsa or Content Manager. AC
          must still be taught the vJoy buttons once, and the game remains
          responsible for steering, pedals and force feedback.
        </p>
      </div>
    </section>
  );
}
