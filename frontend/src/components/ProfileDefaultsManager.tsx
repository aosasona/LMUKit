import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft, Star } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { displayClassTag } from "../lib/classTags";
import type { Profile, VirtualProfileDefault, Workspace } from "../models";
import { SearchSelect } from "./SearchSelect";

export function ProfileDefaultsManager({
  profiles,
  workspaceKind,
  onNotice,
  onClose,
  onChanged,
}: {
  profiles: Profile[];
  workspaceKind: Workspace["kind"];
  onNotice: (message: string) => void;
  onClose: () => void;
  onChanged: () => Promise<void>;
}) {
  const isLmu = workspaceKind === "le_mans_ultimate";
  const [defaults, setDefaults] = useState<VirtualProfileDefault[]>([]);
  const [busyTag, setBusyTag] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const tags = useMemo(
    () =>
      Array.from(
        new Set(profiles.flatMap((profile) => profile.classTags)),
      ).sort((left, right) =>
        (isLmu ? displayClassTag(left) : left).localeCompare(
          isLmu ? displayClassTag(right) : right,
        ),
      ),
    [isLmu, profiles],
  );

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
      onNotice(
        profile
          ? profile.name +
              " is now selected automatically for " +
              (isLmu ? displayClassTag(classTag) : classTag) +
              "."
          : (isLmu ? displayClassTag(classTag) : classTag) +
              " automatic selection cleared.",
      );
    } catch (reason) {
      const message = String(reason);
      setError(message);
      onNotice(message);
    } finally {
      setBusyTag(null);
    }
  };

  return (
    <section className="profile-defaults-manager">
      <header>
        <span className="profile-defaults-heading-icon">
          <Star />
        </span>
        <div>
          <span className="eyebrow">Automatic selection</span>
          <h2>{isLmu ? "Class defaults" : "Car-tag defaults"}</h2>
          <p>
            Choose the profile Pivot should activate for each detected{" "}
            {isLmu ? "class" : "car tag"}. Every profile is available; choosing
            one also attaches this tag to it when needed.
          </p>
        </div>
        <button type="button" className="secondary compact" onClick={onClose}>
          <ArrowLeft /> Back to profiles
        </button>
      </header>
      {error && <div className="virtual-error">{error}</div>}
      <div className="profile-default-assignment-list">
        {tags.map((classTag) => {
          const assigned = defaults.find(
            (entry) => entry.classTag.toLowerCase() === classTag.toLowerCase(),
          );
          return (
            <div className={busyTag === classTag ? "busy" : ""} key={classTag}>
              <span className="profile-default-class">
                <strong
                  className={`class-tag class-${classTag.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`}
                >
                  {isLmu ? displayClassTag(classTag) : classTag}
                </strong>
                <small>
                  {assigned
                    ? "Currently " + assigned.profileName
                    : "Manual selection only"}
                </small>
              </span>
              <SearchSelect
                label="Default profile"
                value={assigned?.profileName ?? null}
                options={profiles.map((profile) => profile.name)}
                allLabel="Manual selection only"
                onChange={(value) => void assign(classTag, value)}
              />
            </div>
          );
        })}
        {!tags.length && (
          <div className="empty">
            Add {isLmu ? "class" : "car"} tags to profiles before assigning
            automatic defaults.
          </div>
        )}
      </div>
    </section>
  );
}
