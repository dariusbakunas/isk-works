import { Hammer, X } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router";

import type { BlueprintLibraryRow } from "../../../api/industry";
import { EveTypeImage } from "../../../components/eve-type-image";
import { formatRuns, kindLabel, planBuildHref } from "./blueprint-format";
import { TypeInfoButton } from "../../../components/type-info/type-info-button";

export function BlueprintInspector({ blueprint, onClose }: { blueprint: BlueprintLibraryRow; onClose: () => void }) {
  const href = planBuildHref(blueprint);
  return (
    <aside
      aria-label="Selected blueprint"
      className="absolute inset-y-0 right-0 z-40 w-72 overflow-y-auto border-l border-border bg-panel shadow-2xl"
    >
      <header className="flex items-start gap-2 px-3 py-3">
        <EveTypeImage
          size={40}
          typeId={blueprint.blueprintTypeId}
          typeName={blueprint.blueprintName}
          variation={blueprint.kind === "copy" ? "bpc" : "bp"}
        />
        <div className="min-w-0 flex-1">
          <span className="text-[0.625rem] font-semibold uppercase text-muted">Selected blueprint</span>
          <div className="flex min-w-0 items-center gap-1">
            <h2 className="truncate text-sm font-semibold" title={blueprint.blueprintName}>{blueprint.blueprintName}</h2>
            <TypeInfoButton alwaysVisible name={blueprint.blueprintName} typeId={blueprint.blueprintTypeId} />
          </div>
        </div>
        <button aria-label="Close blueprint details" className="p-1 text-muted hover:text-foreground" onClick={onClose} type="button">
          <X className="h-4 w-4" />
        </button>
      </header>
      <div className="px-3 pb-3">
        {href ? (
          <Link className="iw-button-primary inline-flex min-h-8 w-full items-center justify-center gap-1.5 text-xs" to={href}>
            <Hammer aria-hidden="true" className="h-3.5 w-3.5" />
            Plan build
          </Link>
        ) : (
          <p className="text-[0.6875rem] text-muted">
            {blueprint.activity === "reaction"
              ? "Reactions are planned from Create Build by product."
              : "This blueprint's product is not in the active static data."}
          </p>
        )}
      </div>
      <Section title="Blueprint">
        <Value label="Kind" value={blueprint.activity === "reaction" ? "Reaction formula" : kindLabel(blueprint.kind)} />
        <Value label="ME / TE" value={blueprint.activity === "reaction" ? "—" : `${blueprint.materialEfficiency} / ${blueprint.timeEfficiency}`} />
        <Value label="Runs" value={formatRuns(blueprint)} />
        <Value label="Activity" value={blueprint.activity === "reaction" ? "Reaction" : "Manufacturing"} />
      </Section>
      <Section title="Product">
        <div className="grid grid-cols-[5rem_minmax(0,1fr)] gap-2">
          <dt className="text-muted">Item</dt>
          <dd className="flex min-w-0 items-center justify-end gap-1 text-right font-mono tabular-nums text-foreground">
            <span className="break-words">{blueprint.productName ?? "Unknown"}</span>
            {blueprint.productTypeId !== null && blueprint.productName ? (
              <TypeInfoButton alwaysVisible name={blueprint.productName} typeId={blueprint.productTypeId} />
            ) : null}
          </dd>
        </div>
        <Value label="Group" value={blueprint.productGroupName ?? "—"} />
        <Value label="Category" value={blueprint.productCategoryName ?? "—"} />
      </Section>
      <Section title="Location">
        <Value label="Character" private value={blueprint.characterName ?? "Unknown character"} />
        <Value label="Location" value={blueprint.locationName ?? `Location ${blueprint.locationId}`} />
        <Value label="Flag" value={blueprint.locationFlag} />
      </Section>
      <Section title="Used by builds">
        {blueprint.usedByBuilds.length === 0 ? (
          <p className="text-muted">No Build uses this blueprint.</p>
        ) : (
          <ul className="space-y-1">
            {blueprint.usedByBuilds.map((build) => (
              <li key={build.buildId}>
                <Link className="text-primary hover:underline" data-private to={`/builds/${build.buildId}`}>{build.name}</Link>
              </li>
            ))}
          </ul>
        )}
      </Section>
      <details className="border-t border-border px-3 py-3 text-xs">
        <summary className="cursor-pointer font-semibold text-muted">Provenance</summary>
        <dl className="mt-2 space-y-1 text-[0.625rem] text-muted">
          <Value label="Synced" value={new Date(blueprint.observedAt).toLocaleString()} />
          <Value label="Item ID" value={String(blueprint.eveItemId)} />
        </dl>
      </details>
    </aside>
  );
}

function Section({ children, title }: { children: ReactNode; title: string }) {
  return (
    <section className="border-t border-border px-3 py-3 text-xs">
      <h3 className="mb-2 text-[0.625rem] font-semibold uppercase text-muted">{title}</h3>
      <dl className="space-y-1.5">{children}</dl>
    </section>
  );
}

function Value({ label, value, private: isPrivate = false }: { label: string; value: string; private?: boolean }) {
  return (
    <div className="grid grid-cols-[5rem_minmax(0,1fr)] gap-2">
      <dt className="text-muted">{label}</dt>
      <dd className="break-words text-right font-mono tabular-nums text-foreground" data-private={isPrivate ? "" : undefined}>{value}</dd>
    </div>
  );
}
