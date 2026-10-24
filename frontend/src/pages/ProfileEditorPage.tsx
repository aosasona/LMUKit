import {
  AlertTriangle,
  Code2,
  FileJson,
  Save,
  SlidersHorizontal,
  Workflow,
} from "lucide-react";
import { useEffect, useState } from "react";
import { ForceFeedbackEditor } from "../components/ForceFeedbackEditor";
import type { InputMode } from "../components/InputModeSwitch";
import { ProfileVirtualMappings } from "../components/ProfileVirtualMappings";
import { WheelImage } from "../components/WheelImage";
import { displayClassTag } from "../lib/classTags";
import type {
  Binding,
  BindingAssignmentCandidate,
  JsonObject,
  Profile,
} from "../models";
import { BindingEditorPage } from "./BindingEditorPage";

type EditorSection = "virtual" | "legacy";
type LegacySection = "bindings" | "ffb" | "json";

export function ProfileEditorPage({
  profile,
  bindings,
  document,
  baseline,
  loading,
  busy,
  inputMode,
  wheelImageRevision,
  onClear,
  onAssign,
  onChange,
  onSave,
  onNotice,
}: {
  profile?: Profile;
  bindings: Binding[];
  document: JsonObject | null;
  baseline: JsonObject | null;
  loading: boolean;
  busy: boolean;
  inputMode: InputMode;
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
  onNotice: (message: string) => void;
}) {
  const [section, setSection] = useState<EditorSection>("virtual");
  const [legacySection, setLegacySection] = useState<LegacySection>("bindings");
  const dirty = Boolean(
    document &&
      baseline &&
      JSON.stringify(document) !== JSON.stringify(baseline),
  );

  useEffect(() => {
    setSection("virtual");
    setLegacySection("bindings");
  }, [profile?.id]);

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
                  {displayClassTag(tag)}
                </span>
              ))}
              {!profile.classTags.length && <span>Generic</span>}
            </div>
          </div>
          <span className={dirty ? "editor-state dirty" : "editor-state"}>
            {dirty ? "Unsaved LMU file changes" : "Profile saved"}
          </span>
        </header>
      )}
      <div className="profile-editor-tabs">
        <button
          className={section === "virtual" ? "active" : ""}
          onClick={() => setSection("virtual")}
        >
          <Workflow /> Virtual mappings
        </button>
        <button
          className={section === "legacy" ? "active legacy" : "legacy"}
          onClick={() => setSection("legacy")}
        >
          <FileJson /> LMU file tools
        </button>
        {section === "legacy" && dirty && (
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
      {section === "virtual" && profile && (
        <div className="profile-editor-virtual">
          <div className="profile-editor-intro">
            <span className="eyebrow">Current mapping system</span>
            <h3>Map physical controls to named virtual actions</h3>
            <p>
              These mappings are used by live remapping and can differ for every
              profile without changing LMU's saved bindings.
            </p>
          </div>
          <ProfileVirtualMappings
            key={`editor-virtual-${profile.id}`}
            profile={profile}
            busy={busy}
            inputMode={inputMode}
            onNotice={onNotice}
            presentation="workspace"
          />
        </div>
      )}
      {section === "virtual" && !profile && (
        <div className="empty large">
          <AlertTriangle />
          <strong>Select a profile to edit its virtual mappings</strong>
        </div>
      )}
      {section === "legacy" && (
        <section className="profile-editor-legacy">
          <header className="profile-editor-legacy-head">
            <div>
              <span className="eyebrow">Legacy JSON configuration</span>
              <h3>LMU file tools</h3>
              <p>
                Edit the saved DirectInput file used by Legacy mode. These
                controls do not change Virtual Input mappings.
              </p>
            </div>
            <div className="profile-editor-legacy-tabs">
              <button
                className={legacySection === "bindings" ? "active" : ""}
                onClick={() => setLegacySection("bindings")}
              >
                <SlidersHorizontal /> File bindings
              </button>
              <button
                className={legacySection === "ffb" ? "active" : ""}
                onClick={() => setLegacySection("ffb")}
              >
                <Save /> Force feedback
              </button>
              <button
                className={legacySection === "json" ? "active" : ""}
                onClick={() => setLegacySection("json")}
              >
                <Code2 /> JSON preview
              </button>
            </div>
          </header>
          {legacySection === "bindings" && (
            <BindingEditorPage
              profile={profile}
              bindings={bindings}
              busy={busy}
              onClear={onClear}
              onAssign={onAssign}
            />
          )}
          {legacySection !== "bindings" && loading && (
            <div className="empty large">
              <strong>Loading profile…</strong>
            </div>
          )}
          {legacySection !== "bindings" && !loading && !document && (
            <div className="empty large">
              <AlertTriangle />
              <strong>The profile document could not be loaded</strong>
            </div>
          )}
          {legacySection === "ffb" && document && (
            <ForceFeedbackEditor document={document} onChange={onChange} />
          )}
          {legacySection === "json" && document && (
            <section className="json-preview panel">
              <span className="eyebrow">Read-only document</span>
              <h2>Complete profile JSON</h2>
              <p>
                Unknown and device-specific fields are preserved when you save.
              </p>
              <pre>{JSON.stringify(document, null, 2)}</pre>
            </section>
          )}
        </section>
      )}
    </section>
  );
}
