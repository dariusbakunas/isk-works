import { X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { deleteBuild, listBuilds, type Build } from "../../../api/industry";
import type { BlueprintSearchResult, ReactionFormulaSearchResult } from "../../../api/sde";
import { EveTypeImage } from "../../../components/eve-type-image";
import {
  ButtonLink,
  ConfirmDialog,
  EmptyState,
  InlineAlert,
  PageHeader,
  Panel,
} from "../../../components/primitives";
import { apiMessage } from "../shared/api-error";
import { Field } from "../shared/field";
import {
  activeFilterChips,
  categoryOptions,
  DEFAULT_CRITERIA,
  filterAndSortBuilds,
  hasActiveFilters,
  type BuildLibraryCriteria,
} from "./build-library-filters";
import { BuildLibraryCard } from "./components/build-library-card";
import { BuildLibraryToolbar } from "./components/build-library-toolbar";

type AsyncState<T> =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; data: T };

export function BuildsPage() {
  const [state, setState] = useState<AsyncState<Build[]>>({ status: "loading" });
  const [criteria, setCriteria] = useState<BuildLibraryCriteria>(DEFAULT_CRITERIA);
  const [deleteTarget, setDeleteTarget] = useState<Build | null>(null);
  const [deleteError, setDeleteError] = useState("");

  useEffect(() => {
    listBuilds()
      .then((data) => setState({ status: "ready", data }))
      .catch((error) => setState({ status: "error", message: apiMessage(error) }));
  }, []);

  const builds = state.status === "ready" ? state.data : [];
  const categories = useMemo(() => categoryOptions(builds), [builds]);
  const visibleBuilds = useMemo(() => filterAndSortBuilds(builds, criteria), [builds, criteria]);
  const filtersActive = hasActiveFilters(criteria);

  function patchCriteria(patch: Partial<BuildLibraryCriteria>) {
    setCriteria((current) => ({ ...current, ...patch }));
  }

  function clearFilters() {
    setCriteria(DEFAULT_CRITERIA);
  }

  async function removeBuild(build: Build) {
    try {
      await deleteBuild(build.id, build.revision);
      // Refetch rather than patch local state -- deleting a build with its
      // own linked children cascades server-side.
      const data = await listBuilds();
      setState({ status: "ready", data });
      setDeleteError("");
    } catch (requestError) {
      setDeleteError(apiMessage(requestError));
    }
  }

  return (
    <>
      <PageHeader eyebrow="Production planning" title="Builds">
        Durable build intentions, captured recipes, and retained planning assumptions.
      </PageHeader>

      {state.status === "loading" ? <Panel>Loading Builds...</Panel> : null}
      {state.status === "error" ? (
        <InlineAlert title="Builds unavailable">{state.message}</InlineAlert>
      ) : null}
      {deleteError ? <InlineAlert title="Delete failed">{deleteError}</InlineAlert> : null}

      {state.status === "ready" && builds.length === 0 ? (
        <Panel>
          <EmptyState
            action={
              <ButtonLink to="/builds/new" variant="primary">
                Create your first build
              </ButtonLink>
            }
            title="No builds yet"
          >
            A build captures a manufacturing recipe, a run count, and the planning assumptions
            that flow into Epics and the Board.
          </EmptyState>
        </Panel>
      ) : null}

      {state.status === "ready" && builds.length > 0 ? (
        <>
          <BuildLibraryToolbar
            categories={categories}
            criteria={criteria}
            filtersActive={filtersActive}
            matchCount={visibleBuilds.length}
            onChange={patchCriteria}
            onClear={clearFilters}
            totalCount={builds.length}
          />

          {visibleBuilds.length === 0 ? (
            <Panel>
              <EmptyState
                action={
                  <button className="iw-button-secondary" onClick={clearFilters} type="button">
                    Clear all filters
                  </button>
                }
                title="No builds match these filters"
              >
                <span className="flex flex-wrap gap-1.5">
                  {activeFilterChips(criteria).map((chip) => (
                    <span className="iw-badge" key={chip}>
                      {chip}
                    </span>
                  ))}
                </span>
              </EmptyState>
            </Panel>
          ) : (
            <div className="grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(15rem,1fr))]">
              {visibleBuilds.map((build) => (
                <BuildLibraryCard build={build} key={build.id} onDelete={setDeleteTarget} />
              ))}
            </div>
          )}
        </>
      ) : null}

      <ConfirmDialog
        confirmLabel="Delete Build"
        onCancel={() => setDeleteTarget(null)}
        onConfirm={() => {
          const target = deleteTarget;
          setDeleteTarget(null);
          if (target) void removeBuild(target);
        }}
        open={deleteTarget !== null}
        title="Delete this Build?"
      >
        This removes the Build and its production plan. Tickets and Epics created from it will remain. This
        cannot be undone.
      </ConfirmDialog>
    </>
  );
}

