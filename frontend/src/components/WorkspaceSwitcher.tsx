import { Check, ChevronDown, Flag, Gauge } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Workspace } from "../models";

function workspaceLabel(kind: Workspace["kind"]) {
  if (kind === "le_mans_ultimate") return "LMU";
  if (kind === "assetto_corsa") return "AC";
  return "Custom";
}

export function WorkspaceSwitcher({
  workspaces,
  activeId,
  busy,
  onChange,
}: {
  workspaces: Workspace[];
  activeId: string;
  busy: boolean;
  onChange: (workspaceId: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const active =
    workspaces.find((workspace) => workspace.id === activeId) ?? workspaces[0];

  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent | KeyboardEvent) => {
      if (event instanceof KeyboardEvent && event.key === "Escape") {
        setOpen(false);
        return;
      }
      if (
        event instanceof PointerEvent &&
        !root.current?.contains(event.target as Node)
      ) {
        setOpen(false);
      }
    };
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", close);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("keydown", close);
    };
  }, [open]);

  if (!active) return null;

  return (
    <div className="workspace-switcher" ref={root}>
      <button
        type="button"
        className={
          open
            ? "workspace-switcher-trigger open"
            : "workspace-switcher-trigger"
        }
        aria-haspopup="listbox"
        aria-expanded={open}
        disabled={busy}
        onClick={() => setOpen((current) => !current)}
      >
        <span className="workspace-switcher-icon">
          {active.kind === "assetto_corsa" ? <Gauge /> : <Flag />}
        </span>
        <span>
          <small>Workspace</small>
          <strong>{active.name}</strong>
        </span>
        <ChevronDown />
      </button>
      {open && (
        <div className="workspace-switcher-menu" role="listbox">
          <span>Switch workspace</span>
          {workspaces.map((workspace) => (
            <button
              type="button"
              role="option"
              aria-selected={workspace.id === active.id}
              className={workspace.id === active.id ? "active" : ""}
              key={workspace.id}
              onClick={() => {
                setOpen(false);
                if (workspace.id !== active.id) onChange(workspace.id);
              }}
            >
              <i>{workspaceLabel(workspace.kind)}</i>
              <span>
                <strong>{workspace.name}</strong>
                <small>
                  {workspace.kind === "le_mans_ultimate"
                    ? "LMU profiles, settings and car detection"
                    : workspace.kind === "assetto_corsa"
                      ? "Assetto Corsa virtual mappings"
                      : "Process-triggered virtual mappings"}
                </small>
              </span>
              {workspace.id === active.id && <Check />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
