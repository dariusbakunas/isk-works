import type { ReactNode } from "react";
import { Link } from "react-router";

export function PageHeader({
  eyebrow,
  title,
  titlePrivate = false,
  children,
}: {
  eyebrow: string;
  title: string;
  /** Marks the title as free-form user text (e.g. a price-source name) so
   * it is masked from LogRocket session replay. */
  titlePrivate?: boolean;
  children: ReactNode;
}) {
  return (
    <header className="mb-3">
      <p className="iw-eyebrow">{eyebrow}</p>
      <h1 className="iw-title" data-private={titlePrivate ? "" : undefined}>{title}</h1>
      <p className="iw-muted mt-1 max-w-3xl">{children}</p>
    </header>
  );
}

export function Panel({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <section className={`iw-panel min-w-0 p-3 ${className}`}>{children}</section>;
}

// A `Panel`'s own section divider -- an eyebrow label followed by a
// hairline rule filling the rest of the row.
export function SectionHead({ children }: { children: ReactNode }) {
  return (
    <div className="mb-1.5 flex items-center gap-2">
      <span className="iw-eyebrow whitespace-nowrap">{children}</span>
      <div className="h-px flex-1 bg-border" />
    </div>
  );
}

// A dense label/value row for read-only key-value panels.
export function KV({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-3 py-[3px] text-xs">
      <span className="shrink-0 text-muted">{label}</span>
      <span className="min-w-0 text-right font-mono text-foreground">{value}</span>
    </div>
  );
}

export function ButtonLink({
  to,
  children,
  variant = "secondary",
}: {
  to: string;
  children: ReactNode;
  variant?: "primary" | "secondary";
}) {
  return (
    <Link className={variant === "primary" ? "iw-button-primary" : "iw-button-secondary"} to={to}>
      {children}
    </Link>
  );
}

export function TextField({
  id,
  label,
  value,
  error,
  onChange,
}: {
  id: string;
  label: string;
  value: string;
  error?: string;
  onChange: (value: string) => void;
}) {
  return (
    <div>
      <label className="mb-1 block text-sm font-semibold" htmlFor={id}>
        {label}
      </label>
      <input
        id={id}
        className="iw-input"
        value={value}
        aria-describedby={`${id}-error`}
        aria-invalid={Boolean(error)}
        onChange={(event) => onChange(event.target.value)}
      />
      <p id={`${id}-error`} className="mt-1 min-h-5 text-sm text-danger">
        {error}
      </p>
    </div>
  );
}

export function StatusBadge({ children }: { children: ReactNode }) {
  return <span className="iw-badge">{children}</span>;
}

export function EmptyState({
  title,
  children,
  action,
}: {
  title: string;
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="rounded-md border border-dashed border-border bg-background/35 p-3">
      <h3 className="text-sm font-semibold text-foreground">{title}</h3>
      <p className="iw-muted mt-1">{children}</p>
      {action ? <div className="mt-3">{action}</div> : null}
    </div>
  );
}

export function InlineAlert({
  title,
  children,
  tone = "error",
}: {
  title: string;
  children: ReactNode;
  tone?: "error" | "info" | "success" | "warning";
}) {
  const toneClasses = {
    error: "border-danger/60 bg-danger/10 text-danger",
    info: "border-primary/60 bg-primary/10 text-primary",
    success: "border-positive/60 bg-positive/10 text-positive",
    warning: "border-warning/60 bg-warning/10 text-warning",
  }[tone];
  return (
    <div className={`rounded-md border px-3 py-2 text-sm ${toneClasses}`} role={tone === "error" ? "alert" : "status"}>
      <strong className="block">{title}</strong>
      <span className="text-foreground/80">{children}</span>
    </div>
  );
}

export function LoadingState({ children }: { children: ReactNode }) {
  return (
    <div className="grid min-h-[var(--iw-viewport-h)] place-items-center bg-background text-foreground">
      <div className="iw-panel p-3 text-sm text-muted" role="status">
        {children}
      </div>
    </div>
  );
}

export function ConfirmDialog({
  open,
  title,
  children,
  confirmLabel,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  title: string;
  children: ReactNode;
  confirmLabel: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section
        aria-labelledby="confirm-title"
        aria-modal="true"
        className="iw-dialog w-full max-w-md p-4"
        role="dialog"
      >
        <h2 className="text-base font-semibold" id="confirm-title">{title}</h2>
        <p className="iw-muted mt-2">{children}</p>
        <div className="mt-4 flex justify-end gap-2">
          <button className="iw-button-secondary" onClick={onCancel} type="button">Cancel</button>
          <button className="iw-button-danger" onClick={onConfirm} type="button">{confirmLabel}</button>
        </div>
      </section>
    </div>
  );
}

// ─── Tone-based presentational primitives (shared across the app) ─────────
// Originally local to the Plan detail page; promoted here so the global
// Board doesn't reimplement a 4th copy of the same tone system. `batch` is
// reserved for Acquisition Run / Execution Batch badges and accents --
// never used for ticket status, so a batched ticket never reads as a 5th
// status.

export type Tone = "primary" | "positive" | "danger" | "warning" | "muted" | "batch" | "reaction";

const badgeToneClasses: Record<Tone, string> = {
  primary: "border-primary/50 bg-primary/10 text-primary",
  positive: "border-positive/50 bg-positive/10 text-positive",
  danger: "border-danger/50 bg-danger/10 text-danger",
  warning: "border-warning/50 bg-warning/10 text-warning",
  muted: "border-border bg-panel-strong text-muted",
  batch: "border-batch/50 bg-batch/10 text-batch",
  reaction: "border-reaction/50 bg-reaction/10 text-reaction",
};

const dotToneClasses: Record<Tone, string> = {
  primary: "bg-primary",
  positive: "bg-positive",
  danger: "bg-danger",
  warning: "bg-warning",
  muted: "bg-muted",
  batch: "bg-batch",
  reaction: "bg-reaction",
};

const barToneClasses: Record<Tone, string> = {
  primary: "bg-primary",
  positive: "bg-positive",
  danger: "bg-danger",
  warning: "bg-warning",
  muted: "bg-muted",
  batch: "bg-batch",
  reaction: "bg-reaction",
};

export const textToneClasses: Record<Tone, string> = {
  primary: "text-primary",
  positive: "text-positive",
  danger: "text-danger",
  warning: "text-warning",
  muted: "text-muted",
  batch: "text-batch",
  reaction: "text-reaction",
};

// A dense "label = value" row for equation-style panels (e.g. a cost
// breakdown), with an optional leading operator and an `emphasis` variant
// for the final total row.
export function EqRow({
  label,
  op,
  value,
  tone,
  emphasis,
}: {
  label: string;
  op?: string;
  value: ReactNode;
  tone?: Tone;
  emphasis?: boolean;
}) {
  return (
    <div
      className={`flex items-baseline justify-between gap-2 py-[3px] text-xs ${
        emphasis ? "mt-1 border-t border-border pt-1.5 font-semibold" : ""
      }`}
    >
      <span className={emphasis ? "text-foreground" : "text-muted"}>
        {op ? <span className="mr-1.5 font-mono text-muted">{op}</span> : null}
        {label}
      </span>
      <span className={`font-mono font-semibold ${tone ? textToneClasses[tone] : "text-foreground"}`}>{value}</span>
    </div>
  );
}

export function Badge({
  tone,
  children,
  square = false,
}: {
  tone: Tone;
  children: ReactNode;
  /** Dense square-cornered tag instead of the default pill -- matches the
   * Board's design, which never uses pill badges. */
  square?: boolean;
}) {
  return (
    <span
      className={`inline-flex items-center whitespace-nowrap border font-semibold ${square ? "rounded-[2px] px-1 py-px text-[10px]" : "rounded-full px-2 py-0.5 text-xs"} ${badgeToneClasses[tone]}`}
    >
      {children}
    </span>
  );
}

export function StatusDot({ tone }: { tone: Tone }) {
  return <span aria-hidden="true" className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${dotToneClasses[tone]}`} />;
}

export function ProgressBar({ percent, tone = "primary" }: { percent: number; tone?: Tone }) {
  return (
    <div className="h-1.5 overflow-hidden rounded-full bg-border">
      <div
        className={`h-full rounded-full ${barToneClasses[tone]}`}
        style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
      />
    </div>
  );
}
