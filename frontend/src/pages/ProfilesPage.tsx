import {
  FolderOpen,
  Gamepad2,
  Image as ImageIcon,
  Search,
  Upload,
} from "lucide-react";
import { useState } from "react";
import { ActionMenu } from "../components/ActionMenu";
import { ConfirmDialog, type Confirmation } from "../components/ConfirmDialog";
import { TagPicker } from "../components/TagPicker";
import { WheelImage } from "../components/WheelImage";
import { SearchSelect } from "../components/SearchSelect";
import type { Device, Profile, Wheel } from "../models";

const DEFAULT_CLASSES = ["GT3", "GTE", "LMP3", "LMP2", "HY"] as const;

function wheelLabel(profile: Profile) {
  return (
    [profile.wheelBrand, profile.wheelName].filter(Boolean).join(" ") || null
  );
}

function classSlug(tag: string) {
  return tag.toLowerCase().replace(/[^a-z0-9]+/g, "-");
}

type Props = {
  profiles: Profile[];
  wheels: Wheel[];
  selected?: Profile;
  wheelImageUrl: string | null;
  wheelImageRevision: number;
  activeId: string | null;
  activeDirty: boolean | null;
  selectedId: string | null;
  query: string;
  busy: boolean;
  loading: boolean;
  setQuery: (value: string) => void;
  setSelectedId: (value: string) => void;
  onActivate: (profile: Profile) => void;
  onImport: (file: File) => void;
  onCapture: (name: string) => void;
  onUpdate: (profile: Profile) => void;
  onDelete: (profile: Profile) => void;
  onReveal: () => void;
  onRemoveDevice: (profile: Profile, device: Device) => void;
  onSaveWheelImage: (profile: Profile, file: File) => void;
  onRemoveWheelImage: (profile: Profile) => void;
  onSaveCategories: (
    profile: Profile,
    profileName: string,
    classTags: string[],
    customTags: string[],
  ) => void;
  onCreateWheel: (profile: Profile, brand: string, name: string) => void;
  onAssignWheel: (profile: Profile, wheel: Wheel | null) => void;
};

