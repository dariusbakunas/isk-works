import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { useNavigate, useSearchParams } from "react-router";

import {
  createBuild,
  createLinkedBuild,
  getBlueprintAutomaticEiv,
  getReactionFormulaAutomaticEiv,
  listBlueprintObservations,
  listFacilities,
  listPriceSources,
  previewCreateBuildCandidate,
  renameBuild,
  updateBuild,
  type Build,
  type BlueprintObservation,
  type BlueprintSelection,
  type DraftPlanningInput,
  type FulfillmentScope,
  type CreateBuildPlanPreview,
  type MarketPricingPolicy,
  type FacilityProfile,
  type ManufacturingFacilityCommand,
  type MarketScope,
  type ReactionFacilityCommand,
  type PriceSource,
} from "../../../api/industry";
import { selectAvailableBlueprint } from "./planner/available-blueprint";
import {
  getSdeStatus,
  planBuild,
  planReaction,
  searchBlueprints,
  searchReactionFormulas,
  type BlueprintSearchResult,
  type BuildPlan,
  type ReactionFormulaSearchResult,
  type ReactionPlan,
} from "../../../api/sde";
import { apiMessage } from "../shared/api-error";
import { useDebouncedLookup } from "../../../hooks/use-debounced-lookup";
import { automaticPricingRetryDelay, MAX_MARKET_SYNC_RETRIES } from "./planner/preview-retry";
import { useFacilitySelection } from "./planner/use-facility-selection";


// Jita 4-4 -- the market scope a build starts from until the user picks
// otherwise via the Material Acquisition / Output Valuation scope pickers.
export const DEFAULT_SCOPE: MarketScope = { regionId: 10_000_002, locationId: 60_003_760 };

type RecipeStatus = "idle" | "loading" | "ready" | "error";

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

import type { ProductSearchResult } from "./builds-page";
import {
  buildPricingSelections,
  buildComponentResolutions,
  buildFulfillmentScopes,
  type ComponentResolutionState,
  parseRuns,
  createManualBlueprintSelection,
} from "./components/planner-panels";

type SelectedProduct =
  | { kind: "manufacturing"; result: BlueprintSearchResult }
  | { kind: "reaction"; result: ReactionFormulaSearchResult };

