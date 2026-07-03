/**
 * Error boundary — catches render-time exceptions in the React tree so a crash
 * in one component shows a visible error card instead of blanking the window.
 * In dev, shows the stack; in prod, a friendlier message.
 */
import { Component, type ReactNode } from "react";

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: { componentStack: string }) {
    console.error("UI crash:", error, info);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="flex h-full items-center justify-center bg-canvas p-8">
          <div className="max-w-lg rounded-xl border border-danger/30 bg-danger-soft p-5 shadow-md">
            <h2 className="text-[14px] font-semibold text-danger">
              Something rendered badly
            </h2>
            <p className="mt-1.5 text-[12px] text-ink-2">
              {this.state.error.message}
            </p>
            <pre className="mt-3 max-h-48 overflow-auto rounded-md bg-surface p-2 text-[10px] text-ink-3">
              {this.state.error.stack}
            </pre>
            <button
              onClick={() => this.setState({ error: null })}
              className="mt-3 rounded-md bg-surface px-3 py-1.5 text-[12px] font-medium text-ink-2 shadow-xs hover:bg-surface-3"
            >
              Try again
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
