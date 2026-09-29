import { AlertTriangle, Code2, Save, SlidersHorizontal } from "lucide-react";
import { useState } from "react";
import { ForceFeedbackEditor } from "../components/ForceFeedbackEditor";
import { WheelImage } from "../components/WheelImage";
import type {
  Binding,
  BindingAssignmentCandidate,
  JsonObject,
  Profile,
} from "../models";
import { BindingEditorPage } from "./BindingEditorPage";

type EditorSection = "bindings" | "ffb" | "json";

export function ProfileEditorPage({
  profile,
  bindings,
  document,
  baseline,
  loading,
  busy,
  wheelImageRevision,
  onClear,
  onAssign,
  onChange,
  onSave,
}: {
  profile?: Profile;
  bindings: Binding[];
  document: JsonObject | null;
  baseline: JsonObject | null;
  loading: boolean;
  busy: boolean;
  wheelImageRevision: number;
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
  const [section, setSection] = useState<EditorSection>("bindings");
  const dirty = Boolean(
    document &&
      baseline &&
      JSON.stringify(document) !== JSON.stringify(baseline),
  );

  return (
    <section className="profile-editor-page">
      {profile && (
        <header className="profile-editor-context">
          <WheelImage
            wheelId={profile.wheelId}
            hasImage={profile.hasWheelImage}
            alt={`${profile.wheelName ?? profile.name} wheel`}
            className="profile-editor-wheel"
            revision={wheelImageRevision}
          />
          <div>
            <span className="eyebrow">Editing profile</span>
            <h2>{profile.name}</h2>
            <div className="profile-editor-context-tags">
              {(profile.wheelName || profile.wheelBrand) && (
                <span className="wheel-tag">
                  {[profile.wheelBrand, profile.wheelName]
                    .filter(Boolean)
                    .join(" ")}
                </span>
              )}
              {profile.classTags.map((tag) => (
                <span className="class-tag" key={tag}>
                  {tag}
                </span>
              ))}
              {!profile.classTags.length && <span>Generic</span>}
            </div>
          </div>
          <span className={dirty ? "editor-state dirty" : "editor-state"}>
            {dirty ? "Unsaved changes" : "Saved profile"}
          </span>
        </header>
      )}
      <div className="profile-editor-tabs">
        <button
          className={section === "bindings" ? "active" : ""}
          onClick={() => setSection("bindings")}
        >
          <SlidersHorizontal /> Bindings
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
        {dirty && (
          <div className="profile-editor-save">
            <span>Unsaved profile changes</span>
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
              Save &amp; use profile
            </button>
          </div>
        )}
      </div>
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
