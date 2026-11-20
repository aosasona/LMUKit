import {
  Gamepad2,
  Disc3,
  LayoutDashboard,
  CircleHelp,
  Download,
  LoaderCircle,
  Power,
  Settings,
  SlidersHorizontal,
  Workflow,
  Wrench,
} from "lucide-react";
import type { AppUpdateState } from "../hooks/useAppUpdater";
import type { Page } from "../models";
import type { Workspace } from "../models";
import { WorkspaceSwitcher } from "./WorkspaceSwitcher";

const items = [
  ["home", "Overview", LayoutDashboard],
  ["profiles", "Profiles", Gamepad2],
  ["wheels", "Wheels", Disc3],
  ["bindings", "Profile editor", SlidersHorizontal],
  ["virtual", "Virtual input", Workflow],
  ["game", "Game settings", Wrench],
  ["settings", "Settings", Settings],
] as const;

export function Sidebar({
  page,
  profileCount,
  workspaces,
  activeWorkspaceId,
  activeWorkspaceKind,
  busy,
  update,
  onWorkspaceChange,
  onNavigate,
  onReviewUpdate,
  onExit,
}: {
  page: Page;
  profileCount: number;
  workspaces: Workspace[];
  activeWorkspaceId: string;
  activeWorkspaceKind: Workspace["kind"];
  busy: boolean;
  update: AppUpdateState;
  onWorkspaceChange: (workspaceId: string) => void;
  onNavigate: (page: Page) => void;
  onReviewUpdate: () => void;
  onExit: () => void;
}) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-mark">
          <span>P</span>
        </div>
        <div>
          <strong>Pivot</strong>
          <small>Race setup, simplified</small>
        </div>
      </div>
      <WorkspaceSwitcher
        workspaces={workspaces}
        activeId={activeWorkspaceId}
        busy={busy}
        onChange={onWorkspaceChange}
      />
      <nav>
        <span className="nav-label">Workspace</span>
        {items
          .filter(
            ([id]) =>
              id !== "game" || activeWorkspaceKind === "le_mans_ultimate",
          )
          .map(([id, label, Icon]) => (
            <button
              className={page === id ? "nav-item active" : "nav-item"}
              onClick={() => onNavigate(id)}
              key={id}
            >
              <Icon size={18} strokeWidth={1.8} />
              <span>{label}</span>
              {id === "profiles" && <em>{profileCount}</em>}
            </button>
          ))}
      </nav>
      <div className="sidebar-foot">
        {(update.phase === "available" ||
          update.phase === "downloading" ||
          update.phase === "installing") && (
          <button className="nav-item sidebar-update" onClick={onReviewUpdate}>
            {update.phase === "available" ? (
              <Download size={18} strokeWidth={1.8} />
            ) : (
              <LoaderCircle className="spin" size={18} strokeWidth={1.8} />
            )}
            <span>
              {update.phase === "available"
                ? "Update available"
                : update.phase === "downloading"
                  ? "Downloading update"
                  : "Installing update"}
            </span>
            {update.availableVersion && <em>{update.availableVersion}</em>}
          </button>
        )}
        <button
          className={page === "help" ? "nav-item active" : "nav-item"}
          onClick={() => onNavigate("help")}
        >
          <CircleHelp size={18} strokeWidth={1.8} />
          <span>Help & setup</span>
        </button>
        <button className="app-exit" onClick={onExit}>
          <Power size={16} />
          <span>Exit Pivot</span>
        </button>
      </div>
    </aside>
  );
}
