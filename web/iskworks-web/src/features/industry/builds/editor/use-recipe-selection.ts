import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router";

import {
  listBlueprintObservations,
  type Build,
  type BlueprintObservation,
  type BlueprintSelection,
} from "../../../../api/industry";
import {
  planBuild,
  planReaction,
  searchBlueprints,
  searchReactionFormulas,
  type BlueprintSearchResult,
  type BuildPlan,
  type ReactionFormulaSearchResult,
  type ReactionPlan,
} from "../../../../api/sde";
import { apiMessage } from "../../../../api/api-error";
import { useDebouncedLookup } from "../../../../hooks/use-debounced-lookup";
import { useStableCallback } from "../../../../hooks/use-stable-callback";
import type { ProductSearchResult } from "../builds-page";
import { createManualBlueprintSelection, parseRuns } from "../components/planner-panels";
import { selectAvailableBlueprint } from "../planner/available-blueprint";

type RecipeStatus = "idle" | "loading" | "ready" | "error";

export type SelectedProduct =
  | { kind: "manufacturing"; result: BlueprintSearchResult }
  | { kind: "reaction"; result: ReactionFormulaSearchResult };

export interface RecipeSelectionOptions {
  initialBuild: Build | null;
  /** Whether the SDE is imported (`null` until known). */
  sdeReady: boolean | null;
  runs: string;
  name: string;
  setName: (name: string) => void;
  setError: (message: string) => void;
  /** A product was picked: reset whatever depends on the previous one. */
  onProductSelected: () => void;
}

function initialSelection(initialBuild: Build | null): SelectedProduct | null {
  const initialProduct = initialBuild?.recipe.products[0];
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
}

/**
 * What the Build makes and from which blueprint: the product picker and its
 * search, a recipe picked by the URL, the recipe loaded for the current
 * runs, and the blueprint assumptions (an owned blueprint, or manual
 * ME/TE).
 */
