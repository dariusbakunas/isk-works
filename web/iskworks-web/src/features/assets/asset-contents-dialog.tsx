import { ChevronRight, X } from "lucide-react";
import { useEffect, useId, useMemo, useState } from "react";

import type { FlatAssetQuery, FlatAssetRow } from "../../api/assets";
import { EveTypeImage } from "../../components/eve-type-image";
import { OperationalTable, OperationalTableGroup, OperationalTableRow, type OperationalColumn } from "../../components/operational-table";
import { TypeInfoButton } from "../../components/type-info/type-info-button";
import { ownedBlueprint } from "../../components/type-info/type-info-state";
import { formatAssetDecimal } from "./asset-format";
import { ContentsButton } from "./contents-button";
import { useAssetsQuery } from "./use-assets-query";

const columns: OperationalColumn[] = [
  { key: "item", label: "Item", width: "minmax(14rem,1fr)" },
  { key: "quantity", label: "Quantity", width: "6rem", align: "right", numeric: true },
  { key: "packagedVolume", label: "Volume", width: "7rem", align: "right", numeric: true },
  { key: "group", label: "Group", width: "10rem", hideBelow: "tablet" },
];

/** Where an item sits inside its container, by its ESI location flag. */
const slotGroups: Array<{ label: string; matches: (flag: string) => boolean }> = [
  { label: "High slots", matches: (flag) => flag.startsWith("HiSlot") },
  { label: "Mid slots", matches: (flag) => flag.startsWith("MedSlot") },
  { label: "Low slots", matches: (flag) => flag.startsWith("LoSlot") },
  { label: "Rigs", matches: (flag) => flag.startsWith("RigSlot") },
  { label: "Subsystems", matches: (flag) => flag.startsWith("SubSystemSlot") },
  { label: "Service slots", matches: (flag) => flag.startsWith("ServiceSlot") },
  { label: "Drone bay", matches: (flag) => flag === "DroneBay" },
  { label: "Fighter bay", matches: (flag) => flag === "FighterBay" || flag.startsWith("FighterTube") },
  { label: "Cargo hold", matches: (flag) => flag === "Cargo" },
];

export function slotGroupLabel(flag: string): string {
  const known = slotGroups.find(({ matches }) => matches(flag));
  if (known) return known.label;
  const spaced = flag.replace(/^Specialized/, "").replace(/([a-z])([A-Z0-9])/g, "$1 $2").trim();
  return spaced ? spaced.charAt(0).toUpperCase() + spaced.slice(1).toLowerCase() : "Other";
}

function slotGroupRank(label: string) {
  const index = slotGroups.findIndex((group) => group.label === label);
  return index < 0 ? slotGroups.length : index;
}

/**
 * The stacks inside a container (a ship's fitting, cargo and drones; a
 * station container's contents), grouped by slot or bay. A container inside
 * it opens in place, with a breadcrumb back out.
 */
