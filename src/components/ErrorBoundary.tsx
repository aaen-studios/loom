import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
  info: string | null;
}

/**
 * Catches render errors so a single broken component cannot leave a blank
 * window. Shows the message and a reload button instead.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, info: null };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    this.setState({ info: info.componentStack ?? null });
    console.error("[loom] render error:", error, info.componentStack);
  }

  render() {
    const { error, info } = this.state;
    if (!error) return this.props.children;

    return (
      <div className="flex h-full w-full items-center justify-center bg-[#0a0d16] p-6 text-[#eef1f8]">
        <div className="w-full max-w-xl rounded-sheet border border-white/10 bg-white/5 p-5">
          <h1 className="text-[16px] font-semibold">Loom hit an error</h1>
          <p className="mt-1 text-[13px] text-white/70">
            Something in the interface failed to render. Your chats are safe in
            <span className="font-mono"> ~/.loom</span>.
          </p>
          <pre className="mt-3 max-h-48 overflow-auto rounded-row bg-black/40 p-3 font-mono text-[11.5px] whitespace-pre-wrap">
            {error.message}
            {info ? `\n${info.slice(0, 1200)}` : ""}
          </pre>
          <div className="mt-3 flex gap-2">
            <button
              type="button"
              onClick={() => window.location.reload()}
              className="rounded-control bg-white px-3 py-1.5 text-[13px] font-medium text-[#0a0d16]"
            >
              Reload
            </button>
            <button
              type="button"
              onClick={() => this.setState({ error: null, info: null })}
              className="rounded-control border border-white/20 px-3 py-1.5 text-[13px] text-white/80"
            >
              Try again
            </button>
          </div>
        </div>
      </div>
    );
  }
}
