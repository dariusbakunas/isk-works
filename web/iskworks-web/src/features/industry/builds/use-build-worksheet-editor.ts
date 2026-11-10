import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router";

import {
  getBlueprintAutomaticEiv,
  getReactionFormulaAutomaticEiv,
  listFacilities,
  listPriceSources,
  renameBuild,
  type Build,
  type DraftPlanningInput,
  type FulfillmentScope,
  type MarketPricingPolicy,
  type FacilityProfile,
  type ManufacturingFacilityCommand,
  type MarketScope,
  type ReactionFacilityCommand,
  type PriceSource,
} from "../../../api/industry";
import { getSdeStatus, type BuildPlan, type ReactionPlan } from "../../../api/sde";
import { apiMessage } from "../../../api/api-error";
import {
  buildPricingSelections,
  buildComponentResolutions,
  buildFulfillmentScopes,
  type ComponentResolutionState,
  parseRuns,
} from "./components/planner-panels";
import { useBuildPreviewSync } from "./editor/use-build-preview-sync";
import { useBuildSaveQueue } from "./editor/use-build-save-queue";
import { useLinkedBuilds } from "./editor/use-linked-builds";
import { useRecipeSelection } from "./editor/use-recipe-selection";
import { useFacilitySelection } from "./planner/use-facility-selection";

// Jita 4-4 -- the market scope a build starts from until the user picks
// otherwise via the Material Acquisition / Output Valuation scope pickers.
export const DEFAULT_SCOPE: MarketScope = { regionId: 10_000_002, locationId: 60_003_760 };

/**
 * The single Build-editor right-rail inspector state. Build Settings and the
 * Selected Item inspector are mutually exclusive modes of the same surface --
 * modelled as one discriminated union (not two independent booleans) so they
 * can never be mounted at once. Only the worksheet `rowKey` is stored; the
 * resolved `WorksheetItem` is derived from the current preview on every render
 * so a stale row can't survive a fresh worksheet.
 */
export type BuildInspectorMode =
  | { kind: "closed" }
  | { kind: "buildSettings" }
  | { kind: "selectedItem"; rowKey: string };

/**
 * The Build editor's model. It owns the planning inputs (name, runs,
 * pricing, scopes, facilities, sourcing) and composes four seams around
 * them: what the Build makes (`useRecipeSelection`), saving
 * (`useBuildSaveQueue`), the linked producer Builds (`useLinkedBuilds`) and
 * the worksheet preview (`useBuildPreviewSync`).
 */
