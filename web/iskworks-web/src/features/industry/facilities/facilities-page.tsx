import { Download, Plus, Search, Upload } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import {
  deleteFacility,
  exportFacilities,
  listFacilities,
  type FacilityProfile,
} from "../../../api/industry";
import { ConfirmDialog, EmptyState, InlineAlert, PageHeader, Panel } from "../../../components/primitives";
import { apiMessage } from "../shared/api-error";
import { FacilityCard } from "./facility-card";
import {
  EMPTY_FACILITY_FILTERS,
  FACILITY_ACTIVITY_FILTERS,
  facilityMatchesFilters,
  filteredFacilityEmptyMessage,
  hasActiveFacilityFilters,
  type FacilityFilterState,
} from "./facility-filters";
import { FacilityFormDialog } from "./facility-form-dialog";
import { ImportFacilitiesModal } from "./import-facilities-modal";

export function FacilitiesPage() {
  const [facilities, setFacilities] = useState<FacilityProfile[]>([]);
  const [editing, setEditing] = useState<FacilityProfile | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [error, setError] = useState("");
  const [exportError, setExportError] = useState("");
  const [importOpen, setImportOpen] = useState(false);
  const [deleting, setDeleting] = useState<FacilityProfile | null>(null);
  const [filters, setFilters] = useState<FacilityFilterState>(EMPTY_FACILITY_FILTERS);

  function load() {
    listFacilities().then(setFacilities).catch((requestError) => setError(apiMessage(requestError)));
  }

  useEffect(load, []);

  async function handleExport() {
    setExportError("");
    try {
      const data = await exportFacilities();
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = `iskworks-facilities-${new Date().toISOString().slice(0, 10)}.json`;
      link.click();
      URL.revokeObjectURL(url);
    } catch (requestError) {
      setExportError(apiMessage(requestError));
    }
  }

  function closeForm() {
    setFormOpen(false);
    setEditing(null);
  }

  function create() {
    setError("");
    setEditing(null);
    setFormOpen(true);
  }

  function edit(profile: FacilityProfile) {
    setError("");
    setEditing(profile);
    setFormOpen(true);
  }

  async function remove(profile: FacilityProfile) {
    try {
      await deleteFacility(profile.id, profile.revision);
      if (editing?.id === profile.id) closeForm();
      setDeleting(null);
      load();
    } catch (requestError) {
      setError(apiMessage(requestError));
    }
  }

  const activeFacilities = useMemo(
    () => facilities.filter((profile) => !profile.archivedAt),
    [facilities],
  );
  const visibleFacilities = useMemo(
    () => activeFacilities.filter((profile) => facilityMatchesFilters(profile, filters)),
    [activeFacilities, filters],
  );
  const filtersActive = hasActiveFacilityFilters(filters);

  return (
    <>
      <PageHeader eyebrow="Industry assumptions" title="Facilities">
        Reusable manufacturing locations and modifiers. Existing Build snapshots remain unchanged when profiles are edited.
      </PageHeader>
      {error ? <InlineAlert title="Facility action unavailable">{error}</InlineAlert> : null}
      {exportError ? <InlineAlert title="Export failed">{exportError}</InlineAlert> : null}
      <div className="mt-4 flex flex-wrap justify-end gap-2">
        <button className="iw-button-secondary" onClick={handleExport} type="button">
          <Download aria-hidden="true" className="mr-2 h-4 w-4" />
          Export
        </button>
        <button className="iw-button-secondary" onClick={() => setImportOpen(true)} type="button">
          <Upload aria-hidden="true" className="mr-2 h-4 w-4" />
          Import
        </button>
        <button className="iw-button-primary" onClick={create} type="button">
          <Plus className="mr-2 h-4 w-4" />
          New Facility
        </button>
      </div>
      {importOpen ? <ImportFacilitiesModal onClose={() => setImportOpen(false)} onImported={load} /> : null}

      {activeFacilities.length > 0 ? (
        <div className="mt-4 flex flex-wrap items-center gap-2">
          <div
            aria-label="Filter by activity"
            className="flex overflow-hidden rounded-md border border-border text-xs"
            role="tablist"
          >
            {FACILITY_ACTIVITY_FILTERS.map((option) => (
              <button
                aria-selected={filters.activity === option.id}
                className={`px-2.5 py-1 font-semibold ${
                  filters.activity === option.id ? "bg-primary/15 text-primary" : "text-muted"
                }`}
                key={option.id}
                onClick={() => setFilters((current) => ({ ...current, activity: option.id }))}
                role="tab"
                type="button"
              >
                {option.label}
              </button>
            ))}
          </div>
          <label className="relative">
            <Search aria-hidden="true" className="pointer-events-none absolute left-2 top-1/2 h-4 w-4 -translate-y-1/2 text-muted" />
            <span className="sr-only">Search facilities</span>
            <input
              className="iw-input w-56 pl-8"
              onChange={(event) => setFilters((current) => ({ ...current, search: event.target.value }))}
              placeholder="Search facilities…"
              type="search"
              value={filters.search}
            />
          </label>
          {filtersActive ? (
            <button
              className="iw-button-secondary"
              onClick={() => setFilters(EMPTY_FACILITY_FILTERS)}
              type="button"
            >
              Clear filters
            </button>
          ) : null}
        </div>
      ) : null}

      <div className="mt-4">
        {activeFacilities.length === 0 ? (
          <Panel>
            <EmptyState
              action={<button className="iw-button-primary" onClick={create} type="button">Create Facility</button>}
              title="No Facility Profiles"
            >
              Create a Facility Profile to compare manufacturing assumptions.
            </EmptyState>
          </Panel>
        ) : visibleFacilities.length === 0 ? (
          <Panel>
            <EmptyState
              action={
                <button className="iw-button-secondary" onClick={() => setFilters(EMPTY_FACILITY_FILTERS)} type="button">
                  Reset filters
                </button>
              }
              title="No facilities match"
            >
              {filteredFacilityEmptyMessage(filters)}
            </EmptyState>
          </Panel>
        ) : (
          <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
            {visibleFacilities.map((profile) => (
              <FacilityCard key={profile.id} onDelete={setDeleting} onEdit={edit} profile={profile} />
            ))}
          </div>
        )}
      </div>
      <FacilityFormDialog
        facility={editing}
        onClose={closeForm}
        onSaved={() => {
          closeForm();
          load();
        }}
        open={formOpen}
      />
      <ConfirmDialog
        confirmLabel="Delete Facility"
        onCancel={() => setDeleting(null)}
        onConfirm={() => { if (deleting) void remove(deleting); }}
        open={deleting !== null}
        title="Delete this Facility?"
      >
        Delete this Facility profile permanently? Saved Build history keeps its captured Facility details. Drafts using this Facility will return to “Facility not selected.”
      </ConfirmDialog>
    </>
  );
}
