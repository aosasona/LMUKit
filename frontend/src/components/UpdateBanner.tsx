import { ArrowRight, Download } from "lucide-react";
import type { AppUpdateState } from "../hooks/useAppUpdater";

export function UpdateBanner({
  update,
  onReview,
}: {
  update: AppUpdateState;
  onReview: () => void;
}) {
  if (update.phase !== "available" || !update.availableVersion) return null;
  return (
    <aside className="update-banner" aria-live="polite">
      <Download />
      <span>
        <strong>Pivot {update.availableVersion} is available</strong>
        <small>Review the release and install it from Settings.</small>
      </span>
      <button type="button" className="secondary" onClick={onReview}>
        Review update <ArrowRight />
      </button>
    </aside>
  );
}
