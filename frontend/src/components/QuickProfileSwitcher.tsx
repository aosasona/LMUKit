import { Search, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Profile } from "../models";
import { WheelImage } from "./WheelImage";

export function QuickProfileSwitcher({
  profiles,
  activeId,
  busy,
  imageRevision,
  onClose,
  onActivate,
}: {
  profiles: Profile[];
  activeId: string | null;
  busy: boolean;
  imageRevision: number;
  onClose: () => void;
  onActivate: (profile: Profile) => Promise<void>;
}) {
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const matches = useMemo(() => {
    const filter = query.trim().toLowerCase();
    return profiles.filter((profile) =>
      `${profile.name} ${profile.wheelBrand ?? ""} ${profile.wheelName ?? ""} ${profile.classTags.join(" ")} ${profile.customTags.join(" ")}`
        .toLowerCase()
        .includes(filter),
    );
  }, [profiles, query]);

  useEffect(() => input.current?.focus(), []);
  useEffect(
    () =>
      setSelected((value) => Math.max(0, Math.min(value, matches.length - 1))),
    [matches.length],
  );

  async function activate(profile: Profile) {
    await onActivate(profile);
    onClose();
  }

  return (
    <div
      className="dialog-backdrop quick-switcher-backdrop"
      onMouseDown={onClose}
      onKeyDown={(event) => {
        if (event.key === "Escape") onClose();
        if (event.key === "ArrowDown") {
          event.preventDefault();
          setSelected((value) => Math.min(value + 1, matches.length - 1));
        }
        if (event.key === "ArrowUp") {
          event.preventDefault();
          setSelected((value) => Math.max(value - 1, 0));
        }
        if (event.key === "Enter" && matches[selected]) {
          event.preventDefault();
          void activate(matches[selected]);
        }
      }}
    >
      <section
        className="quick-switcher"
        role="dialog"
        aria-modal="true"
        aria-labelledby="quick-switcher-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header>
          <div>
            <span className="eyebrow">Ctrl Alt Space</span>
            <h2 id="quick-switcher-title">Choose a profile</h2>
          </div>
          <button className="dialog-close" onClick={onClose} aria-label="Close">
            <X />
          </button>
        </header>
        <label className="binding-action-search">
          <Search />
          <input
            ref={input}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setSelected(0);
            }}
            placeholder="Search profiles, wheels, or classes"
          />
        </label>
        <div className="quick-switcher-results">
          {matches.map((profile, index) => (
            <button
              className={index === selected ? "selected" : ""}
              disabled={busy}
              key={profile.id}
              onMouseEnter={() => setSelected(index)}
              onClick={() => void activate(profile)}
            >
              <WheelImage
                wheelId={profile.wheelId}
                hasImage={profile.hasWheelImage}
                alt={`${profile.wheelName ?? profile.name} wheel`}
                className="quick-switcher-wheel"
                revision={imageRevision}
              />
              <span>
                <strong>{profile.name}</strong>
                <small>
                  {profile.id === activeId ? "Prepared · " : ""}
                  {profile.classTags.length
                    ? profile.classTags.join(" · ")
                    : "Generic profile"}
                </small>
              </span>
              {profile.hotkeySlot && <kbd>Ctrl Alt {profile.hotkeySlot}</kbd>}
            </button>
          ))}
          {!matches.length && (
            <div className="empty">
              <span>No matching profiles.</span>
            </div>
          )}
        </div>
        <footer>
          <span>↑ ↓ Navigate</span>
          <span>Enter Use profile</span>
          <span>Esc Close</span>
        </footer>
      </section>
    </div>
  );
}
