import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";

export function TitleBar({ onError }: { onError: (message: string) => void }) {
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
      onDoubleClick={(event) => {
        if (!(event.target as HTMLElement).closest("button"))
          run(() => getCurrentWindow().toggleMaximize());
      }}
    >
      <div className="titlebar-brand" data-tauri-drag-region>
        <img src="/app-icon.png" alt="" />
        <span data-tauri-drag-region>LMUKit</span>
      </div>
      <div className="window-controls">
        <button
          aria-label="Minimize"
          onClick={() => run(() => getCurrentWindow().minimize())}
        >
          <Minus />
        </button>
        <button
          aria-label="Maximize or restore"
          onClick={() => run(() => getCurrentWindow().toggleMaximize())}
        >
          <Square />
        </button>
        <button
          className="close"
          aria-label="Close"
          onClick={() => run(() => getCurrentWindow().close())}
        >
          <X />
        </button>
      </div>
    </div>
  );
}
