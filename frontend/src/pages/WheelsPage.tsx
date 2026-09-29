import { Image as ImageIcon, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import { ActionMenu } from "../components/ActionMenu";
import { ConfirmDialog, type Confirmation } from "../components/ConfirmDialog";
import { WheelImage } from "../components/WheelImage";
import type { Profile, Wheel } from "../models";

type Props = {
  wheels: Wheel[];
  profiles: Profile[];
  busy: boolean;
  onCreate: (brand: string, name: string) => void;
  onUpdate: (wheel: Wheel, brand: string, name: string) => void;
  onDelete: (wheel: Wheel) => void;
  onSaveImage: (wheel: Wheel, file: File) => void;
  onRemoveImage: (wheel: Wheel) => void;
  imageRevision: number;
};

export function WheelsPage({
  wheels,
  profiles,
  busy,
  onCreate,
  onUpdate,
  onDelete,
  onSaveImage,
  onRemoveImage,
  imageRevision,
}: Props) {
  const [selectedId, setSelectedId] = useState<string | null>(
    wheels[0]?.id ?? null,
  );
  const [brand, setBrand] = useState("");
  const [name, setName] = useState("");
  const [newBrand, setNewBrand] = useState("");
  const [newName, setNewName] = useState("");
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const selected = wheels.find((wheel) => wheel.id === selectedId) ?? wheels[0];

  useEffect(() => {
    if (!selected) return;
    setSelectedId(selected.id);
    setBrand(selected.brand ?? "");
    setName(selected.name);
  }, [selected?.id, selected?.brand, selected?.name]);

  return (
    <section className="wheels-page">
      <div className="wheel-library-grid">
        <section className="panel wheel-collection">
          <div className="section-head">
            <div>
              <span className="eyebrow">Reusable hardware</span>
              <h3>Your wheels</h3>
            </div>
            <span>{wheels.length} saved</span>
          </div>
          <div className="wheel-card-grid">
            {wheels.map((wheel) => (
              <button
                type="button"
                className={
                  wheel.id === selected?.id
                    ? "wheel-card selected"
                    : "wheel-card"
                }
                key={wheel.id}
                onClick={() => setSelectedId(wheel.id)}
              >
                <WheelImage
                  wheelId={wheel.id}
                  hasImage={wheel.hasImage}
                  alt={`${wheel.name} wheel`}
                  className="wheel-card-image"
                  revision={imageRevision}
                />
                <span>
                  <strong>{wheel.name}</strong>
                  <small>{wheel.brand ?? "Unbranded"}</small>
                  <em>
                    {wheel.profileCount} profile
                    {wheel.profileCount === 1 ? "" : "s"}
                  </em>
                </span>
              </button>
            ))}
            {!wheels.length && (
              <div className="empty">Create your first reusable wheel.</div>
            )}
          </div>
        </section>

        <aside className="panel wheel-inspector">
          {selected ? (
            <>
              <div className="wheel-inspector-head">
                <div>
                  <span className="eyebrow">Wheel details</span>
                  <h3>
                    {[selected.brand, selected.name].filter(Boolean).join(" ")}
                  </h3>
                </div>
                <ActionMenu
                  items={[
                    {
                      label: "Remove image",
                      hidden: !selected.hasImage,
                      danger: true,
                      onSelect: () =>
                        setConfirmation({
                          title: "Remove this wheel image?",
                          description:
                            "The image will disappear from every linked profile.",
                          confirmLabel: "Remove image",
                          danger: true,
                          onConfirm: () => {
                            onRemoveImage(selected);
                            setConfirmation(null);
                          },
                        }),
                    },
                    {
                      label: "Delete wheel",
                      hidden: selected.profileCount > 0,
                      danger: true,
                      onSelect: () =>
                        setConfirmation({
                          title: `Delete ${selected.name}?`,
                          description:
                            "This unused wheel and its managed image will be permanently removed.",
                          confirmLabel: "Delete wheel",
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
              <WheelImage
                wheelId={selected.id}
                hasImage={selected.hasImage}
                alt={`${selected.name} wheel`}
                className="wheel-inspector-image"
                revision={imageRevision}
              />
              <label className="secondary image-picker wheel-image-button">
                <ImageIcon />{" "}
                {selected.hasImage ? "Replace image" : "Add image"}
                <input
                  type="file"
                  accept="image/png,image/jpeg,image/webp"
                  disabled={busy}
                  onClick={(event) => {
                    event.currentTarget.value = "";
                  }}
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (file) onSaveImage(selected, file);
                  }}
                />
              </label>
              <div className="wheel-fields">
                <label className="identity-field">
                  <span>Brand</span>
                  <input
                    value={brand}
                    onChange={(event) => setBrand(event.target.value)}
                  />
                </label>
                <label className="identity-field">
                  <span>Wheel name</span>
                  <input
                    value={name}
                    onChange={(event) => setName(event.target.value)}
                  />
                </label>
              </div>
              <button
                type="button"
                className="primary"
                disabled={
                  busy ||
                  !name.trim() ||
                  (name.trim() === selected.name &&
                    brand.trim() === (selected.brand ?? ""))
                }
                onClick={() => onUpdate(selected, brand, name)}
              >
                Save wheel
              </button>
              <div className="linked-profiles">
                <span>Linked profiles</span>
                {profiles
                  .filter((profile) => profile.wheelId === selected.id)
                  .map((profile) => (
                    <strong key={profile.id}>{profile.name}</strong>
                  ))}
                {!selected.profileCount && (
                  <small>Not assigned to any profiles</small>
                )}
              </div>
            </>
          ) : (
            <div className="empty">Select or create a wheel to manage it.</div>
          )}
        </aside>
      </div>
      <section className="panel new-wheel-panel">
        <div>
          <span className="eyebrow">Expand your garage</span>
          <h3>Create a wheel</h3>
        </div>
        <input
          placeholder="Brand (optional)"
          value={newBrand}
          onChange={(event) => setNewBrand(event.target.value)}
        />
        <input
          placeholder="Wheel name"
          value={newName}
          onChange={(event) => setNewName(event.target.value)}
        />
        <button
          type="button"
          className="primary"
          disabled={busy || !newName.trim()}
          onClick={() => {
            onCreate(newBrand, newName);
            setNewBrand("");
            setNewName("");
          }}
        >
          <Plus /> Create wheel
        </button>
      </section>
      <ConfirmDialog
        confirmation={confirmation}
        busy={busy}
        onClose={() => setConfirmation(null)}
      />
    </section>
  );
}
