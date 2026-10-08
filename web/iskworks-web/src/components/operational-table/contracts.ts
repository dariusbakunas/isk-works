import type { ReactNode } from "react";

export interface OperationalColumn {
  key: string;
  label: string;
  title?: string;
  width: string;
  align?: "left" | "right";
  numeric?: boolean;
  sticky?: boolean;
  hideBelow?: "tablet" | "desktop";
  /** Optional click-to-sort header -- omit for a plain, unsortable column
   * (every existing consumer). `direction: null` means this column isn't
   * the active sort. */
  sort?: { direction: "asc" | "desc" | null; onToggle: () => void };
}

export interface OperationalTableSelection {
  selectedRowKey: string | null;
  onSelectRow: (rowKey: string) => void;
}

export interface OperationalTableRowProps {
  rowKey: string;
  cells: Record<string, ReactNode>;
  status?: "neutral" | "positive" | "warning" | "blocking";
  disabled?: boolean;
  interactive?: boolean;
  onActivate?: () => void;
  /** Visual-only attention edge (2px left bar + faint tint). Unlike
   * `status`, which is semantic data on the cells, this changes how the row
   * looks -- opt-in so existing tables are unaffected. */
  accent?: "warning" | "blocking";
  /** Top-align cells instead of centering them, for rows whose cells stack
   * several sub-lines that must line up across columns. */
  align?: "center" | "top";
}
