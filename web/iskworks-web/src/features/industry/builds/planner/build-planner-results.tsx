import { InlineAlert } from "../../../../components/primitives";
import type { BuildPlannerPreviewView } from "./contracts";

/**
 * Blocking candidate notices only. Advisory `preview.warnings` are surfaced
 * compactly by `CandidateSummary`'s `WarningBadge`, not here -- they're
 * read-once and were eating the worksheet's vertical space.
 */
export function BuildPlannerResults({
  preview,
  updating,
}: {
  preview: BuildPlannerPreviewView;
  updating: boolean;
}) {
  const hasContent = updating || preview.validation.blockers.length > 0;
  if (!hasContent) return null;

  return (
    <section className="border-y border-border py-4" aria-label="Candidate plan results">
      {updating ? <span className="text-xs text-muted" role="status">Updating preview...</span> : null}

      {preview.validation.blockers.map((blocker) => (
        <div className="mt-3" key={blocker.code}>
          <InlineAlert title="Candidate cannot be planned">{blocker.message}</InlineAlert>
        </div>
      ))}
    </section>
  );
}
