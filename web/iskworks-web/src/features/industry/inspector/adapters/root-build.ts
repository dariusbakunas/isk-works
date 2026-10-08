// The canonical inspector model for a ROOT Build target -- the build the
// page is editing, with no parent/component relationship.
//
// `buildRootInspector(editor)` is the SINGLE builder that decides which
// sections a root Build gets and what they contain. Both hosts call it with
// the same argument -- `BuildInspector` (the Worksheet's "build settings"
// inspector mode) and `BuildGraphView` (the Graph's root node). The
// authoritative root state is the resident `useBuildWorksheetEditor`
// instance, which both hosts already hold; selection source adds nothing, so
// the rendered inspector is identical by construction.

import { rootFacility } from "../../../../api/industry";
import { formatIskSummary } from "../../../../components/money";
import { formatPercent } from "../../shared/formatting";
import { parseRuns } from "../../builds/components/planner-panels";
import type { BuildWorksheetEditorModel } from "../../builds/use-build-worksheet-editor";
import { recipeCurrencyChip, recipeCurrencyExplanation } from "../../builds/graph/recipe-currency";
import type {
  CostState,
  InspectorActions,
  InspectorModel,
  InspectorWarning,
  ProvenanceSlice,
} from "../inspector-model";
import { jobSplitLabel } from "../../builds/planner/job-split";

const NUMBER = new Intl.NumberFormat("en-US");

function money(value: string | null | undefined): string | null {
  return value != null ? formatIskSummary(value) : null;
}

function recipeCurrencyBadge(
  state: import("../../../../api/industry").RecipeCurrency,
): ProvenanceSlice["recipeCurrency"] {
  const chip = recipeCurrencyChip(state);
  if (!chip) return null;
  return {
    label: chip.label,
    tone: chip.tone === "danger" ? "blocking" : "neutral",
    explanation: recipeCurrencyExplanation(state),
  };
}

/**
 * Canonical inspector model + actions for the root Build. Section set and
 * content are fixed here; the two hosts only supply this one `editor` and an
 * `onClose`.
 */
