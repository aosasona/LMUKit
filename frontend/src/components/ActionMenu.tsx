import { MoreHorizontal } from "lucide-react";
import { useRef } from "react";

export type ActionItem = {
  label: string;
  danger?: boolean;
  hidden?: boolean;
  onSelect: () => void;
};

export function ActionMenu({
  label = "More actions",
  items,
}: {
  label?: string;
  items: ActionItem[];
}) {
  const menu = useRef<HTMLDetailsElement>(null);
  const visible = items.filter((item) => !item.hidden);
  if (!visible.length) return null;
  return (
    <details className="action-menu" ref={menu}>
      <summary aria-label={label}>
        <MoreHorizontal />
      </summary>
      <div>
        {visible.map((item) => (
          <button
            className={item.danger ? "danger-item" : ""}
            key={item.label}
            onClick={(event) => {
              event.stopPropagation();
              menu.current?.removeAttribute("open");
              item.onSelect();
            }}
          >
            {item.label}
          </button>
        ))}
      </div>
    </details>
  );
}
