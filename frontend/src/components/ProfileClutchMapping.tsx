import { invoke } from "@tauri-apps/api/core";
import { Gauge, LoaderCircle, Trash2 } from "lucide-react";
import { useEffect, useState, type CSSProperties } from "react";
import type {
  ClutchConfiguration,
  ClutchInputCapture,
  VirtualAnalogAxisMapping,
  VirtualAxisId,
  VirtualAxisInput,
  VirtualClutchInput,
  VirtualClutchMapping,
  VirtualRoutingStatus,
} from "../models";

type Mode = "off" | "direct" | "dual" | "combined";

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

function inputFromCapture(capture: ClutchInputCapture): VirtualClutchInput {
  return capture.kind === "axis"
    ? { kind: "axis", ...axisFromCapture(capture) }
    : {
        kind: "button",
        device_id: capture.deviceId,
        device_name: capture.deviceName,
        physical_button: capture.physicalButton,
      };
}

function describe(input: VirtualClutchInput | VirtualAxisInput | null) {
  if (!input) return "Not assigned";
  if ("kind" in input && input.kind === "button")
    return `${input.device_name || "Controller"} · Button ${input.physical_button}`;
  const axis = "kind" in input ? input : input;
  return `${axis.device_name || "Controller"} · Axis ${axis.physical_axis}`;
}

function errorMessage(reason: unknown) {
  if (reason instanceof Error) return reason.message;
  if (typeof reason === "string" && reason.trim()) return reason;
  return "Pivot could not complete that clutch setup step. Please try again.";
}

function AxisOptions({
  input,
  disabled,
  onChange,
}: {
  input: VirtualClutchInput | null;
  disabled: boolean;
  onChange: (input: VirtualClutchInput) => void;
}) {
  if (!input || input.kind !== "axis") return null;
  const update = (patch: Partial<VirtualAxisInput["calibration"]>) =>
    onChange({
      ...input,
      calibration: { ...input.calibration, ...patch },
    });
  return (
    <div className="clutch-source-axis-options">
      <label className="inline-switch">
        <span>Invert</span>
        <input
          type="checkbox"
          checked={input.calibration.inverted}
          disabled={disabled}
          onChange={(event) => update({ inverted: event.target.checked })}
        />
      </label>
      <label>
        <span>Released DZ</span>
        <input
          type="number"
          min="0"
          max="49"
          value={input.calibration.released_deadzone_percent}
          disabled={disabled}
          onChange={(event) =>
            update({ released_deadzone_percent: Number(event.target.value) })
          }
        />
      </label>
      <label>
        <span>Engaged DZ</span>
        <input
          type="number"
          min="0"
          max="49"
          value={input.calibration.engaged_deadzone_percent}
          disabled={disabled}
          onChange={(event) =>
            update({ engaged_deadzone_percent: Number(event.target.value) })
          }
        />
      </label>
    </div>
  );
}

