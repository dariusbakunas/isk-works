import { AlertTriangle, ArrowRight } from "lucide-react";
import { useId, type ReactNode } from "react";
import { Link } from "react-router";

import type { Loaded } from "../../hooks/use-load";

export type PanelState =
  | { kind: "loading" }
  | { kind: "error"; message: string; retry: () => void }
  | { kind: "empty"; text: ReactNode }
  | { kind: "ready" };

/** A load as a panel state. Nothing loaded and no error (a load still
 * waiting to start) reads as loading, so a panel never flashes empty. */
export function panelState<T>(load: Loaded<T>, isEmpty: (data: T) => boolean, emptyText: ReactNode): PanelState {
  if (load.error) return { kind: "error", message: load.errorMessage ?? "Request failed.", retry: load.retry };
  if (load.loading || load.data === null) return { kind: "loading" };
  return isEmpty(load.data) ? { kind: "empty", text: emptyText } : { kind: "ready" };
}

/** The frame every Overview panel shares: a title row (with an optional
 * stamp and View all link) and the panel's loading/error/empty states. */
export function PanelFrame({
  title,
  stamp,
  link,
  state,
  children,
  className = "",
}: {
  title: string;
  stamp?: ReactNode;
  link?: { to: string; label?: string };
  state: PanelState;
  children?: ReactNode;
  className?: string;
}) {
  const titleId = useId();
  return (
    <section aria-labelledby={titleId} className={`iw-panel flex min-w-0 flex-col ${className}`}>
      <header className="flex h-9 shrink-0 items-center gap-2 border-b border-border px-3">
        <h2 className="text-[0.6875rem] font-semibold uppercase tracking-[0.12em] text-muted" id={titleId}>
          {title}
        </h2>
        {stamp ? <span className="truncate text-[0.6875rem] text-muted/80">{stamp}</span> : null}
        {link ? (
          <Link
            aria-label={`View all ${title}`}
            className="ml-auto inline-flex shrink-0 items-center gap-1 text-xs text-primary hover:underline"
            to={link.to}
          >
            {link.label ?? "View all"}
            <ArrowRight aria-hidden="true" className="h-3 w-3" />
          </Link>
        ) : null}
      </header>
      <PanelBody state={state} title={title}>{children}</PanelBody>
    </section>
  );
}

function PanelBody({ state, title, children }: { state: PanelState; title: string; children: ReactNode }) {
  if (state.kind === "loading") {
    return (
      <div aria-label={`Loading ${title}`} className="grid gap-2.5 p-3" role="status">
        {[72, 94, 58, 82].map((width) => (
          <span key={width} className="block h-3 animate-pulse rounded-sm bg-panel-strong" style={{ width: `${width}%` }} />
        ))}
      </div>
    );
  }
  if (state.kind === "error") {
    return (
      <div className="grid min-h-28 flex-1 place-items-center p-3 text-center">
        <div>
          <AlertTriangle aria-hidden="true" className="mx-auto mb-1.5 h-4 w-4 text-danger" />
          <p className="text-xs text-foreground">{state.message}</p>
          <button className="mt-1.5 text-xs text-primary hover:underline" onClick={state.retry} type="button">
            Retry
          </button>
        </div>
      </div>
    );
  }
  if (state.kind === "empty") {
    return <div className="grid min-h-24 flex-1 place-items-center p-3 text-center text-xs text-muted">{state.text}</div>;
  }
  return <>{children}</>;
}
