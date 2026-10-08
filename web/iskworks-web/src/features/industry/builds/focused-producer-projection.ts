import type {
  AcquisitionLine,
  ExecutionNode,
  ExecutionPlanProjection,
  LogisticsDestination,
} from "../../../api/industry";
import { sumDecimalStrings } from "../shared/decimal";

/**
 * Selects a producer and everything it directly or transitively needs from
 * the root's already-authoritative projection. This never invokes a planner
 * and never uses the descendant Build's persisted `runs`.
 */
export function focusExecutionPlan(
  plan: ExecutionPlanProjection,
  producerBuildId: string,
): ExecutionPlanProjection | null {
  const focusOccurrence = plan.occurrences.find((occurrence) => occurrence.buildId === producerBuildId);
  if (!focusOccurrence) return null;

  const retainedNodeIds = new Set<string>([focusOccurrence.nodeId]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const edge of plan.edges) {
      if (retainedNodeIds.has(edge.to) && !retainedNodeIds.has(edge.from)) {
        retainedNodeIds.add(edge.from);
        changed = true;
      }
    }
  }

  const retainedOccurrenceIds = new Set(
    plan.occurrences
      .filter((occurrence) => retainedNodeIds.has(occurrence.nodeId))
      .map((occurrence) => occurrence.id),
  );
  const nodes = plan.nodes
    .filter((node) => retainedNodeIds.has(node.id))
    .map((node) => focusedNode(node, retainedNodeIds, focusOccurrence.nodeId));
  const stages = plan.stages
    .map((stage) => ({ ...stage, nodeIds: stage.nodeIds.filter((id) => retainedNodeIds.has(id)) }))
    .filter((stage) => stage.nodeIds.length > 0);
  const acquisitions = plan.acquisitions
    .map((line) => focusedAcquisition(line, retainedNodeIds))
    .filter((line): line is AcquisitionLine => line != null);

  const destinations = plan.logistics.destinations
    .map((destination) => focusedDestination(destination, retainedOccurrenceIds))
    .filter((destination): destination is LogisticsDestination => destination != null);
  const logisticsTotal = sumDecimalStrings(destinations.map((destination) => destination.totalVolumeM3));
  return {
    ...plan,
    rootNodeId: focusOccurrence.nodeId,
    nodes,
    stages,
    edges: plan.edges.filter((edge) => retainedNodeIds.has(edge.from) && retainedNodeIds.has(edge.to)),
    // Requirement rows are occurrence-owned authoritative evidence. Focus
    // filters occurrences but never recomputes or re-nets their requirements.
    occurrences: plan.occurrences.filter((occurrence) => retainedOccurrenceIds.has(occurrence.id)),
    acquisitions,
    unresolved: plan.unresolved.filter((item) => retainedNodeIds.has(item.owningNodeId)),
    logistics: {
      ...plan.logistics,
      destinations,
      totalVolumeM3: logisticsTotal.value ?? "0",
      volumeComplete: destinations.every((destination) => destination.volumeComplete),
    },
  };
}

function focusedNode(
  node: ExecutionNode,
  retainedNodeIds: Set<string>,
  focusNodeId: string,
): ExecutionNode {
  const consumers = node.consumers.filter((consumer) => retainedNodeIds.has(consumer.nodeId));
  return {
    ...node,
    consumers,
    // The focused operation keeps its root-owned aggregate demand. A
    // prerequisite's Required column is the retained consumer-edge demand
    // (107 under Auto-Integrity), while its production sizing/runs/economics
    // remain the canonical shared operation totals (161 across the plan).
    requiredQuantity: node.id === focusNodeId
      ? node.requiredQuantity
      : sum(consumers.map((consumer) => consumer.requiredQuantity)),
    plannedInventoryQuantity: node.id === focusNodeId
      ? node.plannedInventoryQuantity
      : sum(consumers.map((consumer) => consumer.plannedInventoryQuantity)),
  };
}

function focusedAcquisition(
  line: AcquisitionLine,
  retainedNodeIds: Set<string>,
): AcquisitionLine | null {
  const consumers = line.consumers.filter((consumer) => retainedNodeIds.has(consumer.nodeId));
  if (consumers.length === 0) return null;
  const freshCost = sumDecimalStrings(consumers.map((consumer) => consumer.freshCost));
  const unitPrices = new Set(consumers.map((consumer) => consumer.freshUnitPrice));
  return {
    ...line,
    consumers,
    requiredQuantity: sum(consumers.map((consumer) => consumer.requiredQuantity)),
    plannedInventoryQuantity: sum(consumers.map((consumer) => consumer.plannedInventoryQuantity)),
    shortageQuantity: sum(consumers.map((consumer) => consumer.quantity)),
    freshCost: freshCost.partial ? null : freshCost.value,
    freshUnitPrice: unitPrices.size === 1 ? consumers[0].freshUnitPrice : null,
  };
}

function focusedDestination(
  destination: LogisticsDestination,
  retainedOccurrenceIds: Set<string>,
): LogisticsDestination | null {
  const lines = destination.lines
    .map((line) => {
      const consumers = line.consumers.filter((consumer) => retainedOccurrenceIds.has(consumer.operationId));
      if (consumers.length === 0) return null;
      const quantity = sum(consumers.map((consumer) => consumer.requiredQuantity));
      const producedQuantity = sum(consumers.filter((consumer) => consumer.source === "produced").map((consumer) => consumer.shortageQuantity));
      return {
        ...line,
        consumers,
        quantity,
        plannedInventoryQuantity: sum(consumers.map((consumer) => consumer.plannedInventoryQuantity)),
        shortageQuantity: sum(consumers.map((consumer) => consumer.shortageQuantity)),
        acquireQuantity: sum(consumers.filter((consumer) => consumer.source === "acquire").map((consumer) => consumer.shortageQuantity)),
        producedQuantity,
        unresolvedQuantity: sum(consumers.filter((consumer) => consumer.source === "unresolved").map((consumer) => consumer.shortageQuantity)),
        totalVolumeM3: line.unitVolumeM3 == null ? null : multiplyDecimalByInteger(line.unitVolumeM3, quantity),
        producers: line.producers.length === 1
          ? [{ ...line.producers[0], quantity: producedQuantity }]
          : line.producers,
      };
    })
    .filter((line): line is LogisticsDestination["lines"][number] => line != null);
  if (lines.length === 0) return null;
  const total = sumDecimalStrings(lines.map((line) => line.totalVolumeM3));
  return {
    ...destination,
    operationIds: destination.operationIds.filter((id) => retainedOccurrenceIds.has(id)),
    lines,
    totalVolumeM3: total.value ?? "0",
    volumeComplete: !total.partial,
  };
}

function sum(values: number[]): number {
  return values.reduce((total, value) => total + value, 0);
}

function multiplyDecimalByInteger(value: string, multiplier: number): string {
  const match = /^(\d+)(?:\.(\d+))?$/.exec(value);
  if (!match || !Number.isSafeInteger(multiplier)) throw new Error(`Invalid logistics quantity: ${value} x ${multiplier}`);
  const fraction = match[2] ?? "";
  const coefficient = BigInt(`${match[1]}${fraction}`) * BigInt(multiplier);
  if (fraction.length === 0) return coefficient.toString();
  const digits = coefficient.toString().padStart(fraction.length + 1, "0");
  return `${digits.slice(0, -fraction.length)}.${digits.slice(-fraction.length)}`;
}
