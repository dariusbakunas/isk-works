import { PackageOpen } from "lucide-react";

/** Opens a container's contents; sits inside a clickable row, so it keeps its click to itself. */
export function ContentsButton({ count, name, onOpen }: { count: number; name: string; onOpen: () => void }) {
  return (
    <button
      aria-label={`Show contents of ${name} (${count.toLocaleString()} item${count === 1 ? "" : "s"})`}
      className="inline-flex shrink-0 items-center gap-1 rounded-[3px] border border-border bg-panel-strong px-1.5 py-0.5 font-mono text-[0.625rem] text-muted hover:border-primary/70 hover:text-foreground"
      onClick={(event) => { event.stopPropagation(); onOpen(); }}
      // Only the keys the row would also act on: Escape must still reach the dialog.
      onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") event.stopPropagation(); }}
      title="Show contents"
      type="button"
    >
      <PackageOpen aria-hidden="true" className="h-3 w-3" />
      {count.toLocaleString()}
    </button>
  );
}
