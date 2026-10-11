import { invoke } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  Check,
  Gamepad2,
  LoaderCircle,
  RefreshCw,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type { VirtualAction, VirtualBindingDevice } from "../models";

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
        </div>
        <div className="virtual-safety">
          <AlertTriangle />
          <span>Steering, pedals, and FFB stay directly connected to LMU.</span>
        </div>
      </div>

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