export function ProfileClutchMapping({
  profileId,
  disabled,
  routing,
  onNotice,
  onConfiguredChange,
}: {
  profileId: string;
  disabled: boolean;
  routing: VirtualRoutingStatus;
  onNotice: (message: string) => void;
  onConfiguredChange?: (configured: boolean) => void;
}) {
  const [mode, setMode] = useState<Mode>("off");
  const [axis, setAxis] = useState<VirtualAxisId | "">("");
  const [availableAxes, setAvailableAxes] = useState<VirtualAxisId[]>([]);
  const [direct, setDirect] = useState<VirtualAxisInput | null>(null);
  const [full, setFull] = useState<VirtualClutchInput | null>(null);
  const [bite, setBite] = useState<VirtualClutchInput | null>(null);
  const [bitePoint, setBitePoint] = useState(50);
  const [sharedBitePoint, setSharedBitePoint] = useState(50);
  const [useShared, setUseShared] = useState(false);
  const [firmwareBite, setFirmwareBite] = useState<number | null>(null);
  const [analogAxes, setAnalogAxes] = useState<VirtualAnalogAxisMapping[]>([]);
  const [learningAnalog, setLearningAnalog] = useState(false);
  const [learning, setLearning] = useState<"direct" | "full" | "bite" | null>(
    null,
  );
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void invoke<ClutchConfiguration>("virtual_profile_clutch", { profileId })
      .then((configuration) => {
        if (cancelled) return;
        setAvailableAxes(configuration.availableVirtualAxes);
        setAnalogAxes(configuration.analogAxes);
        setAxis(
          configuration.workspace?.virtual_axis ??
            configuration.availableVirtualAxes[0] ??
            "",
        );
        setSharedBitePoint(
          configuration.workspace?.shared_bite_point_percent ?? 50,
        );
        const mapping = configuration.mapping;
        onConfiguredChange?.(
          mapping !== null || configuration.analogAxes.length > 0,
        );
        setMode(mapping?.mode ?? "off");
        setDirect(
          mapping?.mode === "direct" || mapping?.mode === "combined"
            ? mapping.input
            : null,
        );
        setFull(mapping?.mode === "dual" ? mapping.full : null);
        setBite(mapping?.mode === "dual" ? mapping.bite : null);
        if (mapping?.mode === "dual" || mapping?.mode === "combined") {
          setBitePoint(mapping.bite_point_percent);
          setUseShared(mapping.use_workspace_bite_point);
        }
        setFirmwareBite(
          mapping?.mode === "combined"
            ? mapping.firmware_bite_point_percent
            : null,
        );
      })
      .catch((reason) => !cancelled && setError(errorMessage(reason)));
    return () => {
      cancelled = true;
    };
  }, [profileId]);

  const learn = async (target: "direct" | "full" | "bite") => {
    setLearning(target);
    setError(null);
    onNotice(
      target === "direct"
        ? mode === "combined"
          ? "Move both clutch paddles through their full combined travel until capture finishes."
          : "Move the clutch through its full travel until capture finishes."
        : `Press or fully move the ${target} clutch input.`,
    );
    try {
      const capture = await invoke<ClutchInputCapture>("learn_clutch_input", {
        allowButton: target !== "direct",
      });
      if (target === "direct") {
        if (capture.kind !== "axis")
          throw new Error("Direct and combined modes require an analog axis.");
        setDirect(axisFromCapture(capture));
      } else if (target === "full") setFull(inputFromCapture(capture));
      else setBite(inputFromCapture(capture));
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setLearning(null);
    }
  };

  const updateDirectCalibration = (
    patch: Partial<VirtualAxisInput["calibration"]>,
  ) =>
    setDirect((current) =>
      current
        ? { ...current, calibration: { ...current.calibration, ...patch } }
        : current,
    );

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const analogOutputs = analogAxes.map((mapping) => mapping.virtual_axis);
      if (new Set(analogOutputs).size !== analogOutputs.length)
        throw new Error("Each analog mapping needs its own vJoy output axis.");
      if (mode !== "off" && axis && analogOutputs.includes(axis))
        throw new Error(
          "The clutch and another analog mapping cannot share a vJoy output axis.",
        );
      let mapping: VirtualClutchMapping | null = null;
      if (mode !== "off") {
        if (!axis)
          throw new Error("Choose the persistent vJoy clutch axis first.");
        if (mode === "direct") {
          if (!direct)
            throw new Error("Capture the physical clutch axis first.");
          mapping = { mode, input: direct };
        } else if (mode === "combined") {
          if (!direct)
            throw new Error("Capture the combined clutch axis first.");
          mapping = {
            mode,
            input: direct,
            firmware_bite_point_percent: firmwareBite,
            bite_point_percent: bitePoint,
            use_workspace_bite_point: useShared,
          };
        } else {
          if (!full || !bite)
            throw new Error(
              "Assign both full-clutch and bite-point inputs first.",
            );
          mapping = {
            mode,
            full,
            bite,
            bite_point_percent: bitePoint,
            use_workspace_bite_point: useShared,
          };
        }
        await invoke("save_workspace_clutch_settings", {
          profileId,
          virtualAxis: axis,
          sharedBitePointPercent: sharedBitePoint,
        });
      }
      await invoke("save_virtual_profile_clutch", { profileId, mapping });
      await invoke("save_virtual_profile_analog_axes", {
        profileId,
        analogAxes,
      });
      onConfiguredChange?.(mapping !== null || analogAxes.length > 0);
      onNotice(
        mode === "off"
          ? "Clutch mapping removed."
          : "Clutch mapping saved. Restart live remapping once to apply editor changes.",
      );
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setSaving(false);
    }
  };

  const live =
    routing.active && routing.profileId === profileId
      ? routing.clutchOutputPercent
      : null;

  const addAnalogAxis = async () => {
    setLearningAnalog(true);
    setError(null);
    onNotice(
      "Move only the analog control you want through its full travel for three seconds.",
    );
    try {
      const capture = await invoke<ClutchInputCapture>("learn_clutch_input", {
        allowButton: false,
      });
      if (capture.kind !== "axis")
        throw new Error("An analog axis was not detected.");
      const used = new Set(analogAxes.map((mapping) => mapping.virtual_axis));
      if (axis) used.add(axis);
      const output = availableAxes.find((candidate) => !used.has(candidate));
      if (!output)
        throw new Error("No unused vJoy axis remains for this profile.");
      setAnalogAxes((current) => [
        ...current,
        {
          name: `Analog axis ${current.length + 1}`,
          input: axisFromCapture(capture),
          virtual_axis: output,
        },
      ]);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setLearningAnalog(false);
    }
  };

  const updateAnalog = (index: number, next: VirtualAnalogAxisMapping) =>
    setAnalogAxes((current) =>
      current.map((mapping, currentIndex) =>
        currentIndex === index ? next : mapping,
      ),
    );
  const primaryInput =
    routing.active && routing.profileId === profileId
      ? routing.clutchPrimaryInputPercent
      : null;
  const biteInput =
    routing.active && routing.profileId === profileId
      ? routing.clutchBiteInputPercent
      : null;
  return (
    <section className="clutch-mapping-panel">
      <header>
        <span>
          <Gauge />
          <span>
            <strong>Clutch paddles</strong>
            <small>
              Bind one vJoy clutch axis in the simulator, then let each profile
              choose what drives it.
            </small>
          </span>
        </span>
        {mode !== "off" && (
          <button
            type="button"
            className="ghost compact"
            disabled={disabled || saving}
            onClick={() => setMode("off")}
          >
            <Trash2 /> Remove clutch setup
          </button>
        )}
      </header>
      {mode === "off" ? (
        <div className="clutch-empty-state">
          <div>
            <strong>How does this wheel expose its clutch?</strong>
            <p>
              Simagic wheels configured for combined dual clutch normally use
              the combined-axis option.
            </p>
          </div>
          <button
            type="button"
            className="primary"
            disabled={disabled}
            onClick={() => setMode("combined")}
          >
            Set up clutch
          </button>
        </div>
      ) : (
        <>
          <div className="clutch-section-heading">
            <span className="clutch-step">1</span>
            <div>
              <strong>Choose the physical clutch type</strong>
              <small>
                This is how the wheel reports its paddles to Windows.
              </small>
            </div>
          </div>
          <div className="clutch-mode-tabs">
            {(["combined", "dual", "direct"] as const).map((value) => (
              <button
                type="button"
                key={value}
                className={mode === value ? "active" : ""}
                disabled={disabled}
                onClick={() => setMode(value)}
              >
                <strong>
                  {value === "combined"
                    ? "Combined axis"
                    : value === "dual"
                      ? "Two separate inputs"
                      : "Single clutch axis"}
                </strong>
                <small>
                  {value === "combined"
                    ? "Both paddles appear as one axis — usual Simagic mode"
                    : value === "dual"
                      ? "Each paddle appears as its own axis or button"
                      : "A pedal or one analog paddle, passed straight through"}
                </small>
              </button>
            ))}
          </div>

          <div className="clutch-section-heading">
            <span className="clutch-step">2</span>
            <div>
              <strong>Capture the physical input</strong>
              <small>
                {mode === "combined"
                  ? "Move both paddles across their full range while Pivot listens."
                  : mode === "dual"
                    ? "Capture each paddle separately."
                    : "Move the clutch across its full range while Pivot listens."}
              </small>
            </div>
          </div>
          <div className="clutch-grid clutch-capture-grid">
            {mode === "dual" ? (
              <>
                <div className="clutch-source">
                  <span>Full-clutch paddle</span>
                  <strong>{describe(full)}</strong>
                  <AxisOptions
                    input={full}
                    disabled={disabled}
                    onChange={setFull}
                  />
                  <button
                    type="button"
                    className="secondary"
                    disabled={disabled || learning !== null}
                    onClick={() => void learn("full")}
                  >
                    {learning === "full" ? (
                      <LoaderCircle className="spin" />
                    ) : null}{" "}
                    Capture full paddle
                  </button>
                </div>
                <div className="clutch-source">
                  <span>Bite-point paddle</span>
                  <strong>{describe(bite)}</strong>
                  <AxisOptions
                    input={bite}
                    disabled={disabled}
                    onChange={setBite}
                  />
                  <button
                    type="button"
                    className="secondary"
                    disabled={disabled || learning !== null}
                    onClick={() => void learn("bite")}
                  >
                    {learning === "bite" ? (
                      <LoaderCircle className="spin" />
                    ) : null}{" "}
                    Capture bite paddle
                  </button>
                </div>
              </>
            ) : (
              <div className="clutch-source clutch-source-wide">
                <span>
                  {mode === "combined"
                    ? "Firmware-combined paddles"
                    : "Physical clutch axis"}
                </span>
                <strong>{describe(direct)}</strong>
                <small>
                  {direct
                    ? `Captured range ${direct.calibration.minimum} to ${direct.calibration.maximum}`
                    : "Nothing captured yet"}
                </small>
                <button
                  type="button"
                  className="secondary"
                  disabled={disabled || learning !== null}
                  onClick={() => void learn("direct")}
                >
                  {learning === "direct" ? (
                    <LoaderCircle className="spin" />
                  ) : null}{" "}
                  {direct ? "Capture again" : "Start capture"}
                </button>
              </div>
            )}
          </div>
          {(mode === "dual" || mode === "combined") && (
            <div className="clutch-settings-card">
              <div className="clutch-section-heading">
                <span className="clutch-step">3</span>
                <div>
                  <strong>Set the bite point</strong>
                  <small>
                    Adjust this at any time without rebinding the simulator.
                  </small>
                </div>
              </div>
              <div className="bite-point-control">
                <input
                  type="range"
                  min="0"
                  max="100"
                  style={
                    {
                      "--range-progress": `${useShared ? sharedBitePoint : bitePoint}%`,
                    } as CSSProperties
                  }
                  value={useShared ? sharedBitePoint : bitePoint}
                  disabled={disabled}
                  onChange={(event) =>
                    useShared
                      ? setSharedBitePoint(Number(event.target.value))
                      : setBitePoint(Number(event.target.value))
                  }
                />
                <label>
                  <input
                    type="number"
                    min="0"
                    max="100"
                    value={useShared ? sharedBitePoint : bitePoint}
                    disabled={disabled}
                    onChange={(event) =>
                      useShared
                        ? setSharedBitePoint(Number(event.target.value))
                        : setBitePoint(Number(event.target.value))
                    }
                  />
                  <span>%</span>
                </label>
                <label className="inline-switch">
                  <span>
                    Use this bite point for all profiles in this workspace
                  </span>
                  <input
                    type="checkbox"
                    checked={useShared}
                    disabled={disabled}
                    onChange={(event) => setUseShared(event.target.checked)}
                  />
                </label>
              </div>
            </div>
          )}
          <div className="clutch-live-card">
            <div>
              <strong>Live clutch output</strong>
              <small>
                {live === null
                  ? "Save, then start this profile to test it live."
                  : `Sending ${axis ? axis.toUpperCase() : "the selected axis"} to vJoy`}
              </small>
            </div>
            <div className="clutch-live">
              <div>
                <i style={{ width: `${live ?? 0}%` }} />
              </div>
              <strong>
                {live === null
                  ? "Start this profile to preview"
                  : `${live.toFixed(0)}%`}
              </strong>
            </div>
          </div>
          <div className="clutch-input-indicators">
            <span>
              <small>
                {mode === "dual" ? "Full paddle" : "Physical input"}
              </small>
              <i>
                <b style={{ width: `${primaryInput ?? 0}%` }} />
              </i>
              <strong>
                {primaryInput === null ? "—" : `${primaryInput.toFixed(0)}%`}
              </strong>
            </span>
            {mode === "dual" && (
              <span>
                <small>Bite paddle</small>
                <i>
                  <b style={{ width: `${biteInput ?? 0}%` }} />
                </i>
                <strong>
                  {biteInput === null ? "—" : `${biteInput.toFixed(0)}%`}
                </strong>
              </span>
            )}
          </div>

          <details className="clutch-advanced">
            <summary>Advanced output and calibration</summary>
            <div className="clutch-advanced-content">
              <label>
                <span>vJoy clutch axis</span>
                <select
                  value={axis}
                  disabled={disabled}
                  onChange={(event) =>
                    setAxis(event.target.value as VirtualAxisId)
                  }
                >
                  <option value="">Select vJoy axis…</option>
                  {availableAxes.map((value) => (
                    <option key={value} value={value}>
                      {value.toUpperCase()}
                    </option>
                  ))}
                </select>
                <small>
                  Keep this axis stable and bind it once as Clutch in the
                  simulator.
                </small>
              </label>
              {direct && mode !== "dual" && (
                <div className="clutch-calibration">
                  <label className="inline-switch">
                    <span>Invert physical axis</span>
                    <input
                      type="checkbox"
                      checked={direct.calibration.inverted}
                      disabled={disabled}
                      onChange={(event) =>
                        updateDirectCalibration({
                          inverted: event.target.checked,
                        })
                      }
                    />
                  </label>
                  <label>
                    <span>Released deadzone</span>
                    <input
                      type="number"
                      min="0"
                      max="49"
                      value={direct.calibration.released_deadzone_percent}
                      onChange={(event) =>
                        updateDirectCalibration({
                          released_deadzone_percent: Number(event.target.value),
                        })
                      }
                    />
                    <em>%</em>
                  </label>
                  <label>
                    <span>Engaged deadzone</span>
                    <input
                      type="number"
                      min="0"
                      max="49"
                      value={direct.calibration.engaged_deadzone_percent}
                      onChange={(event) =>
                        updateDirectCalibration({
                          engaged_deadzone_percent: Number(event.target.value),
                        })
                      }
                    />
                    <em>%</em>
                  </label>
                </div>
              )}
              {mode === "combined" && (
                <label className="firmware-bite">
                  <span>Known firmware bite point (optional)</span>
                  <input
                    type="number"
                    min="0"
                    max="100"
                    value={firmwareBite ?? ""}
                    placeholder="Off"
                    disabled={disabled}
                    onChange={(event) =>
                      setFirmwareBite(
                        event.target.value === ""
                          ? null
                          : Number(event.target.value),
                      )
                    }
                  />
                  <em>%</em>
                  <small>
                    Only use this when you know the wheel firmware's fixed bite
                    point. Otherwise leave it off.
                  </small>
                </label>
              )}
            </div>
          </details>
        </>
      )}
      <details className="generic-axis-mappings">
        <summary>Other analog axis mappings</summary>
        <div className="generic-axis-content">
          <header>
            <span>
              <strong>Other analog axes</strong>
              <small>
                Optional calibrated passthrough routes on this profile.
              </small>
            </span>
            <button
              type="button"
              className="secondary compact"
              disabled={disabled || learningAnalog}
              onClick={() => void addAnalogAxis()}
            >
              {learningAnalog ? <LoaderCircle className="spin" /> : null}
              {learningAnalog ? "Move an axis…" : "Add analog axis"}
            </button>
          </header>
          {analogAxes.map((mapping, index) => {
            const liveAxis =
              routing.active && routing.profileId === profileId
                ? routing.analogAxisOutputs.find(
                    (output) => output.name === mapping.name,
                  )
                : undefined;
            return (
              <div
                className="generic-axis-row"
                key={`${mapping.input.device_id}-${mapping.input.physical_axis}-${index}`}
              >
                <label>
                  <span>Name</span>
                  <input
                    value={mapping.name}
                    disabled={disabled}
                    onChange={(event) =>
                      updateAnalog(index, {
                        ...mapping,
                        name: event.target.value,
                      })
                    }
                  />
                </label>
                <label>
                  <span>vJoy output</span>
                  <select
                    value={mapping.virtual_axis}
                    disabled={disabled}
                    onChange={(event) =>
                      updateAnalog(index, {
                        ...mapping,
                        virtual_axis: event.target.value as VirtualAxisId,
                      })
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
                  <small>{describe(mapping.input)}</small>
                  <b>
                    {liveAxis ? `${liveAxis.outputPercent.toFixed(0)}%` : "—"}
                  </b>
                </span>
                <label className="inline-switch">
                  <span>Invert</span>
                  <input
                    type="checkbox"
                    checked={mapping.input.calibration.inverted}
                    disabled={disabled}
                    onChange={(event) =>
                      updateAnalog(index, {
                        ...mapping,
                        input: {
                          ...mapping.input,
                          calibration: {
                            ...mapping.input.calibration,
                            inverted: event.target.checked,
                          },
                        },
                      })
                    }
                  />
                </label>
                <label className="generic-axis-deadzone">
                  <span>Released DZ</span>
                  <input
                    type="number"
                    min="0"
                    max="49"
                    value={mapping.input.calibration.released_deadzone_percent}
                    disabled={disabled}
                    onChange={(event) =>
                      updateAnalog(index, {
                        ...mapping,
                        input: {
                          ...mapping.input,
                          calibration: {
                            ...mapping.input.calibration,
                            released_deadzone_percent: Number(
                              event.target.value,
                            ),
                          },
                        },
                      })
                    }
                  />
                </label>
                <label className="generic-axis-deadzone">
                  <span>Engaged DZ</span>
                  <input
                    type="number"
                    min="0"
                    max="49"
                    value={mapping.input.calibration.engaged_deadzone_percent}
                    disabled={disabled}
                    onChange={(event) =>
                      updateAnalog(index, {
                        ...mapping,
                        input: {
                          ...mapping.input,
                          calibration: {
                            ...mapping.input.calibration,
                            engaged_deadzone_percent: Number(
                              event.target.value,
                            ),
                          },
                        },
                      })
                    }
                  />
                </label>
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
            );
          })}
          {!analogAxes.length && <p>No additional analog axes mapped.</p>}
        </div>
      </details>
      {error && <div className="virtual-error">{error}</div>}
      <footer>
        <button
          type="button"
          className="primary"
          disabled={disabled || saving || learning !== null}
          onClick={() => void save()}
        >
          {saving ? <LoaderCircle className="spin" /> : null}
          {mode === "off" ? "Save changes" : "Save clutch setup"}
        </button>
      </footer>
    </section>
  );
}
