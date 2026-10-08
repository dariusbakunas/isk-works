import {
  createContext,
  useContext,
  useMemo,
  useState,
  type CSSProperties,
  type HTMLAttributes,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { ChevronDown, ChevronRight } from "lucide-react";

import type {
  OperationalColumn,
  OperationalTableRowProps,
  OperationalTableSelection,
} from "./contracts";

interface TableContextValue extends OperationalTableSelection {
  columns: OperationalColumn[];
}

const TableContext = createContext<TableContextValue | null>(null);

export function OperationalTable({
  ariaLabel,
  children,
  columns,
  selectedRowKey,
  onSelectRow,
  scrollClassName = "",
}: {
  ariaLabel: string;
  children: ReactNode;
  columns: OperationalColumn[];
  // Extends (not replaces) the table's own horizontal-scroll wrapper --
  // for a caller that also needs vertical scroll within a bounded height
  // (e.g. `h-full overflow-y-auto`), so the sticky `<thead>` below sticks
  // to that single scroll container instead of an unbounded one. Setting
  // *only* `overflow-x-auto` here (the default) makes the browser compute
  // `overflow-y` as `auto` too per the CSS Overflow spec's x/y
  // interdependency -- harmless on its own, but if a caller then wraps
  // this component in a second, separately-scrolling container, that
  // second container never becomes this element's nearest scrolling
  // ancestor, and `sticky` positioning silently stops working.
  scrollClassName?: string;
} & OperationalTableSelection) {
  const context = useMemo(
    () => ({ columns, selectedRowKey, onSelectRow }),
    [columns, onSelectRow, selectedRowKey],
  );
  const gridTemplateColumns = columns.map((column) => column.width).join(" ");

  return (
    <TableContext.Provider value={context}>
      <div className={`max-w-full overflow-x-auto ${scrollClassName}`} data-operational-table-scroll>
        <table
          aria-label={ariaLabel}
          className="w-full min-w-max border-collapse text-sm"
          style={{ "--operational-columns": gridTemplateColumns } as CSSProperties}
        >
          <thead className="sticky top-0 z-20 bg-background">
            <tr className="grid grid-cols-[var(--operational-columns)] border-b border-border">
              {columns.map((column) => (
                <th
                  aria-sort={
                    column.sort
                      ? column.sort.direction === "asc"
                        ? "ascending"
                        : column.sort.direction === "desc"
                          ? "descending"
                          : "none"
                      : undefined
                  }
                  className={cellClass(column, true, false)}
                  key={column.key}
                  scope="col"
                  title={column.title}
                >
                  {column.sort ? (
                    <button
                      className="flex w-full items-center gap-1 whitespace-nowrap hover:text-foreground"
                      onClick={column.sort.onToggle}
                      style={{ justifyContent: column.align === "right" ? "flex-end" : "flex-start" }}
                      type="button"
                    >
                      {column.label}
                      {column.sort.direction ? <span aria-hidden="true">{column.sort.direction === "desc" ? "↓" : "↑"}</span> : null}
                    </button>
                  ) : (
                    column.label
                  )}
                </th>
              ))}
            </tr>
          </thead>
          {children}
        </table>
      </div>
    </TableContext.Provider>
  );
}

export function OperationalTableGroup({
  children,
  defaultExpanded = true,
  emptyMessage = "No rows",
  expanded: controlledExpanded,
  groupKey,
  headerProps,
  itemCount,
  label,
  labelCase = "upper",
  labelPrivate = false,
  leading,
  onExpandedChange,
  showCount = true,
  status = "neutral",
  summary,
  trailing,
}: {
  children: ReactNode;
  defaultExpanded?: boolean;
  /** Shown in place of rows when an expanded group has none. */
  emptyMessage?: ReactNode;
  expanded?: boolean;
  groupKey: string;
  /** Extra attributes for the header row (e.g. drag-and-drop handlers). */
  headerProps?: HTMLAttributes<HTMLTableRowElement>;
  itemCount: number;
  label: string;
  /** `normal` for groups labelled by a proper name (e.g. a character). */
  labelCase?: "upper" | "normal";
  /** Masks the label from session replay (e.g. a character name). */
  labelPrivate?: boolean;
  /** Rendered inside the toggle, before the label (e.g. a portrait). */
  leading?: ReactNode;
  onExpandedChange?: (expanded: boolean) => void;
  /** Hide the bare item-count badge when `summary` already states it. */
  showCount?: boolean;
  status?: "neutral" | "positive" | "warning" | "blocking";
  summary?: ReactNode;
  /** Rendered after (outside) the toggle button, so it may hold its own
   * interactive controls such as a menu. */
  trailing?: ReactNode;
}) {
  const [internalExpanded, setInternalExpanded] = useState(defaultExpanded);
  const expanded = controlledExpanded ?? internalExpanded;

  function toggleExpanded() {
    const nextExpanded = !expanded;
    if (controlledExpanded === undefined) setInternalExpanded(nextExpanded);
    onExpandedChange?.(nextExpanded);
  }

  return (
    <tbody data-group-key={groupKey}>
      <tr
        {...headerProps}
        className="grid grid-cols-[var(--operational-columns)] border-b border-border bg-panel-strong"
      >
        <th className="col-span-full flex items-center p-0 text-left" scope="rowgroup">
          <button
            aria-expanded={expanded}
            aria-label={`${expanded ? "Collapse" : "Expand"} ${label}`}
            className={`flex w-full min-w-0 items-center gap-1.5 px-2 text-xs font-semibold text-muted hover:text-foreground ${
              labelCase === "upper" ? "h-7 uppercase" : "min-h-7 py-1"
            }`}
            onClick={toggleExpanded}
            type="button"
          >
            {expanded
              ? <ChevronDown aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
              : <ChevronRight aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />}
            {leading}
            <span
              className={labelCase === "normal" ? "text-sm text-foreground" : undefined}
              data-private={labelPrivate ? "" : undefined}
            >
              {label}
            </span>
            {showCount ? <span className={groupStatusClass(status)}>{itemCount}</span> : null}
            {summary ? (
              <span className={`${trailing ? "" : "ml-auto"} flex min-w-0 items-center font-normal normal-case text-muted`}>
                {summary}
              </span>
            ) : null}
          </button>
          {trailing ? <span className="flex shrink-0 items-center px-2">{trailing}</span> : null}
        </th>
      </tr>
      {expanded ? children : null}
      {expanded && itemCount === 0 ? (
        <tr className="grid grid-cols-[var(--operational-columns)]">
          <td className="col-span-full px-2 py-3 text-muted">{emptyMessage}</td>
        </tr>
      ) : null}
    </tbody>
  );
}

export function OperationalTableRow({
  accent,
  align = "center",
  cells,
  disabled = false,
  interactive = true,
  onActivate,
  rowKey,
  status = "neutral",
}: OperationalTableRowProps) {
  const { columns, onSelectRow, selectedRowKey } = useTableContext();
  const selected = selectedRowKey === rowKey;

  function activate() {
    if (onActivate) onActivate();
    else onSelectRow(rowKey);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLTableRowElement>) {
    if (event.key === "Enter" && !disabled && interactive) {
      event.preventDefault();
      activate();
      return;
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const table = event.currentTarget.closest("table");
    const rows = [...(table?.querySelectorAll<HTMLTableRowElement>(
      "tr[data-operational-row='true']:not([aria-disabled='true'])",
    ) ?? [])];
    const current = rows.indexOf(event.currentTarget);
    const offset = event.key === "ArrowDown" ? 1 : -1;
    rows[current + offset]?.focus();
  }

  return (
    <tr
      aria-disabled={disabled || undefined}
      aria-selected={selected}
      className={`group grid min-h-8 grid-cols-[var(--operational-columns)] border-b border-border transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary ${
        selected
          ? "shadow-[inset_2px_0_0_var(--color-primary)]"
          : accent === "blocking"
            ? "bg-destructive/5 shadow-[inset_2px_0_0_var(--color-destructive)]"
            : accent === "warning"
              ? "bg-warning/5 shadow-[inset_2px_0_0_var(--color-warning)]"
              : ""
      } ${disabled ? "cursor-not-allowed opacity-50" : interactive ? "cursor-pointer" : "cursor-default"}`}
      data-accent={accent}
      data-operational-row={interactive ? "true" : undefined}
      data-row-key={rowKey}
      onClick={() => {
        if (!disabled && interactive) activate();
      }}
      onKeyDown={handleKeyDown}
      tabIndex={disabled ? -1 : interactive ? 0 : undefined}
    >
      {columns.map((column) => (
        <td className={cellClass(column, false, selected, align)} data-status={status} key={column.key}>
          {cells[column.key] ?? null}
        </td>
      ))}
    </tr>
  );
}

function useTableContext() {
  const context = useContext(TableContext);
  if (!context) throw new Error("Operational table components require OperationalTable.");
  return context;
}

function cellClass(
  column: OperationalColumn,
  header: boolean,
  selected: boolean,
  align: "center" | "top" = "center",
) {
  // A body cell is a centered flex row; restoring it as `block` at the
  // breakpoint would drop the vertical centering (values sat top-aligned
  // next to the centered columns). Literal class names for Tailwind.
  const hidden = column.hideBelow === "desktop"
    ? header ? "hidden lg:block" : "hidden lg:flex"
    : column.hideBelow === "tablet"
      ? header ? "hidden sm:block" : "hidden sm:flex"
      : "";
  return [
    "min-w-0 overflow-hidden px-2",
    header
      ? "py-1.5 text-xs font-semibold uppercase text-muted"
      : align === "top"
        ? "flex min-h-8 items-start py-1.5"
        : "flex min-h-8 items-center py-1",
    column.align === "right" ? "justify-end text-right" : "justify-start text-left",
    column.numeric ? "font-mono tabular-nums" : "",
    column.sticky ? `${header ? "z-30 bg-inherit" : "z-10"} sticky left-0` : "",
    header
      ? ""
      : selected
        ? "bg-primary/10"
        : column.sticky
          ? "bg-background group-hover:bg-panel"
          : "group-hover:bg-panel",
    hidden,
  ].join(" ");
}

function groupStatusClass(status: "neutral" | "positive" | "warning" | "blocking") {
  return status === "positive"
    ? "text-positive"
    : status === "warning"
      ? "text-warning"
      : status === "blocking"
        ? "text-destructive"
        : "text-muted";
}
