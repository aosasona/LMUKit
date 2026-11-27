import { invoke } from "@tauri-apps/api/core";
import { RefreshCw, Sparkles } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { SearchSelect } from "../components/SearchSelect";
import { WheelImage } from "../components/WheelImage";
import { DEFAULT_LMU_CLASS_TAGS, displayClassTag } from "../lib/classTags";
import type {
  AutoProfileSelectionStatus,
  Profile,
  VirtualProfileDefault,
  Workspace,
} from "../models";

function classSlug(tag: string) {
  return tag.toLowerCase().replace(/[^a-z0-9]+/g, "-");
}

function wheelLabel(profile: Profile | undefined) {
  if (!profile) return "No profile assigned";
  return (
    [profile.wheelBrand, profile.wheelName].filter(Boolean).join(" ") ||
    "No wheel assigned"
  );
}

export function AutomaticSelectionPage({
  profiles,
  workspaceKind,
  inputMode,
  status,
  busy,
  wheelImageRevision,
  onSetEnabled,
  onResume,
  onNotice,
  onChanged,
}: {
  profiles: Profile[];
  workspaceKind: Workspace["kind"];
  inputMode: "legacy" | "virtual";
  status: AutoProfileSelectionStatus;
  busy: boolean;
  wheelImageRevision: number;
  onSetEnabled: (enabled: boolean) => void;
  onResume: () => void;
  onNotice: (message: string) => void;
  onChanged: () => Promise<void>;
}) {
  const isLmu = workspaceKind === "le_mans_ultimate";
  const [defaults, setDefaults] = useState<VirtualProfileDefault[]>([]);
  const [busyTag, setBusyTag] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const tags = useMemo(() => {
    const known = isLmu ? [...DEFAULT_LMU_CLASS_TAGS] : [];
    return Array.from(
      new Set([...known, ...profiles.flatMap((profile) => profile.classTags)]),
    ).sort((left, right) =>
      (isLmu ? displayClassTag(left) : left).localeCompare(
        isLmu ? displayClassTag(right) : right,
      ),
    );
  }, [isLmu, profiles]);

  useEffect(() => {
    setError(null);
    void invoke<VirtualProfileDefault[]>("virtual_profile_defaults")
      .then(setDefaults)
      .catch((reason) => setError(String(reason)));
  }, [profiles]);

  const assign = async (classTag: string, profileName: string | null) => {
    const existing = defaults.find(
      (entry) => entry.classTag.toLowerCase() === classTag.toLowerCase(),
    );
    if (!profileName && !existing) return;
    setBusyTag(classTag);
    setError(null);
    try {
      const profile = profiles.find(
        (candidate) => candidate.name === profileName,
      );
      if (
        profile &&
        !profile.classTags.some(
          (tag) => tag.toLowerCase() === classTag.toLowerCase(),
        )
      ) {
        await invoke("set_profile_categories", {
          profileId: profile.id,
          classTags: [...profile.classTags, classTag],
          customTags: profile.customTags,
        });
      }
      const next = await invoke<VirtualProfileDefault[]>(
        profile
          ? "set_virtual_profile_default"
          : "clear_virtual_profile_default",
        profile ? { profileId: profile.id, classTag } : { classTag },
      );
      setDefaults(next);
      await onChanged();
      const label = isLmu ? displayClassTag(classTag) : classTag;
      onNotice(
        profile
          ? `${profile.name} will be selected automatically for ${label}.`
          : `${label} automatic selection cleared.`,
      );
    } catch (reason) {
      const message = String(reason);
      setError(message);
      onNotice(message);
    } finally {
      setBusyTag(null);
    }
  };

  const configuredCount = defaults.filter((entry) =>
    tags.some((tag) => tag.toLowerCase() === entry.classTag.toLowerCase()),
  ).length;

  return (
    <section className="automatic-selection-page">
      <header className="automatic-selection-hero">
        <div className="automatic-selection-heading">
          <span className="automatic-selection-icon">
            <Sparkles />
          </span>
          <div>
            <span className="eyebrow">Hands-free profile switching</span>
            <h2>
              {isLmu
                ? "Match profiles to car classes"
                : "Match profiles to cars"}
            </h2>
            <p>
              Pivot keeps your current wheel mappings and activates the profile
              you choose when{" "}
              {isLmu ? "LMU confirms a class" : "AC confirms a car tag"}.
            </p>
          </div>
        </div>
        <div className="automatic-selection-control">
          <div>
            <strong>
              {status.enabled
                ? "Automatic selection on"
                : "Automatic selection off"}
            </strong>
            <span>
              {inputMode !== "virtual"
                ? "Switch to Virtual Input to use automatic selection."
                : status.message}
            </span>
          </div>
          {status.enabled && status.paused && (
            <button
              type="button"
              className="secondary compact"
              disabled={busy || inputMode !== "virtual"}
              onClick={onResume}
            >
              <RefreshCw /> Resume auto
            </button>
          )}
          <label className="inline-switch">
            <span>Enable</span>
            <input
              type="checkbox"
              checked={status.enabled}
              disabled={busy || inputMode !== "virtual"}
              onChange={(event) => onSetEnabled(event.target.checked)}
            />
          </label>
        </div>
      </header>

      <div className="automatic-selection-summary">
        <div>
          <strong>{configuredCount}</strong>
          <span>assigned</span>
        </div>
        <p>
          Pick any profile below. If it does not already carry the matching tag,
          Pivot adds it automatically.
        </p>
      </div>

      {error && <div className="virtual-error">{error}</div>}
      <div className="automatic-default-grid">
        {tags.map((classTag) => {
          const assigned = defaults.find(
            (entry) => entry.classTag.toLowerCase() === classTag.toLowerCase(),
          );
          const profile = profiles.find(
            (candidate) => candidate.id === assigned?.profileId,
          );
          const label = isLmu ? displayClassTag(classTag) : classTag;
          return (
            <article
              className={`automatic-default-card class-${classSlug(classTag)} ${busyTag === classTag ? "busy" : ""}`}
              key={classTag}
            >
              <header>
                <span className="automatic-class-tag">{label}</span>
                <em>{profile ? "Assigned" : "Manual only"}</em>
              </header>
              <div className="automatic-profile-preview">
                <WheelImage
                  wheelId={profile?.wheelId ?? null}
                  hasImage={profile?.hasWheelImage ?? false}
                  alt={`${profile?.wheelName ?? profile?.name ?? label} wheel`}
                  className="automatic-wheel-image"
                  revision={wheelImageRevision}
                />
                <div>
                  <small>Selected profile</small>
                  <h3>{profile?.name ?? "Choose a profile"}</h3>
                  <span>{wheelLabel(profile)}</span>
                </div>
              </div>
              <SearchSelect
                label="Profile"
                value={assigned?.profileName ?? null}
                options={profiles.map((candidate) => candidate.name)}
                allLabel="Manual selection only"
                onChange={(value) => void assign(classTag, value)}
              />
            </article>
          );
        })}
        {!tags.length && (
          <div className="automatic-selection-empty empty large">
            Add a car tag to a profile to create an automatic assignment.
          </div>
        )}
      </div>
    </section>
  );
}
