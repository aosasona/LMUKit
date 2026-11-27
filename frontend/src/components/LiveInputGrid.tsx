import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Gamepad2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { LiveInputDevice, LiveInputEvent } from "../models";

export function LiveInputGrid() {
  const monitorId = useRef(
    Date.now() * 1000 + Math.floor(Math.random() * 1000),
  );
  const [devices, setDevices] = useState<LiveInputDevice[]>([]);
  const [pressed, setPressed] = useState<Record<string, number[]>>({});
  const [latest, setLatest] = useState<LiveInputEvent | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    const subscriptions = Promise.all([
      listen<LiveInputEvent>("live-input-event", ({ payload }) => {
        if (!mounted) return;
        setLatest(payload);
        setPressed((current) => {
          const buttons = new Set(current[payload.deviceId] ?? []);
          if (payload.pressed) buttons.add(payload.buttonId);
          else buttons.delete(payload.buttonId);
          return { ...current, [payload.deviceId]: [...buttons] };
        });
      }),
      listen<LiveInputDevice[]>("live-input-devices", ({ payload }) => {
        if (mounted) setDevices(payload);
      }),
      listen<string>("live-input-error", ({ payload }) => {
        if (mounted) setError(payload);
      }),
    ]);

    void subscriptions.then(() =>
      invoke<LiveInputDevice[]>("start_live_input_monitor", {
        monitorId: monitorId.current,
      })
        .then((value) => {
          if (mounted) setDevices(value);
        })
        .catch((reason) => {
          if (mounted) setError(String(reason));
        }),
    );

    return () => {
      mounted = false;
      void subscriptions.then((unlisten) =>
        unlisten.forEach((remove) => remove()),
      );
      void invoke("stop_live_input_monitor", {
        monitorId: monitorId.current,
      });
    };
  }, []);

  return (
    <aside className="live-input-rail" aria-label="Live controller inputs">
      <header>
        <span>
          <Gamepad2 />
          <strong>Live inputs</strong>
        </span>
        <small>Always listening</small>
      </header>
      {error && <p className="live-input-error">{error}</p>}
      {!error && !devices.length && (
        <div className="live-input-empty">
          <Gamepad2 />
          <span>
            <strong>No controller found</strong>
            <small>Connect a DirectInput wheel or controller.</small>
          </span>
        </div>
      )}
      <div className="live-input-devices">
        {devices.map((device) => {
          const held = new Set(pressed[device.deviceId] ?? []);
          const buttonCount = Math.max(
            device.buttonCount,
            latest?.deviceId === device.deviceId ? latest.buttonId : 0,
          );
          return (
            <section key={device.deviceId}>
              <div className="live-input-device-heading">
                <strong>{device.deviceName}</strong>
                <small>{buttonCount} buttons</small>
              </div>
              <div className="live-input-button-grid">
                {Array.from(
                  { length: buttonCount },
                  (_, index) => index + 1,
                ).map((button) => (
                  <span
                    className={[
                      held.has(button) ? "pressed" : "",
                      latest?.deviceId === device.deviceId &&
                      latest.buttonId === button
                        ? "latest"
                        : "",
                    ]
                      .filter(Boolean)
                      .join(" ")}
                    key={button}
                    title={`Button ${button}`}
                  >
                    {button}
                  </span>
                ))}
              </div>
            </section>
          );
        })}
      </div>
      {latest && (
        <footer>
          <span className={latest.pressed ? "pressed" : ""} />
          <div>
            <strong>Button {latest.buttonId}</strong>
            <small>{latest.deviceName}</small>
          </div>
        </footer>
      )}
    </aside>
  );
}