export function useRecipeSelection({
  initialBuild,
  sdeReady,
  runs,
  name,
  setName,
  setError,
  onProductSelected,
}: RecipeSelectionOptions) {
  const [searchParams, setSearchParams] = useSearchParams();
  const initialBlueprint = initialBuild?.draftPlanning?.input?.blueprintSelection;
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<SelectedProduct | null>(() => initialSelection(initialBuild));
  const [recipe, setRecipe] = useState<BuildPlan | ReactionPlan | null>(null);
  const [recipeStatus, setRecipeStatus] = useState<RecipeStatus>(
    initialBuild && selected ? "loading" : "idle",
  );
  const initializingExistingBuild = Boolean(initialBuild && recipeStatus === "loading");
  const recipeSequence = useRef(0);
  const routeBlueprintLoaded = useRef(false);
  const [blueprintDialogOpen, setBlueprintDialogOpen] = useState(false);
  const [blueprintMode, setBlueprintMode] = useState<"manual" | "observedAsset">(initialBlueprint?.mode ?? "manual");
  const [blueprintKind, setBlueprintKind] = useState<"original" | "copy">(initialBlueprint?.mode === "manual" ? initialBlueprint.kind : "original");
  const [blueprintMe, setBlueprintMe] = useState(String(initialBlueprint?.mode === "manual" ? initialBlueprint.materialEfficiency : 0));
  const [blueprintTe, setBlueprintTe] = useState(String(initialBlueprint?.mode === "manual" ? initialBlueprint.timeEfficiency : 0));
  const [licensedRuns, setLicensedRuns] = useState(initialBlueprint?.mode === "manual" && initialBlueprint.licensedRuns !== null ? String(initialBlueprint.licensedRuns) : "");
  const [blueprintNotes, setBlueprintNotes] = useState(initialBlueprint?.mode === "manual" ? initialBlueprint.notes : "");
  const [observedBlueprints, setObservedBlueprints] = useState<BlueprintObservation[]>([]);
  const [selectedObservationId, setSelectedObservationId] = useState(initialBlueprint?.mode === "observedAsset" ? initialBlueprint.observationId : "");

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

  // The deep link's selection, through the latest `selectProduct` (rebuilt
  // every render) without re-running the deep-link effect.
  const selectRouteManufacturing = useStableCallback(
    (result: BlueprintSearchResult, observationId: string) => selectProduct("manufacturing", result, observationId),
  );
  const selectRouteReaction = useStableCallback(
    (result: ReactionFormulaSearchResult) => selectProduct("reaction", result),
  );

  useEffect(() => {
    if (initialBuild || !sdeReady || routeBlueprintLoaded.current) return;
    routeBlueprintLoaded.current = true;
    // `?reactionFormulaTypeId=` (the Type Info window's "Plan a build" for a
    // reaction product) starts a reaction build the same way.
    const reactionFormulaTypeId = Number(searchParams.get("reactionFormulaTypeId"));
    if (Number.isSafeInteger(reactionFormulaTypeId) && reactionFormulaTypeId > 0) {
      planReaction(reactionFormulaTypeId, 1)
        .then((loadedRecipe) => {
          const product = loadedRecipe.products[0];
          if (!product) {
            setSearchParams({}, { replace: true });
            return;
          }
          return selectRouteReaction({
            reactionFormulaTypeId: loadedRecipe.reactionFormulaTypeId,
            reactionFormulaName: loadedRecipe.reactionFormulaName,
            productTypeId: product.typeId,
            productName: product.typeName,
            groupName: null,
            published: true,
          });
        })
        .catch(() => setSearchParams({}, { replace: true }));
      return;
    }
    const blueprintTypeId = Number(searchParams.get("blueprintTypeId"));
    if (!Number.isSafeInteger(blueprintTypeId) || blueprintTypeId <= 0) return;
    const observationId = searchParams.get("observationId") ?? "";

    planBuild(blueprintTypeId, 1)
      .then((loadedRecipe) => {
        const product = loadedRecipe.products[0];
        if (!product) {
          setSearchParams({}, { replace: true });
          return;
        }
        return selectRouteManufacturing({
          blueprintTypeId: loadedRecipe.blueprintTypeId,
          blueprintName: loadedRecipe.blueprintName,
          productTypeId: product.typeId,
          productName: product.typeName,
          groupName: null,
          published: true,
          manufacturingAvailable: true,
        }, observationId);
      })
      .catch(() => setSearchParams({}, { replace: true }));
  }, [initialBuild, sdeReady, searchParams, setSearchParams, selectRouteManufacturing, selectRouteReaction]);

  useEffect(() => {
    if (!initialBuild || !selected || selected.kind !== "manufacturing") return;
    listBlueprintObservations(selected.result.blueprintTypeId)
      .then(setObservedBlueprints)
      .catch((requestError) => setError(apiMessage(requestError)));
  }, [initialBuild, selected, setError]);

  async function selectProduct(
    kind: "manufacturing",
    result: BlueprintSearchResult,
    preferredObservationId?: string,
  ): Promise<void>;
  async function selectProduct(
    kind: "reaction",
    result: ReactionFormulaSearchResult,
  ): Promise<void>;
  async function selectProduct(
    kind: "manufacturing" | "reaction",
    result: BlueprintSearchResult | ReactionFormulaSearchResult,
    // A specific owned blueprint to start from (the Blueprints library's
    // "Plan build"); ignored when it is no longer listed for this type.
    preferredObservationId = "",
  ) {
    const parsedRuns = parseRuns(runs);
    if (!parsedRuns) {
      setError("Runs must be a whole number between 1 and 1,000,000.");
      return;
    }
    setError("");
    setSelected({ kind, result } as SelectedProduct);
    onProductSelected();
    setBlueprintMode("manual");
    setBlueprintKind("original");
    setBlueprintMe("0");
    setBlueprintTe("0");
    setLicensedRuns("");
    setBlueprintNotes("");
    setObservedBlueprints([]);
    setSelectedObservationId("");
    if (!name) setName(`${result.productName} build`);
    if (kind === "reaction") return;
    try {
      const observations = await listBlueprintObservations((result as BlueprintSearchResult).blueprintTypeId);
      const availableBlueprint = selectAvailableBlueprint(observations, parsedRuns, preferredObservationId);
      setObservedBlueprints(observations);
      setSelectedObservationId(availableBlueprint?.id ?? "");
      setBlueprintMode(availableBlueprint ? "observedAsset" : "manual");
    } catch (requestError) {
      setError(apiMessage(requestError));
    }
  }

  function selectFromDialog(entry: ProductSearchResult) {
    if (entry.kind === "manufacturing") {
      setSearchParams({ blueprintTypeId: String(entry.result.blueprintTypeId) }, { replace: true });
      void selectProduct("manufacturing", entry.result);
    } else {
      setSearchParams({ reactionFormulaTypeId: String(entry.result.reactionFormulaTypeId) }, { replace: true });
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
  }, [runs, selected, setError]);

  // An owned blueprint that can't cover the runs any more is swapped for
  // one that can, or, failing that, for manual assumptions.
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

  // A recipe in the URL (`?blueprintTypeId=` or `?reactionFormulaTypeId=`)
  // keeps the product picker closed while it loads.
  const hasRouteBlueprint = ["blueprintTypeId", "reactionFormulaTypeId"].some((key) => {
    const typeId = Number(searchParams.get(key));
    return Number.isSafeInteger(typeId) && typeId > 0;
  });
  const selectedName = selected
    ? (selected.kind === "manufacturing" ? selected.result.blueprintName : selected.result.reactionFormulaName)
    : null;

  return {
    query,
    setQuery,
    selected,
    recipe,
    recipeStatus,
    initializingExistingBuild,
    productResults,
    productSearching: blueprintLookup.searching || reactionFormulaLookup.searching,
    hasRouteBlueprint,
    selectFromDialog,
    selectedName,
    blueprintDialogOpen,
    setBlueprintDialogOpen,
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
    createBlueprintSelection,
  };
}
