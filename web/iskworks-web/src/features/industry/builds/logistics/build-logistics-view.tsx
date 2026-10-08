// The Build Logistics view -- what needs to be where,
// and how much space it takes. Renders the backend's own Logistics plan
// (`ExecutionPlanProjection.logistics`, `crates/iskworks-core/src/logistics.rs`)
// from the same execution-plan projection the Plan view uses: destinations
// are the consuming operation's facility, lines aggregate per (destination,
// item), quantities come from the planner's own allocation (never re-netted
// here) and volumes from the SDE's packaged volume. This view computes
// nothing; it only formats.
//
// Each destination group is the natural unit of a future "Create hauling
// ticket" action. Source is shown only where it is trustworthy: the
// producing operation's facility for produced inputs; bought inputs and
// planned inventory have no known source location yet.

import type { LogisticsDestination, LogisticsLine } from "../../../../api/industry";
import { EveTypeImage } from "../../../../components/eve-type-image";
import {
  OperationalTable,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../../components/operational-table";
import { EmptyState, InlineAlert, Panel } from "../../../../components/primitives";
import { Quantity } from "../stages/execution-node-row";
import { useBuildExecutionPlan } from "../stages/use-build-execution-plan";
import type { BuildWorksheetEditorModel } from "../use-build-worksheet-editor";
import { focusExecutionPlan } from "../focused-producer-projection";
import { useMemo } from "react";

const COLUMNS: OperationalColumn[] = [
  { key: "item", label: "Item", width: "minmax(200px,1fr)", sticky: true },
  { key: "quantity", label: "Needed", width: "100px", align: "right", numeric: true },
  { key: "inventory", label: "Planned Use", width: "110px", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "toSource", label: "To Acquire", width: "110px", align: "right", numeric: true },
  { key: "produced", label: "From Production", width: "130px", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "unit", label: "Unit m³", width: "90px", align: "right", numeric: true, hideBelow: "desktop" },
  { key: "volume", label: "Volume m³", width: "120px", align: "right", numeric: true },
];

const m3Format = new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 });
const unitFormat = new Intl.NumberFormat("en-US", { maximumFractionDigits: 4 });

/** Format a decimal-string volume for display (display rounding only). */
export function formatM3(value: string | null, unit = false): string {
  if (value == null) return "—";
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return value;
  return (unit ? unitFormat : m3Format).format(parsed);
}

export function BuildLogisticsView({
  editor,
  active,
  focusedProducerId,
}: {
  editor: BuildWorksheetEditorModel;
  active: boolean;
  focusedProducerId?: string;
}) {
  const { plan: rootPlan, loading, refreshError, hardError } = useBuildExecutionPlan({
    buildId: editor.initialBuild?.id ?? "",
    previewKey: editor.previewKey,
    active,
    linkedBuildsByTypeId: editor.linkedBuildsByTypeId, linkedBuildsSettling: editor.linkedBuildsSettling,
  });
  const plan = useMemo(
    () => rootPlan && focusedProducerId ? focusExecutionPlan(rootPlan, focusedProducerId) : rootPlan,
    [rootPlan, focusedProducerId],
  );
  const logistics = plan?.logistics;

  return (
    <section aria-labelledby="logistics-heading" className="mt-1">
      <header className="mb-3">
        <h2 className="text-sm font-semibold text-foreground" id="logistics-heading">
          Logistics
        </h2>
        <p className="mt-0.5 text-sm text-muted">
          What each facility needs for its production, and how much cargo space it takes. Inputs
          belong where the operation that consumes them runs. Source locations are shown only when
          known (a producing facility).
        </p>
      </header>

      {hardError ? (
        <InlineAlert title="Logistics could not be loaded">{hardError}</InlineAlert>
      ) : !logistics ? (
        <Panel>
          <div className="p-3 text-sm text-muted" role="status">
            Calculating logistics...
          </div>
        </Panel>
      ) : logistics.destinations.length === 0 ? (
        <EmptyState title="Nothing to move">This Build has no production inputs yet.</EmptyState>
      ) : (
        <>
          {refreshError ? (
            <div className="mb-2">
              <InlineAlert title="Logistics may be out of date" tone="warning">
                {refreshError}
              </InlineAlert>
            </div>
          ) : null}
          {loading ? (
            <p className="mb-2 text-xs text-muted" role="status">
              Updating...
            </p>
          ) : null}
          <p className="mb-3 text-xs text-muted">
            Total cargo:{" "}
            <span className="font-mono font-semibold text-foreground">
              {formatM3(logistics.totalVolumeM3)} m³
            </span>
            {logistics.volumeComplete ? null : (
              <span> (some items have no known volume)</span>
            )}{" "}
            across {logistics.destinations.length} destination
            {logistics.destinations.length === 1 ? "" : "s"}
          </p>
          {logistics.destinations.map((destination) => (
            <DestinationSection destination={destination} key={destination.key} />
          ))}
        </>
      )}
    </section>
  );
}

