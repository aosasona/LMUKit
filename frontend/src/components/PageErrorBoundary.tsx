import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, RefreshCw } from "lucide-react";
import { Component, type ErrorInfo, type ReactNode } from "react";

type Props = {
  children: ReactNode;
  resetKey: string;
};

type State = {
  error: Error | null;
};

/** Keeps a page-level React failure from turning Pivot into an unexplained blank panel. */
export class PageErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    const detail = [error.name, error.message, info.componentStack]
      .filter(Boolean)
      .join("\n");
    void invoke("report_frontend_error", { detail }).catch(() => undefined);
  }

  componentDidUpdate(previous: Props) {
    if (previous.resetKey !== this.props.resetKey && this.state.error) {
      this.setState({ error: null });
    }
  }

  render() {
    if (!this.state.error) return this.props.children;

    return (
      <section className="page-crash" role="alert">
        <AlertTriangle />
        <div>
          <span className="eyebrow">Page recovery</span>
          <h2>This page could not be displayed</h2>
          <p>
            Pivot recorded the failure in Diagnostics. Choose another page from
            the sidebar, or reload the interface and try again.
          </p>
          <button
            type="button"
            className="primary"
            onClick={() => window.location.reload()}
          >
            <RefreshCw /> Reload Pivot
          </button>
        </div>
      </section>
    );
  }
}
