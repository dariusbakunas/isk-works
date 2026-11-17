import type { ReactNode } from "react";

/** One headline figure in a panel's totals row. */
export function Stat({ label, children, accent = false }: { label: string; children: ReactNode; accent?: boolean }) {
  return (
    <div className="min-w-[6rem] flex-1 border-r border-border px-3 py-2 last:border-r-0">
      <div className="text-[0.625rem] font-semibold uppercase tracking-wider text-muted">{label}</div>
      <div className={`font-mono text-lg font-bold tabular-nums ${accent ? "text-primary" : "text-foreground"}`}>{children}</div>
    </div>
  );
}
