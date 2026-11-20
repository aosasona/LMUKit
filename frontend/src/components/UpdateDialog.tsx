import { CalendarDays, Download, Sparkles, X } from "lucide-react";
import { useEffect } from "react";
import type { BundledReleaseNotes } from "../lib/releaseNotes";

type Props =
  | {
      kind: "available";
      version: string;
      notes: string | null;
      publishedAt: string | null;
      busy: boolean;
      onClose: () => void;
      onInstall: () => void;
    }
  | {
      kind: "whats-new";
      version: string;
      release: BundledReleaseNotes;
      busy?: false;
      onClose: () => void;
    };

export function UpdateDialog(props: Props) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !props.busy) props.onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [props]);

  const isAvailable = props.kind === "available";
  const title = isAvailable
    ? `Pivot ${props.version} is ready`
    : props.release.title;
  const summary = isAvailable
    ? "A signed Pivot update is available. Review what changed, then install it whenever you’re ready."
    : props.release.summary;

  return (
    <div
      className="dialog-backdrop update-dialog-backdrop"
      role="presentation"
      onPointerDown={(event) => {
        if (event.target === event.currentTarget && !props.busy)
          props.onClose();
      }}
    >
      <section
        className="update-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="update-dialog-title"
        onPointerDown={(event) => event.stopPropagation()}
      >
        <button
          type="button"
          className="dialog-close"
          aria-label="Close"
          disabled={props.busy}
          onClick={props.onClose}
        >
          <X />
        </button>
        <header>
          <span className="update-dialog-icon">
            {isAvailable ? <Download /> : <Sparkles />}
          </span>
          <div>
            <span className="eyebrow">
              {isAvailable
                ? "Update available"
                : `Welcome to Pivot ${props.version}`}
            </span>
            <h2 id="update-dialog-title">{title}</h2>
            <p>{summary}</p>
          </div>
        </header>

        {isAvailable ? (
          <>
            {props.publishedAt && (
              <div className="update-dialog-date">
                <CalendarDays /> Published{" "}
                {formatReleaseDate(props.publishedAt)}
              </div>
            )}
            <section className="update-dialog-notes">
              <strong>What’s changed</strong>
              <p>
                {props.notes ?? "See the release page for the complete notes."}
              </p>
            </section>
          </>
        ) : (
          <section className="update-dialog-notes bundled">
            <strong>What’s changed</strong>
            <ul>
              {props.release.highlights.map((highlight) => (
                <li key={highlight}>{highlight}</li>
              ))}
            </ul>
          </section>
        )}

        <footer>
          {isAvailable ? (
            <>
              <button
                type="button"
                className="secondary"
                onClick={props.onClose}
              >
                Later
              </button>
              <button
                type="button"
                className="primary update-dialog-install"
                disabled={props.busy}
                onClick={props.onInstall}
              >
                <Download /> Download and install
              </button>
            </>
          ) : (
            <button type="button" className="primary" onClick={props.onClose}>
              Got it
            </button>
          )}
        </footer>
      </section>
    </div>
  );
}

function formatReleaseDate(value: string) {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime())
    ? value
    : parsed.toLocaleDateString(undefined, {
        day: "numeric",
        month: "long",
        year: "numeric",
      });
}
