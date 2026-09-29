import {
  FolderOpen,
  Gamepad2,
  Image as ImageIcon,
  RefreshCw,
  Search,
  Tags,
  Trash2,
  Upload,
} from "lucide-react";
import { useState } from "react";
import type { Device, Profile } from "../models";

type Props = {
  profiles: Profile[];
  selected?: Profile;
  wheelImageUrl: string | null;
  activeId: string | null;
  activeDirty: boolean | null;
  selectedId: string | null;
  query: string;
  busy: boolean;
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
    wheelTags: string[],
    classTags: string[],
  ) => void;
};

export function ProfilesPage(props: Props) {
  const {
    profiles,
    selected,
    wheelImageUrl,
    activeId,
    activeDirty,
    selectedId,
    query,
    busy,
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
  } = props;
  const [confirmDevice, setConfirmDevice] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [captureName, setCaptureName] = useState("");
  const [wheelFilter, setWheelFilter] = useState<string | null>(null);
  const [classFilter, setClassFilter] = useState<string | null>(null);
  const wheelTags = Array.from(
    new Set(profiles.flatMap((profile) => profile.wheelTags)),
  ).sort();
  const classTags = Array.from(
    new Set(profiles.flatMap((profile) => profile.classTags)),
  ).sort();
  const visibleProfiles = profiles.filter(
    (profile) =>
      (!wheelFilter || profile.wheelTags.includes(wheelFilter)) &&
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
      {(wheelTags.length > 0 || classTags.length > 0) && (
        <div className="category-browser">
          <div>
            <span>Wheel</span>
            <button
              className={!wheelFilter ? "active" : ""}
              onClick={() => setWheelFilter(null)}
            >
              All
            </button>
            {wheelTags.map((tag) => (
              <button
                className={wheelFilter === tag ? "active" : ""}
                key={tag}
                onClick={() => setWheelFilter(tag)}
              >
                {tag}
              </button>
            ))}
          </div>
          <div>
            <span>Class</span>
            <button
              className={!classFilter ? "active" : ""}
              onClick={() => setClassFilter(null)}
            >
              All
            </button>
            {classTags.map((tag) => (
              <button
                className={classFilter === tag ? "active" : ""}
                key={tag}
                onClick={() => setClassFilter(tag)}
              >
                {tag}
              </button>
            ))}
          </div>
        </div>
      )}
      <div className="profile-grid">
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
              setConfirmDevice(null);
              setConfirmDelete(false);
            }}
          >
            <header>
              <span className="profile-card-icon">
                <Gamepad2 />
              </span>
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
                <span className="active-badge">Active</span>
              )}
            </header>
            <div className="profile-tags">
              {profile.wheelTags.map((tag) => (
                <span className="wheel-tag" key={`wheel-${tag}`}>
                  {tag}
                </span>
              ))}
              {profile.classTags.map((tag) => (
                <span className="class-tag" key={`class-${tag}`}>
                  {tag}
                </span>
              ))}
              {!profile.wheelTags.length && !profile.classTags.length && (
                <span className="untagged">Add wheel and class categories</span>
              )}
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
      {!visibleProfiles.length && (
        <div className="empty large">
          <Gamepad2 />
          <strong>No profiles found</strong>
          <span>
            Drop a preset here, import one, or capture your current LMU
            bindings. Adjust the category filters if profiles are hidden.
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
              <label className="secondary compact image-picker">
                <ImageIcon />
                {selected.hasWheelImage ? "Replace image" : "Assign image"}
                <input
                  type="file"
                  accept="image/png,image/jpeg,image/webp"
                  disabled={busy}
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (file) onSaveWheelImage(selected, file);
                    event.target.value = "";
                  }}
                />
              </label>
              {selected.hasWheelImage && (
                <button
                  className="remove-device"
                  disabled={busy}
                  onClick={() => onRemoveWheelImage(selected)}
                >
                  <Trash2 /> Remove image
                </button>
              )}
            </div>
          </div>
          <div className="profile-management">
            <button
              className="secondary compact"
              disabled={busy}
              onClick={() => onUpdate(selected)}
            >
              <RefreshCw /> Update from LMU
            </button>
            {selected.hotkeySlot && (
              <span className="migration-note">
                <kbd>Ctrl Alt {selected.hotkeySlot}</kbd> shortcut migration
                pending
              </span>
            )}
            {confirmDelete ? (
              <>
                <button
                  className="danger"
                  disabled={busy}
                  onClick={() => onDelete(selected)}
                >
                  Delete permanently
                </button>
                <button
                  className="secondary compact"
                  onClick={() => setConfirmDelete(false)}
                >
                  Cancel
                </button>
              </>
            ) : (
              <button
                className="remove-device"
                onClick={() => setConfirmDelete(true)}
              >
                <Trash2 /> Delete profile
              </button>
            )}
          </div>
          <CategoryEditor
            key={selected.id}
            profile={selected}
            busy={busy}
            onSave={onSaveCategories}
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
                {confirmDevice === device.key ? (
                  <div className="confirm-remove">
                    <span>Remove?</span>
                    <button
                      className="danger"
                      disabled={busy}
                      onClick={() => onRemoveDevice(selected, device)}
                    >
                      Yes, remove
                    </button>
                    <button
                      className="secondary compact"
                      onClick={() => setConfirmDevice(null)}
                    >
                      Cancel
                    </button>
                  </div>
                ) : (
                  <button
                    className="remove-device"
                    aria-label={`Remove ${device.name}`}
                    onClick={() => setConfirmDevice(device.key)}
                  >
                    <Trash2 /> Remove
                  </button>
                )}
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
    </section>
  );
}

