import {
  FolderOpen,
  Gamepad2,
  Image as ImageIcon,
  RefreshCw,
  Search,
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
  } = props;
  const [confirmDevice, setConfirmDevice] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [captureName, setCaptureName] = useState("");
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
      <div className="profile-table-head">
        <span>Profile</span>
        <span>Devices</span>
        <span>Bindings</span>
        <span>Shortcut</span>
        <span />
      </div>
      {profiles.map((profile) => (
        <div
          className={
            selectedId === profile.id ? "profile-row selected" : "profile-row"
          }
          key={profile.id}
          onClick={() => {
            setSelectedId(profile.id);
            setConfirmDevice(null);
            setConfirmDelete(false);
          }}
        >
          <div className="profile-main">
            <span className="device-thumb">
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
          </div>
          <span>
            {profile.devices
              .slice(0, 2)
              .map((device) => device.name)
              .join(", ") || "Unknown device"}
          </span>
          <span>{profile.bindingCount}</span>
          <span>
            {profile.hotkeySlot ? (
              <kbd>Ctrl Alt {profile.hotkeySlot}</kbd>
            ) : (
              <small>Not set</small>
            )}
          </span>
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
        </div>
      ))}
      {!profiles.length && (
        <div className="empty large">
          <Gamepad2 />
          <strong>No profiles found</strong>
          <span>
            Drop a preset here, import one, or capture your current LMU
            bindings.
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
