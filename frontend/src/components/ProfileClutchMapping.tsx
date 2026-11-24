import { invoke } from "@tauri-apps/api/core";
import { Gauge, HelpCircle, LoaderCircle, Radio, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import type {
  ClutchConfiguration,
  ClutchInputCapture,
  VirtualAnalogAxisMapping,
  VirtualAxisId,
  VirtualAxisInput,
  VirtualRoutingStatus,
} from "../models";

function axisFromCapture(
  capture: Extract<ClutchInputCapture, { kind: "axis" }>,
): VirtualAxisInput {
  return {
    device_id: capture.deviceId,
    device_name: capture.deviceName,
    physical_axis: capture.physicalAxis,
    calibration: {
      minimum: capture.minimum,
      maximum: capture.maximum,
      inverted: false,
      released_deadzone_percent: 1,
      engaged_deadzone_percent: 1,
    },
  };
}

function describeAxis(input: VirtualAxisInput | null) {
  if (!input) return "No clutch axis captured";
  return `${input.device_name || "Controller"} · Axis ${input.physical_axis}`;
}

function errorMessage(reason: unknown) {
  if (reason instanceof Error) return reason.message;
  if (typeof reason === "string" && reason.trim()) return reason;
  return "Pivot could not complete that clutch setup step. Please try again.";
}

function FieldHelp({ text }: { text: string }) {
  return (
    <span
      className="field-help"
      tabIndex={0}
      aria-label={text}
      data-tooltip={text}
    >
      <HelpCircle />
    </span>
  );
}

export function ProfileClutchMapping({
  profileId,
  disabled,
  routing,
  onNotice,
  onConfiguredChange,
  onStartLive,
}: {
  profileId: string;
  disabled: boolean;
  routing: VirtualRoutingStatus;
  onNotice: (message: string) => void;
  onConfiguredChange?: (configured: boolean) => void;
  onStartLive?: () => void;
}) {
  const [enabled, setEnabled] = useState(false);
  const [axis, setAxis] = useState<VirtualAxisId | "">("");
  const [availableAxes, setAvailableAxes] = useState<VirtualAxisId[]>([]);
  const [input, setInput] = useState<VirtualAxisInput | null>(null);
  const [analogAxes, setAnalogAxes] = useState<VirtualAnalogAxisMapping[]>([]);
  const [loading, setLoading] = useState(true);
  const [learning, setLearning] = useState(false);
  const [learningAnalog, setLearningAnalog] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [testingVirtualAxis, setTestingVirtualAxis] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [legacyDual, setLegacyDual] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void invoke<ClutchConfiguration>("virtual_profile_clutch", { profileId })
      .then((configuration) => {
        if (cancelled) return;
        const mapping = configuration.mapping;
        setAvailableAxes(configuration.availableVirtualAxes);
        setAnalogAxes(configuration.analogAxes);
        setAxis(
          configuration.workspace?.virtual_axis ??
            configuration.availableVirtualAxes[0] ??
            "",
        );
        setEnabled(mapping !== null);
        setSaved(Boolean(mapping && configuration.workspace));
        setLegacyDual(mapping?.mode === "dual");
        setInput(
          mapping?.mode === "direct" || mapping?.mode === "combined"
            ? mapping.input
            : null,
        );
        onConfiguredChange?.(
          mapping !== null || configuration.analogAxes.length > 0,
        );
      })
      .catch((reason) => !cancelled && setError(errorMessage(reason)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
      void invoke("cancel_binding_lookup");
    };
  }, [profileId]);

  const capture = async () => {
    setLearning(true);
    setError(null);
    onNotice(
      "Move both clutch paddles through the complete range until Pivot finishes listening.",
    );
    try {
      const result = await invoke<ClutchInputCapture>("learn_clutch_input", {
        allowButton: false,
      });
      if (result.kind !== "axis")
        throw new Error("Pivot detected a button instead of an analog axis.");
      setInput(axisFromCapture(result));
      setEnabled(true);
      setSaved(false);
      setLegacyDual(false);
      onNotice("Clutch axis captured. Save it to use this passthrough.");
    } catch (reason) {
      const message = errorMessage(reason);
      if (message.toLowerCase().includes("cancelled"))
        onNotice("Clutch capture cancelled.");
      else setError(message);
    } finally {
      setLearning(false);
    }
  };

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const analogOutputs = analogAxes.map((mapping) => mapping.virtual_axis);
      if (new Set(analogOutputs).size !== analogOutputs.length)
        throw new Error(
          "Each additional input needs its own vJoy output axis.",
        );
      if (enabled) {
        if (!input) throw new Error("Capture the physical clutch axis first.");
        if (!axis)
          throw new Error(
            "No vJoy axis is available. Enable an axis on vJoy device 1, then reopen this profile.",
          );
        if (analogOutputs.includes(axis))
          throw new Error(
            "The clutch and an additional input cannot share a vJoy output axis.",
          );
        await invoke("save_workspace_clutch_settings", {
          profileId,
          virtualAxis: axis,
          sharedBitePointPercent: 50,
        });
      }
      await invoke("save_virtual_profile_clutch", {
        profileId,
        mapping: enabled && input ? { mode: "direct", input } : null,
      });
      await invoke("save_virtual_profile_analog_axes", {
        profileId,
        analogAxes,
      });
      onConfiguredChange?.(enabled || analogAxes.length > 0);
      setSaved(Boolean(enabled && input && axis));
      onNotice(
        enabled
          ? "Clutch passthrough saved. Restart live remapping once to apply it."
          : "Clutch passthrough removed.",
      );
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setSaving(false);
    }
  };

  const sendVirtualAxisTest = async (delayMs: number) => {
    setTestingVirtualAxis(true);
    setError(null);
    try {
      await invoke("test_virtual_clutch_axis", { profileId, delayMs });
      onNotice(
        delayMs
          ? "Virtual clutch test sent after five seconds."
          : "Virtual clutch test sent.",
      );
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setTestingVirtualAxis(false);
    }
  };

  const addAnalogAxis = async () => {
    setLearningAnalog(true);
    setError(null);
    try {
      const result = await invoke<ClutchInputCapture>("learn_clutch_input", {
        allowButton: false,
      });
      if (result.kind !== "axis")
        throw new Error("Pivot did not detect an analog axis.");
      const used = new Set(analogAxes.map((mapping) => mapping.virtual_axis));
      if (axis) used.add(axis);
      const output = availableAxes.find((candidate) => !used.has(candidate));
      if (!output) throw new Error("No unused vJoy axis remains on device 1.");
      setAnalogAxes((current) => [
        ...current,
        {
          name: `Analog input ${current.length + 1}`,
          input: axisFromCapture(result),
          virtual_axis: output,
        },
      ]);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setLearningAnalog(false);
    }
  };

  const live =
    routing.active && routing.profileId === profileId
      ? routing.clutchOutputPercent
      : null;

  return (
    <section className="clutch-mapping-panel simplified">
      <header>
        <span>
          <Gauge />
          <span>
            <strong>Clutch axis passthrough</strong>
            <small>
              SimPro or SimHub combines the paddles. Pivot relays the resulting
              axis without calculating a bite point.
            </small>
          </span>
        </span>
        {enabled && (
          <button
            type="button"
            className="ghost compact"
            disabled={disabled || saving}
            onClick={() => {
              setEnabled(false);
              setSaved(false);
            }}
          >
            <Trash2 /> Remove
          </button>
        )}
      </header>

      {loading ? (
        <div className="clutch-loading">
          <LoaderCircle className="spin" /> Loading clutch setup…
        </div>
      ) : (
        <>
          {legacyDual && (
            <div className="profile-mapping-notice">
              This profile contains an older software dual-clutch mapping.
              Capture the firmware-combined axis below to replace it with the
              simpler passthrough.
            </div>
          )}

          <div className="clutch-passthrough-flow">
            <div className="clutch-source-card">
              <span className="clutch-step">1</span>
              <div>
                <strong>Capture the combined clutch axis</strong>
                <small>{describeAxis(input)}</small>
                {input && (
                  <em>
                    Range {input.calibration.minimum} to{" "}
                    {input.calibration.maximum}
                  </em>
                )}
              </div>
              <button
                type="button"
                className="secondary"
                disabled={disabled}
                onClick={() =>
                  learning
                    ? void invoke("cancel_binding_lookup")
                    : void capture()
                }
              >
                {learning ? <Radio className="spin" /> : null}
                {learning
                  ? "Cancel capture"
                  : input
                    ? "Capture again"
                    : "Start capture"}
              </button>
            </div>

            <div className="clutch-source-card output">
              <span className="clutch-step">2</span>
              <div>
                <strong>Relay it to vJoy</strong>
                <small>
                  {axis
                    ? `${axis.toUpperCase()} axis on vJoy device 1`
                    : "No configured vJoy axis found"}
                </small>
                <em>Bind this axis once as Clutch in the simulator.</em>
              </div>
              <span className="clutch-ready-state">
                {input && axis ? "Ready to save" : "Needs setup"}
              </span>
            </div>
          </div>

          <div className="clutch-live-card compact">
            <div>
              <strong>Live output</strong>
              <small>
                {live === null
                  ? "Available after this profile starts live remapping"
                  : "Current value sent to vJoy"}
              </small>
            </div>
            <div className="clutch-live">
              <div>
                <i style={{ width: `${live ?? 0}%` }} />
              </div>
              <strong>
                {live === null ? "Not running" : `${live.toFixed(0)}%`}
              </strong>
            </div>
          </div>

          <section className="clutch-binding-walkthrough">
            <header>
              <span className="eyebrow">One-time game binding</span>
              <strong>Bind the vJoy axis as Clutch in the game</strong>
            </header>
            <ol>
              <li className={input ? "complete" : ""}>
                <b>1</b>
                <span>
                  <strong>Save this captured axis</strong>
                  <small>
                    Pivot relays your wheel software's finished clutch value.
                  </small>
                </span>
              </li>
              <li>
                <b>2</b>
                <span>
                  <strong>Make the game listen for Clutch</strong>
                  <small>
                    Do not pull a physical paddle while its binding listener is
                    open: the game can see both the real wheel and vJoy.
                  </small>
                </span>
              </li>
              <li className={saved ? "complete" : ""}>
                <b>3</b>
                <span>
                  <strong>Send the vJoy clutch test</strong>
                  <small>
                    Pivot moves only the selected vJoy axis to fully engaged,
                    then releases it. Use the delayed test to focus the game
                    first.
                  </small>
                </span>
                <span className="clutch-test-actions">
                  <button
                    type="button"
                    className="secondary compact"
                    disabled={
                      !saved ||
                      disabled ||
                      routing.active ||
                      testingVirtualAxis
                    }
                    onClick={() => void sendVirtualAxisTest(0)}
                  >
                    Send now
                  </button>
                  <button
                    type="button"
                    className="primary compact"
                    disabled={
                      !saved ||
                      disabled ||
                      routing.active ||
                      testingVirtualAxis
                    }
                    onClick={() => void sendVirtualAxisTest(5_000)}
                  >
                    {testingVirtualAxis ? "Sending…" : "Send in 5s"}
                  </button>
                </span>
              </li>
              <li>
                <b>4</b>
                <span>
                  <strong>Start live remapping and verify the paddles</strong>
                  <small>
                    After LMU has bound vJoy, start remapping and use the live
                    output meter to confirm Pivot relays the combined axis.
                  </small>
                </span>
                {!routing.active && (
                  <button
                    type="button"
                    className="secondary compact"
                    disabled={!saved || disabled}
                    onClick={onStartLive}
                  >
                    Start remapping
                  </button>
                )}
              </li>
            </ol>
          </section>

          <details className="clutch-advanced">
            <summary>Advanced clutch settings</summary>
            <div className="clutch-advanced-content">
              <label>
                <span>
                  vJoy output axis
                  <FieldHelp text="The persistent virtual axis bound as Clutch in the simulator. The first available vJoy axis is selected automatically." />
                </span>
                <select
                  value={axis}
                  disabled={disabled}
                  onChange={(event) =>
                    setAxis(event.target.value as VirtualAxisId)
                  }
                >
                  <option value="">No vJoy axis available</option>
                  {availableAxes.map((value) => (
                    <option key={value} value={value}>
                      {value.toUpperCase()}
                    </option>
                  ))}
                </select>
              </label>

              {input && (
                <div className="clutch-calibration">
                  <label className="inline-switch">
                    <span>
                      Invert axis
                      <FieldHelp text="Reverse released and fully engaged only when the simulator shows the clutch moving backwards." />
                    </span>
                    <input
                      type="checkbox"
                      checked={input.calibration.inverted}
                      disabled={disabled}
                      onChange={(event) =>
                        setInput({
                          ...input,
                          calibration: {
                            ...input.calibration,
                            inverted: event.target.checked,
                          },
                        })
                      }
                    />
                  </label>
                  <label>
                    <span>
                      Released deadzone
                      <FieldHelp text="Ignores a small amount of noise at the released end. The 1% default suits most devices." />
                    </span>
                    <input
                      type="number"
                      min="0"
                      max="49"
                      value={input.calibration.released_deadzone_percent}
                      disabled={disabled}
                      onChange={(event) =>
                        setInput({
                          ...input,
                          calibration: {
                            ...input.calibration,
                            released_deadzone_percent: Number(
                              event.target.value,
                            ),
                          },
                        })
                      }
                    />
                    <em>%</em>
                  </label>
                  <label>
                    <span>
                      Engaged deadzone
                      <FieldHelp text="Ensures full engagement near the end of travel. Increase it only if the output cannot reach 100%." />
                    </span>
                    <input
                      type="number"
                      min="0"
                      max="49"
                      value={input.calibration.engaged_deadzone_percent}
                      disabled={disabled}
                      onChange={(event) =>
                        setInput({
                          ...input,
                          calibration: {
                            ...input.calibration,
                            engaged_deadzone_percent: Number(
                              event.target.value,
                            ),
                          },
                        })
                      }
                    />
                    <em>%</em>
                  </label>
                </div>
              )}
            </div>
          </details>

          <details className="generic-axis-mappings">
            <summary>Additional analog passthroughs</summary>
            <div className="generic-axis-content">
              <header>
                <span>
                  <strong>Other analog inputs</strong>
                  <small>
                    Optional, profile-specific passthroughs for controls other
                    than the clutch.
                  </small>
                </span>
                <button
                  type="button"
                  className="secondary compact"
                  disabled={disabled || learningAnalog}
                  onClick={() => void addAnalogAxis()}
                >
                  {learningAnalog ? <LoaderCircle className="spin" /> : null}
                  {learningAnalog ? "Move an axis…" : "Add analog input"}
                </button>
              </header>

              {analogAxes.map((mapping, index) => (
                <div
                  className="generic-axis-row simplified"
                  key={`${mapping.input.device_id}-${mapping.input.physical_axis}-${index}`}
                >
                  <label>
                    <span>Name</span>
                    <input
                      value={mapping.name}
                      disabled={disabled}
                      onChange={(event) =>
                        setAnalogAxes((current) =>
                          current.map((item, currentIndex) =>
                            currentIndex === index
                              ? { ...item, name: event.target.value }
                              : item,
                          ),
                        )
                      }
                    />
                  </label>
                  <label>
                    <span>
                      vJoy output
                      <FieldHelp text="Each physical input needs a different configured axis on vJoy device 1." />
                    </span>
                    <select
                      value={mapping.virtual_axis}
                      disabled={disabled}
                      onChange={(event) =>
                        setAnalogAxes((current) =>
                          current.map((item, currentIndex) =>
                            currentIndex === index
                              ? {
                                  ...item,
                                  virtual_axis: event.target
                                    .value as VirtualAxisId,
                                }
                              : item,
                          ),
                        )
                      }
                    >
                      {availableAxes.map((value) => (
                        <option value={value} key={value}>
                          {value.toUpperCase()}
                        </option>
                      ))}
                    </select>
                  </label>
                  <span className="generic-axis-source">
                    <small>{describeAxis(mapping.input)}</small>
                  </span>
                  <button
                    type="button"
                    className="ghost icon-button"
                    aria-label={`Remove ${mapping.name}`}
                    disabled={disabled}
                    onClick={() =>
                      setAnalogAxes((current) =>
                        current.filter(
                          (_, currentIndex) => currentIndex !== index,
                        ),
                      )
                    }
                  >
                    <Trash2 />
                  </button>
                </div>
              ))}
              {!analogAxes.length && <p>No additional analog inputs mapped.</p>}
            </div>
          </details>
        </>
      )}

      {error && <div className="virtual-error">{error}</div>}
      <footer>
        <button
          type="button"
          className="primary"
          disabled={disabled || loading || saving || learning}
          onClick={() => void save()}
        >
          {saving ? <LoaderCircle className="spin" /> : null}
          Save clutch passthrough
        </button>
      </footer>
    </section>
  );
}
