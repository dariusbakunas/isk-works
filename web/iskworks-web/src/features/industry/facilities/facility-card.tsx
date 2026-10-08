import { Building2, Pencil, Trash2 } from "lucide-react";

import type { FacilityProfile } from "../../../api/industry";
import { EveTypeImage } from "../../../components/eve-type-image";
import { Badge } from "../../../components/primitives";
import { effectiveFacilityReductionPercent, formatBonusPercent } from "./facility-bonus";

const KIND_LABELS: Record<FacilityProfile["kind"], string> = {
  upwellStructure: "Upwell",
  npcStation: "NPC Station",
  manual: "Manual",
};

function rigSummary(rigs: FacilityProfile["rigs"]): { label: string; title?: string } {
  if (rigs.length === 0) return { label: "No rigs captured" };
  const named = rigs.filter((rig) => rig.typeName.trim().length > 0);
  if (named.length === 0) {
    return { label: `${rigs.length} rig${rigs.length === 1 ? "" : "s"}` };
  }
  const [first, ...rest] = named;
  const label = rest.length === 0 ? first.typeName : `${first.typeName} +${rest.length} more`;
  return { label, title: named.map((rig) => rig.typeName).join(", ") };
}

function Bonus({ label, value }: { label: string; value: number }) {
  return (
    <div>
      <span className="block text-[11px] uppercase tracking-wide text-muted">{label}</span>
      <strong className="mt-0.5 block font-mono text-sm tabular-nums">{formatBonusPercent(value)}</strong>
    </div>
  );
}

export function FacilityCard({
  profile,
  onEdit,
  onDelete,
}: {
  profile: FacilityProfile;
  onEdit: (profile: FacilityProfile) => void;
  onDelete: (profile: FacilityProfile) => void;
}) {
  const material = effectiveFacilityReductionPercent(profile, "material");
  const time = effectiveFacilityReductionPercent(profile, "time");
  const hasBonus = material > 0 || time > 0;
  const rig = rigSummary(profile.rigs);
  const meta = [profile.structureTypeName, profile.solarSystemName].filter(Boolean).join(" · ");

  return (
    <section className="iw-panel flex h-full min-w-0 flex-col gap-3 p-3">
      <div className="flex min-w-0 items-start gap-3">
        {profile.structureTypeId ? (
          <EveTypeImage
            size={40}
            typeId={profile.structureTypeId}
            typeName={profile.structureTypeName || profile.name}
            variation="render"
          />
        ) : (
          <span
            aria-label="Facility image unavailable"
            className="grid h-10 w-10 shrink-0 place-items-center rounded border border-border bg-panel-strong text-muted"
            role="img"
          >
            <Building2 aria-hidden="true" className="h-5 w-5" />
          </span>
        )}
        <div className="min-w-0 flex-1">
          <h2 className="truncate font-semibold" data-private="" title={profile.name}>
            {profile.name}
          </h2>
          <div className="mt-1 flex flex-wrap items-center gap-1.5">
            <Badge tone={profile.role === "reaction" ? "reaction" : "primary"}>
              {profile.role === "reaction" ? "Reaction" : "Manufacturing"}
            </Badge>
            <Badge tone="muted">{KIND_LABELS[profile.kind]}</Badge>
          </div>
        </div>
      </div>

      <p className="iw-muted min-w-0 truncate text-xs" title={meta || undefined}>
        {meta || "System not set"}
      </p>

      {hasBonus ? (
        <div className="grid grid-cols-2 gap-2 border-t border-border pt-2">
          {material > 0 ? <Bonus label="Material" value={material} /> : null}
          {time > 0 ? <Bonus label="Time" value={time} /> : null}
        </div>
      ) : (
        <p className="iw-muted border-t border-border pt-2 text-xs">No captured bonuses</p>
      )}

      <p className="iw-muted text-xs" title={rig.title}>
        {rig.label}
      </p>

      <div className="mt-auto flex items-center gap-2 pt-1">
        <button className="iw-button-secondary" onClick={() => onEdit(profile)} type="button">
          <Pencil className="mr-2 h-4 w-4" />
          Edit
        </button>
        <button
          aria-label="Delete facility"
          className="iw-icon-button"
          onClick={() => onDelete(profile)}
          title="Delete Facility"
          type="button"
        >
          <Trash2 className="h-4 w-4" />
        </button>
      </div>
    </section>
  );
}
