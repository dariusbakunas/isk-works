// Frontend warning -> node mapping. The projection's top-level
// `warnings[]` each carry a `graphNodeId`; the per-node `warning` fields on
// the DTO are currently always null, so this is the source of truth for
// node warning chips and selected-node inspector callouts. The global
// warning list stays as-is.

import type { GraphWarning, GraphWarningCode } from "../../../../api/industry";

/** Which warnings warrant a card-level chip (vs. inspector / global only). */
const CARD_WARNING_CODES: ReadonlySet<GraphWarningCode> = new Set([
  "runsDiverged",
  "linkedBuildUnresolved",
]);

export function warningsByNodeId(
  warnings: readonly GraphWarning[],
): Map<string, GraphWarning[]> {
  const map = new Map<string, GraphWarning[]>();
  for (const warning of warnings) {
    const list = map.get(warning.graphNodeId) ?? [];
    list.push(warning);
    map.set(warning.graphNodeId, list);
  }
  return map;
}

export function isCardWarning(code: GraphWarningCode): boolean {
  return CARD_WARNING_CODES.has(code);
}

/** The chip label for a node that has one or more warnings. */
export function warningChipLabel(warnings: readonly GraphWarning[]): string | null {
  const card = warnings.filter((warning) => isCardWarning(warning.code));
  if (card.length === 0) return null;
  return card.length === 1 ? warningLabel(card[0].code) : `${card.length} warnings`;
}

export function warningLabel(code: GraphWarningCode): string {
  switch (code) {
    case "runsDiverged":
      return "Runs diverged";
    case "linkedBuildUnresolved":
      return "Linked build unresolved";
    case "staleRecipe":
      return "Recipe changed";
    case "marketPriceUnavailable":
      return "Market price unavailable";
    default:
      return code;
  }
}
