import {
  Gamepad2,
  Keyboard,
  LayoutDashboard,
  Settings,
  SlidersHorizontal,
  Wrench,
} from "lucide-react";
import type { Page } from "../models";

const items = [
  ["home", "Overview", LayoutDashboard],
  ["profiles", "Profiles", Gamepad2],
  ["bindings", "Binding editor", SlidersHorizontal],
  ["game", "Game settings", Wrench],
  ["settings", "Settings", Settings],
] as const;

export function Sidebar({
  page,
  profileCount,
  onNavigate,
}: {
  page: Page;
  profileCount: number;
  onNavigate: (page: Page) => void;
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
        <div className="game-state">
          <span className="pulse" />
          <div>
            <strong>Workspace ready</strong>
            <small>Close LMU before changes</small>
          </div>
        </div>
        <button className="keyboard-hint">
          <Keyboard size={16} />
          <span>Quick switcher</span>
          <kbd>Ctrl Alt Space</kbd>
        </button>
      </div>
    </aside>
  );
}
