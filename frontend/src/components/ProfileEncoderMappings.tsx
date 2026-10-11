import { invoke } from "@tauri-apps/api/core";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  ChevronDown,
  CirclePlus,
  Gauge,
  LoaderCircle,
  Plus,
  Radio,
  RotateCcw,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type {
  Profile,
  VirtualAction,
  VirtualEncoderControl,
  VirtualEncoderGroup,
  VirtualProfileMapping,
} from "../models";
import { ConfirmDialog, type Confirmation } from "./ConfirmDialog";

type CaptureKind = "clockwise" | "counterclockwise" | "cycle";
type Step = "details" | "increase" | "decrease" | "modeButton" | "actions";
const emptyControl = (): VirtualEncoderControl => ({
  deviceId: "",
  deviceName: "",
  physicalButton: 0,
});
const newEncoder = (actions: VirtualAction[]): VirtualEncoderGroup => ({
  name: "Encoder 1",
  clockwise: emptyControl(),
  counterclockwise: emptyControl(),
  cycle: null,
  modes: [
    {
      name: "Default",
      clockwiseAction: actions[0]?.action ?? "",
      counterclockwiseAction: actions[1]?.action ?? actions[0]?.action ?? "",
    },
  ],
});
const controlLabel = (control: VirtualEncoderControl | null) =>
  control?.physicalButton
    ? `${control.deviceName || "Controller"} · Button ${control.physicalButton}`
    : "Waiting for a control";

const searchTokens = (value: string) =>
  value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim()
    .split(/\s+/)
    .filter(Boolean);

const controlKey = (control: VirtualEncoderControl | null) =>
  control?.physicalButton
    ? `${control.deviceId}:${control.physicalButton}`
    : null;

function findEncoderConflicts(
  draft: VirtualEncoderGroup | null,
  savedEncoders: VirtualEncoderGroup[],
  buttonMappings: VirtualProfileMapping[],
  originalName: string | null,
) {
  const byControl: Record<CaptureKind, string[]> = {
    clockwise: [],
    counterclockwise: [],
    cycle: [],
  };
  if (!draft) return { byControl, messages: [] as string[] };

  const current: Array<{
    kind: CaptureKind;
    label: string;
    control: VirtualEncoderControl | null;
  }> = [
    { kind: "clockwise", label: "Increase", control: draft.clockwise },
    {
      kind: "counterclockwise",
      label: "Decrease",
      control: draft.counterclockwise,
    },
    { kind: "cycle", label: "Mode button", control: draft.cycle },
  ];
  const uses = new Map<string, string[]>();

  for (const mapping of buttonMappings) {
    const key = `${mapping.deviceId}:${mapping.physicalButton}`;
    const labels = uses.get(key) ?? [];
    labels.push(`${mapping.logicalAction} binding`);
    uses.set(key, labels);
    for (const modifier of mapping.modifiers) {
      const modifierKey = `${modifier.deviceId}:${modifier.physicalButton}`;
      const modifierLabels = uses.get(modifierKey) ?? [];
      modifierLabels.push(`${mapping.logicalAction} modifier`);
      uses.set(modifierKey, modifierLabels);
    }
  }
  for (const encoder of savedEncoders) {
    if (encoder.name === originalName) continue;
    for (const [label, control] of [
      [`${encoder.name} Increase`, encoder.clockwise],
      [`${encoder.name} Decrease`, encoder.counterclockwise],
      [`${encoder.name} mode button`, encoder.cycle],
    ] as const) {
      const key = controlKey(control);
      if (!key) continue;
      const labels = uses.get(key) ?? [];
      labels.push(label);
      uses.set(key, labels);
    }
  }

  for (const entry of current) {
    const key = controlKey(entry.control);
    if (!key) continue;
    const duplicate = current.find(
      (candidate) =>
        candidate.kind !== entry.kind && controlKey(candidate.control) === key,
    );
    if (duplicate) {
      byControl[entry.kind].push(
        `${entry.label} cannot use the same control as ${duplicate.label}.`,
      );
    }
    for (const use of uses.get(key) ?? []) {
      byControl[entry.kind].push(`${entry.label} is already used by ${use}.`);
    }
  }
  return {
    byControl,
    messages: [...new Set(Object.values(byControl).flat())],
  };
}

