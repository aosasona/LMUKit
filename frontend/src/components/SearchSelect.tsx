import { Check, ChevronDown, Search } from "lucide-react";
import { useRef, useState } from "react";
import { positionDropdown, useDismissableDetails } from "../lib/dropdown";

export function SearchSelect({
  label,
  value,
  options,
  allLabel = "All",
  onChange,
}: {
  label: string;
  value: string | null;
  options: string[];
  allLabel?: string;
  onChange: (value: string | null) => void;
}) {
  const [query, setQuery] = useState("");
  const details = useRef<HTMLDetailsElement>(null);
  useDismissableDetails(details);
  const filtered = options.filter((option) =>
    option.toLowerCase().includes(query.trim().toLowerCase()),
  );
  const choose = (next: string | null) => {
    onChange(next);
    setQuery("");
    details.current?.removeAttribute("open");
  };
  return (
    <label className="search-select">
      <span>{label}</span>
      <details
        ref={details}
        onToggle={(event) => positionDropdown(event.currentTarget, 260)}
      >
        <summary>
          <span>{value ?? allLabel}</span>
          <ChevronDown />
        </summary>
        <div>
          <div className="search-select-input">
            <Search />
            <input
              value={query}
              placeholder={`Search ${label.toLowerCase()}…`}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
          <button
            type="button"
            className={!value ? "selected" : ""}
            onClick={() => choose(null)}
          >
            {allLabel}
            {!value && <Check />}
          </button>
          {filtered.map((option) => (
            <button
              type="button"
              className={value === option ? "selected" : ""}
              key={option}
              onClick={() => choose(option)}
            >
              {option}
              {value === option && <Check />}
            </button>
          ))}
          {!filtered.length && <em>No matches</em>}
        </div>
      </details>
    </label>
  );
}