export function ProfilesPage(props: Props) {
  const {
    profiles,
    wheels,
    selected,
    wheelImageUrl,
    wheelImageRevision,
    activeId,
    activeDirty,
    selectedId,
    query,
    busy,
    loading,
    setQuery,
    setSelectedId,
    onActivate,
    onImport,
    onCapture,
    onUpdate,
    onDelete,
    onReveal,
    onRemoveDevice,
    onSaveWheelImage,
    onRemoveWheelImage,
    onSaveCategories,
    onCreateWheel,
    onAssignWheel,
  } = props;
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [captureName, setCaptureName] = useState("");
  const [wheelFilter, setWheelFilter] = useState<string | null>(null);
  const [classFilter, setClassFilter] = useState<string | null>(null);
  const wheelOptions = Array.from(
    new Set(
      profiles
        .map((profile) => wheelLabel(profile))
        .filter((label): label is string => Boolean(label)),
    ),
  ).sort();
  const classTags = Array.from(
    new Set(profiles.flatMap((profile) => profile.classTags)),
  ).sort();
  const visibleProfiles = profiles.filter(
    (profile) =>
      [
        profile.name,
        profile.wheelBrand,
        profile.wheelName,
        ...profile.classTags,
        ...profile.customTags,
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase()
        .includes(query.trim().toLowerCase()) &&
      (!wheelFilter || wheelLabel(profile) === wheelFilter) &&
      (!classFilter || profile.classTags.includes(classFilter)),
  );
  const acceptFiles = (files: FileList | null) => {
    if (files?.[0]) onImport(files[0]);
  };
  return (
    <section
      className="profiles-page panel"
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        acceptFiles(event.dataTransfer.files);
      }}
    >
      <div className="profiles-toolbar">
        <div className="search-box">
          <Search size={17} />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search your profiles"
          />
        </div>
        <div className="profile-tools">
          <div className="capture-box">
            <input
              value={captureName}
              onChange={(event) => setCaptureName(event.target.value)}
              placeholder="New profile name"
            />
            <button
              disabled={busy || !captureName.trim()}
              onClick={() => {
                onCapture(captureName);
                setCaptureName("");
              }}
            >
              Capture LMU
            </button>
          </div>
          <button className="secondary compact" onClick={onReveal}>
            <FolderOpen /> Folder
          </button>
          <label className="primary compact image-picker">
            <Upload /> Import
            <input
              type="file"
              accept="application/json,.json"
              disabled={busy}
              onChange={(event) => {
                acceptFiles(event.target.files);
                event.target.value = "";
              }}
            />
          </label>
        </div>
      </div>
      {(wheelOptions.length > 0 || classTags.length > 0) && (
        <div className="category-browser">
          <SearchSelect
            label="Wheel"
            value={wheelFilter}
            options={wheelOptions}
            onChange={setWheelFilter}
          />
          <SearchSelect
            label="Class"
            value={classFilter}
            options={classTags}
            onChange={setClassFilter}
          />
        </div>
      )}
      <div className="profile-grid">
        {loading &&
          Array.from({ length: 6 }, (_, index) => (
            <div className="profile-card skeleton" key={index} />
          ))}
        {visibleProfiles.map((profile) => (
          <article
            className={
              selectedId === profile.id
                ? "profile-card selected"
                : "profile-card"
            }
            key={profile.id}
            onClick={() => {
              setSelectedId(profile.id);
            }}
          >
            <header>
              <WheelImage
                wheelId={profile.wheelId}
                hasImage={profile.hasWheelImage}
                alt={`${profile.wheelName ?? profile.name} wheel`}
                className="profile-card-icon"
                revision={wheelImageRevision}
              />
              <div>
                <strong>{profile.name}</strong>
                <small>
                  {profile.id === activeId
                    ? activeDirty
                      ? "LMU has unsaved changes"
                      : "Prepared for next launch"
                    : "Saved profile"}
                </small>
              </div>
              {profile.id === activeId && (
                <span
                  className={
                    activeDirty ? "active-badge dirty" : "active-badge"
                  }
                >
                  {activeDirty ? "Changed" : "Active"}
                </span>
              )}
            </header>
            <div className="profile-tags">
              {wheelLabel(profile) && (
                <span className="wheel-tag">{wheelLabel(profile)}</span>
              )}
              {profile.classTags.map((tag) => (
                <span
                  className={`class-tag class-${classSlug(tag)}`}
                  key={`class-${tag}`}
                >
                  {tag}
                </span>
              ))}
              {!profile.classTags.length && (
                <span className="generic-tag">Generic</span>
              )}
              {profile.customTags.map((tag) => (
                <span className="custom-tag" key={`custom-${tag}`}>
                  {tag}
                </span>
              ))}
            </div>
            <div className="profile-card-meta">
              <span>{profile.bindingCount} bindings</span>
              <span>{profile.devices.length} devices</span>
              {profile.hotkeySlot && <kbd>Ctrl Alt {profile.hotkeySlot}</kbd>}
            </div>
            <button
              className={
                profile.id === activeId ? "prepared" : "secondary compact"
              }
              disabled={busy || profile.id === activeId}
              onClick={(event) => {
                event.stopPropagation();
                onActivate(profile);
              }}
            >
              {profile.id === activeId ? "Prepared" : "Use profile"}
            </button>
          </article>
        ))}
      </div>
      {!loading && !visibleProfiles.length && (
        <div className="empty large">
          <Gamepad2 />
          <strong>
            {profiles.length ? "No matching profiles" : "Your garage is empty"}
          </strong>
          <span>
            {profiles.length
              ? "Clear the search or adjust the wheel and class filters."
              : "Drop a preset here, import one, or capture your current LMU bindings."}
          </span>
        </div>
      )}
      {selected && (
        <div className="device-manager">
          <div className="device-manager-head">
            <div>
              <span className="eyebrow">Profile setup</span>
              <h3>{selected.name}</h3>
            </div>
            <div className="profile-image-actions">
              {wheelImageUrl && (
                <img src={wheelImageUrl} alt="Assigned wheel" />
              )}
              <label
                className={`secondary compact image-picker ${!selected.wheelId ? "disabled" : ""}`}
                title={
                  selected.wheelId
                    ? "This image is shared by every profile using this wheel"
                    : "Assign or create a wheel first"
                }
              >
                <ImageIcon />
                {selected.hasWheelImage
                  ? "Replace wheel image"
                  : "Add wheel image"}
                <input
                  type="file"
                  accept="image/png,image/jpeg,image/webp"
                  disabled={busy || !selected.wheelId}
                  onClick={(event) => {
                    event.currentTarget.value = "";
                  }}
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (file) onSaveWheelImage(selected, file);
                  }}
                />
              </label>
              <ActionMenu
                items={[
                  {
                    label: "Update from LMU",
                    hidden: !selected.differsFromLive,
                    onSelect: () =>
                      setConfirmation({
                        title: `Update ${selected.name} from LMU?`,
                        description:
                          "This replaces the saved profile with LMU's current bindings. A recovery backup will be created first.",
                        confirmLabel: "Update from LMU",
                        onConfirm: () => {
                          onUpdate(selected);
                          setConfirmation(null);
                        },
                      }),
                  },
                  {
                    label: "Remove wheel image",
                    hidden: !selected.hasWheelImage,
                    danger: true,
                    onSelect: () =>
                      setConfirmation({
                        title: "Remove wheel image?",
                        description:
                          "This removes the image from the wheel library and every profile using this wheel.",
                        confirmLabel: "Remove image",
                        danger: true,
                        onConfirm: () => {
                          onRemoveWheelImage(selected);
                          setConfirmation(null);
                        },
                      }),
                  },
                  {
                    label: "Delete profile",
                    danger: true,
                    onSelect: () =>
                      setConfirmation({
                        title: `Delete ${selected.name}?`,
                        description:
                          "This permanently removes the saved profile and its managed image. LMU's live configuration is not changed.",
                        confirmLabel: "Delete profile",
                        danger: true,
                        onConfirm: () => {
                          onDelete(selected);
                          setConfirmation(null);
                        },
                      }),
                  },
                ]}
              />
            </div>
          </div>
          <div className="profile-management">
            {selected.hotkeySlot && (
              <span className="migration-note">
                <kbd>Ctrl Alt {selected.hotkeySlot}</kbd> shortcut migration
                pending
              </span>
            )}
          </div>
          {selected.id === activeId && activeDirty && (
            <div className="profile-dirty-notice">
              LMU's live bindings differ from this saved profile. Use the
              actions menu to update the saved copy, or activate it again to
              restore the saved bindings.
            </div>
          )}
          <CategoryEditor
            key={selected.id}
            profile={selected}
            wheels={wheels}
            busy={busy}
            onSave={onSaveCategories}
            onCreateWheel={onCreateWheel}
            onAssignWheel={onAssignWheel}
          />
          <p className="device-note">
            Removing a device also removes all of its bindings.
          </p>
          <div className="device-list">
            {selected.devices.map((device) => (
              <div className="device-row" key={device.key}>
                <span className="device-thumb">
                  <Gamepad2 />
                </span>
                <div>
                  <strong>{device.name}</strong>
                  <small>
                    {device.bindingCount} binding
                    {device.bindingCount === 1 ? "" : "s"} · {device.key}
                  </small>
                </div>
                <ActionMenu
                  label={`Actions for ${device.name}`}
                  items={[
                    {
                      label: "Delete device",
                      danger: true,
                      onSelect: () =>
                        setConfirmation({
                          title: `Delete ${device.name}?`,
                          description: `This removes the device and its ${device.bindingCount} binding${device.bindingCount === 1 ? "" : "s"} from ${selected.name}. A recovery backup will be created.`,
                          confirmLabel: "Delete device",
                          danger: true,
                          onConfirm: () => {
                            onRemoveDevice(selected, device);
                            setConfirmation(null);
                          },
                        }),
                    },
                  ]}
                />
              </div>
            ))}
            {!selected.devices.length && (
              <div className="empty">
                This profile has no registered devices.
              </div>
            )}
          </div>
        </div>
      )}
      <ConfirmDialog
        confirmation={confirmation}
        busy={busy}
        onClose={() => setConfirmation(null)}
      />
    </section>
  );
}

