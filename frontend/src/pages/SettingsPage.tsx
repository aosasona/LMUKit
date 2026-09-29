import { Minus, Plus, Type } from "lucide-react";
import { useEffect, useState } from "react";

const MIN_SCALE = 0.85;
const MAX_SCALE = 1.4;
const STEP = 0.05;

export function SettingsPage({
  fontScale,
  busy,
  onSaveFontScale,
}: {
  fontScale: number;
  busy: boolean;
  onSaveFontScale: (scale: number) => void;
}) {
  const [draft, setDraft] = useState(fontScale);

  useEffect(() => setDraft(fontScale), [fontScale]);

  const preview = (scale: number) => {
    const normalized = Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
    setDraft(normalized);
    document.documentElement.style.setProperty(
      "--font-scale",
      normalized.toString(),
    );
  };

  return (
    <section className="settings-page">
      <div className="settings-card">
        <header>
          <span className="settings-icon">
            <Type />
          </span>
          <div>
            <span className="eyebrow">Appearance</span>
            <h2>Text size</h2>
            <p>
              Scale every font in LMUKit without changing your Windows settings.
            </p>
          </div>
          <strong>{Math.round(draft * 100)}%</strong>
        </header>
        <div className="font-scale-control">
          <button
            type="button"
            aria-label="Decrease text size"
            disabled={busy || draft <= MIN_SCALE}
            onClick={() => {
              const next = Math.max(MIN_SCALE, draft - STEP);
              preview(next);
              onSaveFontScale(next);
            }}
          >
            <Minus />
          </button>
          <input
            type="range"
            min={MIN_SCALE}
            max={MAX_SCALE}
            step={STEP}
            value={draft}
            aria-label="Text size"
            onChange={(event) => preview(Number(event.target.value))}
            onPointerUp={() => onSaveFontScale(draft)}
            onKeyUp={() => onSaveFontScale(draft)}
            onBlur={() => onSaveFontScale(draft)}
          />
          <button
            type="button"
            aria-label="Increase text size"
            disabled={busy || draft >= MAX_SCALE}
            onClick={() => {
              const next = Math.min(MAX_SCALE, draft + STEP);
              preview(next);
              onSaveFontScale(next);
            }}
          >
            <Plus />
          </button>
        </div>
        <div className="font-scale-presets">
          {[0.9, 1, 1.1, 1.2, 1.3].map((scale) => (
            <button
              type="button"
              className={Math.abs(draft - scale) < 0.001 ? "active" : ""}
              disabled={busy}
              key={scale}
              onClick={() => {
                preview(scale);
                onSaveFontScale(scale);
              }}
            >
              {Math.round(scale * 100)}%
            </button>
          ))}
        </div>
        <div className="font-scale-preview">
          <span>Preview</span>
          <strong>Readable at racing distance</strong>
          <p>
            Profile details, controls, menus, and status messages all follow
            this setting.
          </p>
        </div>
      </div>
    </section>
  );
}
