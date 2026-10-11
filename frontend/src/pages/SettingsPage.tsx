import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  AlertCircle,
  Check,
  Copy,
  Download,
  FileJson,
  FolderOpen,
  LoaderCircle,
  Minus,
  Plus,
  PlusCircle,
  RefreshCw,
  Trash2,
  Type,
} from "lucide-react";
import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import type { CompanionApp, DiagnosticStatus, Profile } from "../models";

const MIN_SCALE = 0.85;
const MAX_SCALE = 1.4;
const STEP = 0.05;

const formatBytes = (bytes: number) =>
  bytes < 1024
    ? `${bytes} B`
    : bytes < 1024 * 1024
      ? `${(bytes / 1024).toFixed(1)} KB`
      : `${(bytes / (1024 * 1024)).toFixed(1)} MB`;

type Props = {
  fontScale: number;
  busy: boolean;
  profiles: Profile[];
  configPath: string;
  settingsPath: string;
  companionApps: CompanionApp[];
  launchMode: "desktop" | "vr";
  onSaveFontScale: (scale: number) => void;
  onSaveSettings: (
    configPath: string,
    settingsPath: string,
    apps: CompanionApp[],
    launchMode: "desktop" | "vr",
  ) => Promise<void>;
  onSetHotkey: (profile: Profile, slot: number | null) => void;
  onCreateLaunchOption: (
    configPath: string,
    settingsPath: string,
    apps: CompanionApp[],
    launchMode: "desktop" | "vr",
  ) => Promise<string | null>;
  onBrowse: (
    kind: "bindings" | "settings" | "executable",
  ) => Promise<string | null>;
};

