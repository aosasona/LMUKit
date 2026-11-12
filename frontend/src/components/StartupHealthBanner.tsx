import { AlertTriangle, CheckCircle2, RefreshCw, XCircle } from "lucide-react";
import type { StartupHealth } from "../models";

export function StartupHealthBanner({
  health,
  loading,
  onRefresh,
}: {
  health: StartupHealth | null;
  loading: boolean;
  onRefresh: () => void;
}) {
  if (!health && !loading) return null;
  const Icon =
    health?.level === "ready"
      ? CheckCircle2
      : health?.level === "error"
        ? XCircle
        : AlertTriangle;
  return (
    <aside
      className={`startup-health ${health?.level ?? "checking"}`}
      aria-live="polite"
    >
      {loading ? <RefreshCw className="spin" /> : <Icon />}
      <span>
        <strong>{loading ? "Checking virtual input…" : health?.title}</strong>
        <small>
          {loading
            ? "Confirming vJoy, this workspace, and the current route."
            : health?.message}
        </small>
      </span>
      <button
        type="button"
        className="ghost compact"
        disabled={loading}
        onClick={onRefresh}
      >
        <RefreshCw /> Check again
      </button>
    </aside>
  );
}
