import { Component, type ReactNode } from 'react';

interface Props {
  children: ReactNode;
  /** Optional label shown in the fallback (e.g. "Overview", "Dashboard"). */
  label?: string;
}

interface State {
  error: Error | null;
}

/**
 * Catches render-time crashes so the whole app doesn't go black.
 * Shows the error message + a reload button.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    // Log for debugging – visible in the Tauri devtools console.
    console.error(`[ErrorBoundary${this.props.label ? `:${this.props.label}` : ''}]`, error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
          <div className="text-[15px] font-semibold text-fg">
            {this.props.label ? `${this.props.label} crashed` : 'Something went wrong'}
          </div>
          <pre className="max-w-[600px] overflow-auto rounded-lg border border-err/30 bg-err/10 p-3 text-left text-[12px] text-err">
            {this.state.error.message}
          </pre>
          <button
            onClick={() => this.setState({ error: null })}
            className="rounded-lg border border-line bg-panel-2 px-4 py-2 text-[13px] text-fg hover:bg-hover"
          >
            Try again
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
