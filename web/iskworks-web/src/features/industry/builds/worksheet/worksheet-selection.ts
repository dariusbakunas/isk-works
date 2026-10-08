// Maps Worksheet rows onto the Plan inspector's selection. Rows are keyed by
// `type:sourcing:producer`, which changes whenever sourcing changes, so the
// selection is tracked by the stable thing the row stands for -- the producer
// Build, or the bought type -- never by row id.

import type {
  AcquisitionLine,
  BuildWorksheetRow,
  ExecutionPlanProjection,
} from "../../../../api/industry";
import type { StagesSelection } from "../stages/stages-inspector";

export type WorksheetTarget =
  | { kind: "producer"; buildId: string }
  | { kind: "buy"; typeId: number };

/** Build/Reaction rows stand for their producer; Buy rows with a shortage for
 * the acquisition. Covered Buy rows and Unresolved rows have no inspector
 * target (the plan lists acquisitions by shortage only). */
export function targetForRow(row: BuildWorksheetRow): WorksheetTarget | null {
  if (row.producerBuildId) return { kind: "producer", buildId: row.producerBuildId };
  if (row.sourcing === "buy" && (row.shortageQuantity ?? 0) > 0) return { kind: "buy", typeId: row.typeId };
  return null;
}

export function selectionForTarget(
  plan: ExecutionPlanProjection,
  target: WorksheetTarget | null,
): StagesSelection | null {
  if (!target) return null;
  if (target.kind === "producer") {
    const occurrence = plan.occurrences.find((candidate) => candidate.buildId === target.buildId && !candidate.isRoot);
    return occurrence ? { kind: "production", nodeId: occurrence.nodeId } : null;
  }
  return plan.acquisitions.some((line) => line.typeId === target.typeId)
    ? { kind: "acquisition", typeId: target.typeId }
    : null;
}

export function rowKeyForTarget(rows: BuildWorksheetRow[], target: WorksheetTarget | null): string | null {
  if (!target) return null;
  const match = rows.find((row) => {
    const rowTarget = targetForRow(row);
    return rowTarget?.kind === target.kind
      && (rowTarget.kind === "producer" ? rowTarget.buildId === (target as { buildId: string }).buildId : rowTarget.typeId === (target as { typeId: number }).typeId);
  });
  return match?.id ?? null;
}

/** Without downstream the Worksheet shows only what one operation (the root,
 * or the focused producer) needs directly, but the plan's acquisition line
 * sums every consumer. Narrow it to that operation's own edges, re-summing
 * the per-consumer figures the plan already carries, so the inspector agrees
 * with the clicked row. */
export function scopeAcquisitionToConsumer(line: AcquisitionLine, consumerBuildId: string): AcquisitionLine {
  const consumers = line.consumers.filter((consumer) => consumer.buildId === consumerBuildId);
  if (consumers.length === 0 || consumers.length === line.consumers.length) return line;
  const sum = (pick: (consumer: AcquisitionLine["consumers"][number]) => number) =>
    consumers.reduce((total, consumer) => total + pick(consumer), 0);
  // Money is a decimal string: carry it only when one edge supplies it,
  // rather than summing strings client-side.
  const [only] = consumers;
  return {
    ...line,
    consumers,
    requiredQuantity: sum((consumer) => consumer.requiredQuantity),
    plannedInventoryQuantity: sum((consumer) => consumer.plannedInventoryQuantity),
    shortageQuantity: sum((consumer) => consumer.quantity),
    freshCost: consumers.length === 1 ? only.freshCost : null,
    freshUnitPrice: consumers.length === 1 ? only.freshUnitPrice : line.freshUnitPrice,
  };
}