function CategoryEditor({
  profile,
  busy,
  onSave,
}: {
  profile: Profile;
  busy: boolean;
  onSave: Props["onSaveCategories"];
}) {
  const [wheels, setWheels] = useState(profile.wheelTags);
  const [classes, setClasses] = useState(profile.classTags);
  const [wheelInput, setWheelInput] = useState("");
  const [classInput, setClassInput] = useState("");
  const addTag = (
    value: string,
    tags: string[],
    setTags: (tags: string[]) => void,
    clear: () => void,
  ) => {
    const tag = value.trim();
    if (
      tag &&
      !tags.some((existing) => existing.toLowerCase() === tag.toLowerCase())
    )
      setTags([...tags, tag]);
    clear();
  };
  const tagInput = (
    label: string,
    value: string,
    setValue: (value: string) => void,
    tags: string[],
    setTags: (tags: string[]) => void,
  ) => (
    <div className="tag-field">
      <span>{label}</span>
      <div className="tag-list">
        {tags.map((tag) => (
          <button
            key={tag}
            title={`Remove ${tag}`}
            onClick={() => setTags(tags.filter((entry) => entry !== tag))}
          >
            {tag} ×
          </button>
        ))}
      </div>
      <div className="tag-entry">
        <Tags />
        <input
          value={value}
          maxLength={40}
          placeholder={`Add ${label.toLowerCase()}`}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === ",") {
              event.preventDefault();
              addTag(value, tags, setTags, () => setValue(""));
            }
          }}
        />
        <button
          disabled={!value.trim()}
          onClick={() => addTag(value, tags, setTags, () => setValue(""))}
        >
          Add
        </button>
      </div>
    </div>
  );
  const changed =
    JSON.stringify(wheels) !== JSON.stringify(profile.wheelTags) ||
    JSON.stringify(classes) !== JSON.stringify(profile.classTags);
  return (
    <section className="category-editor">
      <div>
        <span className="eyebrow">Categories</span>
        <h4>Find this setup by wheel or class</h4>
      </div>
      <div className="category-fields">
        {tagInput("Wheel", wheelInput, setWheelInput, wheels, setWheels)}
        {tagInput("Class", classInput, setClassInput, classes, setClasses)}
      </div>
      <button
        className="primary compact"
        disabled={busy || !changed}
        onClick={() => onSave(profile, wheels, classes)}
      >
        Save categories
      </button>
    </section>
  );
}
