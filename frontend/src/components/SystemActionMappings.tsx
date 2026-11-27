import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AppWindow, Mic, Radio, Trash2, Volume2, VolumeX } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type {
  CaptureAudioDevice,
  PlaybackApplication,
  PhysicalControlCapture,
  SystemActionBinding,
} from "../models";
import { ConfirmDialog, type Confirmation } from "./ConfirmDialog";

function controlLabel(binding: PhysicalControlCapture) {
  return [
    ...binding.modifiers.map(
      (item) => `${item.deviceName || "Controller"} B${item.physicalButton}`,
    ),
    `${binding.deviceName || "Controller"} B${binding.physicalButton}`,
  ].join(" + ");
}

function operationLabel(
  binding: Pick<SystemActionBinding, "actionType" | "operation">,
) {
  if (binding.actionType === "microphone") {
    return binding.operation === "toggle"
      ? "Toggle microphone mute"
      : binding.operation === "mute"
        ? "Mute microphone"
        : "Unmute microphone";
  }
  if (binding.operation === "increase") return "Increase application volume";
  if (binding.operation === "decrease") return "Decrease application volume";
  if (binding.operation === "toggle_mute") return "Toggle application mute";
  if (binding.operation === "mute") return "Mute application";
  return "Unmute application";
}

export function SystemActionMappings({
  profileId,
  disabled,
  onNotice,
  onCountChange,
}: {
  profileId: string;
  disabled: boolean;
  onNotice: (message: string) => void;
  onCountChange?: (count: number) => void;
}) {
  const [bindings, setBindings] = useState<SystemActionBinding[]>([]);
  const [devices, setDevices] = useState<CaptureAudioDevice[]>([]);
  const [applications, setApplications] = useState<PlaybackApplication[]>([]);
  const [capture, setCapture] = useState<PhysicalControlCapture | null>(null);
  const [listening, setListening] = useState(false);
  const [saving, setSaving] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [scope, setScope] = useState<"profile" | "global">("profile");
  const [actionType, setActionType] =
    useState<SystemActionBinding["actionType"]>("microphone");
  const [operation, setOperation] =
    useState<SystemActionBinding["operation"]>("toggle");
  const [target, setTarget] = useState("default");
  const [application, setApplication] = useState("");
  const [stepPercent, setStepPercent] = useState(5);
  const [playFeedbackSound, setPlayFeedbackSound] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

  const load = () =>
    Promise.all([
      invoke<SystemActionBinding[]>("system_action_bindings", { profileId }),
      invoke<CaptureAudioDevice[]>("capture_audio_devices"),
      invoke<PlaybackApplication[]>("playback_applications"),
    ]).then(([nextBindings, nextDevices, nextApplications]) => {
      setBindings(nextBindings);
      setDevices(nextDevices);
      setApplications(nextApplications);
    });

  useEffect(() => {
    let cancelled = false;
    void load().catch((reason) => {
      if (!cancelled) setError(String(reason));
    });
    const unlisten = listen("audio-devices-changed", () => {
      void Promise.all([
        invoke<CaptureAudioDevice[]>("capture_audio_devices"),
        invoke<PlaybackApplication[]>("playback_applications"),
      ])
        .then(([nextDevices, nextApplications]) => {
          if (!cancelled) {
            setDevices(nextDevices);
            setApplications(nextApplications);
          }
        })
        .catch(() => undefined);
    });
    return () => {
      cancelled = true;
      void unlisten.then((dispose) => dispose());
    };
  }, [profileId]);

  const deviceById = useMemo(
    () => new Map(devices.map((device) => [device.id, device])),
    [devices],
  );
  useEffect(
    () => onCountChange?.(bindings.length),
    [bindings.length, onCountChange],
  );

  const targetLabel = (binding: SystemActionBinding) => {
    if (binding.actionType === "application_volume")
      return binding.targetId ?? "Unavailable application";
    if (binding.targetType === "default") return "Default microphone";
    if (binding.targetType === "communications")
      return "Default communications microphone";
    return (
      deviceById.get(binding.targetId ?? "")?.name ?? "Unavailable microphone"
    );
  };

  const captureControl = async () => {
    setListening(true);
    setError(null);
    try {
      await invoke<PlaybackApplication[]>("playback_applications")
        .then(setApplications)
        .catch(() => undefined);
      const result = await invoke<PhysicalControlCapture>(
        "capture_system_action_control",
      );
      setCapture(result);
      onNotice(`Captured ${controlLabel(result)} for a system action.`);
    } catch (reason) {
      if (!String(reason).toLowerCase().includes("cancelled"))
        setError(String(reason));
    } finally {
      setListening(false);
    }
  };

  const persistBinding = async (
    binding: SystemActionBinding,
    replaceExisting = false,
    alsoTrigger = false,
  ) => {
    setSaving(true);
    setError(null);
    try {
      setBindings(
        await invoke<SystemActionBinding[]>("save_system_action_binding", {
          profileId,
          binding,
          replaceExisting,
          alsoTrigger,
        }),
      );
      setCapture(null);
      setConfirmation(null);
      onNotice(`${operationLabel(binding)} binding saved.`);
    } catch (reason) {
      const message = String(reason);
      if (message.includes("triggers another system action")) {
        setConfirmation({
          title: "Replace this system action?",
          description: `${message} The existing system action will be removed from this control.`,
          confirmLabel: "Replace mapping",
          onConfirm: () => void persistBinding(binding, true, alsoTrigger),
        });
      } else if (message.includes("already triggers a virtual action")) {
        setConfirmation({
          title: "Trigger both actions?",
          description: `${message} One press will send the virtual action and run this system action.`,
          confirmLabel: "Trigger both",
          onConfirm: () => void persistBinding(binding, replaceExisting, true),
        });
      } else {
        setError(message);
      }
    } finally {
      setSaving(false);
    }
  };

  const save = () => {
    if (!capture) return;
    const targetType = target.startsWith("specific:") ? "specific" : target;
    const binding: SystemActionBinding = {
      ...capture,
      scope,
      actionType,
      operation,
      targetType:
        actionType === "application_volume"
          ? "application"
          : (targetType as SystemActionBinding["targetType"]),
      targetId:
        actionType === "application_volume"
          ? application
          : target.startsWith("specific:")
            ? target.slice(9)
            : null,
      stepPercent,
      playFeedbackSound: actionType === "microphone" && playFeedbackSound,
    };
    void persistBinding(binding);
  };

  const remove = async (binding: SystemActionBinding) => {
    setRemoving(true);
    setError(null);
    try {
      setBindings(
        await invoke<SystemActionBinding[]>("remove_system_action_binding", {
          profileId,
          binding,
        }),
      );
      setConfirmation(null);
      onNotice("System action binding removed.");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setRemoving(false);
    }
  };

  const requestRemove = (binding: SystemActionBinding) => {
    setConfirmation({
      title: "Remove system action binding?",
      description: `${operationLabel(binding)} for ${targetLabel(binding)} will no longer be triggered by ${controlLabel(binding)}${binding.scope === "global" ? " anywhere in Pivot" : " in this profile"}.`,
      confirmLabel: "Remove binding",
      danger: true,
      onConfirm: () => void remove(binding),
    });
  };

  const testAction = async (binding: SystemActionBinding) => {
    try {
      if (binding.actionType === "application_volume") {
        await invoke<boolean>("test_application_volume_action", {
          operation: binding.operation,
          executable: binding.targetId,
          stepPercent: binding.stepPercent,
        });
        const next = await invoke<PlaybackApplication[]>(
          "playback_applications",
        );
        setApplications(next);
        const current = next.find(
          (item) => item.executable === binding.targetId,
        );
        onNotice(
          current
            ? `${current.executable}: ${current.muted ? "muted" : `${current.volumePercent}%`}`
            : "Application volume action sent.",
        );
      } else {
        const muted = await invoke<boolean>("test_microphone_action", {
          operation: binding.operation,
          targetType: binding.targetType,
          targetId: binding.targetId,
          playFeedbackSound: binding.playFeedbackSound,
        });
        onNotice(muted ? "Microphone muted." : "Microphone active.");
      }
    } catch (reason) {
      setError(String(reason));
      onNotice(String(reason));
    }
  };

  const toggleFeedbackSound = async (binding: SystemActionBinding) => {
    setSaving(true);
    setError(null);
    try {
      setBindings(
        await invoke<SystemActionBinding[]>("save_system_action_binding", {
          profileId,
          binding: {
            ...binding,
            playFeedbackSound: !binding.playFeedbackSound,
          },
          replaceExisting: true,
          alsoTrigger: true,
        }),
      );
      onNotice(
        binding.playFeedbackSound
          ? "Microphone confirmation sound disabled."
          : "Microphone confirmation sound enabled.",
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="system-action-editor">
      <header>
        <span>
          <Mic />
          <span>
            <strong>System actions</strong>
            <small>Native microphone and application volume controls</small>
          </span>
        </span>
        <button
          className="secondary compact"
          disabled={disabled || listening}
          onClick={() => void captureControl()}
        >
          {listening ? <Radio className="spin" /> : <Mic />}
          {listening ? "Press a control…" : "Bind system action"}
        </button>
      </header>

      {error && <div className="virtual-error">{error}</div>}
      {capture && (
        <div className="system-action-form">
          <label>
            <span>Input</span>
            <strong>{controlLabel(capture)}</strong>
          </label>
          <label>
            <span>System action</span>
            <select
              value={actionType}
              onChange={(event) => {
                const next = event.target
                  .value as SystemActionBinding["actionType"];
                setActionType(next);
                setOperation(next === "microphone" ? "toggle" : "increase");
              }}
            >
              <option value="microphone">Microphone mute</option>
              <option value="application_volume">Application volume</option>
            </select>
          </label>
          <label>
            <span>Action</span>
            <select
              value={operation}
              onChange={(event) =>
                setOperation(
                  event.target.value as SystemActionBinding["operation"],
                )
              }
            >
              {actionType === "microphone" ? (
                <>
                  <option value="toggle">Toggle microphone mute</option>
                  <option value="mute">Mute microphone</option>
                  <option value="unmute">Unmute microphone</option>
                </>
              ) : (
                <>
                  <option value="increase">Increase volume</option>
                  <option value="decrease">Decrease volume</option>
                  <option value="toggle_mute">Toggle application mute</option>
                  <option value="mute">Mute application</option>
                  <option value="unmute">Unmute application</option>
                </>
              )}
            </select>
          </label>
          {actionType === "microphone" ? (
            <label>
              <span>Microphone</span>
              <select
                value={target}
                onChange={(event) => setTarget(event.target.value)}
              >
                <option value="default">Default microphone</option>
                <option value="communications">
                  Default communications microphone
                </option>
                {devices.map((device) => (
                  <option key={device.id} value={`specific:${device.id}`}>
                    {device.name}
                    {device.active ? "" : " (unavailable)"}
                  </option>
                ))}
              </select>
            </label>
          ) : (
            <>
              <label>
                <span>Application</span>
                <select
                  value={application}
                  onChange={(event) => setApplication(event.target.value)}
                >
                  <option value="">Choose a running audio application</option>
                  {applications.map((item) => (
                    <option key={item.executable} value={item.executable}>
                      {item.executable} ·{" "}
                      {item.muted ? "muted" : `${item.volumePercent}%`}
                    </option>
                  ))}
                </select>
              </label>
              {(operation === "increase" || operation === "decrease") && (
                <label>
                  <span>Volume step</span>
                  <select
                    value={stepPercent}
                    onChange={(event) =>
                      setStepPercent(Number(event.target.value))
                    }
                  >
                    {[1, 2, 5, 10, 15, 20, 25].map((value) => (
                      <option key={value} value={value}>
                        {value}% per press
                      </option>
                    ))}
                  </select>
                </label>
              )}
            </>
          )}
          <label>
            <span>Available in</span>
            <select
              value={scope}
              onChange={(event) =>
                setScope(event.target.value as "profile" | "global")
              }
            >
              <option value="profile">This profile</option>
              <option value="global">Every workspace and profile</option>
            </select>
          </label>
          {actionType === "microphone" && (
            <label>
              <span>Audio feedback</span>
              <strong className="system-action-sound-choice">
                <span>Play a confirmation sound</span>
                <input
                  type="checkbox"
                  checked={playFeedbackSound}
                  onChange={(event) =>
                    setPlayFeedbackSound(event.target.checked)
                  }
                />
              </strong>
            </label>
          )}
          <div>
            <button className="secondary" onClick={() => setCapture(null)}>
              Cancel
            </button>
            <button
              className="primary"
              disabled={
                saving || (actionType === "application_volume" && !application)
              }
              onClick={save}
            >
              {saving ? "Saving…" : "Save binding"}
            </button>
          </div>
        </div>
      )}

      {!!bindings.length && (
        <div className="system-action-list">
          {bindings.map((binding, index) => {
            const playbackApplication = applications.find(
              (item) => item.executable === binding.targetId,
            );
            const device =
              binding.actionType === "application_volume"
                ? undefined
                : binding.targetType === "specific"
                  ? deviceById.get(binding.targetId ?? "")
                  : devices.find((item) =>
                      binding.targetType === "communications"
                        ? item.defaultCommunications
                        : item.default,
                    );
            return (
              <div
                key={`${binding.scope}-${binding.deviceId}-${binding.physicalButton}-${index}`}
              >
                <i
                  className={
                    binding.actionType === "application_volume"
                      ? playbackApplication?.muted
                        ? "muted"
                        : playbackApplication
                          ? "active"
                          : ""
                      : device?.muted
                        ? "muted"
                        : "active"
                  }
                />
                <span>
                  <strong>{operationLabel(binding)}</strong>
                  <small>
                    {targetLabel(binding)} · {controlLabel(binding)} ·{" "}
                    {binding.scope === "global" ? "Global" : "This profile"}
                    {binding.actionType === "application_volume" &&
                    playbackApplication
                      ? ` · ${playbackApplication.muted ? "muted" : `${playbackApplication.volumePercent}%`}`
                      : ""}
                    {binding.actionType === "application_volume" &&
                    !playbackApplication
                      ? " · not currently playing"
                      : ""}
                    {binding.targetType === "specific" && !device
                      ? " · unavailable"
                      : ""}
                  </small>
                </span>
                <button
                  className="secondary compact"
                  disabled={disabled}
                  onClick={() => void testAction(binding)}
                >
                  Test
                </button>
                {binding.actionType === "microphone" ? (
                  <button
                    type="button"
                    className="system-action-sound"
                    aria-label={`${binding.playFeedbackSound ? "Disable" : "Enable"} confirmation sound`}
                    title={`${binding.playFeedbackSound ? "Disable" : "Enable"} confirmation sound`}
                    disabled={disabled || saving}
                    onClick={() => void toggleFeedbackSound(binding)}
                  >
                    {binding.playFeedbackSound ? <Volume2 /> : <VolumeX />}
                  </button>
                ) : (
                  <span
                    className="system-action-kind"
                    title="Application volume"
                  >
                    <AppWindow />
                  </span>
                )}
                <button
                  type="button"
                  className="system-action-remove"
                  aria-label="Remove system action"
                  title="Remove system action"
                  disabled={disabled || removing}
                  onClick={() => requestRemove(binding)}
                >
                  <Trash2 />
                </button>
              </div>
            );
          })}
        </div>
      )}
      <ConfirmDialog
        confirmation={confirmation}
        busy={saving || removing}
        onClose={() => setConfirmation(null)}
      />
    </section>
  );
}
