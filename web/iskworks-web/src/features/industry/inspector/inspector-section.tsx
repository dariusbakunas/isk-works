import { ChevronDown, ChevronRight } from "lucide-react";
import type { ReactNode } from "react";

import { useInspectorCollapse } from "./inspector-collapse";

export type InspectorTone = "neutral" | "positive" | "blocking";

const toneClass: Record<InspectorTone, string> = {
  neutral: "",
  positive: "text-positive",
  blocking: "text-destructive",
};

/**
 * One collapsible section of the unified inspector. Shared by the Worksheet
 * item inspector and the Graph node inspector -- structure and
 * collapse behavior are identical regardless of which view made the
 * selection.
 *
 * - Collapse state persists across selection changes (see
 *   `InspectorCollapseProvider`); `defaultExpanded` is followed until the
 *   user toggles this specific section.
 * - A collapsed section still shows `summary` (a one-line digest) so the
 *   user can scan without expanding.
 * - `warning` renders whether the section is open or closed, so a problem
 *   is never hidden behind a collapsed header.
 */
export function InspectorSection({
  id,
  label,
  summary,
  warning,
  defaultExpanded = false,
  children,
}: {
  id: string;
  label: string;
  summary?: ReactNode;
  warning?: ReactNode;
  defaultExpanded?: boolean;
  children: ReactNode;
}) {
  const collapse = useInspectorCollapse();
  const expanded = collapse.isExpanded(id, defaultExpanded);
  const Chevron = expanded ? ChevronDown : ChevronRight;

  return (
    <section aria-label={label} className="border-b border-border">
      <h3>
        <button
          aria-expanded={expanded}
          className="flex w-full items-center gap-1.5 px-3 py-2 text-left text-[10px] font-semibold uppercase tracking-wide text-muted transition hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
          onClick={() => collapse.toggle(id, defaultExpanded)}
          type="button"
        >
          <Chevron aria-hidden="true" className="h-3 w-3 shrink-0" />
          <span className="shrink-0">{label}</span>
          {!expanded && summary ? (
            <span className="ml-auto truncate font-normal normal-case text-foreground/75">
              {summary}
            </span>
          ) : null}
        </button>
      </h3>
      {warning ? <div className="px-3 pb-2">{warning}</div> : null}
      {expanded ? <div className="px-3 pb-3">{children}</div> : null}
    </section>
  );
}

/** A 2-up grid of compact label/value metrics -- the Coverage block idiom. */
export function InspectorMetricGrid({
  metrics,
}: {
  metrics: Array<{ label: string; value: string; tone?: InspectorTone }>;
}) {
  return (
    <dl className="grid grid-cols-2 gap-x-3 gap-y-1.5">
      {metrics.map((metric) => (
        <div className="min-w-0" key={metric.label}>
          <dt className="text-[10px] leading-4 text-muted">{metric.label}</dt>
          <dd
            className={`truncate text-right font-mono text-xs font-semibold tabular-nums ${toneClass[metric.tone ?? "neutral"]}`}
            title={metric.value}
          >
            {metric.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

/** A single label / value row. */
export function InspectorRow({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: ReactNode;
  tone?: InspectorTone;
}) {
  return (
    <div className="flex items-start justify-between gap-3 py-1 text-xs">
      <span className="text-muted">{label}</span>
      <strong className={`text-right font-mono ${toneClass[tone]}`}>{value}</strong>
    </div>
  );
}