export function useBuildWorksheetEditor(initialBuild: Build | null) {
  const navigate = useNavigate();
  const initialDraft = initialBuild?.draftPlanning?.input;
  const [sdeReady, setSdeReady] = useState<boolean | null>(null);
  const [sources, setSources] = useState<PriceSource[]>([]);
  const [sourceId, setSourceId] = useState(initialDraft?.manualPriceListId ?? "");
  const [name, setName] = useState(initialBuild?.name ?? "");
  const [runs, setRuns] = useState(String(initialBuild?.runs ?? 1));
  const [notes, setNotes] = useState(initialBuild?.notes ?? "");
  const [prices, setPrices] = useState<Record<number, string>>(() => Object.fromEntries(
    (initialDraft?.pricingSelections ?? [])
      .filter((item) => item.selection.kind === "manual")
      .map((item) => [item.typeId, item.selection.kind === "manual" ? item.selection.unit_price : ""]),
  ));
  // Bumped when a linked Build's own settings are patched out-of-band (the
  // unified inspector editing a linked Build by id). Folded into `previewKey`
  // so the candidate preview re-runs and the worksheet reflects the change.
  const [previewNonce, setPreviewNonce] = useState(0);
  const bumpPreview = useCallback(() => setPreviewNonce((value) => value + 1), []);
  const [inspectorMode, setInspectorMode] = useState<BuildInspectorMode>({ kind: "closed" });
  function openBuildSettings() {
    setInspectorMode({ kind: "buildSettings" });
  }
  function selectWorksheetRow(rowKey: string | null) {
    setInspectorMode(rowKey ? { kind: "selectedItem", rowKey } : { kind: "closed" });
  }
  function closeInspector() {
    setInspectorMode({ kind: "closed" });
  }
  const [materialScope, setMaterialScope] = useState<MarketScope>(initialDraft?.materialScope ?? DEFAULT_SCOPE);
  const [outputScope, setOutputScope] = useState<MarketScope>(initialDraft?.outputScope ?? DEFAULT_SCOPE);
  const [materialPricingPolicy, setMaterialPricingPolicy] = useState<MarketPricingPolicy>(initialDraft?.materialPricingPolicy ?? "highestBuy");
  const [outputPricingPolicy, setOutputPricingPolicy] = useState<MarketPricingPolicy>(initialDraft?.outputPricingPolicy ?? "lowestSell");
  const [itemPricingPolicies, setItemPricingPolicies] = useState<Record<number, MarketPricingPolicy>>(() => Object.fromEntries(
    (initialDraft?.pricingSelections ?? [])
      .filter((item) => item.selection.kind === "market_policy")
      .map((item) => [item.typeId, item.selection.kind === "market_policy" ? item.selection.policy : materialPricingPolicy]),
  ));
  const [componentResolutions, setComponentResolutions] = useState<Record<number, ComponentResolutionState>>(() => Object.fromEntries(
    (initialDraft?.componentResolutions ?? []).map((item) => [item.typeId, { recipe: item.recipe, facilityOverride: item.facilityOverride }]),
  ));
  const [fulfillmentScopes, setFulfillmentScopes] = useState<Record<number, FulfillmentScope>>(() => Object.fromEntries(
    (initialDraft?.fulfillmentScopes ?? []).map((item) => [item.typeId, item.scope]),
  ));
  const [error, setError] = useState("");
  const [allFacilities, setAllFacilities] = useState<FacilityProfile[]>([]);

  const selection = useRecipeSelection({
    initialBuild,
    sdeReady,
    runs,
    name,
    setName,
    setError,
    onProductSelected: () => {
      // Read at selection time, after this render has defined them.
      preview.resetPreview();
      setItemPricingPolicies({});
      manufacturing.reset();
      reaction.reset();
    },
  });
  const { selected, recipe, initializingExistingBuild, createBlueprintSelection } = selection;

  const parsedRuns = parseRuns(runs);
  const rootKind = selected?.kind;
  const manufacturing = useFacilitySelection({
    allFacilities,
    role: "manufacturing",
    runs: parsedRuns,
    enabled: Boolean(selected),
    fetchAutomaticEiv: selected?.kind === "manufacturing"
      ? (eivRuns) => getBlueprintAutomaticEiv(selected.result.blueprintTypeId, eivRuns)
      : undefined,
    initialFacilityProfileId: initialDraft?.manufacturingFacility?.facilityProfileId ?? "",
    initialEstimatedItemValue: initialDraft?.manufacturingFacility?.estimatedItemValue ?? "",
    initialManualEiv: initialDraft?.facilityEivManual ?? false,
  });
  const reaction = useFacilitySelection({
    allFacilities,
    role: "reaction",
    runs: parsedRuns,
    enabled: Boolean(selected),
    fetchAutomaticEiv: selected?.kind === "reaction"
      ? (eivRuns) => getReactionFormulaAutomaticEiv(selected.result.reactionFormulaTypeId, eivRuns)
      : undefined,
    initialFacilityProfileId: initialDraft?.reactionFacility?.facilityProfileId ?? "",
    initialEstimatedItemValue: initialDraft?.reactionFacility?.estimatedItemValue ?? "",
    initialManualEiv: initialDraft?.facilityEivManual ?? false,
  });
  const rootFacilitySelection = rootKind === "manufacturing" ? manufacturing : reaction;

  function facilityCommands(): {
    manufacturingFacility: ManufacturingFacilityCommand | null;
    reactionFacility: ReactionFacilityCommand | null;
  } {
    return {
      manufacturingFacility: manufacturing.selectedFacility ? {
        facilityProfileId: manufacturing.selectedFacility.id,
        blueprintMe: rootKind === "manufacturing" ? Number(selection.blueprintMe) : 0,
        blueprintTe: rootKind === "manufacturing" ? Number(selection.blueprintTe) : 0,
        estimatedItemValue: manufacturing.estimatedItemValue || null,
      } : null,
      reactionFacility: reaction.selectedFacility ? {
        facilityProfileId: reaction.selectedFacility.id,
        estimatedItemValue: reaction.estimatedItemValue || null,
      } : null,
    };
  }

  // The fields shared by every request that describes "the current recipe
  // planning state" -- save()'s draftPlanning, autosave's snapshot, and the
  // preview cache key each need this same computation, just embedded in a
  // differently-shaped envelope (see buildDraftPlanning() and previewKey
  // below). Centralized so those envelopes can't drift out of sync with
  // each other by only updating one of them when a field is added here.
  function buildSharedPlanningFields(currentRecipe: BuildPlan | ReactionPlan) {
    return {
      pricingSelections: buildPricingSelections(
        currentRecipe, prices, itemPricingPolicies, materialPricingPolicy, outputPricingPolicy,
      ),
      blueprintSelection: selected?.kind === "manufacturing" ? createBlueprintSelection() : null,
      ...facilityCommands(),
      componentResolutions: buildComponentResolutions(componentResolutions),
      fulfillmentScopes: buildFulfillmentScopes(fulfillmentScopes),
    };
  }

  // save()'s draftPlanning and autosave's snapshot both persist this exact
  // shape -- one builder so the two can't silently drift apart when a field
  // is added to one but not the other.
  function buildDraftPlanning(currentRecipe: BuildPlan | ReactionPlan): DraftPlanningInput {
    return {
      materialScope,
      outputScope,
      manualPriceListId: source?.id ?? null,
      expectedManualPriceListRevision: source?.revision ?? null,
      materialPricingPolicy,
      outputPricingPolicy,
      facilityEivManual: rootFacilitySelection.manualEiv,
      ...buildSharedPlanningFields(currentRecipe),
    };
  }

  useEffect(() => {
    Promise.all([getSdeStatus(), listPriceSources(), listFacilities()])
      .then(([sde, loadedSources, loadedFacilities]) => {
        setSdeReady(Boolean(sde.active));
        // Manual price lists ("Price Overrides") only -- market scope is
        // the primary pricing source now (set via the Material Acquisition
        // / Output Valuation pickers), so a Price Override is purely an
        // optional fallback for items market can't price at all; an unset
        // selection is a normal, common state, not something to default
        // away from.
        setSources(loadedSources.filter((loadedSource) => loadedSource.kind === "manual"));
        setAllFacilities(loadedFacilities);
      })
      .catch((requestError) => setError(apiMessage(requestError)));
  }, []);

  const source = sources.find((item) => item.id === sourceId);

  const recipeRequest = selected?.kind === "manufacturing"
    ? { mode: "manufacturing" as const, blueprintTypeId: selected.result.blueprintTypeId }
    : selected
      ? { mode: "reaction" as const, reactionFormulaTypeId: selected.result.reactionFormulaTypeId }
      : null;

  // What autosave compares: "" until the draft is complete enough to save.
  const autosaveSnapshot = (() => {
    if (!(selected && recipe && parsedRuns && name.trim())) return "";
    try {
      return JSON.stringify({
        name,
        runs: parsedRuns,
        notes,
        selected,
        draftPlanning: buildDraftPlanning(recipe),
      });
    } catch {
      // Blueprint mode is "observedAsset" but no observation is selected yet
      // (e.g. the user just toggled the mode without picking one) -- nothing
      // to autosave until that's resolved, same as previewKey below.
      return "";
    }
  })();
  const saving = useBuildSaveQueue({
    initialBuild,
    complete: Boolean(recipeRequest && recipe && parsedRuns && name.trim()),
    draftInput: () => ({
      name,
      recipe: recipeRequest!,
      runs: parsedRuns!,
      notes,
      draftPlanning: buildDraftPlanning(recipe!),
    }),
    snapshot: autosaveSnapshot,
    onCreated: (draft) => navigate(`/builds/${draft.id}`, { replace: true }),
    setError,
  });

  const linked = useLinkedBuilds({
    buildId: initialBuild?.id,
    initializing: initializingExistingBuild,
    componentResolutions,
    queueSaveIfChanged: saving.queueSaveIfChanged,
  });

  const previewKey = (() => {
    if (!recipeRequest || !recipe || !parsedRuns) return "";
    try {
      return JSON.stringify({
        recipe: recipeRequest,
        runs: parsedRuns,
        materialScope,
        outputScope,
        manualPriceListId: source?.id ?? null,
        expectedManualPriceListRevision: source?.revision ?? null,
        ...buildSharedPlanningFields(recipe),
        buildId: initialBuild?.id,
        previewNonce,
      });
    } catch {
      return "";
    }
  })();
  const preview = useBuildPreviewSync(previewKey, setError);

  return {
    initialBuild,
    sdeReady,
    query: selection.query,
    setQuery: selection.setQuery,
    selected,
    recipe,
    recipeStatus: selection.recipeStatus,
    initializingExistingBuild,
    sources,
    sourceId,
    setSourceId,
    source,
    name,
    setName,
    runs,
    setRuns,
    notes,
    setNotes,
    prices,
    setPrices,
    estimate: preview.estimate,
    previewUpdating: preview.previewUpdating,
    previewPending: preview.previewPending,
    previewSyncMessage: preview.previewSyncMessage,
    previewKey,
    productResults: selection.productResults,
    productSearching: selection.productSearching,
    hasRouteBlueprint: selection.hasRouteBlueprint,
    selectFromDialog: selection.selectFromDialog,
    selectedName: selection.selectedName,
    blueprintDialogOpen: selection.blueprintDialogOpen,
    setBlueprintDialogOpen: selection.setBlueprintDialogOpen,
    inspectorMode,
    openBuildSettings,
    selectWorksheetRow,
    closeInspector,
    materialScope,
    setMaterialScope,
    outputScope,
    setOutputScope,
    materialPricingPolicy,
    setMaterialPricingPolicy,
    outputPricingPolicy,
    setOutputPricingPolicy,
    itemPricingPolicies,
    setItemPricingPolicies,
    componentResolutions,
    setComponentResolutions,
    fulfillmentScopes,
    setFulfillmentScopes,
    linkedBuildsByTypeId: linked.linkedBuildsByTypeId,
    linkedBuildPending: linked.linkedBuildPending,
    linkedBuildsSettling: linked.linkedBuildsSettling,
    linkedBuildErrors: linked.linkedBuildErrors,
    adoptLinkedBuild: linked.adoptLinkedBuild,
    blueprintMode: selection.blueprintMode,
    setBlueprintMode: selection.setBlueprintMode,
    blueprintKind: selection.blueprintKind,
    setBlueprintKind: selection.setBlueprintKind,
    blueprintMe: selection.blueprintMe,
    setBlueprintMe: selection.setBlueprintMe,
    blueprintTe: selection.blueprintTe,
    setBlueprintTe: selection.setBlueprintTe,
    licensedRuns: selection.licensedRuns,
    setLicensedRuns: selection.setLicensedRuns,
    blueprintNotes: selection.blueprintNotes,
    setBlueprintNotes: selection.setBlueprintNotes,
    observedBlueprints: selection.observedBlueprints,
    selectedObservationId: selection.selectedObservationId,
    setSelectedObservationId: selection.setSelectedObservationId,
    saveStatus: saving.saveStatus,
    setError,
    error,
    allFacilities,
    manufacturing,
    reaction,
    rootFacilitySelection,
    buildLinkedComponent: linked.buildLinkedComponent,
    queueSave: saving.queueSave,
    bumpPreview,
    navigate,
    renameBuild,
    setBuildRevision: saving.setBuildRevision,
  };
}

export type BuildWorksheetEditorModel = ReturnType<typeof useBuildWorksheetEditor>;
