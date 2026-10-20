import { ChevronDown, ChevronRight } from "lucide-react";
import { Fragment, useMemo, useState } from "react";
import { Link } from "react-router";

import {
  MATERIAL_CATEGORIES,
  type MaterialCategory,
  type RunwayEpicReadiness,
  type RunwayMaterial,
} from "../../../../api/inventory-analytics";
import { formatIskAbbreviated, formatIskCompact, formatIskSummary } from "../../../../components/money";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../../components/operational-table";
import { ProgressBar } from "../../../../components/primitives";
import { Private } from "../../../../observability/private";
import { coverageTone, formatPercent, formatQuantity, sumIsk } from "./runway-format";

const COLUMNS: OperationalColumn[] = [
  { key: "material", label: "Material", width: "minmax(14rem, 1fr)", sticky: true },
  { key: "needed", label: "Needed", title: "Still to be used by the Epics (required − consumed)", width: "6.5rem", align: "right", numeric: true },
  { key: "onHand", label: "On hand", width: "6.5rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "reserved", label: "Reserved", title: "Held for these Epics", width: "6rem", align: "right", numeric: true, hideBelow: "desktop" },
  { key: "short", label: "Short", width: "6.5rem", align: "right", numeric: true },
  { key: "shortValue", label: "Short value", title: "At each Epic's frozen unit cost", width: "7rem", align: "right", numeric: true },
  { key: "coverage", label: "Coverage", width: "7.5rem", hideBelow: "tablet" },
  { key: "neededBy", label: "Needed by", width: "minmax(9rem, 13rem)", hideBelow: "desktop" },
];

function dash(value: number) {
  return value > 0 ? formatQuantity(value) : <span className="text-muted">—</span>;
}

function ShortValue({ material }: { material: RunwayMaterial }) {
  if (material.short === 0) return <span className="text-muted">—</span>;
  if (material.shortValue === null) {
    return <span className="text-warning" title="No frozen unit cost on these Epics">no price</span>;
  }
  return (
    <span className="font-semibold text-danger" title={formatIskSummary(material.shortValue)}>
      −{formatIskCompact(material.shortValue)}
    </span>
  );
}

function Coverage({ percent }: { percent: number }) {
  return (
    <div className="flex w-full items-center gap-1.5">
      <div className="flex-1"><ProgressBar percent={percent} tone={coverageTone(percent)} /></div>
      <span className="w-8 text-right font-mono text-[0.625rem] tabular-nums text-muted">{formatPercent(percent)}</span>
    </div>
  );
}

function groupMaterials(materials: RunwayMaterial[]) {
  return MATERIAL_CATEGORIES.map((category) => ({
    category,
    rows: materials.filter((material) => material.category === category),
  })).filter((group) => group.rows.length > 0);
}

function groupSummary(rows: RunwayMaterial[]) {
  const short = rows.filter((row) => row.short > 0);
  if (short.length === 0) return <span className="text-positive">all covered</span>;
  const priced = short.flatMap((row) => (row.shortValue === null ? [] : [row.shortValue]));
  return (
    <span className="font-mono tabular-nums text-danger">
      {short.length} short · {priced.length > 0 ? `−${formatIskAbbreviated(sumIsk(priced))}` : "no price"}
    </span>
  );
}

export function RunwayTable({
  materials,
  epics,
}: {
  materials: RunwayMaterial[];
  epics: RunwayEpicReadiness[];
}) {
  const [expanded, setExpanded] = useState<Set<number>>(() => new Set());
  const epicNames = useMemo(
    () => new Map(epics.map((epic) => [epic.orderId, epic.displayName])),
    [epics],
  );
  const groups = useMemo(() => groupMaterials(materials), [materials]);

  function toggle(typeId: number) {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(typeId)) next.delete(typeId);
      else next.add(typeId);
      return next;
    });
  }

  const epicName = (orderId: string) => epicNames.get(orderId) ?? "Epic";

  return (
    <OperationalTable
      ariaLabel="Runway materials"
      columns={COLUMNS}
      onSelectRow={() => undefined}
      selectedRowKey={null}
    >
      {groups.map(({ category, rows }) => (
        <OperationalTableGroup
          groupKey={category}
          itemCount={rows.length}
          key={category}
          label={category}
          status={rows.some((row) => row.short > 0) ? "blocking" : "positive"}
          summary={groupSummary(rows)}
        >
          {rows.map((material) => {
            const open = expanded.has(material.typeId);
            const Chevron = open ? ChevronDown : ChevronRight;
            return (
              <Fragment key={material.typeId}>
                <OperationalTableRow
                  accent={material.short > 0 ? "blocking" : undefined}
                  cells={{
                    material: (
                      <span className="flex min-w-0 items-center gap-1.5">
                        <Chevron aria-hidden="true" className="h-3 w-3 shrink-0 text-muted" />
                        <Link
                          className="truncate hover:text-primary hover:underline"
                          onClick={(event) => event.stopPropagation()}
                          title="Open in Stock"
                          to={`/inventory?item=${material.typeId}`}
                        >
                          {material.name}
                        </Link>
                        {material.short > 0 ? (
                          <span className="shrink-0 rounded-[2px] border border-danger/50 bg-danger/10 px-1 text-[0.5625rem] font-semibold uppercase text-danger">
                            Short
                          </span>
                        ) : null}
                      </span>
                    ),
                    needed: formatQuantity(material.needed),
                    onHand: (
                      <span className={material.onHand >= material.needed ? "text-positive" : ""}>
                        {formatQuantity(material.onHand)}
                      </span>
                    ),
                    reserved: <span className="text-warning">{dash(material.reserved)}</span>,
                    short: <span className="text-danger">{dash(material.short)}</span>,
                    shortValue: <ShortValue material={material} />,
                    coverage: <Coverage percent={material.coveragePercent} />,
                    neededBy: (
                      <span className="flex min-w-0 items-center gap-1">
                        {material.epics.slice(0, 2).map((epic) => (
                          <Private
                            className="max-w-[5.5rem] truncate rounded-[2px] border border-border bg-panel-strong px-1 text-[0.625rem] text-muted"
                            key={epic.orderId}
                          >
                            {epicName(epic.orderId)}
                          </Private>
                        ))}
                        {material.epics.length > 2 ? (
                          <span className="text-[0.625rem] text-muted">+{material.epics.length - 2}</span>
                        ) : null}
                      </span>
                    ),
                  }}
                  onActivate={() => toggle(material.typeId)}
                  rowKey={String(material.typeId)}
                />
                {open
                  ? material.epics.map((epic) => (
                      <OperationalTableRow
                        cells={{
                          material: (
                            <span className="flex min-w-0 flex-col pl-5 text-xs">
                              <Private className="truncate text-muted">{epicName(epic.orderId)}</Private>
                              <span className="font-mono text-[0.625rem] tabular-nums text-muted">
                                required {formatQuantity(epic.required)} · consumed {formatQuantity(epic.consumed)}
                              </span>
                            </span>
                          ),
                          needed: <span className="text-xs">{formatQuantity(epic.outstanding)}</span>,
                          reserved: <span className="text-xs text-warning">{dash(epic.reserved)}</span>,
                          short: <span className="text-xs text-danger">{dash(epic.short)}</span>,
                        }}
                        interactive={false}
                        key={`${material.typeId}:${epic.orderId}`}
                        rowKey={`${material.typeId}:${epic.orderId}`}
                      />
                    ))
                  : null}
              </Fragment>
            );
          })}
        </OperationalTableGroup>
      ))}
    </OperationalTable>
  );
}

export function categoryFromParam(value: string | null): MaterialCategory | null {
  return MATERIAL_CATEGORIES.find((category) => category === value) ?? null;
}
