import { Check, ChevronDown, Plus, Search } from "lucide-react";
import { useRef, useState } from "react";
import { displayClassTag } from "../lib/classTags";
import { positionDropdown, useDismissableDetails } from "../lib/dropdown";

type Tags = {
  classTags: string[];
  customTags: string[];
};

export function ProfileTagPicker({
  classOptions,
  classTags,
  customTags,
  onChange,
  classNameFor,
  mode = "classes-and-custom",
}: {
  classOptions: readonly string[];
  classTags: string[];
  customTags: string[];
  onChange: (tags: Tags) => void;
  classNameFor: (tag: string) => string;
  mode?: "classes-and-custom" | "car-tags";
}) {
  const [query, setQuery] = useState("");
  const [availableCustomTags, setAvailableCustomTags] = useState(customTags);
  const details = useRef<HTMLDetailsElement>(null);
  useDismissableDetails(details);
  const normalizedQuery = query.trim().toLowerCase();
  const displayTag = (tag: string) =>
    mode === "car-tags" ? tag : displayClassTag(tag);
  const selected = [...classTags, ...customTags];
  const availableClasses = Array.from(new Set([...classOptions, ...classTags]));
  const visibleClasses = availableClasses.filter(
    (tag) =>
      tag.toLowerCase().includes(normalizedQuery) ||
      displayTag(tag).toLowerCase().includes(normalizedQuery),
  );
  const visibleCustomTags =
    mode === "car-tags"
      ? []
      : availableCustomTags.filter((tag) =>
          tag.toLowerCase().includes(normalizedQuery),
        );
  const candidate = query.trim();
  const knownTags = [...availableClasses, ...availableCustomTags];
  const canCreate =
    candidate.length > 0 &&
    !knownTags.some((tag) => tag.toLowerCase() === normalizedQuery);

  const toggleClass = (tag: string) => {
    onChange({
      classTags: classTags.includes(tag)
        ? classTags.filter((entry) => entry !== tag)
        : [...classTags, tag],
      customTags,
    });
  };
  const toggleCustomTag = (tag: string) => {
    onChange({
      classTags,
      customTags: customTags.includes(tag)
        ? customTags.filter((entry) => entry !== tag)
        : [...customTags, tag],
    });
  };
  const createTag = () => {
    if (!canCreate) return;
    if (mode === "car-tags") {
      onChange({ classTags: [...classTags, candidate], customTags });
      setQuery("");
      return;
    }
    setAvailableCustomTags((tags) => [...tags, candidate]);
    onChange({ classTags, customTags: [...customTags, candidate] });
    setQuery("");
  };

  return (
    <div className="tag-picker profile-tag-picker">
      <span>{mode === "car-tags" ? "Assetto Corsa car tags" : "Profile tags"}</span>
      <details
        ref={details}
        onToggle={(event) => positionDropdown(event.currentTarget, 300)}
      >
        <summary>
          <span
            className={
              selected.length ? "tag-picker-value" : "tag-picker-empty"
            }
          >
            {selected.length
              ? [...classTags.map(displayTag), ...customTags].join(", ")
              : mode === "car-tags"
                ? "Manual only — no matching car tags"
                : "Generic — no class restrictions"}
          </span>
          <ChevronDown size={16} />
        </summary>
        <div className="tag-picker-menu">
          <div className="tag-picker-search">
            <Search size={15} />
            <input
              value={query}
              placeholder={
                mode === "car-tags"
                  ? "Enter an exact AC car tag…"
                  : "Find or create a tag…"
              }
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && canCreate) {
                  event.preventDefault();
                  createTag();
                }
              }}
            />
          </div>
          <div className="tag-picker-options">
            {visibleClasses.length > 0 && (
              <section>
                <span className="tag-picker-section-label">
                  {mode === "car-tags" ? "Matching car tags" : "Car classes"}
                </span>
                {visibleClasses.map((tag) => (
                  <button
                    type="button"
                    className={`${classTags.includes(tag) ? "selected" : ""} ${classNameFor(tag)}`}
                    key={tag}
                    onClick={() => toggleClass(tag)}
                  >
                    <span>{displayTag(tag)}</span>
                    {classTags.includes(tag) && <Check size={15} />}
                  </button>
                ))}
              </section>
            )}
            {visibleCustomTags.length > 0 && (
              <section>
                <span className="tag-picker-section-label">Custom tags</span>
                {visibleCustomTags.map((tag) => (
                  <button
                    type="button"
                    className={customTags.includes(tag) ? "selected" : ""}
                    key={tag}
                    onClick={() => toggleCustomTag(tag)}
                  >
                    <span>{tag}</span>
                    {customTags.includes(tag) && <Check size={15} />}
                  </button>
                ))}
              </section>
            )}
            {canCreate && (
              <button
                type="button"
                className="tag-picker-create"
                onClick={createTag}
              >
                <Plus size={15} />
                <span>Create “{candidate}”</span>
              </button>
            )}
            {!visibleClasses.length &&
              !visibleCustomTags.length &&
              !canCreate && (
                <span className="tag-picker-no-results">No matching tags</span>
              )}
          </div>
        </div>
      </details>
      <small>
        {mode === "car-tags"
          ? "Use exact tags from the car's ui_car.json. Any matching tag can select this profile automatically."
          : "Choose any relevant car classes, or type to create your own tag."}
      </small>
    </div>
  );
}
