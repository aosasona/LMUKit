import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  Check,
  Gamepad2,
  LoaderCircle,
  Radio,
  RefreshCw,
  Send,
  Square,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type {
  Profile,
  VirtualAction,
  VirtualBindingDevice,
  VirtualButtonPulse,
  VirtualRoutingStatus,
  VirtualSetupEvent,
  VirtualSetupStatus,
  Wheel,
} from "../models";
import type { InputMode } from "../components/InputModeSwitch";

const WHEEL_FALLBACK = "__wheel_fallback__";
const RECOMMENDED_VJOY_BUTTON_COUNT = 128;

export function VirtualInputPage({
  busy,
  inputMode,
  profiles,
  wheels,
  onNotice,
  onOpenProfiles,
}: {
  busy: boolean;
  inputMode: InputMode;
  profiles: Profile[];
  wheels: Wheel[];
  onNotice: (message: string) => void;
  onOpenProfiles: () => void;
}) {
  const [devices, setDevices] = useState<VirtualBindingDevice[]>([]);
  const [selectedKey, setSelectedKey] = useState("");
  const [actions, setActions] = useState<VirtualAction[]>([]);
  const [loading, setLoading] = useState(true);
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [setup, setSetup] = useState<VirtualSetupStatus | null>(null);
  const [setupBusy, setSetupBusy] = useState(false);
  const [setupError, setSetupError] = useState<string | null>(null);
  const [lastPulse, setLastPulse] = useState<VirtualButtonPulse | null>(null);
  const [routingBusy, setRoutingBusy] = useState(false);
  const [selectedWheelId, setSelectedWheelId] = useState("");
  const [selectedClass, setSelectedClass] = useState(WHEEL_FALLBACK);
  const [routing, setRouting] = useState<VirtualRoutingStatus>({
    active: false,
    profileId: null,
    profileName: null,
    identity: null,
    routedEventCount: 0,
    lastEvent: null,
    error: null,
    selectionReason: null,
    activeEncoderModes: [],
  });
  const profilesForWheel = profiles.filter(
    (profile) => profile.wheelId === selectedWheelId,
  );
  const availableClasses = Array.from(
    new Set(profilesForWheel.flatMap((profile) => profile.classTags)),
  ).sort();
  const availableClassesKey = availableClasses.join("\u0000");

  useEffect(() => {
    if (!wheels.length) {
      setSelectedWheelId("");
      return;
    }
    if (!wheels.some((wheel) => wheel.id === selectedWheelId)) {
      setSelectedWheelId(wheels[0].id);
    }
  }, [selectedWheelId, wheels]);

  useEffect(() => {
    if (
      selectedClass !== WHEEL_FALLBACK &&
      !availableClasses.includes(selectedClass)
    ) {
      setSelectedClass(WHEEL_FALLBACK);
    }
  }, [availableClassesKey, selectedClass]);

  useEffect(() => {
    const refresh = () => {
      void invoke<VirtualRoutingStatus>("virtual_routing_status")
        .then(setRouting)
        .catch(() => undefined);
    };
    refresh();
    const timer = window.setInterval(refresh, 500);
    return () => window.clearInterval(timer);
  }, []);

  const recordPulse = useCallback(
    (pulse: VirtualButtonPulse, announce = true) => {
      setLastPulse(pulse);
      setSetup((current) =>
        current
          ? {
              ...current,
              usedButtonCount: current.usedButtonCount + 1,
              nextVirtualButton: pulse.nextVirtualButton,
            }
          : current,
      );
      setSetupError(null);
      if (announce) {
        onNotice(`Sent vJoy B${pulse.virtualButton} to LMU.`);
      }
    },
    [onNotice],
  );

  const preview = useCallback(async (deviceKey: string) => {
    if (!deviceKey) {
      setActions([]);
      return;
    }
    setLoading(true);
    setError(null);
    setSaved(false);
    try {
      setActions(
        await invoke<VirtualAction[]>("preview_virtual_layout", { deviceKey }),
      );
    } catch (reason) {
      setActions([]);
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  const scan = useCallback(
    async (autoSave = false): Promise<"saved" | "review" | "failed"> => {
      setLoading(true);
      setError(null);
      setSaved(false);
      try {
        const next = await invoke<VirtualBindingDevice[]>(
          "virtual_binding_devices",
        );
        setDevices(next);
        const candidates = next.filter(
          (device) => device.likelyVirtual && device.buttonBindingCount > 0,
        );
        const preferred =
          (autoSave && candidates.length === 1
            ? candidates[0].key
            : next.find((device) => device.likelyVirtual)?.key) ??
          next[0]?.key ??
          "";
        setSelectedKey(preferred);
        if (autoSave && candidates.length !== 1) {
          setActions([]);
          const reason = candidates.length
            ? "LMU contains more than one vJoy device with bindings. Select the current 128-button device below and save it once."
            : "No named vJoy button bindings were found after the rescan.";
          setError(reason);
          onNotice(reason);
          return "review";
        }
        if (preferred) {
          const previewed = await invoke<VirtualAction[]>(
            "preview_virtual_layout",
            {
              deviceKey: preferred,
            },
          );
          setActions(previewed);
        } else {
          setActions([]);
        }
        if (!autoSave) return "review";

        const deviceKey = candidates[0].key;
        const imported = await invoke<VirtualAction[]>(
          "import_virtual_layout",
          {
            deviceKey,
          },
        );
        setSelectedKey(deviceKey);
        setActions(imported);
        setSaved(true);
        onNotice(
          `${imported.length} named LMU actions rescanned and saved automatically.`,
        );
        return "saved";
      } catch (reason) {
        setDevices([]);
        setActions([]);
        setSelectedKey("");
        setError(String(reason));
        if (autoSave) onNotice(String(reason));
        return "failed";
      } finally {
        setLoading(false);
      }
    },
    [onNotice],
  );

  useEffect(() => {
    void scan(false);
  }, [scan]);

  useEffect(() => {
    const result = listen<VirtualSetupEvent>(
      "virtual-setup-result",
      (event) => {
        if (event.payload.pulse) {
          recordPulse(event.payload.pulse);
        } else if (event.payload.error) {
          setSetupError(event.payload.error);
          onNotice(event.payload.error);
        }
      },
    );
    return () => {
      void result.then((unlisten) => unlisten());
      void invoke("stop_virtual_binding_setup");
    };
  }, [onNotice, recordPulse]);

  const startSetup = async () => {
    setSetupBusy(true);
    setSetupError(null);
    setLastPulse(null);
    try {
      const status = await invoke<VirtualSetupStatus>(
        "start_virtual_binding_setup",
      );
      setSetup(status);
      if (status.buttonCount < RECOMMENDED_VJOY_BUTTON_COUNT) {
        onNotice(
          `vJoy device 1 exposes only ${status.buttonCount} buttons. Configure it for ${RECOMMENDED_VJOY_BUTTON_COUNT} before assigning LMU actions.`,
        );
      } else {
        onNotice("Binding setup ready. Ctrl+Alt+V sends the next vJoy button.");
      }
    } catch (reason) {
      setSetupError(String(reason));
      onNotice(String(reason));
    } finally {
      setSetupBusy(false);
    }
  };

  const stopSetup = async (rescan: boolean) => {
    setSetupBusy(true);
    try {
      await invoke("stop_virtual_binding_setup");
      setSetup(null);
      if (rescan) {
        await scan(true);
      } else {
        onNotice("Binding setup stopped.");
      }
    } catch (reason) {
      setSetupError(String(reason));
      onNotice(String(reason));
    } finally {
      setSetupBusy(false);
    }
  };

  const sendNextButton = async () => {
    setSetupBusy(true);
    setSetupError(null);
    try {
      recordPulse(await invoke<VirtualButtonPulse>("send_next_virtual_button"));
    } catch (reason) {
      setSetupError(String(reason));
      onNotice(String(reason));
    } finally {
      setSetupBusy(false);
    }
  };

  const importLayout = async () => {
    if (!selectedKey || !actions.length) return;
    setImporting(true);
    setError(null);
    try {
      const imported = await invoke<VirtualAction[]>("import_virtual_layout", {
        deviceKey: selectedKey,
      });
      setActions(imported);
      setSaved(true);
      onNotice(
        `${imported.length} named LMU actions saved for profile mapping.`,
      );
    } catch (reason) {
      setError(String(reason));
      onNotice(String(reason));
    } finally {
      setImporting(false);
    }
  };

  const stopRouting = async () => {
    setRoutingBusy(true);
    try {
      const next = await invoke<VirtualRoutingStatus>("stop_virtual_routing");
      setRouting(next);
      onNotice("Wheel remapping disabled and all buttons released.");
    } catch (reason) {
      setError(String(reason));
      onNotice(String(reason));
    } finally {
      setRoutingBusy(false);
    }
  };

  const startDefaultRouting = async () => {
    if (!selectedWheelId) return;
    const classTag = selectedClass === WHEEL_FALLBACK ? null : selectedClass;
    setRoutingBusy(true);
    setError(null);
    try {
      const next = await invoke<VirtualRoutingStatus>(
        "start_default_virtual_routing",
        { wheelId: selectedWheelId, classTag },
      );
      setRouting(next);
      onNotice(
        `${next.profileName} selected from the ${classTag ?? "wheel fallback"}.`,
      );
    } catch (reason) {
      setError(String(reason));
      onNotice(String(reason));
    } finally {
      setRoutingBusy(false);
    }
  };

  return (
    <section className="virtual-input-page">
      <div className="virtual-mode-banner panel">
        <Gamepad2 />
        <div>
          <span className="eyebrow">Experimental · button-only</span>
          <strong>Use different wheel buttons for the same LMU actions</strong>
          <p>
            Assign buttons from each profile in Profiles, then enable its live
            remapping here while LMUKit is open.
          </p>
          {!!routing.activeEncoderModes.length && (
            <div className="active-encoder-modes">
              {routing.activeEncoderModes.map((mode) => (
                <span key={mode.encoderName}>
                  {mode.encoderName} <strong>{mode.modeName}</strong>
                </span>
              ))}
            </div>
          )}
        </div>
        <div className="virtual-mode-guard">
          <AlertTriangle />
          <span>Steering, pedals, and FFB always remain direct.</span>
        </div>
      </div>

      <details className="virtual-setup-details panel">
        <summary>
          <span>
            <strong>One-time LMU button setup</strong>
            <small>Configure or change the vJoy buttons LMU receives</small>
          </span>
          <span>Open setup</span>
        </summary>
        <section className="virtual-wizard">
          <header className="virtual-wizard-header">
            <div>
              <span className="eyebrow">One-time setup</span>
              <h2>Teach LMU the virtual buttons</h2>
              <p>LMUKit advances through unused buttons and never edits LMU.</p>
            </div>
            <span className={setup ? "active" : ""}>
              <i /> {setup ? "Setup active" : "Setup stopped"}
            </span>
          </header>

          <div className="virtual-wizard-steps">
            <div>
              <b>1</b>
              <span>
                <strong>Listen in LMU</strong>
                <small>Choose the action you want to bind.</small>
              </span>
            </div>
            <div>
              <b>2</b>
              <span>
                <strong>Send the next button</strong>
                <small>
                  Press <kbd>Ctrl Alt V</kbd> or use Send below.
                </small>
              </span>
            </div>
            <div>
              <b>3</b>
              <span>
                <strong>Save and rescan</strong>
                <small>LMUKit names only bindings LMU actually saved.</small>
              </span>
            </div>
          </div>

          <div className="virtual-wizard-console">
            {setupError && <div className="virtual-error">{setupError}</div>}
            {setup ? (
              <>
                {setup.buttonCount < RECOMMENDED_VJOY_BUTTON_COUNT && (
                  <div className="virtual-capacity-warning">
                    <AlertTriangle />
                    <span>
                      <strong>
                        Device 1 has only {setup.buttonCount} virtual buttons
                      </strong>
                      <small>
                        Close LMU and LMUKit, open Configure vJoy, set Buttons
                        to
                        {` ${RECOMMENDED_VJOY_BUTTON_COUNT}`}, apply the change,
                        then reopen both apps. Windows may re-enumerate the
                        controller after its capacity changes, so existing LMU
                        vJoy assignments may need to be registered again once.
                      </small>
                    </span>
                  </div>
                )}
                <div className="virtual-wizard-status">
                  <Radio />
                  <span>
                    <strong>{setup.identity}</strong>
                    <small>
                      {setup.usedButtonCount} of {setup.buttonCount} buttons
                      reserved or already bound
                    </small>
                  </span>
                </div>
                <div className="virtual-next-button">
                  <span>Next output</span>
                  <strong>
                    {setup.nextVirtualButton
                      ? `B${setup.nextVirtualButton}`
                      : "None available"}
                  </strong>
                  {lastPulse && (
                    <small>Last sent: B{lastPulse.virtualButton}</small>
                  )}
                </div>
                <div className="virtual-wizard-actions">
                  <button
                    type="button"
                    className="primary"
                    disabled={
                      busy || setupBusy || setup.nextVirtualButton === null
                    }
                    onClick={() => void sendNextButton()}
                  >
                    {setupBusy ? <LoaderCircle className="spin" /> : <Send />}
                    Send next button
                  </button>
                  <button
                    type="button"
                    className="secondary"
                    disabled={busy || setupBusy}
                    onClick={() => void stopSetup(true)}
                  >
                    <Check /> Finish &amp; rescan
                  </button>
                  <button
                    type="button"
                    className="ghost"
                    disabled={busy || setupBusy}
                    onClick={() => void stopSetup(false)}
                  >
                    <Square /> Stop
                  </button>
                </div>
              </>
            ) : (
              <>
                <div className="virtual-wizard-status idle">
                  <Gamepad2 />
                  <span>
                    <strong>Ready to check vJoy device 1</strong>
                    <small>
                      The global shortcut is enabled only while setup is active.
                    </small>
                  </span>
                </div>
                <button
                  type="button"
                  className="primary virtual-start-button"
                  disabled={busy || setupBusy || routing.active}
                  onClick={() => void startSetup()}
                >
                  {setupBusy && <LoaderCircle className="spin" />}
                  Start binding setup
                </button>
              </>
            )}
          </div>
        </section>
      </details>

      <section className="virtual-router panel">
        {error && (
          <div className="virtual-error virtual-router-error">{error}</div>
        )}
        <div className="virtual-router-copy">
          <span className="eyebrow">Live wheel remapping</span>
          <h2>
            {routing.active
              ? routing.profileName
              : "Choose a profile from your garage"}
          </h2>
          <p>
            {routing.active
              ? `${routing.selectionReason ?? "Selected manually"}. Its assigned buttons are being sent to LMU.`
              : inputMode === "virtual"
                ? "Select and configure a profile in Profiles, then start it from the same detail pane."
                : "Select Virtual Input in the global input-method switch before starting live remapping."}
          </p>
        </div>
        <div className="virtual-router-health">
          <i className={routing.active ? "active" : ""} />
          <span>
            <strong>{routing.active ? "Remapping on" : "Remapping off"}</strong>
            <small>
              {routing.selectionReason ??
                routing.lastEvent ??
                "No profile is currently being routed"}
            </small>
          </span>
        </div>
        {routing.active ? (
          <button
            type="button"
            className="secondary"
            disabled={routingBusy}
            onClick={() => void stopRouting()}
          >
            <Square /> Disable remapping
          </button>
        ) : (
          <button
            type="button"
            className="primary"
            disabled={setup !== null}
            onClick={onOpenProfiles}
          >
            <Gamepad2 /> Open profiles
          </button>
        )}
        <div className="virtual-default-switcher">
          <span>
            <strong>Switch by wheel and class</strong>
            <small>Uses the defaults configured on your profiles.</small>
          </span>
          <label>
            <span>Wheel</span>
            <select
              value={selectedWheelId}
              onChange={(event) => setSelectedWheelId(event.target.value)}
            >
              {wheels.map((wheel) => (
                <option value={wheel.id} key={wheel.id}>
                  {[wheel.brand, wheel.name].filter(Boolean).join(" ")}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>Class</span>
            <select
              value={selectedClass}
              onChange={(event) => setSelectedClass(event.target.value)}
            >
              <option value={WHEEL_FALLBACK}>Generic / other</option>
              {availableClasses.map((classTag) => (
                <option value={classTag} key={classTag}>
                  {classTag}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            className="primary"
            disabled={
              busy ||
              routingBusy ||
              setup !== null ||
              !selectedWheelId ||
              !selectedClass ||
              inputMode !== "virtual"
            }
            onClick={() => void startDefaultRouting()}
          >
            {routingBusy ? <LoaderCircle className="spin" /> : <RefreshCw />}
            {routing.active ? "Switch profile" : "Use class default"}
          </button>
        </div>
      </section>

      <details className="virtual-layout-details panel">
        <summary>
          <span>
            <strong>LMU virtual-button details</strong>
            <small>Review the vJoy device and saved LMU action layout</small>
          </span>
          <span>Show details</span>
        </summary>
        <div className="virtual-setup-grid">
          <section className="virtual-device-panel panel">
            <div className="section-heading">
              <div>
                <span className="eyebrow">LMU devices</span>
                <h3>Select the virtual controller</h3>
              </div>
              <button
                type="button"
                className="icon-button"
                aria-label="Rescan LMU bindings"
                disabled={loading || busy}
                onClick={() => void scan(true)}
              >
                <RefreshCw className={loading ? "spin" : ""} />
              </button>
            </div>
            <div className="virtual-device-list">
              {devices.map((device) => (
                <button
                  type="button"
                  className={device.key === selectedKey ? "active" : ""}
                  key={device.key}
                  onClick={() => {
                    setSelectedKey(device.key);
                    void preview(device.key);
                  }}
                >
                  <Gamepad2 />
                  <span>
                    <strong>{device.name}</strong>
                    <small>
                      {device.buttonBindingCount} virtual button bindings
                    </small>
                  </span>
                  {device.likelyVirtual && <em>Likely vJoy</em>}
                </button>
              ))}
              {!loading && !devices.length && (
                <p className="virtual-empty">
                  No devices were found in LMU&apos;s saved direct input.json.
                </p>
              )}
            </div>
          </section>

          <section className="virtual-actions-panel panel">
            <div className="section-heading">
              <div>
                <span className="eyebrow">Shared layout</span>
                <h3>Actions LMU will receive</h3>
              </div>
              <strong>{actions.length}</strong>
            </div>
            {error && <div className="virtual-error">{error}</div>}
            {loading ? (
              <div className="virtual-loading">
                <LoaderCircle className="spin" /> Reading LMU bindings…
              </div>
            ) : (
              <div className="virtual-action-list">
                {actions.map((action) => (
                  <div key={`${action.virtualButton}-${action.action}`}>
                    <span>{action.action}</span>
                    <code>B{action.virtualButton}</code>
                  </div>
                ))}
                {!actions.length && !error && (
                  <p className="virtual-empty">
                    Bind some buttons from this device in LMU, let LMU save the
                    file, then rescan.
                  </p>
                )}
              </div>
            )}
            <div className="virtual-import-bar">
              <span>
                {saved ? (
                  <>
                    <Check /> Layout saved
                  </>
                ) : (
                  "Rescans save automatically when the vJoy device is unambiguous."
                )}
              </span>
              <button
                type="button"
                className="primary"
                disabled={
                  busy ||
                  loading ||
                  importing ||
                  routing.active ||
                  !actions.length
                }
                onClick={() => void importLayout()}
              >
                {importing && <LoaderCircle className="spin" />}
                Save selected layout
              </button>
            </div>
          </section>
        </div>
      </details>
    </section>
  );
}