export function useBuildWorksheetEditor(initialBuild: Build | null) {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const initialDraft = initialBuild?.draftPlanning?.input;
  const initialBlueprint = initialDraft?.blueprintSelection;
  const initialProduct = initialBuild?.recipe.products[0];
  const [sdeReady, setSdeReady] = useState<boolean | null>(null);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<SelectedProduct | null>(() => {
    if (!initialBuild || !initialProduct) return null;
    return initialBuild.recipe.kind === "manufacturing" ? {
      kind: "manufacturing",
      result: {
        blueprintTypeId: initialBuild.recipe.blueprintTypeId,
        blueprintName: initialBuild.recipe.blueprintName,
        productTypeId: initialProduct.typeId,
        productName: initialProduct.typeName,
        groupName: null,
        published: true,
        manufacturingAvailable: true,
      },
    } : {
      kind: "reaction",
      result: {
        reactionFormulaTypeId: initialBuild.recipe.reactionFormulaTypeId,
        reactionFormulaName: initialBuild.recipe.reactionFormulaName,
        productTypeId: initialProduct.typeId,
        productName: initialProduct.typeName,
        groupName: null,
        published: true,
      },
    };
  });
  const [recipe, setRecipe] = useState<BuildPlan | ReactionPlan | null>(null);
  const [recipeStatus, setRecipeStatus] = useState<RecipeStatus>(
    initialBuild && selected ? "loading" : "idle",
  );
  const initializingExistingBuild = Boolean(initialBuild && recipeStatus === "loading");
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
  const [estimate, setEstimate] = useState<CreateBuildPlanPreview | null>(null);
  const [previewUpdating, setPreviewUpdating] = useState(false);
  const [previewRetryAttempt, setPreviewRetryAttempt] = useState(0);
  const [previewRetryToken, setPreviewRetryToken] = useState(0);
  const [appliedPreviewKey, setAppliedPreviewKey] = useState("");
  const previewRetry = useRef({ key: "", attempts: 0 });
  const previewSequence = useRef(0);
  const recipeSequence = useRef(0);
  const routeBlueprintLoaded = useRef(false);
  const [blueprintDialogOpen, setBlueprintDialogOpen] = useState(false);
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
  const [blueprintMode, setBlueprintMode] = useState<"manual" | "observedAsset">(initialBlueprint?.mode ?? "manual");
  const [blueprintKind, setBlueprintKind] = useState<"original" | "copy">(initialBlueprint?.mode === "manual" ? initialBlueprint.kind : "original");
  const [blueprintMe, setBlueprintMe] = useState(String(initialBlueprint?.mode === "manual" ? initialBlueprint.materialEfficiency : 0));
  const [blueprintTe, setBlueprintTe] = useState(String(initialBlueprint?.mode === "manual" ? initialBlueprint.timeEfficiency : 0));
  const [licensedRuns, setLicensedRuns] = useState(initialBlueprint?.mode === "manual" && initialBlueprint.licensedRuns !== null ? String(initialBlueprint.licensedRuns) : "");
  const [blueprintNotes, setBlueprintNotes] = useState(initialBlueprint?.mode === "manual" ? initialBlueprint.notes : "");
  const [observedBlueprints, setObservedBlueprints] = useState<BlueprintObservation[]>([]);
  const [selectedObservationId, setSelectedObservationId] = useState(initialBlueprint?.mode === "observedAsset" ? initialBlueprint.observationId : "");
  const [buildRevision, setBuildRevisionState] = useState(initialBuild?.revision ?? null);
  // Mirrors `buildRevision` for a queued save that runs before the next
  // render: it must send the latest revision.
  const buildRevisionRef = useRef<number | null>(initialBuild?.revision ?? null);
  const setBuildRevision = useCallback((revision: number | null) => {
    buildRevisionRef.current = revision;
    setBuildRevisionState(revision);
  }, []);
  const [, setBusy] = useState(false);
  const [saveStatus, setSaveStatus] = useState<"idle" | "saving" | "saved">("idle");
  const [error, setError] = useState("");
  const [allFacilities, setAllFacilities] = useState<FacilityProfile[]>([]);
  const parsedRunsForEiv = parseRuns(runs);
  const rootKind = selected?.kind;
  const manufacturing = useFacilitySelection({
    allFacilities,
    role: "manufacturing",
    runs: parsedRunsForEiv,
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
    runs: parsedRunsForEiv,
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
        blueprintMe: rootKind === "manufacturing" ? Number(blueprintMe) : 0,
        blueprintTe: rootKind === "manufacturing" ? Number(blueprintTe) : 0,
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
  const blueprintLookup = useDebouncedLookup<BlueprintSearchResult>(
    query,
    searchBlueprints,
    (requestError) => setError(apiMessage(requestError)),
  );
  const reactionFormulaLookup = useDebouncedLookup<ReactionFormulaSearchResult>(
    query,
    searchReactionFormulas,
    (requestError) => setError(apiMessage(requestError)),
  );
  const productResults: ProductSearchResult[] = [
    ...blueprintLookup.results.map((result) => ({ kind: "manufacturing" as const, result })),
    ...reactionFormulaLookup.results.map((result) => ({ kind: "reaction" as const, result })),
  ].sort((a, b) => a.result.productName.localeCompare(b.result.productName));

  useEffect(() => {
    if (initialBuild || !sdeReady || routeBlueprintLoaded.current) return;
    routeBlueprintLoaded.current = true;
    const blueprintTypeId = Number(searchParams.get("blueprintTypeId"));
    if (!Number.isSafeInteger(blueprintTypeId) || blueprintTypeId <= 0) return;

    planBuild(blueprintTypeId, 1)
      .then((loadedRecipe) => {
        const product = loadedRecipe.products[0];
        if (!product) {
          setSearchParams({}, { replace: true });
          return;
        }
        return selectProduct("manufacturing", {
          blueprintTypeId: loadedRecipe.blueprintTypeId,
          blueprintName: loadedRecipe.blueprintName,
          productTypeId: product.typeId,
          productName: product.typeName,
          groupName: null,
          published: true,
          manufacturingAvailable: true,
        });
      })
      .catch(() => setSearchParams({}, { replace: true }));
  }, [initialBuild, sdeReady, searchParams, setSearchParams]);

  useEffect(() => {
    if (!initialBuild || !selected || selected.kind !== "manufacturing") return;
    listBlueprintObservations(selected.result.blueprintTypeId)
      .then(setObservedBlueprints)
      .catch((requestError) => setError(apiMessage(requestError)));
  }, [initialBuild, selected]);

  async function selectProduct(
    kind: "manufacturing",
    result: BlueprintSearchResult,
  ): Promise<void>;
  async function selectProduct(
    kind: "reaction",
    result: ReactionFormulaSearchResult,
  ): Promise<void>;
  async function selectProduct(
    kind: "manufacturing" | "reaction",
    result: BlueprintSearchResult | ReactionFormulaSearchResult,
  ) {
    const parsedRuns = parseRuns(runs);
    if (!parsedRuns) {
      setError("Runs must be a whole number between 1 and 1,000,000.");
      return;
    }
    setBusy(true);
    setError("");
    setSelected({ kind, result } as SelectedProduct);
    setEstimate(null);
    setAppliedPreviewKey("");
    setItemPricingPolicies({});
    manufacturing.reset();
    reaction.reset();
    setBlueprintMode("manual");
    setBlueprintKind("original");
    setBlueprintMe("0");
    setBlueprintTe("0");
    setLicensedRuns("");
    setBlueprintNotes("");
    setObservedBlueprints([]);
    setSelectedObservationId("");
    if (!name) setName(`${result.productName} build`);
    if (kind === "reaction") {
      setBusy(false);
      return;
    }
    try {
      const observations = await listBlueprintObservations((result as BlueprintSearchResult).blueprintTypeId);
      const availableBlueprint = selectAvailableBlueprint(observations, parsedRuns);
      setObservedBlueprints(observations);
      setSelectedObservationId(availableBlueprint?.id ?? "");
      setBlueprintMode(availableBlueprint ? "observedAsset" : "manual");
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  function selectFromDialog(entry: ProductSearchResult) {
    if (entry.kind === "manufacturing") {
      setSearchParams({ blueprintTypeId: String(entry.result.blueprintTypeId) }, { replace: true });
      void selectProduct("manufacturing", entry.result);
    } else {
      void selectProduct("reaction", entry.result);
    }
  }

  useEffect(() => {
    const parsedRuns = parseRuns(runs);
    if (!selected || !parsedRuns) {
      setRecipe(null);
      setRecipeStatus("idle");
      return;
    }
    setRecipeStatus("loading");
    const sequence = ++recipeSequence.current;
    const timer = window.setTimeout(() => {
      const loadRecipe = selected.kind === "manufacturing"
        ? planBuild(selected.result.blueprintTypeId, parsedRuns)
        : planReaction(selected.result.reactionFormulaTypeId, parsedRuns);
      void loadRecipe
        .then((loadedRecipe) => {
          if (sequence === recipeSequence.current) {
            setRecipe(loadedRecipe);
            setRecipeStatus("ready");
          }
        })
        .catch((requestError) => {
          if (sequence === recipeSequence.current) {
            setRecipe(null);
            setRecipeStatus("error");
            setError(apiMessage(requestError));
          }
        });
    }, 120);
    return () => window.clearTimeout(timer);
  }, [runs, selected]);

  useEffect(() => {
    if (blueprintMode !== "observedAsset") {
      return;
    }

    const parsedRuns = parseRuns(runs);
    if (!parsedRuns || observedBlueprints.length === 0) {
      return;
    }

    const availableBlueprint = selectAvailableBlueprint(
      observedBlueprints,
      parsedRuns,
      selectedObservationId,
    );
    if (availableBlueprint) {
      if (selectedObservationId !== availableBlueprint.id) {
        setSelectedObservationId(availableBlueprint.id);
      }
      return;
    }

    setSelectedObservationId("");
    setBlueprintMode("manual");
  }, [blueprintMode, observedBlueprints, runs, selectedObservationId]);

  async function save(): Promise<Build | null> {
    const parsedRuns = parseRuns(runs);
    if (!selected || !recipe || !parsedRuns || !name.trim()) {
      setError("Name, blueprint, and valid runs are required.");
      return null;
    }
    setSaveStatus("saving");
    setError("");
    let draft: Build | null = null;
    try {
      const draftPlanning: DraftPlanningInput = buildDraftPlanning(recipe);
      const draftInput = {
        name,
        recipe: selected.kind === "manufacturing"
          ? {
              mode: "manufacturing" as const,
              blueprintTypeId: selected.result.blueprintTypeId,
            }
          : {
              mode: "reaction" as const,
              reactionFormulaTypeId: selected.result.reactionFormulaTypeId,
            },
        runs: parsedRuns,
        notes,
        draftPlanning,
      };
      const expectedRevision = buildRevisionRef.current ?? buildRevision;
      draft = initialBuild && expectedRevision !== null
        ? await updateBuild(initialBuild.id, {
            ...draftInput,
            expectedRevision,
          })
        : await createBuild(draftInput);
      setBuildRevision(draft.revision);
      setSaveStatus("saved");
      if (!initialBuild) {
        navigate(`/builds/${draft.id}`, { replace: true });
      }
    } catch (requestError) {
      setSaveStatus("idle");
      setError(
        draft
          ? `Build saved, but commit did not finish. ${apiMessage(requestError)}`
          : apiMessage(requestError),
      );
    }
    return draft;
  }

  // Serializes every save request (autosave, "Build this component") through
  // one queue, always invoking the freshest `save` closure -- so a save
  // requested while another is still in flight waits its turn instead of
  // racing it with a stale `expectedRevision`.
  const saveRef = useRef(save);
  saveRef.current = save;
  // The draft as last persisted (see `autosaveSnapshot` below), and the one
  // the latest render would save. Equal means a save would only bump the
  // revision.
  const lastSavedSnapshotRef = useRef("");
  const currentSnapshotRef = useRef("");
  const buildIdRef = useRef(initialBuild?.id);
  buildIdRef.current = initialBuild?.id;
  const saveQueueRef = useRef<Promise<unknown>>(Promise.resolve());
  function enqueue<T>(task: () => Promise<T>): Promise<T> {
    const queued = saveQueueRef.current.catch(() => null).then(task);
    saveQueueRef.current = queued;
    return queued;
  }
  async function saveRecordingSnapshot(): Promise<Build | null> {
    const snapshot = currentSnapshotRef.current;
    const draft = await saveRef.current();
    if (draft) lastSavedSnapshotRef.current = snapshot;
    return draft;
  }
  function queueSave(): Promise<Build | null> {
    return enqueue(saveRecordingSnapshot);
  }
  // Like `queueSave`, but an existing build with nothing unsaved is not
  // re-saved: resolves to its id either way, or null if the save failed.
  function queueSaveIfChanged(): Promise<string | null> {
    return enqueue(async () => {
      const buildId = buildIdRef.current;
      const snapshot = currentSnapshotRef.current;
      if (buildId && snapshot && snapshot === lastSavedSnapshotRef.current) return buildId;
      return (await saveRecordingSnapshot())?.id ?? null;
    });
  }

  async function buildLinkedComponent(typeId: number): Promise<Build> {
    const buildId = await queueSaveIfChanged();
    if (!buildId) throw new Error("Save the build before creating a linked build.");
    return createLinkedBuild(buildId, { componentTypeId: typeId });
  }

  // ---- Active linked-build lifecycle (editor-owned) --------------------
  //
  // Every direct material currently resolved to "Build" needs a real linked
  // Build. This lives here, on editor state, rather than in an effect inside
  // `AcquisitionControl`, so it runs whether or not the inspector /
  // `AcquisitionControl` is mounted.
  // `AcquisitionControl` is purely presentational with respect to this
  // lifecycle -- it reads `linkedBuildsByTypeId` / `linkedBuildPending` /
  // `linkedBuildErrors` and never issues the calls itself.
  const [linkedBuildsByTypeId, setLinkedBuildsByTypeId] = useState<Record<number, Build>>({});
  const [linkedBuildPending, setLinkedBuildPending] = useState<Record<number, boolean>>({});
  const linkedBuildsSettling = Object.keys(linkedBuildPending).length > 0;
  const [linkedBuildErrors, setLinkedBuildErrors] = useState<Record<number, string>>({});
  // Per-type in-flight guard -- a ref, so effect reruns / StrictMode double
  // invokes can't launch a second create/resync for the same component.
  const linkedBuildInFlightRef = useRef<Set<number>>(new Set());

  // The one call that create-or-reuses (and, server-side, resyncs `runs` on)
  // a linked Build. `buildLinkedComponent` saves the parent through the
  // shared queue first, then hits `create_or_reuse_linked_build`, which
  // reuses a retained/inactive linked Build rather than duplicating it.
  function runLinkedBuildCall(typeId: number) {
    if (!initialBuild?.id) return;
    if (linkedBuildInFlightRef.current.has(typeId)) return;
    linkedBuildInFlightRef.current.add(typeId);
    setLinkedBuildPending((current) => ({ ...current, [typeId]: true }));
    setLinkedBuildErrors((current) => {
      if (!(typeId in current)) return current;
      const next = { ...current };
      delete next[typeId];
      return next;
    });
    buildLinkedComponent(typeId)
      .then((build) => {
        setLinkedBuildsByTypeId((current) => ({ ...current, [typeId]: build }));
      })
      .catch(() => {
        setLinkedBuildErrors((current) => ({
          ...current,
          [typeId]: "Could not create the linked build. Try again.",
        }));
      })
      .finally(() => {
        linkedBuildInFlightRef.current.delete(typeId);
        setLinkedBuildPending((current) => {
          const next = { ...current };
          delete next[typeId];
          return next;
        });
      });
  }
  function ensureLinkedBuild(typeId: number) {
    if (linkedBuildsByTypeId[typeId]) return;
    runLinkedBuildCall(typeId);
  }
  const ensureLinkedBuildRef = useRef(ensureLinkedBuild);
  ensureLinkedBuildRef.current = ensureLinkedBuild;

  // Adopt a freshly-loaded producer Build into the map -- e.g. after the
  // Graph inspector patches that Build's own settings by id. Keeps the
  // Worksheet inspector (which reads this map) in step with a cross-view
  // edit. Only a producer already in the map is replaced.
  const adoptLinkedBuild = useCallback((build: Build) => {
    setLinkedBuildsByTypeId((current) => {
      const entry = Object.entries(current).find(([, linked]) => linked.id === build.id);
      if (!entry || entry[1] === build) return current;
      return { ...current, [entry[0]]: build };
    });
  }, []);

  // Ensure + prune: keep the linked-Build map in step with which direct
  // materials are currently Build-resolved.
  useEffect(() => {
    if (!initialBuild?.id || initializingExistingBuild) return;
    const activeTypeIds = new Set(Object.keys(componentResolutions).map(Number));

    const stale = Object.keys(linkedBuildsByTypeId)
      .map(Number)
      .filter((typeId) => !activeTypeIds.has(typeId));
    if (stale.length > 0) {
      // Build -> Buy: forget the linked Build locally. Nothing is deleted
      // server-side -- it stays retained/inactive and is reused if the row
      // flips back to Build.
      setLinkedBuildsByTypeId((current) => {
        const next = { ...current };
        for (const typeId of stale) delete next[typeId];
        return next;
      });
      setLinkedBuildErrors((current) => {
        if (!stale.some((typeId) => typeId in current)) return current;
        const next = { ...current };
        for (const typeId of stale) delete next[typeId];
        return next;
      });
    }

    for (const typeId of activeTypeIds) ensureLinkedBuildRef.current(typeId);
    // `linkedBuildPending`/`linkedBuildErrors` are deliberately *not* deps:
    // updating them must not re-run this effect (that would auto-retry a
    // just-failed creation in a loop). A genuine Buy->Build toggle changes
    // `componentResolutions` and does re-run it, so retry stays possible.
  }, [initialBuild?.id, initializingExistingBuild, componentResolutions, linkedBuildsByTypeId]);

  // Autosave: debounces ~800ms after any change to the fields that make up
  // the draft, then persists via the same queue everything else uses. The
  // first time this becomes computable for an *existing* build is just the
  // already-saved data settling in from `initialBuild`, not an edit, so it
  // seeds the baseline instead of triggering a save; a brand new build has
  // no such baseline (its `name` starts empty, so the first valid snapshot
  // already reflects a real keystroke) and saves immediately.
  const parsedRunsForAutosave = parseRuns(runs);
  const autosaveSnapshot = (() => {
    if (!(selected && recipe && parsedRunsForAutosave && name.trim())) return "";
    try {
      return JSON.stringify({
        name,
        runs: parsedRunsForAutosave,
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
  currentSnapshotRef.current = autosaveSnapshot;
  const hasSeededSnapshotRef = useRef(false);

  useEffect(() => {
    if (!autosaveSnapshot) return;
    if (!hasSeededSnapshotRef.current) {
      hasSeededSnapshotRef.current = true;
      if (initialBuild) {
        lastSavedSnapshotRef.current = autosaveSnapshot;
        return;
      }
    }
    if (autosaveSnapshot === lastSavedSnapshotRef.current) return;
    const timer = window.setTimeout(() => {
      void queueSave();
    }, 800);
    return () => window.clearTimeout(timer);
    // Only a snapshot change schedules an autosave; `queueSave` and `initialBuild` are read fresh and must not retrigger it.
  }, [autosaveSnapshot]);

  const previewKey = (() => {
    const parsedRuns = parseRuns(runs);
    if (!selected || !recipe || !parsedRuns) return "";
    try {
      return JSON.stringify({
        recipe: selected.kind === "manufacturing"
          ? { mode: "manufacturing" as const, blueprintTypeId: selected.result.blueprintTypeId }
          : { mode: "reaction" as const, reactionFormulaTypeId: selected.result.reactionFormulaTypeId },
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
  useEffect(() => {
    const timer = window.setTimeout(() => {
      setAppliedPreviewKey(previewKey);
    }, 300);
    return () => window.clearTimeout(timer);
  }, [previewKey]);
  const previewPending = previewKey !== appliedPreviewKey;
  const previewSyncMessage = previewRetryAttempt > 0
    ? `Waiting for fresh market prices to sync (attempt ${previewRetryAttempt} of ${MAX_MARKET_SYNC_RETRIES})...`
    : null;
  useEffect(() => {
    if (!appliedPreviewKey) {
      setEstimate(null);
      return;
    }
    const sequence = ++previewSequence.current;
    const controller = new AbortController();
    if (previewRetry.current.key !== appliedPreviewKey) {
      previewRetry.current = { key: appliedPreviewKey, attempts: 0 };
      setPreviewRetryAttempt(0);
    }
    let retryTimer: number | undefined;
    let retryScheduled = false;
    setPreviewUpdating(true);
    const timer = window.setTimeout(() => {
      void previewCreateBuildCandidate(JSON.parse(appliedPreviewKey), controller.signal)
        .then((plan) => {
          if (sequence !== previewSequence.current) return;
          setEstimate(plan);
          setError("");
          // Every build prices from market scope first (a manual list is
          // only ever an optional fallback), so the "wait for ESI data to
          // sync" retry always applies, regardless of which PriceSource
          // is selected in the (manual-only) dropdown.
          const retryDelay = automaticPricingRetryDelay(
            "esiMarketOrders",
            plan,
            previewRetry.current.attempts,
          );
          if (retryDelay !== null) {
            retryScheduled = true;
            previewRetry.current.attempts += 1;
            setPreviewRetryAttempt(previewRetry.current.attempts);
            retryTimer = window.setTimeout(
              () => setPreviewRetryToken((current) => current + 1),
              retryDelay,
            );
          } else {
            previewRetry.current.attempts = 0;
            setPreviewRetryAttempt(0);
          }
        })
        .catch((requestError) => {
          if (controller.signal.aborted || sequence !== previewSequence.current) return;
          // A transient preview failure (market sync, a rejected edit)
          // surfaces the error but must NOT discard the last worksheet that
          // did compute -- nulling `estimate` here would unmount the whole
          // results view and the Selected Item inspector mid-edit. The
          // estimate is only cleared deliberately elsewhere (product
          // switch, no selection).
          setError(apiMessage(requestError));
          setPreviewRetryAttempt(0);
        })
        .finally(() => {
          if (sequence === previewSequence.current && !retryScheduled) setPreviewUpdating(false);
        });
    }, 180);
    return () => {
      window.clearTimeout(timer);
      if (retryTimer !== undefined) window.clearTimeout(retryTimer);
      controller.abort();
    };
  }, [appliedPreviewKey, previewRetryToken]);

  function createBlueprintSelection(): BlueprintSelection {
    if (blueprintMode === "observedAsset") {
      if (!selectedObservationId) throw new Error("Select an observed blueprint or use manual assumptions.");
      return { mode: "observedAsset", observationId: selectedObservationId };
    }
    return createManualBlueprintSelection(
      blueprintKind,
      blueprintMe,
      blueprintTe,
      licensedRuns,
      blueprintNotes,
    );
  }

  const routeBlueprintTypeId = Number(searchParams.get("blueprintTypeId"));
  const hasRouteBlueprint = Number.isSafeInteger(routeBlueprintTypeId) && routeBlueprintTypeId > 0;
  const selectedName = selected
    ? (selected.kind === "manufacturing" ? selected.result.blueprintName : selected.result.reactionFormulaName)
    : null;

  return {
    initialBuild,
    sdeReady,
    query,
    setQuery,
    selected,
    recipe,
    recipeStatus,
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
    estimate,
    previewUpdating,
    previewPending,
    previewSyncMessage,
    previewKey,
    productResults,
    productSearching: blueprintLookup.searching || reactionFormulaLookup.searching,
    hasRouteBlueprint,
    selectFromDialog,
    selectedName,
    blueprintDialogOpen,
    setBlueprintDialogOpen,
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
    linkedBuildsByTypeId,
    linkedBuildPending,
    linkedBuildsSettling,
    linkedBuildErrors,
    adoptLinkedBuild,
    blueprintMode,
    setBlueprintMode,
    blueprintKind,
    setBlueprintKind,
    blueprintMe,
    setBlueprintMe,
    blueprintTe,
    setBlueprintTe,
    licensedRuns,
    setLicensedRuns,
    blueprintNotes,
    setBlueprintNotes,
    observedBlueprints,
    selectedObservationId,
    setSelectedObservationId,
    saveStatus,
    setError,
    error,
    allFacilities,
    manufacturing,
    reaction,
    rootFacilitySelection,
    buildLinkedComponent,
    queueSave,
    bumpPreview,
    navigate,
    renameBuild,
    setBuildRevision,
  };
}

export type BuildWorksheetEditorModel = ReturnType<typeof useBuildWorksheetEditor>;
