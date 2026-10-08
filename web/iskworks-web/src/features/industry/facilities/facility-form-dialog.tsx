import { Plus, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  createFacility,
  getSystemCostIndex,
  resolveKnownStructure,
  searchKnownStructures,
  searchNpcStations,
  searchSolarSystems,
  updateFacility,
  type FacilityInput,
  type FacilityProfile,
  type KnownStructure,
} from "../../../api/industry";
import { getStructureManufacturingModifiers, searchStructureTypes } from "../../../api/sde";
import { ButtonLink, InlineAlert } from "../../../components/primitives";
import { MoneyInput, type MoneyInputResult } from "../../../components/money-input";
import { apiMessage } from "../shared/api-error";
import { parseStructureReference } from "../shared/structure-reference";
import { Field } from "../shared/field";
import { useDebouncedLookup } from "../../../hooks/use-debounced-lookup";
import { RigTypeSelector } from "./rig-type-selector";

// The structure-derived reduction percentages: reset to this whenever the
// facility kind, role, or selected structure/type changes, before (where
// applicable) an async lookup fills in the structure's real modifiers.
const zeroedStructureModifiers = {
  materialReductionPercent: "0",
  timeReductionPercent: "0",
  jobCostReductionPercent: "0",
};

const emptyFacility: FacilityInput = {
  name: "",
  kind: "manual",
  role: "manufacturing",
  structureId: null,
  structureTypeId: null,
  structureTypeName: "",
  solarSystemId: null,
  solarSystemName: "",
  securityClass: "unknown",
  materialReductionPercent: "0",
  timeReductionPercent: "0",
  jobCostReductionPercent: "0",
  facilityTaxPercent: "0",
  sccSurchargePercent: "4",
  allianceSurchargePercent: "0",
  fixedSupplementalCost: "0",
  manualSystemCostIndex: null,
  notes: "",
  rigs: [],
};

function facilityInput(profile: FacilityProfile): FacilityInput {
  return {
    name: profile.name,
    kind: profile.kind,
    role: profile.role,
    structureId: profile.structureId,
    structureTypeId: profile.structureTypeId,
    structureTypeName: profile.structureTypeName,
    solarSystemId: profile.solarSystemId,
    solarSystemName: profile.solarSystemName,
    securityClass: profile.securityClass,
    materialReductionPercent: profile.materialReductionPercent,
    timeReductionPercent: profile.timeReductionPercent,
    jobCostReductionPercent: profile.jobCostReductionPercent,
    facilityTaxPercent: profile.facilityTaxPercent,
    sccSurchargePercent: profile.sccSurchargePercent,
    allianceSurchargePercent: profile.allianceSurchargePercent,
    fixedSupplementalCost: profile.fixedSupplementalCost,
    manualSystemCostIndex: profile.manualSystemCostIndex,
    notes: profile.notes,
    rigs: profile.rigs,
  };
}

export interface FacilityFormDialogProps {
  facility: FacilityProfile | null;
  open: boolean;
  onClose: () => void;
  onSaved: (profile: FacilityProfile) => void;
}

