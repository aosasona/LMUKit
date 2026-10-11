import { FileJson, Workflow } from "lucide-react";

export type InputMode = "legacy" | "virtual";

export function InputModeSwitch({
  mode,
  busy,
  onChange,
}: {
  mode: InputMode;
  busy: boolean;
  onChange: (mode: InputMode) => void;
}) {
  return (
    <div
      className="input-mode-switch"
      role="radiogroup"
      aria-label="Global input method"
    >
      <span>Input method</span>
      <button
        type="button"
        role="radio"
        aria-checked={mode === "legacy"}
        className={mode === "legacy" ? "active" : ""}
        disabled={busy}
        onClick={() => onChange("legacy")}
      >
        <FileJson /> Legacy JSON
      </button>
      <button
        type="button"
        role="radio"
        aria-checked={mode === "virtual"}
        className={mode === "virtual" ? "active" : ""}
        disabled={busy}
        onClick={() => onChange("virtual")}
      >
        <Workflow /> Virtual Input
      </button>
    </div>
  );
}
