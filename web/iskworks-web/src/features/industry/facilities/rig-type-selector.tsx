import { useEffect, useState } from "react";

import type { FacilityInput } from "../../../api/industry";
import {
  getRigManufacturingModifiers,
  getRigReactionModifiers,
  type RigApplicability,
  searchReactionRigs,
  searchStructureRigs,
  type TypeSearchResult,
} from "../../../api/sde";
import { apiMessage } from "../shared/api-error";
import { Field } from "../shared/field";

/** Human-readable summary of which jobs a rig's bonuses reach. `null`/`null`
 * means the rig has no target filter in the SDE -- it applies to every job
 * of its activity. */
export function describeRigApplicability(appliesTo: RigApplicability, activity: FacilityInput["role"]): string {
  const scope = activity === "reaction" ? "reaction" : "manufacturing";
  const { material, time } = appliesTo;
  if (!material && !time) return `Affects all ${scope} jobs`;
  if (material && time && material.filterId === time.filterId) return `Affects: ${material.name}`;
  const part = (filter: RigApplicability["material"]) => (filter ? filter.name : `all ${scope} jobs`);
  return `Material bonus: ${part(material)} · Time bonus: ${part(time)}`;
}

interface RigTypeSelectorProps {
  rig: FacilityInput["rigs"][number];
  securityClass: FacilityInput["securityClass"];
  structureTypeId: number | null;
  activity: FacilityInput["role"];
  onChange: (patch: Partial<FacilityInput["rigs"][number]>) => void;
  onError: (message: string) => void;
}

export function RigTypeSelector({ rig, securityClass, structureTypeId, activity, onChange, onError }: RigTypeSelectorProps) {
  const [results, setResults] = useState<TypeSearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [open, setOpen] = useState(false);
  const [deriving, setDeriving] = useState(false);
  const [compatibility, setCompatibility] = useState<boolean | null>(null);
  const [appliesTo, setAppliesTo] = useState<RigApplicability | null>(null);
  const searchRigs = activity === "reaction" ? searchReactionRigs : searchStructureRigs;
  const getModifiers = activity === "reaction" ? getRigReactionModifiers : getRigManufacturingModifiers;

  useEffect(() => {
    const query = rig.typeName.trim();
    if (!open || rig.typeId) {
      setResults([]);
      setSearching(false);
      return;
    }
    let cancelled = false;
    setSearching(true);
    const timer = window.setTimeout(() => {
      searchRigs(query, structureTypeId)
        .then((matches) => {
          if (!cancelled) setResults(matches);
        })
        .catch((requestError) => {
          if (!cancelled) onError(apiMessage(requestError));
        })
        .finally(() => {
          if (!cancelled) setSearching(false);
        });
    }, 250);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [onError, open, rig.typeId, rig.typeName, searchRigs, structureTypeId]);

  useEffect(() => {
    if (!rig.typeId) {
      setCompatibility(null);
      setAppliesTo(null);
      return;
    }
    let cancelled = false;
    setDeriving(true);
    getModifiers(rig.typeId, securityClass, structureTypeId)
      .then((modifiers) => {
        if (cancelled) return;
        onChange({
          materialReductionPercent: modifiers.materialReductionPercent,
          timeReductionPercent: modifiers.timeReductionPercent,
        });
        setCompatibility(modifiers.compatibleWithStructure);
        setAppliesTo(modifiers.appliesTo);
      })
      .catch((requestError) => {
        if (!cancelled) onError(apiMessage(requestError));
      })
      .finally(() => {
        if (!cancelled) setDeriving(false);
      });
    return () => {
      cancelled = true;
    };
  }, [getModifiers, rig.typeId, securityClass, structureTypeId]);

  return (
    <div className="relative sm:col-span-2">
      <Field
        label="Rig"
        value={rig.typeName}
        onChange={(value) => {
          setOpen(true);
          onChange({ typeId: 0, typeName: value });
        }}
        onFocus={() => setOpen(true)}
      />
      {searching ? <p className="iw-muted mt-1 text-xs">Searching active SDE...</p> : null}
      {!searching && open && !rig.typeId ? (
        <div className="absolute z-30 mt-1 max-h-56 w-full overflow-y-auto border border-border bg-panel shadow-xl">
          {results.length > 0 ? results.map((type) => (
            <button
              className="block w-full px-3 py-2 text-left hover:bg-panel-strong"
              key={type.typeId}
              onClick={() => {
                onChange({ typeId: type.typeId, typeName: type.typeName });
                setResults([]);
                setOpen(false);
              }}
              type="button"
            >
              <span className="block text-sm">{type.typeName}</span>
              <span className="iw-muted block text-xs">{type.groupName ?? "Unknown rig group"}</span>
            </button>
          )) : <p className="iw-muted px-3 py-2 text-sm">No matching structure rigs in the active SDE.</p>}
        </div>
      ) : null}
      {rig.typeId ? <p className="iw-muted mt-1 text-xs">Selected from active SDE</p> : null}
      {deriving ? <p className="iw-muted mt-1 text-xs">Calculating SDE bonuses...</p> : null}
      {!deriving && appliesTo ? (
        <p className="iw-muted mt-1 text-xs">{describeRigApplicability(appliesTo, activity)}. Its bonus only reaches a Build whose product is in that group.</p>
      ) : null}
      {compatibility === false ? <p className="mt-1 text-xs text-danger">This rig is not compatible with the selected structure type.</p> : null}
    </div>
  );
}
