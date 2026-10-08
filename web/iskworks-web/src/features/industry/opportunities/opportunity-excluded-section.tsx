import { ChevronDown, ChevronUp } from "lucide-react";
import { useState } from "react";

import type { OpportunityCandidate } from "../../../api/opportunities";
import { EveTypeImage } from "../../../components/eve-type-image";
import { Badge } from "../../../components/primitives";

export function ExcludedCandidatesSection({ candidates }: { candidates: OpportunityCandidate[] }) {
  const [open, setOpen] = useState(false);
  const excluded = candidates.filter((candidate) => candidate.eligibility.status === "excludedFromDefaultRanking");
  if (excluded.length === 0) return null;

  return (
    <div className="mt-3 rounded-md border border-border">
      <button
        aria-expanded={open}
        className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-sm font-semibold"
        onClick={() => setOpen((current) => !current)}
        type="button"
      >
        <span>Excluded / nonstandard recipes ({excluded.length})</span>
        {open ? <ChevronUp aria-hidden="true" className="h-3.5 w-3.5" /> : <ChevronDown aria-hidden="true" className="h-3.5 w-3.5" />}
      </button>
      {open ? (
        <ul className="divide-y divide-border border-t border-border">
          {excluded.map((candidate) => (
            <li className="flex flex-wrap items-center justify-between gap-2 px-3 py-2 text-xs" key={candidate.productTypeId}>
              <span className="flex min-w-0 items-center gap-2">
                <EveTypeImage size={24} typeId={candidate.productTypeId} typeName={candidate.productName} />
                <span className="truncate font-medium">{candidate.productName}</span>
              </span>
              <span className="flex flex-wrap items-center justify-end gap-1.5">
                {candidate.eligibility.exclusionReasons.map((reason, index) => (
                  <span key={index} title={reason.message}>
                    <Badge tone="muted">{reason.marketGroupName}</Badge>
                  </span>
                ))}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
