import { TriangleAlert } from "lucide-react";
import { useState } from "react";

/**
 * A compact "N notes" badge that reveals its messages in a small popover on
 * hover or click -- for advisory warnings that a reader only needs to see
 * once and that otherwise eat vertical space (facility assumptions, stale
 * pricing, non-applicable rigs, ...). Not for blockers, which stay inline.
 */
export function WarningBadge({
  warnings,
  label = "Planning notes",
  className = "",
}: {
  warnings: string[];
  label?: string;
  className?: string;
}) {
  const [hovering, setHovering] = useState(false);
  const [pinned, setPinned] = useState(false);
  if (warnings.length === 0) return null;
  const open = hovering || pinned;

  return (
    <div
      className={`relative inline-flex shrink-0 ${className}`}
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
    >
      <button
        aria-expanded={open}
        aria-label={`${warnings.length} ${label.toLowerCase()}`}
        className="inline-flex items-center gap-1 rounded border border-warning/50 bg-warning/10 px-1.5 py-0.5 text-[10px] font-semibold uppercase tabular-nums text-warning hover:bg-warning/20"
        onClick={() => setPinned((value) => !value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") setPinned(false);
        }}
        type="button"
      >
        <TriangleAlert aria-hidden="true" className="h-3.5 w-3.5" />
        {warnings.length}
      </button>
      {open ? (
        <div
          className="absolute right-0 top-full z-30 mt-1 w-[22rem] max-w-[calc(100vw-2rem)] border border-border bg-panel p-2 text-xs shadow-xl"
          role="tooltip"
        >
          <p className="mb-1 font-semibold uppercase tracking-wide text-muted">{label}</p>
          <ul className="space-y-1.5">
            {warnings.map((warning) => (
              <li className="text-muted" key={warning}>
                {warning}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}
