import type { BlueprintSelection, ComponentFacilityOverride } from "../../../../api/industry";
import { buildRootInspector } from "../../inspector/adapters/root-build";
import { InspectorCollapseProvider } from "../../inspector/inspector-collapse";
import { UnifiedItemInspector } from "../../inspector/unified-item-inspector";
import { PlannerContextPanel } from "../planner/planner-context-panel";
import { PlannerInspectorShell } from "../../../../components/planner-inspector-shell";
import { rowKey } from "../planner/production-worksheet";
import type { BuildWorksheetEditorModel } from "../use-build-worksheet-editor";
import {
  applyComponentBlueprintSelection,
  applyComponentFacilityOverride,
  applyComponentResolution,
  applyFulfillmentScope,
  applyPricingSelection,
} from "./planner-panels";

/**
 * The single Build-page right-rail inspector. Build Settings and the Selected
 * Item inspector are mutually exclusive modes of the same surface -- this
 * component switches between them on `editor.inspectorMode` so only one is
 * ever mounted into `#app-right-rail`.
 *
 * The selected item is derived from the *current* preview worksheet on every
 * render (never stored), so a row that disappears from a freshly returned
 * worksheet simply stops rendering.
 */
export function BuildInspector({ editor }: { editor: BuildWorksheetEditorModel }) {
  const {
    inspectorMode,
    closeInspector,
    selected,
    source,
    estimate,
    allFacilities,
    initialBuild,
    componentResolutions,
    fulfillmentScopes,
    linkedBuildsByTypeId,
    linkedBuildPending,
    linkedBuildErrors,
    adoptLinkedBuild,
    setPrices,
    setItemPricingPolicies,
    setComponentResolutions,
    setFulfillmentScopes,
    bumpPreview,
  } = editor;

  if (inspectorMode.kind === "closed" || !selected) return null;

  if (inspectorMode.kind === "buildSettings") {
    const root = buildRootInspector(editor, { onClose: closeInspector });
    return (
      <PlannerInspectorShell
        closeLabel="Close build settings"
        dismissLabel="Dismiss build settings"
        eyebrow="Planning assumptions"
        hideDefaultHeader
        onClose={closeInspector}
        open
        returnFocusSelector="[data-build-settings-trigger]"
        title="Build settings"
        width="wide"
      >
        <InspectorCollapseProvider>
          <UnifiedItemInspector actions={root.actions} model={root.model} />
        </InspectorCollapseProvider>
      </PlannerInspectorShell>
    );
  }

  // selectedItem -- derive the row from the live worksheet every render.
  const worksheet = estimate?.worksheet ?? null;
  const selectedItem = worksheet
    ? [...worksheet.groups.flatMap((group) => group.items), ...worksheet.output.items]
        .find((item) => rowKey(item) === inspectorMode.rowKey) ?? null
    : null;
  if (!worksheet || !selectedItem) return null;

  return (
    <PlannerContextPanel
      allFacilities={allFacilities}
      allowMarketPolicyOverride={source?.kind !== "manual"}
      blueprintSelections={Object.entries(componentResolutions).reduce<Record<number, BlueprintSelection>>(
        (acc, [typeId, resolution]) => {
          if (resolution.blueprintSelection) acc[Number(typeId)] = resolution.blueprintSelection;
          return acc;
        },
        {},
      )}
      buildId={initialBuild?.id}
      buildResolvedTypeIds={new Set(Object.keys(componentResolutions).map(Number))}
      facilityOverrides={Object.entries(componentResolutions).reduce<Record<number, ComponentFacilityOverride>>(
        (acc, [typeId, resolution]) => {
          if (resolution.facilityOverride) acc[Number(typeId)] = resolution.facilityOverride;
          return acc;
        },
        {},
      )}
      fulfillmentScopes={fulfillmentScopes}
      linkedBuildsByTypeId={linkedBuildsByTypeId}
      linkedBuildPending={linkedBuildPending}
      linkedBuildErrors={linkedBuildErrors}
      onBlueprintSelectionChange={(typeId, selection) => {
        applyComponentBlueprintSelection(typeId, selection, setComponentResolutions);
      }}
      onClose={closeInspector}
      onFacilityOverrideChange={(typeId, override) => {
        applyComponentFacilityOverride(typeId, override, setComponentResolutions);
      }}
      onFulfillmentScopeChange={(typeId, scope) => {
        applyFulfillmentScope(typeId, scope, setFulfillmentScopes);
      }}
      onLinkedBuildAdopt={adoptLinkedBuild}
      onLinkedBuildChanged={bumpPreview}
      onPricingChange={(selection) => {
        applyPricingSelection(selection, setPrices, setItemPricingPolicies);
      }}
      onResolutionChange={(typeId, recipeSelection) => {
        applyComponentResolution(typeId, recipeSelection, setComponentResolutions);
      }}
      selectedItem={selectedItem}
      worksheet={worksheet}
    />
  );
}
