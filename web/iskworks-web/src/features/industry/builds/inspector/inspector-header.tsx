import { X } from "lucide-react";

import { EveTypeImage } from "../../../../components/eve-type-image";
import { Badge } from "../../../../components/primitives";
import { TypeInfoButton } from "../../../../components/type-info/type-info-button";
import type { InspectorModel } from "./inspector-model";

/** The inspector's sticky identity header and its warning badges. */
export function IdentityHeader({ model, onClose }: { model: InspectorModel; onClose?: () => void }) {
  const { identity } = model;
  return (
    <div className="sticky top-0 z-10 flex items-start gap-2 border-b border-border bg-panel px-3 py-2">
      {identity.showImage && identity.typeId != null ? (
        <EveTypeImage size={32} typeId={identity.typeId} typeName={identity.name} />
      ) : null}
      <div className="min-w-0 flex-1">
        <span className="block text-[10px] font-semibold uppercase tracking-wide text-muted">
          {identity.kindLabel}
        </span>
        <div className="flex min-w-0 items-center gap-1">
          <div className="truncate text-sm font-semibold">{identity.name}</div>
          {identity.typeId != null ? <TypeInfoButton alwaysVisible name={identity.name} typeId={identity.typeId} /> : null}
        </div>
        {identity.subtitle ? (
          <div className="truncate text-[11px] text-muted">{identity.subtitle}</div>
        ) : null}
        {identity.summary ? (
          <div className="mt-0.5 text-[11px] text-foreground/80">{identity.summary}</div>
        ) : null}
      </div>
      {onClose ? (
        <button
          aria-label="Close inspector"
          className="grid h-7 w-7 shrink-0 place-items-center rounded text-muted transition hover:bg-panel-strong hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
          onClick={onClose}
          type="button"
        >
          <X aria-hidden="true" className="h-3.5 w-3.5" />
        </button>
      ) : null}
    </div>
  );
}

export function WarningBadges({ warnings }: { warnings: InspectorModel["warnings"] }) {
  if (warnings.length === 0) return null;
  return (
    <div
      aria-label="Warnings"
      className="flex flex-wrap gap-1 border-b border-border px-3 py-2"
      role="group"
    >
      {warnings.map((warning) => (
        <Badge key={warning.label} square tone={warning.tone === "blocking" ? "danger" : "warning"}>
          <span title={warning.detail}>{warning.label}</span>
        </Badge>
      ))}
    </div>
  );
}
