import { invoke } from "@tauri-apps/api/core";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  CirclePlus,
  Gauge,
  LoaderCircle,
  Plus,
  Radio,
  RotateCcw,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import type {
  Profile,
  VirtualAction,
  VirtualEncoderControl,
  VirtualEncoderGroup,
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
  const [draft, setDraft] = useState<VirtualEncoderGroup | null>(null);
  const [originalName, setOriginalName] = useState<string | null>(null);
  const [step, setStep] = useState<Step>("details");
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
    ])
      .then(([nextActions, nextEncoders]) => {
        setActions(nextActions);
        setEncoders(nextEncoders);
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
    if (!draft.cycle) setStep("modeButton");
  };
  const removeMode = (index: number) => {
    if (!draft || draft.modes.length === 1) return;
    const modes = draft.modes.filter((_, item) => item !== index);
    setDraft({
      ...draft,
      modes,
      cycle: modes.length === 1 ? null : draft.cycle,
    });
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
                <p>Capture each direction, then choose what it controls.</p>
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
                  setError(null);
                }}
                onEdit={(encoder) => {
                  setDraft(structuredClone(encoder));
                  setOriginalName(encoder.name);
                  setStep("actions");
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
                        onClick={() => setStep("increase")}
                      >
                        Capture directions <ArrowRight />
                      </button>
                    </section>
                  )}
                  {step === "increase" && (
                    <CaptureStep
                      title="Turn it clockwise"
                      description="Turn the encoder one click in the direction you use to increase a value."
                      control={draft.clockwise}
                      capturing={capturing === "clockwise"}
                      onCapture={() => void capture("clockwise")}
                      onCancel={() => {
                        void invoke("cancel_binding_lookup");
                        setCapturing(null);
                      }}
                      onBack={() => setStep("details")}
                      onNext={() => setStep("decrease")}
                    />
                  )}
                  {step === "decrease" && (
                    <CaptureStep
                      title="Turn it counter-clockwise"
                      description="Now turn the same encoder one click in the direction you use to decrease a value."
                      control={draft.counterclockwise}
                      capturing={capturing === "counterclockwise"}
                      onCapture={() => void capture("counterclockwise")}
                      onCancel={() => {
                        void invoke("cancel_binding_lookup");
                        setCapturing(null);
                      }}
                      onBack={() => setStep("increase")}
                      onNext={() => setStep("actions")}
                    />
                  )}
                  {step === "modeButton" && (
                    <CaptureStep
                      title="Choose a mode button"
                      description="Press the wheel button that should switch this encoder to its next set of actions."
                      control={draft.cycle}
                      capturing={capturing === "cycle"}
                      onCapture={() => void capture("cycle")}
                      onCancel={() => {
                        void invoke("cancel_binding_lookup");
                        setCapturing(null);
                      }}
                      onBack={() => {
                        setDraft({ ...draft, modes: draft.modes.slice(0, -1) });
                        setStep("actions");
                      }}
                      onNext={() => setStep("actions")}
                    />
                  )}
                  {step === "actions" && (
                    <section className="encoder-step encoder-actions-step">
                      <div className="encoder-actions-heading">
                        <div>
                          <span className="encoder-step-number">3</span>
                          <h3>What should it control?</h3>
                          <p>
                            Choose an action for each direction. Most encoders
                            need only this one pair.
                          </p>
                        </div>
                        <button
                          type="button"
                          className="ghost compact"
                          onClick={() => setStep("increase")}
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
                      <div className="encoder-simple-modes">
                        {draft.modes.map((mode, index) => (
                          <section key={index}>
                            <header>
                              {draft.modes.length > 1 ? (
                                <input
                                  aria-label={`Mode ${index + 1} name`}
                                  value={mode.name}
                                  onChange={(event) => {
                                    const modes = [...draft.modes];
                                    modes[index] = {
                                      ...mode,
                                      name: event.target.value,
                                    };
                                    setDraft({ ...draft, modes });
                                  }}
                                />
                              ) : (
                                <strong>Actions</strong>
                              )}
                              {draft.modes.length > 1 && (
                                <button
                                  type="button"
                                  className="icon-button"
                                  aria-label={`Remove ${mode.name}`}
                                  onClick={() => removeMode(index)}
                                >
                                  <Trash2 />
                                </button>
                              )}
                            </header>
                            <div>
                              <ActionSelect
                                label="Increase"
                                value={mode.clockwiseAction}
                                actions={actions}
                                onChange={(value) => {
                                  const modes = [...draft.modes];
                                  modes[index] = {
                                    ...mode,
                                    clockwiseAction: value,
                                  };
                                  setDraft({ ...draft, modes });
                                }}
                              />
                              <ActionSelect
                                label="Decrease"
                                value={mode.counterclockwiseAction}
                                actions={actions}
                                onChange={(value) => {
                                  const modes = [...draft.modes];
                                  modes[index] = {
                                    ...mode,
                                    counterclockwiseAction: value,
                                  };
                                  setDraft({ ...draft, modes });
                                }}
                              />
                            </div>
                          </section>
                        ))}
                      </div>
                      <button
                        type="button"
                        className="secondary encoder-add-mode"
                        disabled={draft.modes.length >= 8}
                        onClick={addMode}
                      >
                        <Plus /> Add another mode
                      </button>
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
                    disabled={busy || saving || routingActive || !canSave}
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
}: {
  title: string;
  description: string;
  control: VirtualEncoderControl | null;
  capturing: boolean;
  onCapture: () => void;
  onCancel: () => void;
  onBack: () => void;
  onNext: () => void;
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
      <div className="encoder-capture-actions">
        <button type="button" className="ghost" onClick={onBack}>
          <ArrowLeft /> Back
        </button>
        {capturing ? (
          <button type="button" className="secondary" onClick={onCancel}>
            Cancel capture
          </button>
        ) : !captured ? (
          <button type="button" className="primary" onClick={onCapture}>
            <Radio /> Start capture
          </button>
        ) : (
          <>
            <button type="button" className="secondary" onClick={onCapture}>
              <RotateCcw /> Try again
            </button>
            <button type="button" className="primary" onClick={onNext}>
              Confirm &amp; continue <ArrowRight />
            </button>
          </>
        )}
      </div>
    </section>
  );
}
function ActionSelect({
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
  return (
    <label>
      <span>{label}</span>
      <select value={value} onChange={(event) => onChange(event.target.value)}>
        {actions.map((action) => (
          <option key={action.action}>{action.action}</option>
        ))}
      </select>
    </label>
  );
}
