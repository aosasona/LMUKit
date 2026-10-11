import { invoke } from "@tauri-apps/api/core";
import {
  CirclePlus,
  Gauge,
  LoaderCircle,
  Plus,
  Radio,
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

const emptyControl = (): VirtualEncoderControl => ({
  deviceId: "",
  deviceName: "",
  physicalButton: 0,
});

const newEncoder = (actions: VirtualAction[]): VirtualEncoderGroup => ({
  name: "Encoder 1",
  clockwise: emptyControl(),
  counterclockwise: emptyControl(),
  cycle: emptyControl(),
  modes: [
    {
      name: "Mode 1",
      clockwiseAction: actions[0]?.action ?? "",
      counterclockwiseAction: actions[1]?.action ?? actions[0]?.action ?? "",
    },
  ],
});

function controlLabel(control: VirtualEncoderControl) {
  return control.physicalButton
    ? `${control.deviceName || "Controller"} · B${control.physicalButton}`
    : "Not captured";
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
  const [draft, setDraft] = useState<VirtualEncoderGroup | null>(null);
  const [originalName, setOriginalName] = useState<string | null>(null);
  const [capturing, setCapturing] = useState<
    "clockwise" | "counterclockwise" | "cycle" | null
  >(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

  const load = async () => {
    const [nextActions, nextEncoders] = await Promise.all([
      invoke<VirtualAction[]>("saved_virtual_layout"),
      invoke<VirtualEncoderGroup[]>("virtual_profile_encoder_groups", {
        profileId: profile.id,
      }),
    ]);
    setActions(nextActions);
    setEncoders(nextEncoders);
  };

  useEffect(() => {
    void load().catch((reason) => setError(String(reason)));
  }, [profile.id]);

  const close = async () => {
    if (capturing) await invoke("cancel_binding_lookup");
    setCapturing(null);
    setDraft(null);
    setOriginalName(null);
    setOpen(false);
  };

  const capture = async (kind: "clockwise" | "counterclockwise" | "cycle") => {
    if (!draft) return;
    setCapturing(kind);
    setError(null);
    onNotice(
      kind === "cycle"
        ? "Press and release the button that changes encoder mode."
        : `Turn the encoder ${kind}.`,
    );
    try {
      const control = await invoke<VirtualEncoderControl>(
        "learn_virtual_encoder_control",
      );
      setDraft((current) =>
        current ? { ...current, [kind]: control } : current,
      );
    } catch (reason) {
      if (!String(reason).toLowerCase().includes("cancelled")) {
        setError(String(reason));
      }
    } finally {
      setCapturing(null);
    }
  };

  const save = async () => {
    if (!draft) return;
    setSaving(true);
    setError(null);
    try {
      const next = await invoke<VirtualEncoderGroup[]>(
        "save_virtual_profile_encoder_group",
        { profileId: profile.id, originalName, encoder: draft },
      );
      setEncoders(next);
      setOriginalName(draft.name);
      onNotice(`${draft.name} saved for ${profile.name}.`);
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
      const next = await invoke<VirtualEncoderGroup[]>(
        "remove_virtual_profile_encoder_group",
        { profileId: profile.id, name },
      );
      setEncoders(next);
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

  const valid =
    draft &&
    draft.name.trim() &&
    draft.clockwise.physicalButton > 0 &&
    draft.counterclockwise.physicalButton > 0 &&
    draft.cycle.physicalButton > 0 &&
    draft.modes.length > 0 &&
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
        <Gauge /> Encoder modes{encoders.length ? ` (${encoders.length})` : ""}
      </button>
      {open && (
        <div
          className="dialog-backdrop encoder-dialog-backdrop"
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
                <h2 id="encoder-dialog-title">Encoder modes</h2>
                <p>Turn one encoder to control different action pairs.</p>
              </div>
              <button
                className="dialog-close"
                aria-label="Close"
                onClick={() => void close()}
              >
                <X />
              </button>
            </header>
            <div className="encoder-workspace">
              <aside>
                {encoders.map((encoder) => (
                  <button
                    type="button"
                    className={originalName === encoder.name ? "active" : ""}
                    key={encoder.name}
                    onClick={() => {
                      setDraft(structuredClone(encoder));
                      setOriginalName(encoder.name);
                    }}
                  >
                    <Gauge />
                    <span>
                      <strong>{encoder.name}</strong>
                      <small>{encoder.modes.length} modes</small>
                    </span>
                  </button>
                ))}
                <button
                  type="button"
                  className="encoder-add"
                  onClick={() => {
                    setDraft(newEncoder(actions));
                    setOriginalName(null);
                  }}
                >
                  <CirclePlus /> Add encoder
                </button>
              </aside>
              <main>
                {draft ? (
                  <>
                    {error && <div className="virtual-error">{error}</div>}
                    <label className="encoder-name-field">
                      <span>Encoder name</span>
                      <input
                        value={draft.name}
                        maxLength={120}
                        onChange={(event) =>
                          setDraft({ ...draft, name: event.target.value })
                        }
                      />
                    </label>
                    <div className="encoder-controls">
                      {(
                        [
                          ["clockwise", "Turn clockwise"],
                          ["counterclockwise", "Turn counter-clockwise"],
                          ["cycle", "Change mode button"],
                        ] as const
                      ).map(([kind, label]) => (
                        <div key={kind}>
                          <span>
                            <strong>{label}</strong>
                            <small>{controlLabel(draft[kind])}</small>
                          </span>
                          <button
                            type="button"
                            className="secondary compact"
                            disabled={capturing !== null}
                            onClick={() => void capture(kind)}
                          >
                            {capturing === kind ? (
                              <>
                                <Radio className="spin" /> Listening…
                              </>
                            ) : (
                              "Capture"
                            )}
                          </button>
                        </div>
                      ))}
                    </div>
                    <div className="encoder-modes-head">
                      <span>
                        <strong>Modes</strong>
                        <small>
                          The mode button cycles through this order.
                        </small>
                      </span>
                      <button
                        type="button"
                        className="secondary compact"
                        disabled={draft.modes.length >= 8}
                        onClick={() =>
                          setDraft({
                            ...draft,
                            modes: [
                              ...draft.modes,
                              {
                                name: `Mode ${draft.modes.length + 1}`,
                                clockwiseAction: actions[0]?.action ?? "",
                                counterclockwiseAction:
                                  actions[1]?.action ??
                                  actions[0]?.action ??
                                  "",
                              },
                            ],
                          })
                        }
                      >
                        <Plus /> Add mode
                      </button>
                    </div>
                    <div className="encoder-mode-list">
                      <div className="encoder-mode-labels">
                        <span />
                        <span>Mode name</span>
                        <span>Clockwise action</span>
                        <span>Counter-clockwise action</span>
                        <span />
                      </div>
                      {draft.modes.map((mode, index) => (
                        <div className="encoder-mode-row" key={index}>
                          <b>{index + 1}</b>
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
                          <select
                            aria-label={`${mode.name} clockwise action`}
                            value={mode.clockwiseAction}
                            onChange={(event) => {
                              const modes = [...draft.modes];
                              modes[index] = {
                                ...mode,
                                clockwiseAction: event.target.value,
                              };
                              setDraft({ ...draft, modes });
                            }}
                          >
                            {actions.map((action) => (
                              <option key={action.action}>
                                {action.action}
                              </option>
                            ))}
                          </select>
                          <select
                            aria-label={`${mode.name} counter-clockwise action`}
                            value={mode.counterclockwiseAction}
                            onChange={(event) => {
                              const modes = [...draft.modes];
                              modes[index] = {
                                ...mode,
                                counterclockwiseAction: event.target.value,
                              };
                              setDraft({ ...draft, modes });
                            }}
                          >
                            {actions.map((action) => (
                              <option key={action.action}>
                                {action.action}
                              </option>
                            ))}
                          </select>
                          <button
                            type="button"
                            className="icon-button"
                            aria-label={`Remove ${mode.name}`}
                            disabled={draft.modes.length === 1}
                            onClick={() =>
                              setDraft({
                                ...draft,
                                modes: draft.modes.filter(
                                  (_, item) => item !== index,
                                ),
                              })
                            }
                          >
                            <Trash2 />
                          </button>
                        </div>
                      ))}
                    </div>
                  </>
                ) : (
                  <div className="empty large">
                    <Gauge />
                    <strong>Select or add an encoder</strong>
                  </div>
                )}
              </main>
            </div>
            <footer>
              {draft && originalName ? (
                <button
                  type="button"
                  className="danger ghost"
                  onClick={() =>
                    setConfirmation({
                      title: `Remove ${originalName}?`,
                      description:
                        "This removes the encoder and all of its modes from this profile.",
                      confirmLabel: "Remove encoder",
                      danger: true,
                      onConfirm: () => void remove(originalName),
                    })
                  }
                >
                  <Trash2 /> Remove encoder
                </button>
              ) : (
                <span />
              )}
              <button
                type="button"
                className="primary"
                disabled={busy || saving || routingActive || !valid}
                onClick={() => void save()}
              >
                {saving && <LoaderCircle className="spin" />} Save encoder
              </button>
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
