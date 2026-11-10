import { Disc3, Gamepad2, Workflow } from "lucide-react";
import type { Page, Profile, VirtualRoutingStatus, Workspace } from "../models";

export function WorkspaceOverviewPage({
  workspace,
  profiles,
  routing,
  onNavigate,
}: {
  workspace: Workspace;
  profiles: Profile[];
  routing: VirtualRoutingStatus;
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
        <strong>Initial AC support is manual</strong>
        <p>
          LMUKit does not edit Assetto Corsa configuration files. AC must be
          taught the vJoy buttons once, and the game remains responsible for
          steering, pedals and force feedback.
        </p>
      </div>
    </section>
  );
}
