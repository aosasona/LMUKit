import { Check, ChevronDown, Plus, Search, X } from "lucide-react";
import { useMemo, useState } from "react";
import { positionDropdown } from "../lib/dropdown";

export function TagPicker({
  label,
  options,
  selected,
  onChange,
  allowCreate = false,
  emptyLabel = "None selected",
  tagClassName,
}: {
  label: string;
  options: readonly string[];
  selected: string[];
  onChange: (tags: string[]) => void;
  allowCreate?: boolean;
  emptyLabel?: string;
  tagClassName?: (tag: string) => string;
}) {
  const [query, setQuery] = useState("");
  const choices = useMemo(
    () => Array.from(new Set([...options, ...selected])),
    [options, selected],
  );
  const filtered = choices.filter((option) =>
    option.toLowerCase().includes(query.trim().toLowerCase()),
  );
  const candidate = query.trim();
  const canCreate =
    allowCreate &&
    candidate.length > 0 &&
    !choices.some((option) => option.toLowerCase() === candidate.toLowerCase());

  const toggle = (tag: string) => {
    onChange(
      selected.includes(tag)
        ? selected.filter((entry) => entry !== tag)
        : [...selected, tag],
    );
  };

  return (
    <label className="tag-picker">
      <span>{label}</span>
      <details onToggle={(event) => positionDropdown(event.currentTarget, 250)}>
        <summary>
          <span
            className={
              selected.length ? "tag-picker-value" : "tag-picker-empty"
            }
          >
            {selected.length ? selected.join(", ") : emptyLabel}
          </span>
          <ChevronDown size={16} />
        </summary>
        <div className="tag-picker-menu">
          <div className="tag-picker-search">
            <Search size={15} />
            <input
              value={query}
              placeholder="Search tags…"
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && canCreate) {
                  event.preventDefault();
                  onChange([...selected, candidate]);
                  setQuery("");
                }
              }}
            />
          </div>
          <div className="tag-picker-options">
            {filtered.map((option) => (
              <button
                type="button"
                className={`${selected.includes(option) ? "selected" : ""} ${tagClassName?.(option) ?? ""}`}
                key={option}
                onClick={() => toggle(option)}
              >
                <span>{option}</span>
                {selected.includes(option) && <Check size={15} />}
              </button>
            ))}
            {canCreate && (
              <button
                type="button"
                onClick={() => {
                  onChange([...selected, candidate]);
                  setQuery("");
                }}
              >
                <Plus size={15} />
                <span>Add “{candidate}”</span>
              </button>
            )}
            {!filtered.length && !canCreate && (
              <span className="tag-picker-no-results">No matching tags</span>
            )}
          </div>
        </div>
      </details>
      {selected.length > 0 && (
        <div className="tag-picker-chips">
          {selected.map((tag) => (
            <button
              type="button"
              className={tagClassName?.(tag)}
              key={tag}
              onClick={() => toggle(tag)}
            >
              {tag}
              <X size={12} />
            </button>
          ))}
        </div>
      )}
    </label>
  );
}
