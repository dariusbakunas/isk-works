// The root Build's row-level exceptions, edited in the Plan inspectors -- a
// per-row price override (manual price / market-policy override / reset to
// the Price Source default) and a per-component fulfillment scope.
//
// No separate model: the state is the root editor's own (`prices`,
// `itemPricingPolicies`, `fulfillmentScopes`), written through the shared
// helpers (`applyPricingSelection`, `applyFulfillmentScope`) and autosaved
// like any root edit. The current effective pricing is read from the root
// worksheet rows of the editor's live preview (`estimate.worksheet`).
// These exceptions belong to the ROOT Build's own inputs/output; a nested
// Build's inputs are priced by that Build's own configuration.

import type {
  FulfillmentScope,
  PlannerPricingSelection,
  WorksheetItem,
} from "../../../../api/industry";
import { pricingSlice } from "../../inspector/adapters/material-slices";
import type { RowPricingSlice } from "../../inspector/inspector-model";
import { applyFulfillmentScope, applyPricingSelection } from "../components/planner-panels";
import type { BuildWorksheetEditorModel } from "../use-build-worksheet-editor";

export interface RootRowEditing {
  /** The root worksheet row's pricing, or `null` when the type is not a
   * priced root row (e.g. not a root input, or no live preview yet). */
  pricing: (typeId: number, role: "material" | "output") => RowPricingSlice | null;
  onPricingChange: (selection: PlannerPricingSelection) => void;
  scopeOf: (typeId: number) => "missing" | "full";
  onScopeChange: (typeId: number, scope: "missing" | "full") => void;
}

export function rootRowEditing(editor: BuildWorksheetEditorModel): RootRowEditing | null {
  const { setPrices, setItemPricingPolicies, setFulfillmentScopes } = editor;
  if (!setPrices || !setItemPricingPolicies || !setFulfillmentScopes) return null;
  const worksheet = editor.estimate?.worksheet ?? null;
  const rows: WorksheetItem[] = worksheet
    ? [...worksheet.groups.flatMap((group) => group.items), ...worksheet.output.items]
    : [];
  return {
    pricing: (typeId, role) => {
      const item = rows.find((row) => row.typeId === typeId && row.role === role);
      if (!item) return null;
      return pricingSlice(item, {
        allowPolicyOverride: editor.source?.kind !== "manual",
        readOnly: false,
        sourceName: editor.source?.name,
      }) as RowPricingSlice;
    },
    onPricingChange: (selection) => applyPricingSelection(selection, setPrices, setItemPricingPolicies),
    scopeOf: (typeId) => (editor.fulfillmentScopes?.[typeId] === "full" ? "full" : "missing"),
    onScopeChange: (typeId, scope) =>
      applyFulfillmentScope(
        typeId,
        scope === "full" ? ("full" as FulfillmentScope) : null,
        setFulfillmentScopes,
      ),
  };
}