function CategoryEditor({
  profile,
  wheels,
  busy,
  onSave,
  onCreateWheel,
  onAssignWheel,
}: {
  profile: Profile;
  wheels: Wheel[];
  busy: boolean;
  onSave: Props["onSaveCategories"];
  onCreateWheel: Props["onCreateWheel"];
  onAssignWheel: Props["onAssignWheel"];
}) {
  const [creatingWheel, setCreatingWheel] = useState(false);
  const [profileName, setProfileName] = useState(profile.name);
  const [brand, setBrand] = useState("");
  const [wheelName, setWheelName] = useState("");
  const [classes, setClasses] = useState(profile.classTags);
  const [customTags, setCustomTags] = useState(profile.customTags);
  const changed =
    profileName.trim() !== profile.name ||
    JSON.stringify(classes) !== JSON.stringify(profile.classTags) ||
    JSON.stringify(customTags) !== JSON.stringify(profile.customTags);
  return (
    <section className="category-editor">
      <div>
        <span className="eyebrow">Profile identity</span>
        <h4>Wheel and intended car classes</h4>
      </div>
      <div className="category-fields">
        <label className="identity-field wheel-library-field">
          <span>Profile name</span>
          <input
            value={profileName}
            maxLength={100}
            onChange={(event) => setProfileName(event.target.value)}
          />
        </label>
        <label className="identity-field wheel-library-field">
          <span>Wheel</span>
          <div>
            <select
              value={profile.wheelId ?? ""}
              disabled={busy}
              onChange={(event) => {
                const wheel =
                  wheels.find((entry) => entry.id === event.target.value) ??
                  null;
                onAssignWheel(profile, wheel);
              }}
            >
              <option value="">No wheel assigned</option>
              {wheels.map((wheel) => (
                <option value={wheel.id} key={wheel.id}>
                  {[wheel.brand, wheel.name].filter(Boolean).join(" ")}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="secondary compact"
              onClick={() => setCreatingWheel((current) => !current)}
            >
              {creatingWheel ? "Cancel" : "New wheel"}
            </button>
          </div>
        </label>
        {creatingWheel && (
          <div className="new-wheel-fields">
            <label className="identity-field">
              <span>Brand</span>
              <input
                value={brand}
                maxLength={60}
                placeholder="Simagic"
                onChange={(event) => setBrand(event.target.value)}
              />
            </label>
            <label className="identity-field">
              <span>Wheel name</span>
              <input
                value={wheelName}
                maxLength={60}
                placeholder="GT Neo"
                onChange={(event) => setWheelName(event.target.value)}
              />
            </label>
            <button
              type="button"
              className="primary compact"
              disabled={busy || !wheelName.trim()}
              onClick={() => onCreateWheel(profile, brand, wheelName)}
            >
              Create and assign
            </button>
          </div>
        )}
        <TagPicker
          label="Car classes"
          options={DEFAULT_CLASSES}
          selected={classes}
          onChange={setClasses}
          emptyLabel="Generic (no class)"
          tagClassName={(tag) => `class-${classSlug(tag)}`}
        />
        <TagPicker
          label="Additional tags"
          options={customTags}
          selected={customTags}
          onChange={setCustomTags}
          allowCreate
          emptyLabel="Add a tag"
        />
      </div>
      <button
        className="primary compact"
        disabled={busy || !changed}
        onClick={() => onSave(profile, profileName, classes, customTags)}
      >
        Save profile details
      </button>
    </section>
  );
}