export function FacilityFormDialog({ facility, open, onClose, onSaved }: FacilityFormDialogProps) {
  const formDialog = useRef<HTMLDialogElement>(null);
  const [form, setForm] = useState<FacilityInput>(emptyFacility);
  // Live classification of the one Money field in this dialog. An empty
  // field means zero (the backend's own `parse_facility` default); Save is
  // blocked only while the text is syntactically invalid.
  const [fixedCostResult, setFixedCostResult] = useState<MoneyInputResult>({
    status: "valid",
    canonical: "0",
  });
  const [saving, setSaving] = useState(false);
  const [structureQuery, setStructureQuery] = useState("");
  const [stationQuery, setStationQuery] = useState("");
  const [stationOpen, setStationOpen] = useState(false);
  const [knownStructureOpen, setKnownStructureOpen] = useState(false);
  const [resolvingStructure, setResolvingStructure] = useState(false);
  const [structureNeedsReconnection, setStructureNeedsReconnection] = useState(false);
  const [structureTypeOpen, setStructureTypeOpen] = useState(false);
  const [costIndexState, setCostIndexState] = useState<{
    status: "idle" | "loading" | "esi" | "manual" | "unavailable";
    fetchedAt?: string;
    expiresAt?: string;
  }>({ status: "idle" });
  const [error, setError] = useState("");
  useEffect(() => {
    const element = formDialog.current;
    if (!element) return;
    if (open && !element.open) {
      if (typeof element.showModal === "function") element.showModal();
      else element.setAttribute("open", "");
    }
    if (!open && element.open) {
      if (typeof element.close === "function") element.close();
      else element.removeAttribute("open");
    }
  }, [open]);
  const solarSystemLookup = useDebouncedLookup(
    form.solarSystemName,
    searchSolarSystems,
    (requestError) => setError(apiMessage(requestError)),
    { enabled: open && !form.solarSystemId },
  );
  const stationLookup = useDebouncedLookup(
    stationQuery,
    searchNpcStations,
    (requestError) => setError(apiMessage(requestError)),
    { enabled: open && form.kind === "npcStation" && stationOpen && !form.structureId, minLength: 0 },
  );
  useEffect(() => {
    if (!open || !form.solarSystemId) {
      setCostIndexState({ status: "idle" });
      return;
    }
    let cancelled = false;
    setCostIndexState({ status: "loading" });
    getSystemCostIndex(form.solarSystemId)
      .then((index) => {
        if (cancelled) return;
        setForm((current) => current.solarSystemId === index.solarSystemId
          ? { ...current, manualSystemCostIndex: current.role === "reaction" ? index.reaction : index.manufacturing }
          : current);
        setCostIndexState({
          status: "esi",
          fetchedAt: index.fetchedAt,
          expiresAt: index.expiresAt,
        });
      })
      .catch(() => {
        if (!cancelled) setCostIndexState({ status: "unavailable" });
      });
    return () => {
      cancelled = true;
    };
  }, [form.solarSystemId, form.role, open]);
  const knownStructureLookup = useDebouncedLookup(
    structureQuery,
    searchKnownStructures,
    (requestError) => setError(apiMessage(requestError)),
    { enabled: open && form.kind === "upwellStructure" && knownStructureOpen && !form.structureId, minLength: 0 },
  );
  const structureTypeLookup = useDebouncedLookup(
    form.structureTypeName,
    searchStructureTypes,
    (requestError) => setError(apiMessage(requestError)),
    { enabled: open && form.kind === "upwellStructure" && structureTypeOpen && !form.structureTypeId, minLength: 0 },
  );

  function closeForm() {
    if (saving) return;
    setKnownStructureOpen(false);
    setStructureQuery("");
    setStationOpen(false);
    setStationQuery("");
    setStructureTypeOpen(false);
    setForm(emptyFacility);
    setCostIndexState({ status: "idle" });
    setStructureQuery("");
    setStationQuery("");
    onClose();
  }

  useEffect(() => {
    if (!open) return;
    setError("");
    setForm(facility ? facilityInput(facility) : emptyFacility);
    setCostIndexState({ status: "idle" });
    setStructureQuery("");
    setStationQuery("");
    if (facility?.structureId) {
      const lookup = facility.kind === "npcStation"
        ? searchNpcStations("").then((stations) => {
            const station = stations.find((item) => item.stationId === facility.structureId);
            if (station) setStationQuery(station.stationName);
          })
        : searchKnownStructures("").then((structures) => {
            const structure = structures.find((item) => item.structureId === facility.structureId);
            if (structure) setStructureQuery(structure.structureName);
          });
      lookup.catch(() => {});
    }
  }, [facility, open]);

  async function save() {
    if (form.kind === "upwellStructure" && !form.structureId) {
      setError("Select a resolved ESI structure from the search results.");
      return;
    }
    if (form.kind === "npcStation" && !form.structureId) {
      setError("Select an NPC station from the search results.");
      return;
    }
    if (!form.solarSystemId) {
      setError("Select a solar system from the search results.");
      return;
    }
    const fixedSupplementalCost =
      fixedCostResult.status === "valid"
        ? fixedCostResult.canonical ?? "0"
        : fixedCostResult.status === "empty"
          ? "0"
          : null;
    if (fixedSupplementalCost === null) {
      setError("Enter a valid fixed supplemental cost.");
      return;
    }
    const payload: FacilityInput = { ...form, fixedSupplementalCost };
    setSaving(true);
    setError("");
    try {
      const saved = facility
        ? await updateFacility(facility.id, facility.revision, payload)
        : await createFacility(payload);
      setForm(emptyFacility);
      setStructureQuery("");
      setStationQuery("");
      onSaved(saved);
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setSaving(false);
    }
  }

  function applyKnownStructure(structure: KnownStructure) {
    setStructureQuery(structure.structureName);
    setKnownStructureOpen(false);
    setForm((current) => ({
      ...current,
      structureId: structure.structureId,
      structureTypeId: structure.structureTypeId ?? current.structureTypeId,
      structureTypeName: structure.structureTypeName ?? current.structureTypeName,
      solarSystemId: structure.solarSystemId,
      solarSystemName: structure.solarSystemName ?? current.solarSystemName,
      securityClass: structure.securityClass,
    }));
    if (structure.structureTypeId) {
      getStructureManufacturingModifiers(structure.structureTypeId)
        .then((modifiers) => setForm((current) => ({
          ...current,
          materialReductionPercent: modifiers.materialReductionPercent,
          timeReductionPercent: modifiers.timeReductionPercent,
          jobCostReductionPercent: modifiers.jobCostReductionPercent,
        })))
        .catch((requestError) => setError(apiMessage(requestError)));
    }
  }

  async function resolveStructureById(structureId: number) {
    setError("");
    setStructureNeedsReconnection(false);
    setResolvingStructure(true);
    try {
      const resolution = await resolveKnownStructure(structureId);
      if (!resolution.configured) {
        setError("EVE SSO is not configured, so structures can't be resolved by ID.");
        return;
      }
      if (resolution.structure) {
        applyKnownStructure(resolution.structure);
        return;
      }
      if (resolution.needsReconnection) {
        setStructureNeedsReconnection(true);
        setError("Reconnect an EVE character with structure read access to resolve this ID.");
        return;
      }
      setError(
        resolution.warnings[0]
          ?? "Could not resolve this structure. Make sure a connected character has docking access.",
      );
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setResolvingStructure(false);
    }
  }

  const set = (key: keyof FacilityInput, value: string) =>
    setForm((current) => ({ ...current, [key]: value }));
  function addRig() {
    const slotNumber = [1, 2, 3].find((slot) => !form.rigs.some((rig) => rig.slotNumber === slot));
    if (!slotNumber) return;
    setForm((current) => ({
      ...current,
      rigs: [...current.rigs, {
        slotNumber,
        typeId: 0,
        typeName: "",
        materialReductionPercent: "0",
        timeReductionPercent: "0",
      }],
    }));
  }
  function updateRig(slotNumber: number, patch: Partial<FacilityInput["rigs"][number]>) {
    setForm((current) => ({
      ...current,
      rigs: current.rigs.map((rig) => rig.slotNumber === slotNumber ? { ...rig, ...patch } : rig),
    }));
  }
  return (
    <dialog
        aria-labelledby="facility-form-title"
        className="iw-dialog m-auto max-h-[calc(100vh-2rem)] w-[min(760px,calc(100vw-2rem))] overflow-y-auto p-0 text-foreground backdrop:bg-black/70"
        onCancel={(event) => {
          event.preventDefault();
          closeForm();
        }}
        ref={formDialog}
      >
        {open ? <div className="p-5">
          <div className="flex items-start justify-between gap-3">
            <div>
              <p className="iw-eyebrow">Industry assumptions</p>
              <h2 className="text-base font-semibold" id="facility-form-title">{facility ? "Edit Facility" : "New Facility"}</h2>
            </div>
            <button aria-label="Close Facility form" className="iw-icon-button" disabled={saving} onClick={closeForm} title="Close" type="button"><X className="h-4 w-4" /></button>
          </div>
          {error ? <div className="mt-4"><InlineAlert title="Facility action unavailable">{error}</InlineAlert></div> : null}
          <div className="mt-4 grid gap-3 sm:grid-cols-2">
            <div className="sm:col-span-2"><Field label="Name" value={form.name} onChange={(value) => set("name", value)} /></div>
            <label className="text-sm">Kind<select className="iw-input mt-1" value={form.kind} onChange={(event) => {
              const kind = event.target.value as FacilityInput["kind"];
              setStructureQuery("");
              setStationQuery("");
              setKnownStructureOpen(false);
              setStationOpen(false);
              setForm((current) => ({
                ...current,
                kind,
                structureId: null,
                structureTypeId: null,
                structureTypeName: "",
                ...zeroedStructureModifiers,
              }));
            }}><option value="manual">Manual</option><option value="npcStation">NPC Station</option><option value="upwellStructure">Upwell Structure</option></select></label>
            <label className="text-sm">Role<select className="iw-input mt-1" value={form.role} onChange={(event) => {
              const role = event.target.value as FacilityInput["role"];
              setForm((current) => ({
                ...current,
                role,
                rigs: [],
                ...zeroedStructureModifiers,
              }));
            }}><option value="manufacturing">Manufacturing</option><option value="reaction">Reaction</option></select></label>
            <label className="text-sm">Security<select className="iw-input mt-1" value={form.securityClass} onChange={(event) => setForm((current) => ({ ...current, securityClass: event.target.value as FacilityInput["securityClass"] }))}><option value="unknown">Unknown</option><option value="highSec">High sec</option><option value="lowSec">Low sec</option><option value="nullSec">Null sec</option><option value="wormhole">Wormhole</option></select></label>
            {form.kind === "upwellStructure" ? (
              <>
                <div className="relative">
                  <Field
                    label="Structure"
                    value={structureQuery}
                    onChange={(value) => {
                      setStructureQuery(value);
                      setKnownStructureOpen(true);
                      setForm((current) => ({ ...current, structureId: null }));
                    }}
                    onFocus={() => setKnownStructureOpen(true)}
                  />
                  {knownStructureLookup.searching ? <p className="iw-muted mt-1 text-xs">Searching resolved ESI structures...</p> : null}
                  {!knownStructureLookup.searching && knownStructureOpen && !form.structureId ? (
                    <div className="absolute z-40 mt-1 max-h-56 w-full overflow-y-auto border border-border bg-panel shadow-xl">
                      {knownStructureLookup.results.length > 0 ? knownStructureLookup.results.map((structure) => (
                        <button
                          className="block w-full px-3 py-2 text-left hover:bg-panel-strong"
                          key={structure.structureId}
                          onClick={() => applyKnownStructure(structure)}
                          type="button"
                        >
                          <span className="block text-sm">{structure.structureName}</span>
                          <span className="iw-muted block text-xs">
                            {[structure.structureTypeName, structure.solarSystemName].filter(Boolean).join(" · ") || "Resolved through ESI"}
                          </span>
                        </button>
                      )) : (
                        <div className="px-3 py-2">
                          <p className="iw-muted text-sm">
                            No resolved structures found. Sync assets or blueprints for a character with access to this structure.
                          </p>
                          {(() => {
                            const referencedId = parseStructureReference(structureQuery);
                            if (!referencedId) {
                              return (
                                <p className="iw-muted mt-2 text-xs">
                                  Have docking access but never synced assets there? Shift-drag the
                                  structure into chat, mail, or a note in the EVE client to get a
                                  Show Info link (<code>showinfo:...</code>), then paste it here.
                                </p>
                              );
                            }
                            return (
                              <button
                                className="iw-button-secondary mt-2"
                                disabled={resolvingStructure}
                                onClick={() => void resolveStructureById(referencedId)}
                                type="button"
                              >
                                {resolvingStructure ? "Resolving via ESI..." : `Resolve structure ${referencedId} via ESI`}
                              </button>
                            );
                          })()}
                          {structureNeedsReconnection ? (
                            <p className="mt-2 text-xs">
                              <ButtonLink to="/characters">Open Characters</ButtonLink>
                            </p>
                          ) : null}
                        </div>
                      )}
                    </div>
                  ) : null}
                  {form.structureId ? <p className="iw-muted mt-1 text-xs">Selected from resolved ESI structures</p> : null}
                </div>
                <div className="relative">
                  <Field
                    label="Structure type"
                    value={form.structureTypeName}
                    onChange={(value) => {
                      setStructureTypeOpen(true);
                      setForm((current) => ({
                        ...current,
                        structureTypeId: null,
                        structureTypeName: value,
                      }));
                    }}
                    onFocus={() => setStructureTypeOpen(true)}
                  />
                  {structureTypeLookup.searching ? <p className="iw-muted mt-1 text-xs">Searching active SDE...</p> : null}
                  {!structureTypeLookup.searching && structureTypeOpen && !form.structureTypeId ? (
                    <div className="absolute z-30 mt-1 max-h-56 w-full overflow-y-auto border border-border bg-panel shadow-xl">
                      {structureTypeLookup.results.length > 0 ? structureTypeLookup.results.map((type) => (
                        <button
                          className="block w-full px-3 py-2 text-left hover:bg-panel-strong"
                          key={type.typeId}
                          onClick={() => {
                            setForm((current) => ({
                              ...current,
                              structureTypeId: type.typeId,
                              structureTypeName: type.typeName,
                              ...zeroedStructureModifiers,
                            }));
                            setStructureTypeOpen(false);
                            getStructureManufacturingModifiers(type.typeId)
                              .then((modifiers) => setForm((current) => ({
                                ...current,
                                materialReductionPercent: modifiers.materialReductionPercent,
                                timeReductionPercent: modifiers.timeReductionPercent,
                                jobCostReductionPercent: modifiers.jobCostReductionPercent,
                              })))
                              .catch((requestError) => setError(apiMessage(requestError)));
                          }}
                          type="button"
                        >
                          <span className="block text-sm">{type.typeName}</span>
                          <span className="iw-muted block text-xs">{type.groupName ?? "Unknown group"}</span>
                        </button>
                      )) : <p className="iw-muted px-3 py-2 text-sm">No matching types in the active SDE.</p>}
                    </div>
                  ) : null}
                  {form.structureTypeId ? <p className="iw-muted mt-1 text-xs">Selected from active SDE</p> : null}
                </div>
              </>
            ) : null}
            {form.kind === "npcStation" ? (
              <div className="relative sm:col-span-2">
                <Field
                  label="NPC station"
                  value={stationQuery}
                  onChange={(value) => {
                    setStationQuery(value);
                    setStationOpen(true);
                    setForm((current) => ({ ...current, structureId: null }));
                  }}
                  onFocus={() => setStationOpen(true)}
                />
                {stationLookup.searching ? <p className="iw-muted mt-1 text-xs">Searching active SDE...</p> : null}
                {!stationLookup.searching && stationOpen && !form.structureId ? (
                  <div className="absolute z-40 mt-1 max-h-56 w-full overflow-y-auto border border-border bg-panel shadow-xl">
                    {stationLookup.results.length > 0 ? stationLookup.results.map((station) => (
                      <button
                        className="block w-full px-3 py-2 text-left hover:bg-panel-strong"
                        key={station.stationId}
                        onClick={() => {
                          setStationQuery(station.stationName);
                          setStationOpen(false);
                          setForm((current) => ({
                            ...current,
                            structureId: station.stationId,
                            structureTypeId: station.stationTypeId,
                            structureTypeName: station.stationTypeName ?? "NPC Station",
                            solarSystemId: station.solarSystemId,
                            solarSystemName: station.solarSystemName,
                            securityClass: station.securityClass,
                            ...zeroedStructureModifiers,
                          }));
                        }}
                        type="button"
                      >
                        <span className="block text-sm">{station.stationName}</span>
                        <span className="iw-muted block text-xs">
                          {[station.stationTypeName, station.solarSystemName].filter(Boolean).join(" · ")}
                        </span>
                      </button>
                    )) : <p className="iw-muted px-3 py-2 text-sm">No matching NPC stations in the active SDE.</p>}
                  </div>
                ) : null}
                {form.structureId ? <p className="iw-muted mt-1 text-xs">Station, type, system, and security derived from active SDE</p> : null}
              </div>
            ) : null}
            <div className="relative sm:col-span-2">
              <Field
                    label="Solar system"
                    value={form.solarSystemName}
                    onChange={(value) => {
                      setCostIndexState({ status: "idle" });
                      setForm((current) => ({
                        ...current,
                        solarSystemName: value,
                        solarSystemId: null,
                        manualSystemCostIndex: null,
                      }));
                    }}
              />
              {solarSystemLookup.searching ? <p className="iw-muted mt-1 text-xs">Searching active SDE...</p> : null}
              {!solarSystemLookup.searching && !form.solarSystemId && form.solarSystemName.trim().length >= 2 ? (
                <div className="absolute z-30 mt-1 max-h-56 w-full overflow-y-auto border border-border bg-panel shadow-xl">
                  {solarSystemLookup.results.length > 0 ? solarSystemLookup.results.map((system) => (
                    <button
                      className="block w-full px-3 py-2 text-left text-sm hover:bg-panel-strong"
                      key={system.solarSystemId}
                      onClick={() => {
                        setForm((current) => ({
                          ...current,
                          solarSystemId: system.solarSystemId,
                          solarSystemName: system.solarSystemName,
                          securityClass: system.securityClass,
                        }));
                      }}
                      type="button"
                    >
                      {system.solarSystemName}
                    </button>
                  )) : <p className="iw-muted px-3 py-2 text-sm">No matching systems in the active SDE. Re-import the SDE if it predates solar-system support.</p>}
                </div>
              ) : null}
              {form.solarSystemId ? <p className="iw-muted mt-1 text-xs">Selected from active SDE</p> : null}
            </div>
            <Field label="Structure material reduction %" value={form.materialReductionPercent} onChange={(value) => set("materialReductionPercent", value)} />
            <Field label="Structure time reduction %" value={form.timeReductionPercent} onChange={(value) => set("timeReductionPercent", value)} />
            <Field label="Structure job-cost reduction %" value={form.jobCostReductionPercent} onChange={(value) => set("jobCostReductionPercent", value)} />
            <Field label="Facility tax %" value={form.facilityTaxPercent} onChange={(value) => set("facilityTaxPercent", value)} />
            <div>
              <Field label="SCC surcharge %" value={form.sccSurchargePercent} onChange={(value) => set("sccSurchargePercent", value)} inputMode="decimal" />
              <p className="iw-muted mt-1 text-xs">Defaults to CCP&apos;s 4% manufacturing SCC surcharge; editable if the rule changes.</p>
            </div>
            <Field label="Alliance surcharge %" value={form.allianceSurchargePercent} onChange={(value) => set("allianceSurchargePercent", value)} />
            <div>
              <Field
                label="System cost index"
                value={form.manualSystemCostIndex ?? ""}
                onChange={(value) => {
                  setCostIndexState({ status: "manual" });
                  setForm((current) => ({ ...current, manualSystemCostIndex: value || null }));
                }}
                inputMode="decimal"
              />
              {costIndexState.status === "loading" ? <p className="iw-muted mt-1 text-xs">Fetching {form.role} index from ESI...</p> : null}
              {costIndexState.status === "esi" ? (
                <p className="mt-1 text-xs text-success">
                  ESI {form.role} index · fetched {new Date(costIndexState.fetchedAt ?? "").toLocaleString()} · refreshes after {new Date(costIndexState.expiresAt ?? "").toLocaleTimeString()}
                </p>
              ) : null}
              {costIndexState.status === "manual" ? <p className="iw-muted mt-1 text-xs">Manual override</p> : null}
              {costIndexState.status === "unavailable" ? <p className="mt-1 text-xs text-accent">ESI index unavailable. Enter the current value manually.</p> : null}
            </div>
            <label className="block">
              <span className="mb-1 block text-sm font-semibold">Fixed supplemental cost</span>
              <MoneyInput
                aria-label="Fixed supplemental cost"
                onClear={() => set("fixedSupplementalCost", "0")}
                onCommit={(canonical) => set("fixedSupplementalCost", canonical)}
                onValueChange={setFixedCostResult}
                value={form.fixedSupplementalCost}
              />
            </label>
            <div className="sm:col-span-2 border-t border-border pt-3">
              <div className="flex items-center justify-between gap-3">
                <div><h3 className="text-sm font-semibold">Applicable rigs</h3><p className="iw-muted text-xs">Capture only rigs that affect the {form.role === "reaction" ? "reaction" : "manufacturing"} category represented by this profile.</p></div>
                <button className="iw-button-secondary" disabled={form.rigs.length >= 3} onClick={addRig} type="button"><Plus className="mr-2 h-4 w-4" />Add rig</button>
              </div>
              <div className="mt-3 grid gap-3">
                {form.rigs.map((rig) => (
                  <div className="grid gap-2 border border-border p-3 sm:grid-cols-2" key={rig.slotNumber}>
                    <div className="flex items-center justify-between gap-2 sm:col-span-2"><strong className="text-sm">Rig slot {rig.slotNumber}</strong><button aria-label={`Remove rig slot ${rig.slotNumber}`} className="iw-icon-button" onClick={() => setForm((current) => ({ ...current, rigs: current.rigs.filter((item) => item.slotNumber !== rig.slotNumber) }))} title="Remove rig" type="button"><Trash2 className="h-4 w-4" /></button></div>
                    <RigTypeSelector
                      activity={form.role}
                      onChange={(patch) => updateRig(rig.slotNumber, patch)}
                      onError={setError}
                      rig={rig}
                      securityClass={form.securityClass}
                      structureTypeId={form.structureTypeId}
                    />
                    <Field label="Material reduction %" value={rig.materialReductionPercent} onChange={(value) => updateRig(rig.slotNumber, { materialReductionPercent: value })} />
                    <Field label="Time reduction %" value={rig.timeReductionPercent} onChange={(value) => updateRig(rig.slotNumber, { timeReductionPercent: value })} />
                  </div>
                ))}
              </div>
            </div>
            <div className="sm:col-span-2"><Field label="Notes" value={form.notes} onChange={(value) => set("notes", value)} multiline /></div>
          </div>
          <div className="mt-4 flex gap-2">
            <button className="iw-button-secondary" disabled={saving} onClick={closeForm} type="button">Cancel</button>
            <button className="iw-button-primary" disabled={saving || fixedCostResult.status === "invalid"} onClick={() => void save()} type="button">{saving ? "Saving..." : facility ? "Save Changes" : "Create Facility"}</button>
          </div>
          <p className="iw-muted mt-3 text-xs">Structure and rig bonuses selected from the active SDE are calculated for the chosen security class. Review the captured assumptions before saving.</p>
        </div> : null}
    </dialog>
  );
}
