import { Download, ExternalLink, MoreHorizontal } from "lucide-react";
import type { ReactNode } from "react";

import { DropdownMenu } from "../../../components/dropdown-menu";

/** A titled analytics panel with the shared overflow menu. */
export function ChartCard({
  title,
  subtitle,
  children,
  onViewTransactions,
  onExportCsv,
  className = "",
}: {
  title: string;
  subtitle?: ReactNode;
  children: ReactNode;
  /** Omit to hide the item. */
  onViewTransactions?: () => void;
  onExportCsv?: () => void;
  className?: string;
}) {
  const items = [
    onViewTransactions
      ? { key: "view", label: "View transactions", icon: <ExternalLink aria-hidden="true" className="h-3.5 w-3.5" />, onSelect: onViewTransactions }
      : null,
    onExportCsv
      ? { key: "export", label: "Export CSV", icon: <Download aria-hidden="true" className="h-3.5 w-3.5" />, onSelect: onExportCsv }
      : null,
  ].filter((item): item is NonNullable<typeof item> => item !== null);

  return (
    <section aria-label={title} className={`iw-panel min-w-0 ${className}`}>
      <header className="flex items-start justify-between gap-2 border-b border-border px-3 py-2">
        <div className="min-w-0">
          <h2 className="text-xs font-semibold text-foreground">{title}</h2>
          {subtitle ? <p className="mt-0.5 text-[0.625rem] text-muted">{subtitle}</p> : null}
        </div>
        {items.length > 0 ? (
          <DropdownMenu
            align="end"
            icon={<MoreHorizontal aria-hidden="true" className="h-3.5 w-3.5" />}
            items={items}
            label={`${title} options`}
            variant="icon"
          />
        ) : null}
      </header>
      <div className="p-3">{children}</div>
    </section>
  );
}
