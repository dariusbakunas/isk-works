import { Pencil, X } from "lucide-react";
import { useState } from "react";

import type { FacilityProfile, MarketScope } from "../../../api/industry";
import { MarketScopeSelector } from "../../../components/market-scope-selector";
import { Field } from "../shared/field";
import { describeMarketScope } from "./opportunity-formatters";

export interface OpportunityContextInput {
  facilityId: string;
  marketScope: MarketScope;
  materialEfficiency: number | null;
  timeEfficiency: number | null;
}

export function OpportunityContextStrip({
  facility,
  facilityRole,
  marketScope,
  facilities,
  materialEfficiency,
  timeEfficiency,
  runs,
  onApply,
}: {
  facility: FacilityProfile | null;
  facilityRole: "manufacturing" | "reaction";
  marketScope: MarketScope;
  facilities: FacilityProfile[];
  materialEfficiency: number | null;
  timeEfficiency: number | null;
  runs: number;
  onApply: (input: OpportunityContextInput) => void;
}) {
  const [open, setOpen] = useState(false);
  const [facilityId, setFacilityId] = useState(facility?.id ?? "");
  const [draftScope, setDraftScope] = useState(marketScope);
  const [me, setMe] = useState(String(materialEfficiency ?? 10));
  const [te, setTe] = useState(String(timeEfficiency ?? 20));
  const editableEfficiency = materialEfficiency !== null && timeEfficiency !== null;
  const eligibleFacilities = facilities.filter((candidate) => candidate.role === facilityRole && candidate.archivedAt === null);

  function openEditor() {
    setFacilityId(facility?.id ?? "");
    setDraftScope(marketScope);
    setMe(String(materialEfficiency ?? 10));
    setTe(String(timeEfficiency ?? 20));
    setOpen(true);
  }

  function apply() {
    if (!facilityId) return;
    if (!editableEfficiency) {
      onApply({ facilityId, marketScope: draftScope, materialEfficiency: null, timeEfficiency: null });
      setOpen(false);
      return;
    }
    const parsedMe = Number(me);
    const parsedTe = Number(te);
    if (!Number.isFinite(parsedMe) || !Number.isFinite(parsedTe)) return;
    onApply({ facilityId, marketScope: draftScope, materialEfficiency: parsedMe, timeEfficiency: parsedTe });
    setOpen(false);
  }

  return (
    <div className="mb-2 flex flex-wrap items-center justify-between gap-2 rounded-md border border-border bg-panel px-3 py-1.5 text-xs">
      <span className="text-muted">
        {facility?.name ?? "—"} · {describeMarketScope(marketScope)}
        {editableEfficiency ? ` · ME${materialEfficiency}/TE${timeEfficiency}` : ""} · {runs} run · All inputs
        purchased
      </span>
      <button className="iw-button-secondary" onClick={openEditor} type="button">
        <Pencil aria-hidden="true" className="mr-1.5 h-3.5 w-3.5" />
        Edit
      </button>

      {open ? (
        <div className="fixed inset-0 z-[70] grid place-items-center bg-black/70 p-4" role="presentation">
          <section aria-label="Edit calculation context" aria-modal="true" className="iw-dialog w-[min(480px,100%)] p-4" role="dialog">
            <div className="mb-3 flex items-center justify-between gap-3">
              <div>
                <p className="iw-eyebrow">Opportunities</p>
                <h2 className="text-base font-semibold">Edit calculation context</h2>
              </div>
              <button aria-label="Close" className="iw-icon-button" onClick={() => setOpen(false)} title="Close" type="button">
                <X aria-hidden="true" className="h-4 w-4" />
              </button>
            </div>
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="text-sm font-semibold" htmlFor="opportunity-context-facility">
                Facility
                <select
                  className="iw-input mt-1"
                  id="opportunity-context-facility"
                  onChange={(event) => setFacilityId(event.target.value)}
                  value={facilityId}
                >
                  {eligibleFacilities.map((candidate) => (
                    <option key={candidate.id} value={candidate.id}>
                      {candidate.name}
                    </option>
                  ))}
                </select>
              </label>
              <div>
                <p className="text-sm font-semibold">Market scope</p>
                <div className="mt-1">
                  <MarketScopeSelector onChange={setDraftScope} scope={draftScope} />
                </div>
              </div>
              {editableEfficiency ? (
                <>
                  <Field inputMode="numeric" label="Material Efficiency (0-10)" max={10} min={0} onChange={setMe} type="number" value={me} />
                  <Field inputMode="numeric" label="Time Efficiency (0-20)" max={20} min={0} onChange={setTe} type="number" value={te} />
                </>
              ) : null}
            </div>
            <div className="mt-3 flex justify-end gap-2 border-t border-border pt-3">
              <button className="iw-button-secondary" onClick={() => setOpen(false)} type="button">
                Cancel
              </button>
              <button className="iw-button-primary" disabled={!facilityId} onClick={apply} type="button">
                Apply
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </div>
  );
}
