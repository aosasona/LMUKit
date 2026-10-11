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
  VirtualAction,
  VirtualBindingDevice,
  VirtualButtonPulse,
  VirtualSetupEvent,
  VirtualSetupStatus,
} from "../models";

export function VirtualInputPage({
  busy,
  onNotice,
}: {
  busy: boolean;
  onNotice: (message: string) => void;
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

  const scan = useCallback(async () => {
    setLoading(true);
    setError(null);
    setSaved(false);
    try {
      const next = await invoke<VirtualBindingDevice[]>(
        "virtual_binding_devices",
      );
      setDevices(next);
      const preferred =
        next.find((device) => device.likelyVirtual)?.key ?? next[0]?.key ?? "";
      setSelectedKey(preferred);
      if (preferred) {
        setActions(
          await invoke<VirtualAction[]>("preview_virtual_layout", {
            deviceKey: preferred,
          }),
        );
      } else {
        setActions([]);
      }
    } catch (reason) {
      setDevices([]);
      setActions([]);
      setSelectedKey("");
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void scan();
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
      onNotice("Binding setup ready. Ctrl+Alt+V sends the next vJoy button.");
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
        await scan();
        onNotice("Binding setup finished. LMU bindings rescanned.");
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
        `${imported.length} named virtual actions imported. Virtual routing is still disabled.`,
      );
    } catch (reason) {
      setError(String(reason));
      onNotice(String(reason));
    } finally {
      setImporting(false);
    }
  };

  return (
    <section className="virtual-input-page">
      <div className="virtual-intro panel">
        <div>
          <span className="eyebrow">Experimental · setup step 1</span>
          <h2>Name the virtual controls from LMU</h2>
          <p>
            LMUKit reads the vJoy bindings LMU has already saved and turns B1,
            B2, and the rest into named actions. This scan never edits LMU.
          </p>
          <p>
            Virtual Input Mode requires vJoy to be installed and configured
            separately. LMUKit does not bundle, install, or update the driver.
          </p>
        </div>
        <div className="virtual-safety">
          <AlertTriangle />
          <span>Steering, pedals, and FFB stay directly connected to LMU.</span>
        </div>
      </div>

      <section className="virtual-wizard panel">
        <div className="virtual-wizard-copy">
          <span className="eyebrow">Guided LMU binding</span>
          <h3>Assign the virtual buttons once</h3>
          <ol>
            <li>Start setup, then open LMU&apos;s control assignments.</li>
            <li>Select an LMU action so it is listening for input.</li>
            <li>
              Press <kbd>Ctrl Alt V</kbd>; LMUKit sends the next unused vJoy
              button.
            </li>
            <li>Repeat, save in LMU, then finish and rescan here.</li>
          </ol>
          <p>
            If LMU captures the keyboard shortcut instead, use the on-screen
            send button while its assignment dialog is still listening.
          </p>
        </div>
        <div className="virtual-wizard-control">
          {setupError && <div className="virtual-error">{setupError}</div>}
          {setup ? (
            <>
              <div className="virtual-wizard-status">
                <Radio />
                <span>
                  <strong>{setup.identity}</strong>
                  <small>
                    {setup.buttonCount} configured buttons ·{" "}
                    {setup.usedButtonCount} reserved or already bound
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
                  <strong>vJoy setup is stopped</strong>
                  <small>
                    Starting checks device 1 and enables Ctrl+Alt+V only for
                    this setup session.
                  </small>
                </span>
              </div>
              <button
                type="button"
                className="primary virtual-start-button"
                disabled={busy || setupBusy}
                onClick={() => void startSetup()}
              >
                {setupBusy && <LoaderCircle className="spin" />}
                Start binding setup
              </button>
            </>
          )}
        </div>
      </section>

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
              onClick={() => void scan()}
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
                "Review the named actions before importing."
              )}
            </span>
            <button
              type="button"
              className="primary"
              disabled={busy || loading || importing || !actions.length}
              onClick={() => void importLayout()}
            >
              {importing && <LoaderCircle className="spin" />}
              Use this layout
            </button>
          </div>
        </section>
      </div>
    </section>
  );
}
