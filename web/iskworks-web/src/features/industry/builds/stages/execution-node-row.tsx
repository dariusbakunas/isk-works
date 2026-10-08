// One Execution Plan display node, rendered as a single clickable row --
// the full detail (Used By, Production, Configuration, Cost, Occurrences)
// lives in the inspector (`stages-inspector.tsx`), not inline. Every value
// here is read straight off the endpoint's response -- no client-side
// stage/group/run/surplus calculation of any kind (see the module doc on
// `../../../../api/industry/execution-plan`).

import type { ReactNode } from "react";

import type { EpicNodeProgress, ExecutionNode, ExecutionPlanProjection, TicketStatus } from "../../../../api/industry";
import { Badge } from "../../../../components/primitives";
import { EveTypeImage } from "../../../../components/eve-type-image";
import { formatIskCompact, formatIskSummary } from "../../../../components/money";
import { OperationalTableRow } from "../../../../components/operational-table";

const TICKET_STATUS_LABELS: Record<TicketStatus, string> = {
  todo: "To do",
  inProgress: "In progress",
  complete: "Done",
  canceled: "Canceled",
};

/** The item cell's second line in an Epic's Plan: the step's ticket. */
function epicTicketLine(epic: EpicNodeProgress): string {
  if (!epic.ticketDisplayId) return "No ticket";
  return epic.ticketStatus
    ? `${epic.ticketDisplayId} · ${TICKET_STATUS_LABELS[epic.ticketStatus]}`
    : epic.ticketDisplayId;
}

function executionNodeCells(
  node: ExecutionNode,
  plan: ExecutionPlanProjection,
  showCost: boolean,
  epic?: EpicNodeProgress,
): Record<string, ReactNode> {
  const distinctConsumers = new Set(node.consumers.map((consumer) => consumer.nodeId)).size;
  const grouped = node.occurrenceIds.length > 1;
  // The Build's own final product: nothing downstream requires it, so the
  // backend's required/demand figures are 0 by definition. Show "—" and the
  // projected output instead of zeros that read as "produces nothing".
  const isFinalProduct = plan.occurrences.some(
    (occurrence) => occurrence.isRoot && node.occurrenceIds.includes(occurrence.id),
  );

  const cells: Record<string, ReactNode> = {
    item: (
      <span className="flex min-w-0 flex-col justify-center gap-0">
        <span className="flex min-w-0 items-center gap-2">
          <EveTypeImage size={24} typeId={node.outputTypeId} typeName={node.outputTypeName} />
          <span className="min-w-0 truncate font-medium">{node.outputTypeName}</span>
        </span>
        {/* Facility lives here and in the inspector header, not in its own
         * table column (it would compete with Used By/Cost for desktop
         * width). */}
        <span className="truncate pl-8 text-[11px] text-muted">
          {epic ? epicTicketLine(epic) : (node.facilityName ?? "No facility")}
        </span>
      </span>
    ),
    activity: (
      <Badge square tone={node.activity === "reaction" ? "reaction" : "primary"}>
        {node.activity === "reaction" ? "REACT" : "MANUFACTURE"}
      </Badge>
    ),
    required:
      isFinalProduct && node.requiredQuantity === 0 ? (
        <span className="text-muted" title="The Build's final product -- nothing downstream requires it.">—</span>
      ) : (
        <Quantity value={node.requiredQuantity} />
      ),
    produce: <Quantity value={isFinalProduct ? node.projectedOutput : node.productionDemand} />,
    runs: (
      <span
        className="whitespace-nowrap"
        title={
          grouped
            ? `Sum across ${node.occurrenceIds.length} planned production occurrences. Display only -- never recomputed.`
            : "Display only -- never recomputed."
        }
      >
        <Quantity value={node.projectedRuns} />
        {grouped ? <span className="ml-1 text-[11px] text-muted">total</span> : null}
      </span>
    ),
    usedBy: (
      <span className="whitespace-nowrap text-xs">
        {node.consumers.length === 0 ? (
          <span className="text-muted">—</span>
        ) : (
          `Used by ${distinctConsumers}`
        )}
      </span>
    ),
  };

  if (showCost) {
    // Operation economics, straight off the cost
    // projection -- material/component (direct inputs + consumed child
    // production), THIS operation's own installation, and their total.
    // A missing figure is "Incomplete", never 0 ISK.
    cells.material = moneyOrIncomplete(node.materialComponentCost);
    cells.install = moneyOrIncomplete(node.ownInstallationCost);
    cells.total =
      node.costComplete && node.totalProductionCost != null ? (
        <CostAmount value={node.totalProductionCost} />
      ) : (
        <span className="text-xs text-muted">Incomplete</span>
      );
  }

  return cells;
}

export function ExecutionNodeRow({
  node,
  plan,
  showCost,
  epic,
}: {
  node: ExecutionNode;
  plan: ExecutionPlanProjection;
  showCost: boolean;
  /** An Epic's Plan: the step's ticket replaces the facility line, and a
   * finished step reads as done. */
  epic?: EpicNodeProgress;
}) {
  const status: "neutral" | "warning" | "positive" = epic?.ticketStatus === "complete"
    ? "positive"
    : node.costComplete ? "neutral" : "warning";
  return <OperationalTableRow cells={executionNodeCells(node, plan, showCost, epic)} rowKey={node.id} status={status} />;
}

export function Quantity({ value, className = "" }: { value: number; className?: string }) {
  const full = value.toLocaleString("en-US");
  return (
    <span className={`whitespace-nowrap font-mono tabular-nums ${className}`} title={full}>
      {full}
    </span>
  );
}

function moneyOrIncomplete(value: string | null): ReactNode {
  return value != null ? <CostAmount value={value} /> : <span className="text-xs text-muted">Incomplete</span>;
}

/** Compact-in-cell, exact-on-hover -- the shared ISK table-cell convention
 * (see `price-items-operational-table.tsx`), not `MoneyAmount`'s full-digit
 * display, so a large production cost never crowds this dense row. */
export function CostAmount({ value }: { value: string }) {
  return (
    <span className="whitespace-nowrap font-mono tabular-nums" title={formatIskSummary(value)}>
      {formatIskCompact(value)}
    </span>
  );
}
