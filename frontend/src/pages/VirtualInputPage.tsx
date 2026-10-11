import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  Check,
  Gamepad2,
  LoaderCircle,
  Play,
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
  VirtualProfileMapping,
  VirtualRoutingStatus,
  VirtualSetupEvent,
  VirtualSetupStatus,
} from "../models";

export function VirtualInputPage({
  busy,
  profiles,
  activeProfileId,
  onNotice,
}: {
  busy: boolean;
  profiles: Profile[];
  activeProfileId: string | null;
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
  const [selectedProfileId, setSelectedProfileId] = useState(
    activeProfileId ?? profiles[0]?.id ?? "",
  );
  const [profileMappings, setProfileMappings] = useState<
    VirtualProfileMapping[]
  >([]);
  const [routingBusy, setRoutingBusy] = useState(false);
  const [routing, setRouting] = useState<VirtualRoutingStatus>({
    active: false,
    profileId: null,
    profileName: null,
    identity: null,
    routedEventCount: 0,
    lastEvent: null,
    error: null,
  });

  useEffect(() => {
    if (!selectedProfileId && profiles.length) {
      setSelectedProfileId(activeProfileId ?? profiles[0].id);
    }
  }, [activeProfileId, profiles, selectedProfileId]);

  useEffect(() => {
    if (!selectedProfileId) {
      setProfileMappings([]);
      return;
    }
    void invoke<VirtualProfileMapping[]>("virtual_profile_mapping", {
      profileId: selectedProfileId,
    })
      .then(setProfileMappings)
      .catch((reason) => setError(String(reason)));
  }, [selectedProfileId]);

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
        `${imported.length} named LMU actions saved for profile mapping.`,
      );
    } catch (reason) {
      setError(String(reason));
      onNotice(String(reason));
    } finally {
      setImporting(false);
    }
  };

  const startRouting = async () => {
    if (!selectedProfileId) return;
    setRoutingBusy(true);
    setError(null);
    try {
      const next = await invoke<VirtualRoutingStatus>("start_virtual_routing", {
        profileId: selectedProfileId,
      });
      setRouting(next);
      onNotice(`Wheel remapping enabled for ${next.profileName}.`);
    } catch (reason) {
      setError(String(reason));
      onNotice(String(reason));
    } finally {
      setRoutingBusy(false);
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
        <div className="virtual-router-copy">
          <span className="eyebrow">Live wheel remapping</span>
          <h2>
            {routing.active
              ? "Wheel remapping is on"
              : "Choose the profile you are using"}
          </h2>
          <p>
            {routing.active
              ? `${routing.profileName} is sending its assigned buttons to LMU.`
              : "LMUKit must remain open while remapping is enabled."}
          </p>
        </div>
        <label>
          <span>Current profile</span>
          <select
            value={selectedProfileId}
            disabled={routing.active || routingBusy || !profiles.length}
            onChange={(event) => setSelectedProfileId(event.target.value)}
          >
            {profiles.map((profile) => (
              <option key={profile.id} value={profile.id}>
                {profile.name}
              </option>
            ))}
          </select>
        </label>
        <div className="virtual-router-health">
          <i className={routing.active ? "active" : ""} />
          <span>
            <strong>{routing.active ? "Remapping on" : "Remapping off"}</strong>
            <small>
              {routing.lastEvent ??
                `${profileMappings.length} physical mappings`}
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
            disabled={
              routingBusy ||
              !selectedProfileId ||
              !profileMappings.length ||
              setup !== null
            }
            onClick={() => void startRouting()}
          >
            {routingBusy ? <LoaderCircle className="spin" /> : <Play />}
            Enable remapping
          </button>
        )}
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
                Save LMU button layout
              </button>
            </div>
          </section>
        </div>
      </details>
    </section>
  );
}
