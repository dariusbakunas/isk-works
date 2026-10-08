import { useEffect, useState, type ReactNode } from "react";

import {
  listBlueprintObservations,
  type BlueprintObservation,
  type BlueprintSelection,
  type Build,
  type ComponentFacilityOverride,
  type FacilityProfile,
  type FulfillmentScope,
  type PlannerPricingSelection,
  type ProductionWorksheet,
  type RecipeSelection,
  type WorksheetItem,
} from "../../../../api/industry";
import { recipeForProduct } from "../../../../api/sde";
import { formatIskSummary } from "../../../../components/money";
import { buildWorksheetInspector } from "../../inspector/adapters/worksheet-item";
import { InspectorCollapseProvider } from "../../inspector/inspector-collapse";
import { UnifiedItemInspector } from "../../inspector/unified-item-inspector";
import { useBuildSettings } from "../use-build-settings";
import { PlannerInspectorShell } from "../../../../components/planner-inspector-shell";
import { rowKey } from "./production-worksheet";

export interface PlannerPricingContext {
  sourceName: string;
  sourceRevision: number;
  capturedAt: string;
}

export function PlannerContextPanel({
  allFacilities,
  allowMarketPolicyOverride = false,
  blueprintSelections,
  buildId,
  buildResolvedTypeIds,
  calculationEvidence,
  facilityOverrides,
  fulfillmentScopes,
  linkedBuildsByTypeId,
  linkedBuildPending,
  linkedBuildErrors,
  onLinkedBuildChanged,
  onLinkedBuildAdopt,
  pricingContext,
  readOnly = false,
  selectedItem,
  worksheet,
  onBlueprintSelectionChange,
  onClose = () => {},
  onFacilityOverrideChange,
  onFulfillmentScopeChange,
  onPricingChange,
  onResolutionChange,
}: {
  allFacilities?: FacilityProfile[];
  allowMarketPolicyOverride?: boolean;
  blueprintSelections?: Record<number, BlueprintSelection>;
  /** The persisted parent build's id -- enables linked-build navigation. */
  buildId?: string;
  buildResolvedTypeIds?: Set<number>;
  calculationEvidence?: ReactNode;
  facilityOverrides?: Record<number, ComponentFacilityOverride>;
  fulfillmentScopes?: Record<number, FulfillmentScope>;
  linkedBuildsByTypeId?: Record<number, Build>;
  linkedBuildPending?: Record<number, boolean>;
  linkedBuildErrors?: Record<number, string>;
  /** Re-run the worksheet preview after a linked Build was patched by id. */
  onLinkedBuildChanged?: () => void;
  /** Adopt the freshest copy of a linked Build (loaded/patched by id here)
   * back into the editor's linked-build map, so the Graph inspector sees it. */
  onLinkedBuildAdopt?: (build: Build) => void;
  pricingContext?: PlannerPricingContext | null;
  readOnly?: boolean;
  selectedItem: WorksheetItem | null;
  worksheet: ProductionWorksheet;
  onBlueprintSelectionChange?: (typeId: number, selection: BlueprintSelection | null) => void;
  onClose?: () => void;
  onFacilityOverrideChange?: (typeId: number, override: ComponentFacilityOverride | null) => void;
  onFulfillmentScopeChange?: (typeId: number, scope: FulfillmentScope | null) => void;
  onPricingChange: (selection: PlannerPricingSelection) => void;
  onResolutionChange?: (typeId: number, recipe: RecipeSelection | null) => void;
}) {
  if (!selectedItem) return <PlanSummary worksheet={worksheet} />;
  return (
    <PlannerInspectorShell
      hideDefaultHeader
      onClose={onClose}
      open
      returnFocusRowKey={rowKey(selectedItem)}
      title={selectedItem.typeName}
    >
      <InspectorCollapseProvider>
        <WorksheetItemInspector
          onClose={onClose}
          allFacilities={allFacilities ?? []}
          allowMarketPolicyOverride={allowMarketPolicyOverride}
          blueprintSelection={blueprintSelections?.[selectedItem.typeId]}
          buildResolved={buildResolvedTypeIds?.has(selectedItem.typeId) ?? false}
          calculationEvidence={calculationEvidence}
          facilityOverride={facilityOverrides?.[selectedItem.typeId]}
          fulfillmentScope={fulfillmentScopes?.[selectedItem.typeId]}
          item={selectedItem}
          linkedBuild={linkedBuildsByTypeId?.[selectedItem.typeId] ?? null}
          creatingLinkedBuild={linkedBuildPending?.[selectedItem.typeId] ?? false}
          linkedBuildError={linkedBuildErrors?.[selectedItem.typeId] ?? ""}
          onBlueprintSelectionChange={onBlueprintSelectionChange}
          onFacilityOverrideChange={onFacilityOverrideChange}
          onFulfillmentScopeChange={onFulfillmentScopeChange}
          onLinkedBuildAdopt={onLinkedBuildAdopt}
          onLinkedBuildChanged={onLinkedBuildChanged}
          onPricingChange={onPricingChange}
          onResolutionChange={onResolutionChange}
          parentBuildId={buildId}
          pricingContext={pricingContext ?? null}
          readOnly={readOnly}
          rootTypeId={worksheet.output.items[0]?.typeId ?? null}
        />
      </InspectorCollapseProvider>
    </PlannerInspectorShell>
  );
}

