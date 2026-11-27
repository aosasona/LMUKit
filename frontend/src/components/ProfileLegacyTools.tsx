import { AlertTriangle, Code2, Save, SlidersHorizontal } from "lucide-react";
import { useEffect, useState } from "react";
import type {
  Binding,
  BindingAssignmentCandidate,
  JsonObject,
  Profile,
} from "../models";
import { BindingEditorPage } from "../pages/BindingEditorPage";
import { ForceFeedbackEditor } from "./ForceFeedbackEditor";

type LegacySection = "bindings" | "ffb" | "json";

export function ProfileLegacyTools({
  profile,
  bindings,
  document,
  baseline,
  loading,
  busy,
  onClear,
  onAssign,
  onChange,
  onSave,
}: {
  profile: Profile;
  bindings: Binding[];
  document: JsonObject | null;
  baseline: JsonObject | null;
  loading: boolean;
  busy: boolean;
  onClear: (profile: Profile, binding: Binding) => void;
  onAssign: (
    profile: Profile,
    action: string,
    alternate: boolean,
    candidate: BindingAssignmentCandidate,
  ) => Promise<void>;
  onChange: (document: JsonObject) => void;
  onSave: (activate: boolean) => void;
}) {
  const [section, setSection] = useState<LegacySection>("bindings");
  const dirty = Boolean(
    document &&
      baseline &&
      JSON.stringify(document) !== JSON.stringify(baseline),
  );

  useEffect(() => setSection("bindings"), [profile.id]);

  return (
    <section className="profile-editor-legacy embedded">
      <header className="profile-editor-legacy-head">
        <div>
          <span className="eyebrow">Saved DirectInput configuration</span>
          <h3>LMU file tools</h3>
          <p>
            Edit the profile file used by Legacy mode. This does not change its
            live Virtual Input mappings.
          </p>
        </div>
        <div className="profile-editor-legacy-tabs">
          <button
            className={section === "bindings" ? "active" : ""}
            onClick={() => setSection("bindings")}
          >
            <SlidersHorizontal /> File bindings
          </button>
          <button
            className={section === "ffb" ? "active" : ""}
            onClick={() => setSection("ffb")}
          >
            <Save /> Force feedback
          </button>
          <button
            className={section === "json" ? "active" : ""}
            onClick={() => setSection("json")}
          >
            <Code2 /> JSON preview
          </button>
        </div>
      </header>
      {dirty && (
        <div className="profile-editor-save embedded-save">
          <span>Unsaved LMU file changes</span>
          <button
            className="secondary"
            disabled={busy}
            onClick={() => onSave(false)}
          >
            Save
          </button>
          <button
            className="primary"
            disabled={busy}
            onClick={() => onSave(true)}
          >
            Save &amp; prepare profile
          </button>
        </div>
      )}
      {section === "bindings" && (
        <BindingEditorPage
          profile={profile}
          bindings={bindings}
          busy={busy}
          onClear={onClear}
          onAssign={onAssign}
        />
      )}
      {section !== "bindings" && loading && (
        <div className="empty large">
          <strong>Loading profile…</strong>
        </div>
      )}
      {section !== "bindings" && !loading && !document && (
        <div className="empty large">
          <AlertTriangle />
          <strong>The profile document could not be loaded</strong>
        </div>
      )}
      {section === "ffb" && document && (
        <ForceFeedbackEditor document={document} onChange={onChange} />
      )}
      {section === "json" && document && (
        <section className="json-preview panel">
          <span className="eyebrow">Read-only document</span>
          <h2>Complete profile JSON</h2>
          <p>Unknown and device-specific fields are preserved when you save.</p>
          <pre>{JSON.stringify(document, null, 2)}</pre>
        </section>
      )}
    </section>
  );
}