export function SettingsPage(props: Props) {
  const {
    fontScale,
    busy,
    profiles,
    configPath,
    settingsPath,
    companionApps,
    launchMode,
    onSaveFontScale,
    onSaveSettings,
    onSetHotkey,
    onCreateLaunchOption,
    onBrowse,
  } = props;
  const [draftScale, setDraftScale] = useState(fontScale);
  const [bindings, setBindings] = useState(configPath);
  const [gameSettings, setGameSettings] = useState(settingsPath);
  const [apps, setApps] = useState(companionApps);
  const [mode, setMode] = useState(launchMode);
  const [saveState, setSaveState] = useState<
    "saved" | "pending" | "saving" | "error"
  >("saved");
  const [diagnostics, setDiagnostics] = useState<DiagnosticStatus | null>(null);
  const [diagnosticsBusy, setDiagnosticsBusy] = useState(false);
  const [diagnosticsMessage, setDiagnosticsMessage] = useState<string | null>(
    null,
  );
  const saveRevision = useRef(0);
  const saveSettings = useRef(onSaveSettings);
  const saveQueue = useRef<Promise<void>>(Promise.resolve());
  const latestDraft = useRef({ bindings, gameSettings, apps, mode });
  const pendingSave = useRef(false);
  const dirty =
    bindings.trim() !== configPath ||
    gameSettings.trim() !== settingsPath ||
    JSON.stringify(apps) !== JSON.stringify(companionApps) ||
    mode !== launchMode;

  useEffect(() => setDraftScale(fontScale), [fontScale]);
  useEffect(() => {
    saveSettings.current = onSaveSettings;
  }, [onSaveSettings]);
  useEffect(() => {
    latestDraft.current = { bindings, gameSettings, apps, mode };
  }, [bindings, gameSettings, apps, mode]);
  useEffect(() => {
    if (!dirty) {
      pendingSave.current = false;
      setSaveState("saved");
      return;
    }

    const revision = ++saveRevision.current;
    pendingSave.current = true;
    setSaveState("pending");
    const timeout = window.setTimeout(() => {
      setSaveState("saving");
      const request = saveQueue.current
        .catch(() => undefined)
        .then(() => saveSettings.current(bindings, gameSettings, apps, mode));
      saveQueue.current = request;
      void request
        .then(() => {
          if (revision === saveRevision.current) {
            pendingSave.current = false;
            setSaveState("saved");
          }
        })
        .catch(() => {
          if (revision === saveRevision.current) setSaveState("error");
        });
    }, 500);

    return () => window.clearTimeout(timeout);
  }, [bindings, gameSettings, apps, mode, dirty]);
  useEffect(
    () => () => {
      const draft = latestDraft.current;
      if (pendingSave.current) {
        saveQueue.current = saveQueue.current
          .catch(() => undefined)
          .then(() =>
            saveSettings.current(
              draft.bindings,
              draft.gameSettings,
              draft.apps,
              draft.mode,
            ),
          );
      }
    },
    [],
  );

  const loadDiagnostics = async () => {
    try {
      setDiagnostics(await invoke<DiagnosticStatus>("diagnostic_status"));
    } catch (error) {
      setDiagnosticsMessage(String(error));
    }
  };
  useEffect(() => {
    void loadDiagnostics();
    const timer = window.setInterval(() => void loadDiagnostics(), 3000);
    return () => window.clearInterval(timer);
  }, []);

  const openLogs = async () => {
    setDiagnosticsBusy(true);
    setDiagnosticsMessage(null);
    try {
      await invoke("reveal_logs");
    } catch (error) {
      setDiagnosticsMessage(String(error));
    } finally {
      setDiagnosticsBusy(false);
    }
  };
  const exportDiagnostics = async () => {
    setDiagnosticsBusy(true);
    setDiagnosticsMessage(null);
    try {
      const path = await invoke<string>("export_diagnostic_report");
      setDiagnosticsMessage(`Diagnostic report saved to ${path}`);
      await invoke("reveal_logs");
      await loadDiagnostics();
    } catch (error) {
      setDiagnosticsMessage(String(error));
    } finally {
      setDiagnosticsBusy(false);
    }
  };
  const updateLogRetention = async (days: number) => {
    setDiagnosticsBusy(true);
    setDiagnosticsMessage(null);
    try {
      await invoke("set_log_retention_days", { retentionDays: days });
      await loadDiagnostics();
      setDiagnosticsMessage(
        `Daily logs will be kept for ${days} day${days === 1 ? "" : "s"}.`,
      );
    } catch (error) {
      setDiagnosticsMessage(String(error));
    } finally {
      setDiagnosticsBusy(false);
    }
  };

  const previewScale = (scale: number) => {
    const normalized = Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
    setDraftScale(normalized);
    document.documentElement.style.setProperty(
      "--font-scale",
      normalized.toString(),
    );
  };
  const updateApp = (index: number, update: Partial<CompanionApp>) =>
    setApps((current) =>
      current.map((app, i) => (i === index ? { ...app, ...update } : app)),
    );

  return (
    <section className="settings-page settings-stack">
      <section className="settings-card">
        <SettingsHeading icon={<Type />} eyebrow="Appearance" title="Text size">
          Scale every font in LMUKit without changing your Windows settings.
        </SettingsHeading>
        <div className="font-scale-control">
          <button
            type="button"
            aria-label="Decrease text size"
            disabled={busy || draftScale <= MIN_SCALE}
            onClick={() => {
              const next = Math.max(MIN_SCALE, draftScale - STEP);
              previewScale(next);
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
            value={draftScale}
            style={
              {
                "--range-progress": `${((draftScale - MIN_SCALE) / (MAX_SCALE - MIN_SCALE)) * 100}%`,
              } as CSSProperties
            }
            aria-label="Text size"
            onChange={(event) => previewScale(Number(event.target.value))}
            onPointerUp={() => onSaveFontScale(draftScale)}
            onKeyUp={() => onSaveFontScale(draftScale)}
            onBlur={() => onSaveFontScale(draftScale)}
          />
          <button
            type="button"
            aria-label="Increase text size"
            disabled={busy || draftScale >= MAX_SCALE}
            onClick={() => {
              const next = Math.min(MAX_SCALE, draftScale + STEP);
              previewScale(next);
              onSaveFontScale(next);
            }}
          >
            <Plus />
          </button>
          <strong>{Math.round(draftScale * 100)}%</strong>
        </div>
      </section>

      <section className="settings-card">
        <SettingsHeading icon={<FileJson />} eyebrow="Files" title="LMU paths">
          Choose the live binding and game-settings files LMUKit should manage.
        </SettingsHeading>
        <div className="path-fields">
          <PathField
            label="direct input.json"
            value={bindings}
            onChange={setBindings}
            onBrowse={() =>
              void onBrowse("bindings").then(
                (path) => path && setBindings(path),
              )
            }
          />
          <PathField
            label="Settings.JSON"
            value={gameSettings}
            onChange={setGameSettings}
            onBrowse={() =>
              void onBrowse("settings").then(
                (path) => path && setGameSettings(path),
              )
            }
          />
        </div>
      </section>

      <section className="settings-card">
        <SettingsHeading eyebrow="Shortcuts" title="Profile keyboard shortcuts">
          Ctrl+Alt+Space opens LMUKit. Assign Ctrl+Alt+1–9 for direct profile
          access.
        </SettingsHeading>
        <div className="shortcut-list">
          {profiles.map((profile) => (
            <label key={profile.id}>
              <span>{profile.name}</span>
              <select
                value={profile.hotkeySlot ?? ""}
                onChange={(event) =>
                  onSetHotkey(
                    profile,
                    event.target.value ? Number(event.target.value) : null,
                  )
                }
              >
                <option value="">No shortcut</option>
                {Array.from({ length: 9 }, (_, index) => index + 1).map(
                  (slot) => (
                    <option value={slot} key={slot}>
                      Ctrl Alt {slot}
                    </option>
                  ),
                )}
              </select>
            </label>
          ))}
          {!profiles.length && (
            <span className="settings-empty">
              Create a profile before assigning shortcuts.
            </span>
          )}
        </div>
      </section>

      <section className="settings-card">
        <SettingsHeading eyebrow="Launch setup" title="LMU and companion apps">
          Generate one Steam launch option for LMUKit, your launch mode, and
          selected helpers.
        </SettingsHeading>
        <div className="launch-mode">
          <button
            className={mode === "desktop" ? "active" : ""}
            onClick={() => setMode("desktop")}
          >
            Desktop
          </button>
          <button
            className={mode === "vr" ? "active" : ""}
            onClick={() => setMode("vr")}
          >
            VR (OpenXR)
          </button>
        </div>
        <div className="companion-list">
          {apps.map((app, index) => (
            <div className="companion-row" key={index}>
              <input
                type="checkbox"
                checked={app.enabled}
                onChange={(event) =>
                  updateApp(index, { enabled: event.target.checked })
                }
                aria-label={`Enable ${app.name}`}
              />
              <input
                value={app.name}
                onChange={(event) =>
                  updateApp(index, { name: event.target.value })
                }
              />
              <input
                value={app.path}
                placeholder="Executable path"
                onChange={(event) =>
                  updateApp(index, { path: event.target.value })
                }
              />
              <button
                type="button"
                onClick={() =>
                  void onBrowse("executable").then(
                    (path) => path && updateApp(index, { path, enabled: true }),
                  )
                }
              >
                Browse
              </button>
              <button
                type="button"
                className="icon-button"
                aria-label={`Remove ${app.name}`}
                onClick={() =>
                  setApps((current) => current.filter((_, i) => i !== index))
                }
              >
                <Trash2 />
              </button>
            </div>
          ))}
        </div>
        <div className="settings-actions split-actions">
          <button
            type="button"
            className="secondary"
            onClick={() =>
              setApps((current) => [
                ...current,
                { name: "Companion app", path: "", enabled: true },
              ])
            }
          >
            <PlusCircle /> Add app
          </button>
          <div>
            <span className={`autosave-state ${saveState}`}>
              {saveState === "saved" && <Check />}
              {(saveState === "pending" || saveState === "saving") && (
                <LoaderCircle className="spin" />
              )}
              {saveState === "error" && <AlertCircle />}
              {saveState === "saved"
                ? "Saved automatically"
                : saveState === "pending"
                  ? "Waiting to save…"
                  : saveState === "saving"
                    ? "Saving…"
                    : "Could not save"}
            </span>
            <button
              type="button"
              className="primary"
              disabled={busy}
              onClick={() =>
                void onCreateLaunchOption(
                  bindings,
                  gameSettings,
                  apps,
                  mode,
                ).then(async (option) => {
                  if (option) await navigator.clipboard.writeText(option);
                })
              }
            >
              <Copy /> Copy Steam launch option
            </button>
          </div>
        </div>
      </section>

      <section className="settings-card diagnostics-card">
        <SettingsHeading
          icon={<Activity />}
          eyebrow="Support"
          title="Diagnostics and logs"
        >
          Check virtual-input health and create a privacy-redacted report when
          something goes wrong.
        </SettingsHeading>
        {diagnostics ? (
          <>
            <div className="diagnostic-grid">
              <span>
                <small>Virtual input</small>
                <strong className={diagnostics.routing.error ? "warning" : ""}>
                  {diagnostics.virtualHealth}
                </strong>
              </span>
              <span>
                <small>Active profile</small>
                <strong>{diagnostics.activeProfile ?? "None"}</strong>
              </span>
              <span>
                <small>Input mode</small>
                <strong>{diagnostics.inputMode}</strong>
              </span>
              <span>
                <small>Controllers</small>
                <strong>{diagnostics.devices.length} detected</strong>
              </span>
            </div>
            <div className="diagnostic-log-row">
              <span>
                <small>Log storage</small>
                <strong>
                  {formatBytes(diagnostics.currentLogSize)} ·{" "}
                  {diagnostics.retainedLogFiles} file
                  {diagnostics.retainedLogFiles === 1 ? "" : "s"}
                </strong>
                <code>{diagnostics.logDirectory}</code>
              </span>
              <div>
                <button
                  type="button"
                  className="secondary"
                  disabled={diagnosticsBusy}
                  onClick={() => void loadDiagnostics()}
                >
                  <RefreshCw /> Refresh
                </button>
                <button
                  type="button"
                  className="secondary"
                  disabled={diagnosticsBusy}
                  onClick={() => void openLogs()}
                >
                  <FolderOpen /> Open logs folder
                </button>
                <button
                  type="button"
                  className="primary"
                  disabled={diagnosticsBusy}
                  onClick={() => void exportDiagnostics()}
                >
                  {diagnosticsBusy ? (
                    <LoaderCircle className="spin" />
                  ) : (
                    <Download />
                  )}
                  Export report
                </button>
              </div>
            </div>
            <div className="diagnostic-retention">
              <span>
                <small>Log retention</small>
                <strong>Keep daily logs for</strong>
              </span>
              <div>
                <button
                  type="button"
                  aria-label="Keep logs for fewer days"
                  disabled={
                    diagnosticsBusy || diagnostics.logRetentionDays <= 1
                  }
                  onClick={() =>
                    void updateLogRetention(diagnostics.logRetentionDays - 1)
                  }
                >
                  <Minus />
                </button>
                <strong>{diagnostics.logRetentionDays} days</strong>
                <button
                  type="button"
                  aria-label="Keep logs for more days"
                  disabled={
                    diagnosticsBusy || diagnostics.logRetentionDays >= 30
                  }
                  onClick={() =>
                    void updateLogRetention(diagnostics.logRetentionDays + 1)
                  }
                >
                  <Plus />
                </button>
              </div>
            </div>
            {!!diagnostics.devices.length && (
              <details className="diagnostic-devices">
                <summary>
                  Attached DirectInput controllers
                  <span>{diagnostics.devices.length}</span>
                </summary>
                <div>
                  {diagnostics.devices.map((device, index) => (
                    <span key={`${device.name}-${device.productName}-${index}`}>
                      <strong>{device.productName || device.name}</strong>
                      <small>{device.buttonCount} buttons</small>
                    </span>
                  ))}
                </div>
              </details>
            )}
            {!!diagnostics.recentErrors.length && (
              <div className="diagnostic-errors">
                <strong>Recent warnings and errors</strong>
                {diagnostics.recentErrors.map((error, index) => (
                  <span key={`${index}-${error}`}>{error}</span>
                ))}
              </div>
            )}
          </>
        ) : (
          <div className="diagnostic-loading">
            <LoaderCircle className="spin" /> Reading diagnostic status…
          </div>
        )}
        {diagnosticsMessage && (
          <div className="diagnostic-message">{diagnosticsMessage}</div>
        )}
      </section>
    </section>
  );
}

function PathField({
  label,
  value,
  onChange,
  onBrowse,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  onBrowse: () => void;
}) {
  return (
    <label className="identity-field">
      <span>{label}</span>
      <div className="path-input">
        <input
          value={value}
          onChange={(event) => onChange(event.target.value)}
        />
        <button type="button" onClick={onBrowse}>
          Browse
        </button>
      </div>
    </label>
  );
}

function SettingsHeading({
  icon,
  eyebrow,
  title,
  children,
}: {
  icon?: ReactNode;
  eyebrow: string;
  title: string;
  children: ReactNode;
}) {
  return (
    <header className={icon ? "" : "no-icon"}>
      {icon && <span className="settings-icon">{icon}</span>}
      <div>
        <span className="eyebrow">{eyebrow}</span>
        <h2>{title}</h2>
        <p>{children}</p>
      </div>
    </header>
  );
}
