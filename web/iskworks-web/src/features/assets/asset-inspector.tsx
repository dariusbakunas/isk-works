import { X } from "lucide-react";
import { Link } from "react-router";

import type { FlatAssetRow } from "../../api/assets";
import { EveTypeImage } from "../../components/eve-type-image";
import { formatAssetDecimal } from "./asset-format";

export function AssetInspector({ asset, onClose }: { asset: FlatAssetRow; onClose: () => void }) {
  return (
    <aside aria-label="Selected asset" className="absolute inset-y-0 right-0 z-40 w-72 overflow-y-auto border-l border-border bg-panel shadow-2xl">
      <header className="flex items-start gap-2 px-3 py-3">
        <EveTypeImage size={40} typeId={asset.typeId} typeName={asset.typeName ?? "Unknown item"} variation={asset.blueprint?.kind === "copy" ? "bpc" : asset.blueprint ? "bp" : "icon"} />
        <div className="min-w-0 flex-1"><span className="text-[0.625rem] font-semibold uppercase text-muted">Selected asset</span><h2 className="truncate text-sm font-semibold" title={asset.typeName ?? undefined}>{asset.typeName ?? "—"}</h2></div>
        <button aria-label="Close asset details" className="p-1 text-muted hover:text-foreground" onClick={onClose} type="button"><X className="h-4 w-4" /></button>
      </header>
      <InspectorSection title="Holdings">
        <Value label="Quantity" value={asset.quantity.toLocaleString()} />
        <Value label="Unit volume" value={asset.packagedVolume ? `${formatAssetDecimal(asset.packagedVolume)} m³` : "—"} />
        <Value label="Stack volume" value={asset.totalPackagedVolume ? `${formatAssetDecimal(asset.totalPackagedVolume)} m³` : "—"} />
      </InspectorSection>
      <InspectorSection title="Location">
        <Value label="Character" value={asset.characterName} private />
        <Value label="Location" value={asset.locationName ?? "—"} />
        <Value label="Container" value={asset.containerName ?? "—"} />
        <Value label="Flag" value={asset.locationFlag} />
      </InspectorSection>
      {asset.blueprint ? <InspectorSection title="Blueprint"><Value label="Kind" value={asset.blueprint.kind} /><Value label="ME / TE" value={`${asset.blueprint.materialEfficiency} / ${asset.blueprint.timeEfficiency}`} /><Value label="Licensed runs" value={asset.blueprint.licensedRuns?.toLocaleString() ?? "Unlimited"} /></InspectorSection> : null}
      <InspectorSection title="Reconciliation">
        <Value label="Status" value={labelStatus(asset.reconciliation.state)} />
        <Value label="Observed" value={asset.reconciliation.observedOwnerTypeQuantity.toLocaleString()} />
        <Value label="Accounted" value={asset.reconciliation.accountedOwnerTypeQuantity.toLocaleString()} />
        <Link className="mt-2 inline-flex text-xs font-medium text-primary hover:underline" to={`/inventory?item=${asset.typeId}`}>View in Inventory</Link>
      </InspectorSection>
      <details className="border-t border-border px-3 py-3 text-xs"><summary className="cursor-pointer font-semibold text-muted">Provenance</summary><dl className="mt-2 space-y-1 text-[0.625rem] text-muted"><Value label="Observed" value={new Date(asset.observedAt).toLocaleString()} /><Value label="Item ID" value={String(asset.eveItemId)} /></dl></details>
    </aside>
  );
}

function InspectorSection({ children, title }: { children: React.ReactNode; title: string }) {
  return <section className="border-t border-border px-3 py-3"><h3 className="mb-2 text-[0.625rem] font-semibold uppercase text-muted">{title}</h3><dl className="space-y-1.5 text-xs">{children}</dl></section>;
}

function Value({ label, value, private: isPrivate = false }: { label: string; value: string; private?: boolean }) {
  return <div className="grid grid-cols-[5rem_minmax(0,1fr)] gap-2"><dt className="text-muted">{label}</dt><dd className="break-words text-right font-mono tabular-nums text-foreground" data-private={isPrivate ? "" : undefined}>{value}</dd></div>;
}

function labelStatus(value: string) {
  return value.replace(/([A-Z])/g, " $1").replace(/^./, (letter) => letter.toUpperCase());
}
