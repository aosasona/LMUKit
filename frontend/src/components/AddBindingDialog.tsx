import { Plus, Search, X } from "lucide-react";
import { useMemo, useState } from "react";

export function AddBindingDialog({
  actions,
  onClose,
  onChoose,
}: {
  actions: string[];
  onClose: () => void;
  onChoose: (action: string, alternate: boolean) => void;
}) {
  const [query, setQuery] = useState("");
  const [alternate, setAlternate] = useState(false);
  const action = query.trim();
  const matches = useMemo(
    () =>
      actions
        .filter((item) => item.toLowerCase().includes(action.toLowerCase()))
        .slice(0, 8),
    [action, actions],
  );

  return (
    <div className="dialog-backdrop" onMouseDown={onClose}>
      <section
        className="add-binding-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="add-binding-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <button className="dialog-close" onClick={onClose} aria-label="Close">
          <X />
        </button>
        <span className="eyebrow">Profile action</span>
        <h2 id="add-binding-title">Add a binding</h2>
        <p>
          Search actions found in your LMU files, or enter the exact action name
          used by LMU.
        </p>
        <label className="binding-action-search">
          <Search />
          <input
            autoFocus
            maxLength={128}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="e.g. Pit Request"
          />
        </label>
        {action && matches.length > 0 && (
          <div className="binding-action-results">
            {matches.map((item) => (
              <button key={item} onClick={() => setQuery(item)}>
                {item}
              </button>
            ))}
          </div>
        )}
        <div className="binding-slot-choice" role="group" aria-label="Binding slot">
          <button
            className={!alternate ? "selected" : ""}
            onClick={() => setAlternate(false)}
          >
            Primary
          </button>
          <button
            className={alternate ? "selected" : ""}
            onClick={() => setAlternate(true)}
          >
            Alternate
          </button>
        </div>
        <footer>
          <button className="secondary" onClick={onClose}>
            Cancel
          </button>
          <button
            className="primary"
            disabled={!action}
            onClick={() => onChoose(action, alternate)}
          >
            <Plus /> Listen for control
          </button>
        </footer>
      </section>
    </div>
  );
}
