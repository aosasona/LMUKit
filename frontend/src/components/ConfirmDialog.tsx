import { X } from "lucide-react";

export type Confirmation = {
  title: string;
  description: string;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: () => void;
};

export function ConfirmDialog({
  confirmation,
  busy,
  onClose,
}: {
  confirmation: Confirmation | null;
  busy: boolean;
  onClose: () => void;
}) {
  if (!confirmation) return null;
  return (
    <div
      className="dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <section
        className="confirm-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
      >
        <button
          className="dialog-close"
          aria-label="Close"
          disabled={busy}
          onClick={onClose}
        >
          <X />
        </button>
        <span className="eyebrow">Please confirm</span>
        <h2 id="confirm-title">{confirmation.title}</h2>
        <p>{confirmation.description}</p>
        <div>
          <button className="secondary" disabled={busy} onClick={onClose}>
            Cancel
          </button>
          <button
            className={confirmation.danger ? "danger" : "primary"}
            disabled={busy}
            onClick={confirmation.onConfirm}
          >
            {confirmation.confirmLabel}
          </button>
        </div>
      </section>
    </div>
  );
}
