import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";

export function TitleBar({
  onError,
  onExit,
  remappingActive,
  remappingProfile,
  busy,
  onDisableRemapping,
}: {
  onError: (message: string) => void;
  onExit: () => void;
  remappingActive: boolean;
  remappingProfile?: string | null;
  busy: boolean;
  onDisableRemapping: () => void;
}) {
  const run = (action: () => Promise<void>) =>
    void action().catch((error) =>
      onError(`Window action failed: ${String(error)}`),
    );
  return (
    <div
      className="titlebar"
      data-tauri-drag-region
      onMouseDown={(event) => {
        if (
          event.button === 0 &&
          !(event.target as HTMLElement).closest("button")
        )
          run(() => getCurrentWindow().startDragging());
      }}
    >
      <div className="titlebar-brand" data-tauri-drag-region>
        <img src="/app-icon.png" alt="" />
        <span data-tauri-drag-region>Pivot</span>
      </div>
      <div className="titlebar-actions">
        {remappingActive && (
          <button
            type="button"
            className="titlebar-routing-stop"
            disabled={busy}
            title={
              remappingProfile
                ? `Stop live remapping for ${remappingProfile}`
                : "Stop live remapping and release every virtual output"
            }
            onClick={onDisableRemapping}
          >
            <Square />
            <span>Disable remapping</span>
          </button>
        )}
        <div className="window-controls">
          <button
            aria-label="Minimize"
            onClick={() => run(() => getCurrentWindow().minimize())}
          >
            <Minus />
          </button>
          <button
            className="close"
            aria-label="Hide to notification area"
            title="Hide to notification area · Right-click to exit"
            onClick={() => run(() => getCurrentWindow().close())}
            onContextMenu={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onExit();
            }}
          >
            <X />
          </button>
        </div>
      </div>
    </div>
  );
}