export function AssetContentsDialog({ container, onClose }: { container: FlatAssetRow; onClose: () => void }) {
  const [trail, setTrail] = useState<FlatAssetRow[]>([container]);
  const current = trail[trail.length - 1];
  const titleId = useId();
  const query = useMemo<FlatAssetQuery>(
    () => ({ connectionIds: [current.connectionId], containerItemId: current.eveItemId, sort: "item", order: "asc", limit: 200 }),
    [current.connectionId, current.eveItemId],
  );
  const contents = useAssetsQuery(query);
  const groups = useMemo(() => {
    const byLabel = new Map<string, FlatAssetRow[]>();
    for (const row of contents.rows) {
      const label = slotGroupLabel(row.locationFlag);
      byLabel.set(label, [...(byLabel.get(label) ?? []), row]);
    }
    return [...byLabel.entries()].sort(([a], [b]) => slotGroupRank(a) - slotGroupRank(b) || a.localeCompare(b));
  }, [contents.rows]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);
  const open = (row: FlatAssetRow) => setTrail((value) => [...value, row]);

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section aria-labelledby={titleId} aria-modal="true" className="iw-dialog iw-dialog-frame flex max-h-[85vh] w-full max-w-3xl flex-col p-0" role="dialog">
        <header className="flex items-start gap-3 border-b border-border px-4 py-3">
          <EveTypeImage size={40} typeId={current.typeId} typeName={current.typeName ?? "Unknown item"} />
          <div className="min-w-0 flex-1">
            <h2 className="truncate text-base font-semibold" id={titleId}>{current.typeName ?? "Unknown item"} contents</h2>
            <p className="truncate text-xs text-muted" title={current.locationName ?? undefined}>{current.locationName ?? "Unknown location"}</p>
            {trail.length > 1 ? (
              <nav aria-label="Container path" className="mt-1 flex flex-wrap items-center gap-1 text-xs">
                {trail.map((item, index) => (
                  <span className="flex items-center gap-1" key={`${item.connectionId}:${item.eveItemId}`}>
                    {index > 0 ? <ChevronRight aria-hidden="true" className="h-3 w-3 text-muted" /> : null}
                    {index === trail.length - 1 ? (
                      <span aria-current="location" className="font-medium text-foreground">{item.typeName ?? "Unknown item"}</span>
                    ) : (
                      <button className="text-primary hover:underline" onClick={() => setTrail(trail.slice(0, index + 1))} type="button">{item.typeName ?? "Unknown item"}</button>
                    )}
                  </span>
                ))}
              </nav>
            ) : null}
          </div>
          <button aria-label="Close contents" className="p-1 text-muted hover:text-foreground" onClick={onClose} type="button"><X aria-hidden="true" className="h-4 w-4" /></button>
        </header>
        <div className="min-h-0 flex-1 overflow-hidden text-xs">
          <OperationalTable
            ariaLabel={`${current.typeName ?? "Container"} contents`}
            columns={columns}
            footer={
              <>
                {contents.loading ? <State>Loading contents...</State> : null}
                {!contents.loading && !contents.rows.length && !contents.error ? <State>This container is empty.</State> : null}
                {contents.error ? <div className="p-3 text-center text-destructive">{contents.error.message}</div> : null}
                {contents.hasMore ? (
                  <div className="flex justify-center p-3">
                    <button className="iw-button-secondary min-h-7" disabled={contents.loadingMore} onClick={contents.loadMore} type="button">
                      {contents.loadingMore ? "Loading..." : `Load more (${contents.rows.length.toLocaleString()} of ${(contents.page?.total ?? 0).toLocaleString()})`}
                    </button>
                  </div>
                ) : null}
              </>
            }
            onSelectRow={() => undefined}
            scrollClassName="h-full max-h-[calc(85vh-6rem)] overflow-y-auto"
            selectedRowKey={null}
          >
            {contents.loading ? null : groups.map(([label, rows]) => (
              <OperationalTableGroup groupKey={label} itemCount={rows.length} key={label} label={label}>
                {rows.map((row) => <ContentRow key={`${row.connectionId}:${row.eveItemId}`} onOpen={open} row={row} />)}
              </OperationalTableGroup>
            ))}
          </OperationalTable>
        </div>
      </section>
    </div>
  );
}

function ContentRow({ onOpen, row }: { onOpen: (row: FlatAssetRow) => void; row: FlatAssetRow }) {
  const name = row.typeName ?? "Unknown item";
  const cells = {
    item: (
      <span className="flex min-w-0 items-center gap-2">
        <EveTypeImage size={32} typeId={row.typeId} typeName={name} variation={row.blueprint?.kind === "copy" ? "bpc" : row.blueprint ? "bp" : "icon"} />
        <span className="truncate font-medium text-foreground" title={name}>{name}</span>
        {row.typeName ? <TypeInfoButton blueprint={ownedBlueprint(row.blueprint)} name={row.typeName} typeId={row.typeId} /> : null}
        {row.contentCount > 0 ? <ContentsButton count={row.contentCount} name={name} onOpen={() => onOpen(row)} /> : null}
      </span>
    ),
    quantity: <span className="text-muted">{row.quantity.toLocaleString()}</span>,
    packagedVolume: <span className="text-muted">{row.totalPackagedVolume ? `${formatAssetDecimal(row.totalPackagedVolume)} m³` : "—"}</span>,
    group: <span className="truncate text-muted" title={row.groupName ?? undefined}>{row.groupName ?? "—"}</span>,
  };
  return (
    <OperationalTableRow
      cells={cells}
      interactive={row.contentCount > 0}
      onActivate={row.contentCount > 0 ? () => onOpen(row) : undefined}
      rowKey={`${row.connectionId}:${row.eveItemId}`}
    />
  );
}

function State({ children }: { children: React.ReactNode }) {
  return <div className="p-4 text-center text-muted">{children}</div>;
}