export function buildRootInspector(
  editor: BuildWorksheetEditorModel,
  opts: { onClose?: () => void } = {},
): { model: InspectorModel; actions: InspectorActions } {
  const selected = editor.selected;
  const isReaction = selected?.kind === "reaction";
  const result = selected?.result ?? null;
  const estimate = editor.estimate ?? null;
  const recipe = editor.recipe ?? null;
  const computing = estimate == null || Boolean(editor.previewUpdating);

  const runs = parseRuns(editor.runs) ?? (Number(editor.runs) || 0);
  const outputLine = estimate?.worksheet?.output?.items?.[0] ?? null;
  const productTypeId = result?.productTypeId ?? outputLine?.typeId ?? null;
  const productName = result?.productName ?? outputLine?.typeName ?? editor.name ?? "Build";
  const groupName = result?.groupName ?? editor.initialBuild?.productGroupName ?? null;
  const outputPerRun = recipe?.products[0]?.quantityPerRun ?? null;
  const making =
    outputLine?.requiredQuantity ?? (outputPerRun != null ? runs * outputPerRun : runs);

  // ── COST (from the authoritative preview summary) ──
  const summary = estimate?.worksheet?.summary ?? null;
  const materialRaw = summary?.materialCost ?? null;
  const installationRaw = summary?.installationCost ?? null;
  const totalRaw = summary?.totalCost ?? null;
  const costState: CostState = !estimate
    ? "notComputed"
    : totalRaw != null
      ? "known"
      : materialRaw != null
        ? "incomplete"
        : "notComputed";

  // ── FACILITY (the root's own slot) ──
  const facSel = editor.rootFacilitySelection;
  const facility = facSel.selectedFacility ?? null;
  const facilityRole: "manufacturing" | "reaction" = isReaction ? "reaction" : "manufacturing";

  // ── planned duration (facility-adjusted where the preview has it) ──
  const candidate = estimate?.candidate ?? null;
  const facilityPreview = candidate ? rootFacility(candidate) : null;
  const durationSeconds =
    facilityPreview?.plannedDurationSeconds ??
    candidate?.blueprint?.plannedDurationSeconds ??
    recipe?.durationSeconds ??
    null;

  // ── warnings (identical in both views; visible when collapsed) ──
  const warnings: InspectorWarning[] = [];
  if (estimate && !summary?.pricingComplete) {
    warnings.push({
      label: "Pricing incomplete",
      detail: "One or more lines have no unit price from the selected source.",
      tone: "blocking",
    });
  }
  if (!facility) {
    warnings.push({
      label: "Facility not selected",
      detail: `Select a ${facilityRole} facility to compute installation cost and duration.`,
      tone: "neutral",
    });
  } else if (estimate && installationRaw == null) {
    warnings.push({
      label: "Installation cost missing",
      detail: "The selected facility did not yield an installation cost for this build.",
      tone: "neutral",
    });
  }

  const model: InspectorModel = {
    identity: {
      kind: isReaction ? "reaction" : "rootBuild",
      kindLabel: isReaction ? "REACTION" : "BUILD",
      name: productName,
      subtitle: [groupName, isReaction ? "Reaction" : "Manufacturing"].filter(Boolean).join(" · ") || null,
      typeId: productTypeId,
      showImage: true,
      summary: `Runs ${NUMBER.format(runs)} · Making ${NUMBER.format(making)}`,
    },
    warnings,
    quantities: {
      metrics: [
        { label: "Runs", value: NUMBER.format(runs) },
        outputPerRun != null
          ? { label: "Output per run", value: NUMBER.format(outputPerRun) }
          : null,
        { label: "Making", value: NUMBER.format(making) },
      ].filter((m): m is { label: string; value: string } => m != null),
      summary: `Runs ${NUMBER.format(runs)} · Making ${NUMBER.format(making)}`,
    },
    cost: {
      material: money(materialRaw),
      installation: money(installationRaw),
      total: money(totalRaw),
      state: costState,
      computing,
      summary: money(totalRaw)
        ? `${money(totalRaw)} total`
        : money(materialRaw)
          ? `${money(materialRaw)} materials`
          : computing
            ? "Computing…"
            : "Incomplete",
    },
    facility: {
      name: facility?.name ?? null,
      location: facility
        ? [facility.solarSystemName, facility.structureTypeName].filter(Boolean).join(" · ")
        : null,
      bonuses: facility
        ? `Material −${formatPercent(facility.materialReductionPercent)} · Time −${formatPercent(facility.timeReductionPercent)}`
        : null,
      rigCount: facility?.rigs.length ?? 0,
      state: facility ? "set" : "unset",
      editable: true,
      role: facilityRole,
      selectedFacilityId: facSel.facilityId || null,
      eiv: facility
        ? {
            automaticEiv: facSel.automaticEiv,
            manual: facSel.manualEiv,
            value: facSel.estimatedItemValue,
            loading: facSel.eivLoading,
            error: facSel.eivError,
          }
        : null,
      summary: facility?.name ?? "Not selected",
    },
    pricing: {
      kind: "root",
      materialScope: editor.materialScope,
      materialPolicy: editor.materialPricingPolicy,
      outputScope: editor.outputScope,
      outputPolicy: editor.outputPricingPolicy,
      priceSourceId: editor.sourceId,
      priceSources: editor.sources,
      summary: editor.source?.name ?? "Market price only",
    },
    provenance: {
      summary: "Root build",
      lines: [],
      note: blueprintSourceNote(editor),
      buildId: editor.initialBuild?.id ?? null,
      recipeCurrency: recipeCurrencyBadge(editor.initialBuild?.recipeCurrency ?? "current"),
    },
  };

  if (durationSeconds != null) model.durationSeconds = durationSeconds;

  // ── BLUEPRINT (manufacturing root) / RECIPE (reaction root) ──
  if (isReaction) {
    const formulaName =
      (selected && selected.kind === "reaction" ? selected.result.reactionFormulaName : null) ??
      editor.selectedName ??
      null;
    model.recipe = {
      kind: "formula",
      name: formulaName,
      computing,
      summary: formulaName ?? (computing ? "Computing…" : "—"),
    };
  } else if (
    selected &&
    selected.kind === "manufacturing" &&
    selected.result.blueprintTypeId != null &&
    editor.blueprintMode != null
  ) {
    const isExisting = editor.blueprintMode === "observedAsset";
    const selectedObs = isExisting
      ? editor.observedBlueprints.find((o) => o.id === editor.selectedObservationId) ?? null
      : null;
    const me = isExisting ? selectedObs?.materialEfficiency ?? null : Number(editor.blueprintMe) || 0;
    const te = isExisting ? selectedObs?.timeEfficiency ?? null : Number(editor.blueprintTe) || 0;
    const origin: "BPO" | "BPC" | null = isExisting
      ? selectedObs?.kind === "copy"
        ? "BPC"
        : selectedObs
          ? "BPO"
          : null
      : editor.blueprintKind === "copy"
        ? "BPC"
        : "BPO";
    const licensedRuns = isExisting
      ? selectedObs?.kind === "copy"
        ? selectedObs.licensedRuns
        : null
      : editor.blueprintKind === "copy" && editor.licensedRuns
        ? Number(editor.licensedRuns)
        : null;
    model.blueprint = {
      kind: "blueprint",
      name: editor.selectedName ?? selected.result.blueprintName,
      blueprintTypeId: selected.result.blueprintTypeId,
      mode: isExisting ? "existing" : "manual",
      origin,
      me,
      te,
      licensedRuns,
      notes: editor.blueprintNotes,
      observations: editor.observedBlueprints,
      selectedObservationId: isExisting ? editor.selectedObservationId || null : null,
      requiredRuns: runs || 1,
      computing,
      editable: true,
      summary:
        [
          editor.selectedName ?? selected.result.blueprintName,
          origin,
          me != null && te != null ? `ME ${me} · TE ${te}` : null,
          jobSplitLabel(runs || 1, licensedRuns),
        ]
          .filter(Boolean)
          .join(" · ") || "—",
    };
  }

  // ── INPUTS (informational -- the root's material requirements) ──
  const materialLines = estimate?.worksheet?.groups
    ? estimate.worksheet.groups.flatMap((g) => g.items).filter((i) => i.role === "material")
    : (recipe?.materials ?? []).map((m) => ({
        typeId: m.typeId,
        typeName: m.typeName,
        requiredQuantity: m.totalQuantity,
      }));
  if (materialLines.length > 0) {
    model.inputs = {
      label: "Inputs",
      fallbackTypeId: null,
      entries: materialLines.map((m) => ({
        typeId: m.typeId,
        name: m.typeName,
        quantity: m.requiredQuantity,
      })),
    };
  }

  // ── actions ──
  const actions: InspectorActions = {
    onClose: opts.onClose,
    blueprint: model.blueprint
      ? {
          onSelectObservation: (observationId) => {
            editor.setSelectedObservationId(observationId);
            editor.setBlueprintMode("observedAsset");
          },
          onModelManually: ({ kind, materialEfficiency, timeEfficiency, licensedRuns, notes }) => {
            editor.setBlueprintKind(kind);
            editor.setBlueprintMe(String(materialEfficiency));
            editor.setBlueprintTe(String(timeEfficiency));
            editor.setLicensedRuns(licensedRuns != null ? String(licensedRuns) : "");
            editor.setBlueprintNotes(notes);
            editor.setBlueprintMode("manual");
          },
        }
      : undefined,
    facility: {
      options: editor.allFacilities,
      onSelect: (facilityProfileId) => facSel.setFacilityId(facilityProfileId ?? ""),
      onEivManual: (manual) => {
        facSel.setManualEiv(manual);
        if (!manual && facSel.automaticEiv?.value) facSel.setEstimatedItemValue(facSel.automaticEiv.value);
      },
      onEivCommit: (canonical) => {
        facSel.setManualEiv(true);
        facSel.setEstimatedItemValue(canonical);
      },
      onEivClear: () => {
        facSel.setManualEiv(false);
        facSel.setEstimatedItemValue(facSel.automaticEiv?.value ?? "");
      },
    },
    pricing: {
      onMaterialScope: editor.setMaterialScope,
      onMaterialPolicy: editor.setMaterialPricingPolicy,
      onOutputScope: editor.setOutputScope,
      onOutputPolicy: editor.setOutputPricingPolicy,
      onPriceSource: editor.setSourceId,
    },
  };
  if (editor.initialBuild?.id) {
    actions.copyBuildId = () => void navigator.clipboard?.writeText(editor.initialBuild!.id);
  }

  return { model, actions };
}

/** A short "where the blueprint came from" line for Provenance. */
function blueprintSourceNote(editor: BuildWorksheetEditorModel): string | null {
  if (editor.selected?.kind !== "manufacturing") return null;
  if (editor.blueprintMode === "observedAsset") {
    const obs = editor.observedBlueprints.find((o) => o.id === editor.selectedObservationId);
    if (!obs) return "Owned blueprint (from the latest ESI sync).";
    return `Owned blueprint · ${[obs.ownerName, obs.locationName].filter(Boolean).join(" · ")}`;
  }
  if (editor.blueprintMode === "manual") return "Modelled blueprint (planning assumption).";
  return null;
}