export type ProductSearchResult =
  | { kind: "manufacturing"; result: BlueprintSearchResult }
  | { kind: "reaction"; result: ReactionFormulaSearchResult };

export function ProductSelectionDialog({
  open,
  query,
  results,
  searching,
  onCancel,
  onQuery,
  onSelect,
}: {
  open: boolean;
  query: string;
  results: ProductSearchResult[];
  searching: boolean;
  onCancel: () => void;
  onQuery: (value: string) => void;
  onSelect: (entry: ProductSearchResult) => void;
}) {
  useEffect(() => {
    if (!open) return;
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onCancel();
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onCancel, open]);

  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/75 p-4" role="presentation">
      <section
        aria-labelledby="product-selection-title"
        aria-modal="true"
        className="iw-dialog w-full max-w-2xl p-4"
        role="dialog"
      >
        <div className="flex items-start justify-between gap-3">
          <div>
            <p className="iw-eyebrow">New build</p>
            <h2 className="mt-1 text-lg font-semibold" id="product-selection-title">Select a product</h2>
            <p className="iw-muted mt-1">Choose the item you intend to manufacture or react.</p>
          </div>
          <button className="iw-icon-button" aria-label="Cancel new build" onClick={onCancel} title="Cancel" type="button">
            <X size={18} />
          </button>
        </div>
        <Field
          autoFocus
          className="mt-3"
          label="Product"
          value={query}
          onChange={onQuery}
        />
        {searching ? <p className="iw-muted mt-2 text-xs" role="status">Searching active SDE...</p> : null}
        {results.length > 0 ? (
          <div className="mt-3 max-h-[50vh] overflow-y-auto divide-y divide-border border-y border-border">
            {results.map((entry) => (
              <button
                className="flex w-full items-center gap-2 px-2 py-2 text-left hover:bg-panel-strong"
                key={`${entry.kind}-${entry.kind === "manufacturing" ? entry.result.blueprintTypeId : entry.result.reactionFormulaTypeId}`}
                onClick={() => onSelect(entry)}
                type="button"
              >
                <EveTypeImage size={40} typeId={entry.result.productTypeId} typeName={entry.result.productName} />
                <span className="grid min-w-0 flex-1 gap-1 sm:grid-cols-2">
                  <strong>{entry.result.productName}</strong>
                  <span className="text-sm text-muted">
                    {entry.kind === "manufacturing" ? entry.result.blueprintName : entry.result.reactionFormulaName}
                    {" · "}
                    {entry.kind === "manufacturing" ? "Manufacturing" : "Reaction"}
                  </span>
                </span>
              </button>
            ))}
          </div>
        ) : query.trim() && !searching ? (
          <p className="iw-muted mt-4 border-y border-border py-4 text-sm">No results found.</p>
        ) : null}
        <div className="mt-3 flex justify-end">
          <button className="iw-button-secondary" onClick={onCancel} type="button">Cancel</button>
        </div>
      </section>
    </div>
  );
}