function DestinationSection({ destination }: { destination: LogisticsDestination }) {
  const title = destination.facilityName ?? "No facility selected";
  return (
    <div className="mb-5">
      <h3 className="mb-2 flex flex-wrap items-baseline gap-x-2 text-xs font-semibold uppercase tracking-wide text-muted">
        <span className="text-foreground">{title}</span>
        {destination.solarSystem ? (
          <span className="text-[11px] font-normal normal-case">{destination.solarSystem}</span>
        ) : null}
        <span className="ml-auto text-[11px] font-normal normal-case">
          {destination.lines.length} item{destination.lines.length === 1 ? "" : "s"} ·{" "}
          <span className="font-mono font-semibold text-foreground">
            {formatM3(destination.totalVolumeM3)} m³
          </span>
          {destination.volumeComplete ? null : " + unknown"}
        </span>
      </h3>
      <OperationalTable
        ariaLabel={`Logistics for ${title}`}
        columns={COLUMNS}
        onSelectRow={ignoreSelection}
        selectedRowKey={null}
      >
        <tbody>
          {destination.lines.map((line) => (
            <OperationalTableRow cells={lineCells(line)} key={line.typeId} rowKey={String(line.typeId)} />
          ))}
        </tbody>
      </OperationalTable>
    </div>
  );
}

// Rows are not selectable (there is no line inspector).
const ignoreSelection = () => {};

function lineCells(line: LogisticsLine) {
  const usedBy = line.consumers.map((consumer) => consumer.outputTypeName).join(", ");
  const from = line.producers
    .map((producer) => producer.facilityName ?? "No facility")
    .filter((name, index, all) => all.indexOf(name) === index)
    .join(", ");
  return {
    item: (
      <span className="flex min-w-0 flex-col justify-center">
        <span className="flex min-w-0 items-center gap-2">
          <EveTypeImage size={24} typeId={line.typeId} typeName={line.typeName} />
          <span className="min-w-0 truncate font-medium">{line.typeName}</span>
        </span>
        <span className="truncate pl-8 text-[11px] text-muted" title={`Used by ${usedBy}`}>
          For {usedBy}
        </span>
      </span>
    ),
    quantity: <Quantity value={line.quantity} />,
    inventory: <Quantity value={line.plannedInventoryQuantity} />,
    toSource: (
      <Quantity
        className={line.acquireQuantity + line.unresolvedQuantity > 0 ? "text-warning" : undefined}
        value={line.acquireQuantity + line.unresolvedQuantity}
      />
    ),
    produced: (
      <span title={from ? `Produced at ${from}` : undefined}>
        <Quantity value={line.producedQuantity} />
      </span>
    ),
    unit: <span className="font-mono">{formatM3(line.unitVolumeM3, true)}</span>,
    volume: <span className="font-mono">{formatM3(line.totalVolumeM3)}</span>,
  };
}
