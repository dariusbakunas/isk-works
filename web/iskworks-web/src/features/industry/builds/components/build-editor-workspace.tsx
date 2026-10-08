import { useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";

import {
  exportBuildVerificationWorkbook,
  renameBuild,
  type Build,
  type OrderDetail,
  type PreviewBuildPlanCommand,
} from "../../../../api/industry";
import { EmptyState, InlineAlert, PageHeader, Panel, StatusBadge } from "../../../../components/primitives";
import { apiMessage } from "../../shared/api-error";
import { Field } from "../../shared/field";
import { BuildPlanner } from "../planner/build-planner";
import {
  CreateCandidateResults,
  BlueprintSelectionSection,
  BlueprintPlanningDialog,
  BlueprintSummaryCell,
  parseRuns,
} from "./planner-panels";
import { BuildEditorHeader } from "./build-editor-header";
import { CreateEpicDialog } from "./create-epic-dialog";
import { BuildPageHeader } from "./build-page-header";
import { BuildSettingsSummary } from "./build-settings-summary";
import { BuildWorkspaceSummary } from "./build-workspace-summary";
import { FocusedProducerSummary } from "./focused-producer-summary";
import { BuildInspector } from "./build-inspector";
import { BuildGraphView } from "../graph/build-graph-view";
import { BuildLogisticsView } from "../logistics/build-logistics-view";
import { BuildStagesView } from "../stages/build-stages-view";
import { BuildWorksheetView } from "../worksheet/build-worksheet-view";
import { DEFAULT_SCOPE, type BuildWorksheetEditorModel } from "../use-build-worksheet-editor";

// The saved Build's own configuration as a `PreviewBuildPlanCommand` -- the
// same shape `editor.previewKey` produces once its local recipe/blueprint
// resolution finishes, but derived straight from `initialBuild`/its
// `draftPlanning` (no dependency on that resolution). Used when Create
// Epic is invoked before `previewKey` has ever resolved (see
// `handleCreateOrder`) -- effectively "no live overlay yet, use what's
// saved."
function buildCommandFromBuild(build: Build): PreviewBuildPlanCommand {
  const draft = build.draftPlanning?.input;
  const recipe: PreviewBuildPlanCommand["recipe"] =
    build.recipe.kind === "manufacturing"
      ? { mode: "manufacturing", blueprintTypeId: build.recipe.blueprintTypeId }
      : { mode: "reaction", reactionFormulaTypeId: build.recipe.reactionFormulaTypeId };
  return {
    recipe,
    runs: build.runs,
    materialScope: draft?.materialScope ?? DEFAULT_SCOPE,
    outputScope: draft?.outputScope ?? DEFAULT_SCOPE,
    manualPriceListId: draft?.manualPriceListId ?? null,
    expectedManualPriceListRevision: draft?.expectedManualPriceListRevision ?? null,
    pricingSelections: draft?.pricingSelections ?? [],
    blueprintSelection: draft?.blueprintSelection ?? undefined,
    manufacturingFacility: draft?.manufacturingFacility ?? null,
    reactionFacility: draft?.reactionFacility ?? null,
    componentResolutions: draft?.componentResolutions ?? [],
    fulfillmentScopes: draft?.fulfillmentScopes ?? [],
    buildId: build.id,
  };
}

// The saved Build workspace uses the read-only whole-Build Worksheet as
// its default, with Plan immediately after it.
// Build-level economics, runs and root
// settings are persistent context above them (`BuildWorkspaceSummary`),
// not a destination. Retired view names in old links (`materials`,
// `stages`, `economics`) still resolve to the closest current view.
type BuildView = "plan" | "worksheet" | "logistics" | "graph";
const BUILD_VIEWS: BuildView[] = ["worksheet", "plan", "logistics", "graph"];
const VIEW_LABELS: Record<BuildView, string> = {
  plan: "Plan",
  worksheet: "Worksheet",
  logistics: "Logistics",
  graph: "Graph",
};

function parseBuildView(param: string | null): BuildView {
  switch (param) {
    case "plan":
    case "logistics":
    case "worksheet":
    case "graph":
      return param;
    // Materials' quantities / planned use / shortage live in
    // Logistics (by destination) -- the closest match for that intent.
    case "materials":
      return "logistics";
    // The retired Plan-like destinations retain their original intent.
    case "stages":
    case "economics":
      return "plan";
    // No view, or an unknown view, resolves to the canonical default.
    default:
      return "worksheet";
  }
}

export function BuildEditorWorkspace({
  editor,
  focusedProducer,
}: {
  editor: BuildWorksheetEditorModel;
  focusedProducer?: Build;
}) {
  const {
    blueprintDialogOpen,
    blueprintKind,
    blueprintMe,
    blueprintMode,
    blueprintNotes,
    blueprintTe,
    error,
    estimate,
    initialBuild,
    licensedRuns,
    manufacturing,
    materialPricingPolicy,
    materialScope,
    name,
    notes,
    observedBlueprints,
    outputPricingPolicy,
    outputScope,
    previewPending,
    previewSyncMessage,
    previewUpdating,
    reaction,
    runs,
    saveStatus,
    selected,
    selectedName,
    selectedObservationId,
    setBlueprintDialogOpen,
    inspectorMode,
    openBuildSettings,
    selectWorksheetRow,
    setBlueprintKind,
    setBlueprintMe,
    setBlueprintMode,
    setBlueprintNotes,
    setBlueprintTe,
    setBuildRevision,
    setError,
    setLicensedRuns,
    setName,
    setNotes,
    setRuns,
    setSelectedObservationId,
    sourceId,
    sources,
    sdeReady,
  } = editor;

  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const view = parseBuildView(searchParams.get("view"));
  const setView = (next: BuildView) => {
    const params = new URLSearchParams(searchParams);
    if (next === "worksheet") params.delete("view");
    else params.set("view", next);
    setSearchParams(params);
  };
  // `?settings=open` (the legacy /builds/:id/edit link lands here) opens
  // Build settings once, then drops the param. Retired view names are
  // normalized to the view they now resolve to.
  //
  // Normalize each URL at most once: React Router commits the replacement
  // inside a transition, so re-renders keep seeing the old search params
  // until it lands, and re-running here would interrupt it indefinitely.
  const settingsParam = searchParams.get("settings");
  const viewParam = searchParams.get("view");
  const normalizedSearch = useRef<string | null>(null);
  useEffect(() => {
    if (!initialBuild) return;
    const retiredView = viewParam !== null && viewParam !== "plan" && viewParam !== "worksheet" && viewParam !== "logistics" && viewParam !== "graph";
    const nonCanonicalDefault = viewParam === "worksheet";
    const openSettings = settingsParam !== null;
    if (!retiredView && !nonCanonicalDefault && !openSettings) return;
    if (openSettings && !selected) return; // wait until the root recipe resolves
    const search = searchParams.toString();
    if (normalizedSearch.current === search) return;
    normalizedSearch.current = search;
    if (openSettings) openBuildSettings();
    const params = new URLSearchParams(searchParams);
    params.delete("settings");
    if (view === "worksheet") params.delete("view");
    else params.set("view", view);
    setSearchParams(params, { replace: true });
  }, [initialBuild, settingsParam, viewParam, view, selected, openBuildSettings, searchParams, setSearchParams]);
  // Non-null while the Create Epic dialog is open: the overlay it freezes.
  const [epicCommand, setEpicCommand] = useState<PreviewBuildPlanCommand | null>(null);
  const [orderError, setOrderError] = useState("");
  const [exportingWorkbook, setExportingWorkbook] = useState(false);
  const [exportError, setExportError] = useState("");

  // Engineering / audit `.xlsx` of the CURRENT editor state -- sends the same
  // unsaved planning overlay a preview / the Materials view sends
  // (`editor.previewKey`), receives the workbook, triggers a browser
  // download. Read-only server-side: no Build save, no inventory write.
  async function handleExportVerification() {
    if (!initialBuild) return;
    let command: PreviewBuildPlanCommand;
    try {
      command = JSON.parse(editor.previewKey) as PreviewBuildPlanCommand;
    } catch {
      setExportError("The current Build state could not be prepared for export.");
      return;
    }
    setExportError("");
    setExportingWorkbook(true);
    try {
      const { blob, filename } = await exportBuildVerificationWorkbook(
        initialBuild.id,
        command,
      );
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = filename || `${name || "build"}-verification.xlsx`;
      anchor.click();
      URL.revokeObjectURL(url);
    } catch (requestError) {
      setExportError(apiMessage(requestError));
    } finally {
      setExportingWorkbook(false);
    }
  }

  // "Create Order" only makes sense once the Build is actually saved --
  // editor.initialBuild is null for the /builds/new create flow. The Epic
  // freezes the *live* editor overlay (`editor.previewKey`, the
  // same `PreviewBuildPlanCommand` Materials/Graph/verification-export
  // already send), not the Build's last-saved configuration. `previewKey`
  // is briefly `""` right after the editor (re)mounts -- it depends on the
  // locally-resolved recipe/blueprint detail, not just `initialBuild` --
  // so fall back to a command built straight from the saved Build/its
  // draft planning (`buildCommandFromBuild`) rather than blocking Epic
  // creation on that resolution finishing.
  function handleCreateOrder() {
    if (!initialBuild) return;
    let command: PreviewBuildPlanCommand;
    try {
      command = editor.previewKey
        ? (JSON.parse(editor.previewKey) as PreviewBuildPlanCommand)
        : buildCommandFromBuild(initialBuild);
    } catch {
      setOrderError("The current Build state could not be prepared for Epic creation.");
      return;
    }
    setOrderError("");
    // The dialog previews the Epic's inventory reuse, offers to reserve
    // it, and creates the Epic.
    setEpicCommand(command);
  }

  function handleEpicCreated(order: OrderDetail) {
    setEpicCommand(null);
    // Land on the Board with the new Epic's inspector already open --
    // Board is the workspace the rest of this Epic's lifecycle happens
    // on.
    navigate(`/board?epic=${order.id}`);
  }


  const identity = selected ? (
    <BuildEditorHeader
      onRunsChange={setRuns}
      onShowLogistics={initialBuild && view !== "logistics" ? () => setView("logistics") : undefined}
      preview={estimate}
      runs={runs}
    />
  ) : null;
  const blueprintDialog = selected?.kind === "manufacturing" ? (
    <BlueprintPlanningDialog
      open={blueprintDialogOpen}
      title="Choose Blueprint"
      onClose={() => setBlueprintDialogOpen(false)}
    >
      <BlueprintSelectionSection
        kind={blueprintKind} licensedRuns={licensedRuns} me={blueprintMe}
        mode={blueprintMode} notes={blueprintNotes} observations={observedBlueprints}
        requiredRuns={parseRuns(runs) ?? 1} selectedObservationId={selectedObservationId} te={blueprintTe}
        onKind={setBlueprintKind} onLicensedRuns={setLicensedRuns} onMe={setBlueprintMe}
        onMode={setBlueprintMode} onNotes={setBlueprintNotes}
        onObservation={setSelectedObservationId} onTe={setBlueprintTe}
      />
    </BlueprintPlanningDialog>
  ) : null;
  const settingsToolbar = selected ? (
    <BuildSettingsSummary
      leading={(
        <BlueprintSummaryCell
          cellLabel={selected.kind === "manufacturing" ? "Blueprint" : "Recipe"}
          label={selected.kind === "reaction"
            ? selected.result.reactionFormulaName
            : blueprintMode === "observedAsset"
              ? (() => {
                  const observation = observedBlueprints.find((item) => item.id === selectedObservationId);
                  return observation
                    ? `${observation.blueprintName} · ME ${observation.materialEfficiency} · TE ${observation.timeEfficiency}`
                    : "Available blueprint";
                })()
              : `${selectedName ?? "Manual blueprint"} · ME ${blueprintMe} · TE ${blueprintTe}`}
          onEdit={selected.kind === "manufacturing" ? () => setBlueprintDialogOpen(true) : undefined}
        />
      )}
      manufacturingFacilities={manufacturing.facilities}
      manufacturingFacilityId={manufacturing.facilityId}
      materialPolicy={materialPricingPolicy}
      materialScope={materialScope}
      outputPolicy={outputPricingPolicy}
      outputScope={outputScope}
      reactionFacilities={reaction.facilities}
      reactionFacilityId={reaction.facilityId}
      rootFacilityRole={selected.kind}
      sourceId={sourceId}
      sources={sources}
      updating={previewPending || previewUpdating}
      onEdit={openBuildSettings}
    />
  ) : null;

  return (
    <>
      {selected ? <BuildPageHeader
        typeId={focusedProducer
          ? focusedProducer.recipe.kind === "manufacturing"
            ? focusedProducer.recipe.blueprintTypeId
            : focusedProducer.recipe.reactionFormulaTypeId
          : selected.kind === "manufacturing" ? selected.result.blueprintTypeId : selected.result.reactionFormulaTypeId}
        typeName={focusedProducer?.name ?? selectedName ?? ""}
        variation="bp"
        name={focusedProducer?.name ?? name}
        onNameChange={(nextName) => {
          if (focusedProducer) return;
          if (!initialBuild) {
            setName(nextName);
            return;
          }
          setError("");
          void renameBuild(initialBuild.id, { name: nextName })
            .then((renamed) => {
              setName(renamed.name);
              setBuildRevision(renamed.revision);
            })
            .catch((requestError) => setError(apiMessage(requestError)));
        }}
      >
        <span className="flex flex-wrap items-center gap-2">
          {initialBuild ? <StatusBadge>Draft</StatusBadge> : null}
          <span>{focusedProducer
            ? `Part of ${initialBuild?.name ?? "the top-level Build"}. Demand, runs, costs, and dependencies stay synchronized with that Build.`
            : selected.kind === "manufacturing"
            ? "Capture a real manufacturing recipe, runs, and explicit price assumptions."
            : "Capture a real reaction formula, runs, and explicit price assumptions."}</span>
          {saveStatus === "saving" ? <span className="text-xs text-muted" role="status">Saving...</span> : null}
          {saveStatus === "saved" ? <span className="text-xs text-muted" role="status">All changes saved</span> : null}
        </span>
      </BuildPageHeader> : (
        <PageHeader eyebrow="Build" title="Create Build">
          Capture a real manufacturing recipe or reaction formula, runs, and explicit price assumptions.
        </PageHeader>
      )}
      {initialBuild && sdeReady && !focusedProducer ? (
        // Level 1 -- the Build itself: runs, whole-Build economics and
        // warnings, root settings. Persistent across Plan / Logistics /
        // Graph so every planning change's economic effect stays in view.
        <BuildWorkspaceSummary
          identity={identity}
          toolbar={settingsToolbar}
          blueprintDialog={blueprintDialog}
          notes={<Field label="Notes" value={notes} onChange={setNotes} multiline />}
          statusMessage={previewSyncMessage}
          updating={previewUpdating}
        />
      ) : null}
      {initialBuild && sdeReady && focusedProducer ? (
        <div className="mb-3">
          <FocusedProducerSummary editor={editor} producer={focusedProducer} />
        </div>
      ) : null}
      {initialBuild ? (
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <div
            aria-label="Build view"
            className="inline-flex max-w-full overflow-x-auto rounded border border-border p-0.5 text-sm"
            role="tablist"
          >
            {BUILD_VIEWS.map((tab) => (
              <button
                aria-selected={view === tab}
                className={
                  view === tab
                    ? "shrink-0 rounded bg-panel px-3 py-1 font-medium text-foreground"
                    : "shrink-0 px-3 py-1 text-muted"
                }
                key={tab}
                onClick={() => setView(tab)}
                role="tab"
                type="button"
              >
                {VIEW_LABELS[tab]}
              </button>
            ))}
          </div>
          {!focusedProducer ? <div className="flex flex-wrap items-center gap-2">
            <button
              className="iw-button-secondary"
              disabled={exportingWorkbook || !editor.previewKey}
              onClick={() => void handleExportVerification()}
              type="button"
            >
              {exportingWorkbook ? "Exporting..." : "Export verification workbook"}
            </button>
            <button
              className="iw-button-secondary"
              disabled={epicCommand !== null}
              onClick={handleCreateOrder}
              type="button"
            >
              Create Epic
            </button>
          </div> : null}
        </div>
      ) : null}
      {orderError ? <InlineAlert title="Epic not created">{orderError}</InlineAlert> : null}
      {initialBuild ? (
        <CreateEpicDialog
          buildId={initialBuild.id}
          command={epicCommand}
          onCancel={() => setEpicCommand(null)}
          onCreated={handleEpicCreated}
          open={epicCommand !== null}
        />
      ) : null}
      {exportError ? (
        <InlineAlert title="Verification workbook not generated">{exportError}</InlineAlert>
      ) : null}
      {error && !editor.initializingExistingBuild ? (
        <InlineAlert title="Build not saved">{error}</InlineAlert>
      ) : null}
      {sdeReady === false ? (
        <Panel><EmptyState title="No SDE imported">Import an SDE before creating a Build.</EmptyState></Panel>
      ) : null}
      {sdeReady && initialBuild ? (
        // Mounted-but-hidden so Graph selection, collapsed branches and
        // pan/zoom survive a tab round trip. `active` gates fetching and
        // the right-rail inspector portal.
        <div hidden={view !== "graph"}>
          <BuildGraphView active={view === "graph"} editor={editor} focusedProducerId={focusedProducer?.id} />
        </div>
      ) : null}
      {sdeReady && initialBuild ? (
        <div hidden={view !== "plan"}>
          <BuildStagesView active={view === "plan"} editor={editor} focusedProducerId={focusedProducer?.id} />
        </div>
      ) : null}
      {sdeReady && initialBuild ? (
        <div hidden={view !== "logistics"}>
          <BuildLogisticsView active={view === "logistics"} editor={editor} focusedProducerId={focusedProducer?.id} />
        </div>
      ) : null}
      {sdeReady && initialBuild ? (
        <div hidden={view !== "worksheet"}>
          <BuildWorksheetView active={view === "worksheet"} editor={editor} focusedProducerId={focusedProducer?.id} />
        </div>
      ) : null}
      {
        // The Build settings inspector (right rail). For a saved Build it
        // is only ever in `buildSettings` mode -- the per-row worksheet
        // inspector belongs to the create flow below.
      }
      {sdeReady && !focusedProducer ? <BuildInspector editor={editor} /> : null}
      {sdeReady && !initialBuild ? (
        // The /builds/new create flow keeps the configuration worksheet:
        // there is no plan to show until the Build exists.
        <div className="mt-4">
          <BuildPlanner
            mode="create"
            identity={identity}
            assumptions={blueprintDialog}
            toolbar={settingsToolbar}
            preview={estimate
              ? <CreateCandidateResults
                  onSelectRow={selectWorksheetRow}
                  preview={estimate}
                  selectedRowKey={inspectorMode.kind === "selectedItem" ? inspectorMode.rowKey : null}
                />
              : <div className="border-y border-border py-5 text-sm text-muted">{previewUpdating ? "Updating preview..." : "Candidate results will appear here."}</div>}
            notes={<Field label="Notes" value={notes} onChange={setNotes} multiline />}
            workbench={{
              editorError: error,
              previewUpdating,
              statusMessage: previewSyncMessage,
            }}
          />
        </div>
      ) : null}
    </>
  );
}