function WorksheetItemInspector({
  item,
  buildResolved,
  linkedBuild,
  creatingLinkedBuild,
  linkedBuildError,
  blueprintSelection,
  facilityOverride,
  fulfillmentScope,
  allFacilities,
  parentBuildId,
  rootTypeId,
  pricingContext,
  readOnly,
  allowMarketPolicyOverride,
  calculationEvidence,
  onClose,
  onLinkedBuildChanged,
  onLinkedBuildAdopt,
  onPricingChange,
  onResolutionChange,
  onBlueprintSelectionChange,
  onFacilityOverrideChange,
  onFulfillmentScopeChange,
}: {
  item: WorksheetItem;
  buildResolved: boolean;
  linkedBuild: Build | null;
  creatingLinkedBuild: boolean;
  linkedBuildError: string;
  blueprintSelection?: BlueprintSelection;
  facilityOverride?: ComponentFacilityOverride;
  fulfillmentScope?: FulfillmentScope;
  allFacilities: FacilityProfile[];
  parentBuildId?: string;
  rootTypeId: number | null;
  pricingContext: PlannerPricingContext | null;
  readOnly: boolean;
  allowMarketPolicyOverride: boolean;
  calculationEvidence?: ReactNode;
  onClose: () => void;
  onLinkedBuildChanged?: () => void;
  onLinkedBuildAdopt?: (build: Build) => void;
  onPricingChange: (selection: PlannerPricingSelection) => void;
  onResolutionChange?: (typeId: number, recipe: RecipeSelection | null) => void;
  onBlueprintSelectionChange?: (typeId: number, selection: BlueprintSelection | null) => void;
  onFacilityOverrideChange?: (typeId: number, override: ComponentFacilityOverride | null) => void;
  onFulfillmentScopeChange?: (typeId: number, scope: FulfillmentScope | null) => void;
}) {
  const interactive = !readOnly && Boolean(onResolutionChange);

  // The row's own producible recipe (does a "Build" option apply?).
  const [recipe, setRecipe] = useState<RecipeSelection | null | undefined>(undefined);
  useEffect(() => {
    if (!interactive || item.role !== "material") {
      setRecipe(null);
      return;
    }
    let cancelled = false;
    setRecipe(undefined);
    recipeForProduct(item.typeId)
      .then((result) => !cancelled && setRecipe(result))
      .catch(() => !cancelled && setRecipe(null));
    return () => {
      cancelled = true;
    };
  }, [interactive, item.typeId, item.role]);

  // Owned blueprint instances for a build-resolved manufacturing row.
  const blueprintTypeId =
    linkedBuild?.recipe.kind === "manufacturing"
      ? linkedBuild.recipe.blueprintTypeId
      : recipe && recipe.mode === "manufacturing"
        ? recipe.blueprintTypeId
        : null;
  const [observations, setObservations] = useState<BlueprintObservation[]>([]);
  useEffect(() => {
    if (!interactive || blueprintTypeId == null) {
      setObservations([]);
      return;
    }
    let cancelled = false;
    Promise.resolve(listBlueprintObservations(blueprintTypeId))
      .then((rows) => !cancelled && setObservations(rows ?? []))
      .catch(() => !cancelled && setObservations([]));
    return () => {
      cancelled = true;
    };
  }, [interactive, blueprintTypeId]);

  // A linked Build's own blueprint / facility are edited by id (same path as
  // the Graph) and the worksheet preview is bumped afterwards.
  const linkedSettings = useBuildSettings(
    linkedBuild?.id ?? null,
    onLinkedBuildChanged,
    linkedBuild,
  );

  // Push the freshest copy of this linked Build (after a patch here) back
  // into the editor's map, so the Graph inspector reflects the same edit.
  useEffect(() => {
    if (linkedSettings.build) onLinkedBuildAdopt?.(linkedSettings.build);
  }, [linkedSettings.build, onLinkedBuildAdopt]);

  const { model, actions } = buildWorksheetInspector({
    item,
    buildResolved,
    linkedBuild,
    creatingLinkedBuild,
    linkedBuildError,
    blueprintSelection,
    facilityOverride,
    fulfillmentScope,
    allFacilities,
    parentBuildId,
    rootTypeId,
    recipe,
    observations,
    pricingContext,
    readOnly,
    allowMarketPolicyOverride,
    calculationEvidence,
    linkedSettings: linkedBuild ? linkedSettings : null,
    handlers: {
      onPricingChange,
      onResolutionChange,
      onBlueprintSelectionChange,
      onFacilityOverrideChange,
      onFulfillmentScopeChange,
      onOpenLinkedBuild: (id) => {
        window.location.assign(`/builds/${id}`);
      },
      onCopyBuildId: (id) => void navigator.clipboard?.writeText(id),
    },
  });

  return <UnifiedItemInspector actions={{ ...actions, onClose }} model={model} />;
}

function PlanSummary({ worksheet }: { worksheet: ProductionWorksheet }) {
  const summary = worksheet.summary;
  return (
    <aside aria-labelledby="plan-summary-title" className="border-l-2 border-primary bg-panel px-3 py-3">
      <p className="iw-eyebrow">Current preview</p>
      <h2 className="text-sm font-semibold" id="plan-summary-title">
        Plan Summary
      </h2>
      <dl className="mt-3 space-y-2 text-sm">
        <SummaryLine label="Materials" value={formatIskSummary(summary.materialCost)} />
        <SummaryLine
          label="Installation"
          value={summary.installationCost ? formatIskSummary(summary.installationCost) : "Not configured"}
        />
        <SummaryLine
          label="Revenue"
          value={summary.expectedRevenue ? formatIskSummary(summary.expectedRevenue) : "Incomplete"}
        />
        <SummaryLine
          label="Margin"
          value={summary.estimatedMargin ? formatIskSummary(summary.estimatedMargin) : "Incomplete"}
        />
      </dl>
    </aside>
  );
}

function SummaryLine({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-start justify-between gap-3">
      <dt className="text-muted">{label}</dt>
      <dd className="text-right font-mono">{value}</dd>
    </div>
  );
}
