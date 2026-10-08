// Shared Build Graph layout constants + pure per-node sizing.
//
// `nodeSize(data)` is the single source of truth for a card's box, used by
// both `layout-build-graph.ts` (Dagre) and `graph-node.tsx` (the shell's
// width / minHeight). Deterministic and bounded -- no DOM measurement.
// Image space is reserved whether or not the image loads.

import type { GraphWarning } from "../../../../api/industry";

import { coverageSplit } from "../../inspector/sourcing-coverage";
import { recipeCurrencyChip } from "./recipe-currency";
import type { BuildGraphNodeData } from "./to-react-flow";

export interface NodeSize {
  width: number;
  height: number;
}

const WIDTH: Record<BuildGraphNodeData["nodeType"], number> = {
  root: 236,
  production: 208,
  acquisition: 200,
  unresolvedBuild: 200,
};

// Vertical budget, in px, of the fixed regions of each card type. The
// variable region is the compact Buy list (capped) plus optional chip /
// status rows added below.
const BASE_HEIGHT: Record<BuildGraphNodeData["nodeType"], number> = {
  root: 120, // image + eyebrow + name + runs + cost line
  production: 116, // image + eyebrow + name + need/making + cost line
  acquisition: 108, // name + BUY tag + qty + Switch-to-BUILD button
  unresolvedBuild: 96, // name + BUILD tag + status + qty
};

const COLLAPSED_SUMMARY_H = 22;
const CHIP_ROW_H = 20;
const ERROR_ROW_H = 30;
// A partial-coverage card gains a "Need / Inventory / Buy|Make" triple; a
// production card also keeps its Making/cost lines below it.
const COVERAGE_SPLIT_H = 28;

/** Metadata `nodeSize` needs beyond the domain node itself. */
export interface NodeSizeInput {
  data: BuildGraphNodeData;
  warnings?: GraphWarning[];
  pending?: boolean;
  error?: string | null;
  /** A non-root production node that is currently collapsed shows a one-line
   * "N dependencies" summary instead of its child nodes. */
  collapsedSummary?: boolean;
}

export function nodeSize(input: NodeSizeInput): NodeSize {
  const { nodeType } = input.data;
  const width = WIDTH[nodeType];
  let height = BASE_HEIGHT[nodeType];

  if (nodeType === "root" || nodeType === "production") {
    const production = input.data.node;
    // Direct requirements are their own child nodes now -- the card only
    // gains height for a collapsed one-line dependency summary, plus chips.
    if (input.collapsedSummary) height += COLLAPSED_SUMMARY_H;
    if (recipeCurrencyChip(production.recipeCurrency)) height += CHIP_ROW_H;
    if ((input.warnings?.length ?? 0) > 0) height += CHIP_ROW_H;
    if (nodeType === "production") {
      const cov = coverageSplit(production.requiredQuantity, production.netRequiredQuantity);
      // Full coverage drops the Making/cost lines; partial adds the split
      // triple above them.
      if (cov.fullyCovered) height -= CHIP_ROW_H;
      else if (cov.partiallyCovered) height += COVERAGE_SPLIT_H;
    }
  }

  if (nodeType === "acquisition") {
    const cov = coverageSplit(input.data.node.requiredQuantity, input.data.node.missingQuantity);
    if (cov.partiallyCovered) height += COVERAGE_SPLIT_H;
  }

  if (nodeType === "unresolvedBuild" && input.error) {
    height += ERROR_ROW_H;
  }

  return { width, height };
}

// Dagre spacing, top-to-bottom hierarchy.
export const GRAPH_LAYOUT = {
  rankdir: "TB" as const,
  nodesep: 40,
  ranksep: 88,
  marginx: 24,
  marginy: 24,
};
