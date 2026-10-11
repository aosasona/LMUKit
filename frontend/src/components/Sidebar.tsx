import {
  Gamepad2,
  Disc3,
  LayoutDashboard,
  CircleHelp,
  Power,
  Settings,
  SlidersHorizontal,
  Workflow,
  Wrench,
} from "lucide-react";
import type { Page } from "../models";

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
  onNavigate,
  onExit,
}: {
  page: Page;
  profileCount: number;
  onNavigate: (page: Page) => void;
  onExit: () => void;
}) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-mark">
          <span>LMU</span>
        </div>
        <div>
          <strong>LMUKit</strong>
          <small>Race setup, simplified</small>
        </div>
      </div>
      <nav>
        <span className="nav-label">Workspace</span>
        {items.map(([id, label, Icon]) => (
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
        <button
          className={page === "help" ? "nav-item active" : "nav-item"}
          onClick={() => onNavigate("help")}
        >
          <CircleHelp size={18} strokeWidth={1.8} />
          <span>Help & setup</span>
        </button>
        <button className="app-exit" onClick={onExit}>
          <Power size={16} />
          <span>Exit LMUKit</span>
        </button>
      </div>
    </aside>
  );
}