export function ProfileEncoderMappings({
  profile,
  busy,
  routingActive,
  onNotice,
}: {
  profile: Profile;
  busy: boolean;
  routingActive: boolean;
  onNotice: (message: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [actions, setActions] = useState<VirtualAction[]>([]);
  const [encoders, setEncoders] = useState<VirtualEncoderGroup[]>([]);
  const [buttonMappings, setButtonMappings] = useState<VirtualProfileMapping[]>(
    [],
  );
  const [draft, setDraft] = useState<VirtualEncoderGroup | null>(null);
  const [originalName, setOriginalName] = useState<string | null>(null);
  const [step, setStep] = useState<Step>("details");
  const [activeModeIndex, setActiveModeIndex] = useState(0);
  const [capturing, setCapturing] = useState<CaptureKind | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

  useEffect(() => {
    void Promise.all([
      invoke<VirtualAction[]>("saved_virtual_layout"),
      invoke<VirtualEncoderGroup[]>("virtual_profile_encoder_groups", {
        profileId: profile.id,
      }),
      invoke<VirtualProfileMapping[]>("virtual_profile_mapping", {
        profileId: profile.id,
      }),
    ])
      .then(([nextActions, nextEncoders, nextButtonMappings]) => {
        setActions(nextActions);
        setEncoders(nextEncoders);
        setButtonMappings(nextButtonMappings);
      })
      .catch((reason) => setError(String(reason)));
    return () => {
      void invoke("cancel_binding_lookup");
    };
  }, [profile.id]);

  const close = async () => {
    if (capturing) await invoke("cancel_binding_lookup");
    setCapturing(null);
    setDraft(null);
    setOriginalName(null);
    setOpen(false);
  };
  const capture = async (kind: CaptureKind) => {
    if (!draft) return;
    setCapturing(kind);
    setError(null);
    try {
      const control = await invoke<VirtualEncoderControl>(
        "learn_virtual_encoder_control",
      );
      setDraft((current) =>
        current ? { ...current, [kind]: control } : current,
      );
    } catch (reason) {
      if (!String(reason).toLowerCase().includes("cancelled"))
        setError(String(reason));
    } finally {
      setCapturing(null);
    }
  };
  const cancelCapture = async () => {
    await invoke("cancel_binding_lookup");
    setCapturing(null);
  };
  const beginStep = (nextStep: Step, kind: CaptureKind) => {
    setStep(nextStep);
    void capture(kind);
  };
  const save = async () => {
    if (!draft) return;
    setSaving(true);
    setError(null);
    try {
      setEncoders(
        await invoke<VirtualEncoderGroup[]>(
          "save_virtual_profile_encoder_group",
          {
            profileId: profile.id,
            originalName,
            encoder: draft,
          },
        ),
      );
      onNotice(`${draft.name} saved for ${profile.name}.`);
      setDraft(null);
      setOriginalName(null);
    } catch (reason) {
      setError(String(reason));
      onNotice(String(reason));
    } finally {
      setSaving(false);
    }
  };
  const remove = async (name: string) => {
    setSaving(true);
    try {
      setEncoders(
        await invoke<VirtualEncoderGroup[]>(
          "remove_virtual_profile_encoder_group",
          { profileId: profile.id, name },
        ),
      );
      setDraft(null);
      setOriginalName(null);
      setConfirmation(null);
      onNotice(`${name} removed.`);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSaving(false);
    }
  };
  const addMode = () => {
    if (!draft || draft.modes.length >= 8) return;
    const nextModeIndex = draft.modes.length;
    setDraft({
      ...draft,
      modes: [
        ...draft.modes,
        {
          name: `Mode ${draft.modes.length + 1}`,
          clockwiseAction: actions[0]?.action ?? "",
          counterclockwiseAction:
            actions[1]?.action ?? actions[0]?.action ?? "",
        },
      ],
    });
    setActiveModeIndex(nextModeIndex);
    if (!draft.cycle) beginStep("modeButton", "cycle");
  };
  const removeMode = (index: number) => {
    if (!draft || draft.modes.length === 1) return;
    const modes = draft.modes.filter((_, item) => item !== index);
    setDraft({
      ...draft,
      modes,
      cycle: modes.length === 1 ? null : draft.cycle,
    });
    setActiveModeIndex((current) => Math.min(current, modes.length - 1));
  };
  const canSave =
    draft &&
    draft.name.trim() &&
    draft.clockwise.physicalButton > 0 &&
    draft.counterclockwise.physicalButton > 0 &&
    (draft.modes.length === 1 || Boolean(draft.cycle?.physicalButton)) &&
    draft.modes.every(
      (mode) =>
        mode.name.trim() && mode.clockwiseAction && mode.counterclockwiseAction,
    );
  const conflicts = findEncoderConflicts(
    draft,
    encoders,
    buttonMappings,
    originalName,
  );

  return (
    <>
      <button
        type="button"
        className="secondary compact"
        disabled={busy || routingActive || !actions.length}
        onClick={() => setOpen(true)}
      >
        <Gauge /> Encoders{encoders.length ? ` (${encoders.length})` : ""}
      </button>
      {open && (
        <div
          className="dialog-backdrop"
          role="presentation"
          onPointerDown={(event) => {
            if (event.target === event.currentTarget) void close();
          }}
        >
          <section
            className="encoder-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="encoder-dialog-title"
            onPointerDown={(event) => event.stopPropagation()}
          >
            <header>
              <div>
                <span className="eyebrow">{profile.name}</span>
                <h2 id="encoder-dialog-title">Set up encoders</h2>
                <p>Capture Increase and Decrease, then choose their actions.</p>
              </div>
              <button
                type="button"
                className="dialog-close"
                aria-label="Close"
                onClick={() => void close()}
              >
                <X />
              </button>
            </header>
            {!draft ? (
              <EncoderLanding
                encoders={encoders}
                onAdd={() => {
                  setDraft(newEncoder(actions));
                  setOriginalName(null);
                  setStep("details");
                  setActiveModeIndex(0);
                  setError(null);
                }}
                onEdit={(encoder) => {
                  setDraft(structuredClone(encoder));
                  setOriginalName(encoder.name);
                  setStep("actions");
                  setActiveModeIndex(0);
                  setError(null);
                }}
              />
            ) : (
              <div className="encoder-wizard">
                <WizardProgress
                  step={step}
                  multipleModes={draft.modes.length > 1}
                />
                <div className="encoder-wizard-content">
                  {error && <div className="virtual-error">{error}</div>}
                  {step === "details" && (
                    <section className="encoder-step encoder-details-step">
                      <span className="encoder-step-number">1</span>
                      <h3>Name this encoder</h3>
                      <p>Use a label you recognise, such as Left encoder.</p>
                      <label>
                        <span>Encoder name</span>
                        <input
                          autoFocus
                          value={draft.name}
                          maxLength={120}
                          onChange={(event) =>
                            setDraft({ ...draft, name: event.target.value })
                          }
                        />
                      </label>
                      <button
                        type="button"
                        className="primary encoder-next"
                        disabled={!draft.name.trim()}
                        onClick={() => beginStep("increase", "clockwise")}
                      >
                        Start capture <ArrowRight />
                      </button>
                    </section>
                  )}
                  {step === "increase" && (
                    <CaptureStep
                      title="Capture Increase"
                      description="Turn the encoder once in the direction that should increase the selected action. LMUKit is already listening."
                      control={draft.clockwise}
                      capturing={capturing === "clockwise"}
                      onCapture={() => void capture("clockwise")}
                      onCancel={() => void cancelCapture()}
                      onBack={() => {
                        void cancelCapture();
                        setStep("details");
                      }}
                      onNext={() => beginStep("decrease", "counterclockwise")}
                      nextLabel="Next"
                      warning={conflicts.byControl.clockwise[0]}
                    />
                  )}
                  {step === "decrease" && (
                    <CaptureStep
                      title="Capture Decrease"
                      description="Turn the encoder once in the direction that should decrease the selected action. LMUKit is already listening."
                      control={draft.counterclockwise}
                      capturing={capturing === "counterclockwise"}
                      onCapture={() => void capture("counterclockwise")}
                      onCancel={() => void cancelCapture()}
                      onBack={() => beginStep("increase", "clockwise")}
                      onNext={() => setStep("actions")}
                      nextLabel="Done"
                      warning={conflicts.byControl.counterclockwise[0]}
                    />
                  )}
                  {step === "modeButton" && (
                    <CaptureStep
                      title="Choose a mode button"
                      description="Press the wheel button that should switch this encoder to its next set of actions."
                      control={draft.cycle}
                      capturing={capturing === "cycle"}
                      onCapture={() => void capture("cycle")}
                      onCancel={() => void cancelCapture()}
                      onBack={() => {
                        setDraft({ ...draft, modes: draft.modes.slice(0, -1) });
                        setStep("actions");
                      }}
                      onNext={() => setStep("actions")}
                      nextLabel="Done"
                      warning={conflicts.byControl.cycle[0]}
                    />
                  )}
                  {step === "actions" && (
                    <section className="encoder-step encoder-actions-step">
                      <div className="encoder-actions-heading">
                        <div>
                          <span className="encoder-step-number">3</span>
                          <h3>What should it control?</h3>
                          <p>
                            Search for the action to use for Increase and
                            Decrease. Add modes only when this encoder needs
                            more than one pair.
                          </p>
                        </div>
                        <button
                          type="button"
                          className="ghost compact"
                          onClick={() => beginStep("increase", "clockwise")}
                        >
                          <RotateCcw /> Recapture
                        </button>
                      </div>
                      <div className="encoder-capture-summary">
                        <span>
                          <b>Increase</b>
                          {controlLabel(draft.clockwise)}
                        </span>
                        <span>
                          <b>Decrease</b>
                          {controlLabel(draft.counterclockwise)}
                        </span>
                        {draft.cycle && (
                          <span>
                            <b>Change mode</b>
                            {controlLabel(draft.cycle)}
                          </span>
                        )}
                      </div>
                      {!!conflicts.messages.length && (
                        <div className="encoder-conflicts" role="alert">
                          <strong>Resolve these control conflicts</strong>
                          {conflicts.messages.map((message) => (
                            <span key={message}>{message}</span>
                          ))}
                        </div>
                      )}
                      <div className="encoder-simple-modes">
                        <nav
                          className="encoder-mode-tabs"
                          aria-label="Encoder modes"
                        >
                          {draft.modes.map((mode, index) => (
                            <button
                              type="button"
                              className={
                                index === activeModeIndex ? "active" : ""
                              }
                              key={`${index}-${mode.name}`}
                              onClick={() => setActiveModeIndex(index)}
                            >
                              <span>{index + 1}</span>
                              {mode.name || `Mode ${index + 1}`}
                            </button>
                          ))}
                          <button
                            type="button"
                            className="encoder-mode-tab-add"
                            disabled={draft.modes.length >= 8}
                            onClick={addMode}
                          >
                            <Plus /> Add mode
                          </button>
                        </nav>
                        {draft.modes[activeModeIndex] && (
                          <section className="encoder-mode-card">
                            <header>
                              <label>
                                <span>Mode name</span>
                                <input
                                  aria-label={`Mode ${activeModeIndex + 1} name`}
                                  value={draft.modes[activeModeIndex].name}
                                  onChange={(event) => {
                                    const modes = [...draft.modes];
                                    modes[activeModeIndex] = {
                                      ...modes[activeModeIndex],
                                      name: event.target.value,
                                    };
                                    setDraft({ ...draft, modes });
                                  }}
                                />
                              </label>
                              {draft.modes.length > 1 && (
                                <button
                                  type="button"
                                  className="danger ghost compact"
                                  onClick={() => removeMode(activeModeIndex)}
                                >
                                  <Trash2 /> Remove mode
                                </button>
                              )}
                            </header>
                            <div className="encoder-mode-actions">
                              <ActionPicker
                                label="Increase"
                                value={
                                  draft.modes[activeModeIndex].clockwiseAction
                                }
                                actions={actions}
                                onChange={(value) => {
                                  const modes = [...draft.modes];
                                  modes[activeModeIndex] = {
                                    ...modes[activeModeIndex],
                                    clockwiseAction: value,
                                  };
                                  setDraft({ ...draft, modes });
                                }}
                              />
                              <ActionPicker
                                label="Decrease"
                                value={
                                  draft.modes[activeModeIndex]
                                    .counterclockwiseAction
                                }
                                actions={actions}
                                onChange={(value) => {
                                  const modes = [...draft.modes];
                                  modes[activeModeIndex] = {
                                    ...modes[activeModeIndex],
                                    counterclockwiseAction: value,
                                  };
                                  setDraft({ ...draft, modes });
                                }}
                              />
                            </div>
                          </section>
                        )}
                      </div>
                    </section>
                  )}
                </div>
              </div>
            )}
            <footer>
              {draft ? (
                <button
                  type="button"
                  className="ghost"
                  onClick={() => {
                    setDraft(null);
                    setOriginalName(null);
                  }}
                >
                  <ArrowLeft /> All encoders
                </button>
              ) : (
                <span />
              )}
              <div>
                {draft && originalName && (
                  <button
                    type="button"
                    className="danger ghost"
                    onClick={() =>
                      setConfirmation({
                        title: `Remove ${originalName}?`,
                        description:
                          "This removes the encoder and all its modes from this profile.",
                        confirmLabel: "Remove encoder",
                        danger: true,
                        onConfirm: () => void remove(originalName),
                      })
                    }
                  >
                    <Trash2 /> Remove
                  </button>
                )}
                {draft && step === "actions" && (
                  <button
                    type="button"
                    className="primary"
                    disabled={
                      busy ||
                      saving ||
                      routingActive ||
                      !canSave ||
                      conflicts.messages.length > 0
                    }
                    onClick={() => void save()}
                  >
                    {saving ? <LoaderCircle className="spin" /> : <Check />}{" "}
                    Save encoder
                  </button>
                )}
              </div>
            </footer>
          </section>
        </div>
      )}
      <ConfirmDialog
        confirmation={confirmation}
        busy={saving}
        onClose={() => setConfirmation(null)}
      />
    </>
  );
}

function EncoderLanding({
  encoders,
  onAdd,
  onEdit,
}: {
  encoders: VirtualEncoderGroup[];
  onAdd: () => void;
  onEdit: (encoder: VirtualEncoderGroup) => void;
}) {
  return (
    <div className="encoder-landing">
      <div className="encoder-landing-copy">
        <Gauge />
        <div>
          <h3>Your encoders</h3>
          <p>Each encoder starts with a simple Increase and Decrease pair.</p>
        </div>
        <button type="button" className="primary" onClick={onAdd}>
          <CirclePlus /> Add encoder
        </button>
      </div>
      <div className="encoder-card-grid">
        {encoders.map((encoder) => (
          <button
            type="button"
            key={encoder.name}
            onClick={() => onEdit(encoder)}
          >
            <Gauge />
            <span>
              <strong>{encoder.name}</strong>
              <small>
                {encoder.modes.length === 1
                  ? "Increase / Decrease"
                  : `${encoder.modes.length} modes`}
              </small>
            </span>
            <ArrowRight />
          </button>
        ))}
        {!encoders.length && (
          <div className="encoder-empty">
            No encoders configured for this profile yet.
          </div>
        )}
      </div>
    </div>
  );
}
function WizardProgress({
  step,
  multipleModes,
}: {
  step: Step;
  multipleModes: boolean;
}) {
  const current =
    step === "details" ? 1 : step === "increase" || step === "decrease" ? 2 : 3;
  return (
    <div className="encoder-progress">
      {["Name", "Capture", multipleModes ? "Modes" : "Actions"].map(
        (label, index) => (
          <span className={current >= index + 1 ? "active" : ""} key={label}>
            <b>{index + 1}</b>
            {label}
          </span>
        ),
      )}
    </div>
  );
}
function CaptureStep({
  title,
  description,
  control,
  capturing,
  onCapture,
  onCancel,
  onBack,
  onNext,
  nextLabel,
  warning,
}: {
  title: string;
  description: string;
  control: VirtualEncoderControl | null;
  capturing: boolean;
  onCapture: () => void;
  onCancel: () => void;
  onBack: () => void;
  onNext: () => void;
  nextLabel: "Next" | "Done";
  warning?: string;
}) {
  const captured = Boolean(control?.physicalButton);
  return (
    <section className="encoder-step encoder-capture-step">
      <span className="encoder-step-number">2</span>
      <h3>{title}</h3>
      <p>{description}</p>
      <div
        className={captured ? "encoder-listener captured" : "encoder-listener"}
      >
        {capturing ? (
          <Radio className="spin" />
        ) : captured ? (
          <Check />
        ) : (
          <Gauge />
        )}
        <span>
          <strong>
            {capturing
              ? "Listening…"
              : captured
                ? "Control captured"
                : "Ready to capture"}
          </strong>
          <small>
            {capturing ? "Move the encoder once" : controlLabel(control)}
          </small>
        </span>
      </div>
      {warning && <div className="encoder-capture-warning">{warning}</div>}
      <div className="encoder-capture-actions">
        {!capturing && (
          <button type="button" className="ghost" onClick={onBack}>
            <ArrowLeft /> Back
          </button>
        )}
        {capturing ? (
          <button type="button" className="secondary" onClick={onCancel}>
            Cancel capture
          </button>
        ) : !captured ? (
          <button type="button" className="primary" onClick={onCapture}>
            <Radio /> Listen again
          </button>
        ) : (
          <>
            <button type="button" className="secondary" onClick={onCapture}>
              <RotateCcw /> Try again
            </button>
            <button
              type="button"
              className="primary"
              disabled={Boolean(warning)}
              onClick={onNext}
            >
              {nextLabel} {nextLabel === "Next" ? <ArrowRight /> : <Check />}
            </button>
          </>
        )}
      </div>
    </section>
  );
}
function ActionPicker({
  label,
  value,
  actions,
  onChange,
}: {
  label: string;
  value: string;
  actions: VirtualAction[];
  onChange: (value: string) => void;
}) {
  const [query, setQuery] = useState("");
  const pickerRef = useRef<HTMLDetailsElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const queryTokens = searchTokens(query);
  const visibleActions = actions.filter((action) => {
    const candidateTokens = searchTokens(action.action);
    return queryTokens.every((queryToken) =>
      candidateTokens.some((candidate) => candidate.includes(queryToken)),
    );
  });
  const selected = actions.find((action) => action.action === value);

  useEffect(() => {
    const closeOnOutsideClick = (event: PointerEvent) => {
      if (!pickerRef.current?.contains(event.target as Node)) {
        pickerRef.current?.removeAttribute("open");
      }
    };
    window.addEventListener("pointerdown", closeOnOutsideClick);
    return () => window.removeEventListener("pointerdown", closeOnOutsideClick);
  }, []);

  return (
    <div className="encoder-action-field">
      <span>{label}</span>
      <details
        className="encoder-action-picker"
        ref={pickerRef}
        onToggle={(event) => {
          if (event.currentTarget.open)
            window.setTimeout(() => searchRef.current?.focus(), 0);
        }}
      >
        <summary>
          <span>
            <strong>{selected?.action || "Choose an action"}</strong>
            {selected && <small>vJoy B{selected.virtualButton}</small>}
          </span>
          <ChevronDown />
        </summary>
        <div className="encoder-action-menu">
          <label>
            <Search />
            <input
              ref={searchRef}
              value={query}
              placeholder="Search actions"
              onChange={(event) => setQuery(event.target.value)}
              onClick={(event) => event.stopPropagation()}
            />
          </label>
          <div>
            {visibleActions.map((action) => (
              <button
                type="button"
                className={action.action === value ? "active" : ""}
                key={action.action}
                onClick={(event) => {
                  onChange(action.action);
                  setQuery("");
                  event.currentTarget
                    .closest("details")
                    ?.removeAttribute("open");
                }}
              >
                <span>{action.action}</span>
                <small>B{action.virtualButton}</small>
              </button>
            ))}
            {!visibleActions.length && <p>No matching actions.</p>}
          </div>
        </div>
      </details>
    </div>
  );
}
