import { Badge } from "../../../../../components/primitives";
import type {
  ProvenanceSlice,
} from "../inspector-model";
import { InspectorRow } from "../inspector-section";

export function ProvenanceBody({
  slice,
  onCopyBuildId,
}: {
  slice: ProvenanceSlice;
  onCopyBuildId: (() => void) | undefined;
}) {
  return (
    <div className="space-y-1.5">
      {slice.recipeCurrency ? (
        <div className="space-y-1">
          <Badge square tone={slice.recipeCurrency.tone === "blocking" ? "danger" : "warning"}>
            {slice.recipeCurrency.label}
          </Badge>
          {slice.recipeCurrency.explanation ? (
            <p className="text-[11px] leading-snug text-muted">{slice.recipeCurrency.explanation}</p>
          ) : null}
        </div>
      ) : null}
      <details>
        <summary className="cursor-pointer text-xs font-semibold">{slice.summary}</summary>
        <div className="mt-2 space-y-1">
          {slice.lines.map((line) => (
            <InspectorRow key={line.label} label={line.label} value={line.value} />
          ))}
          {slice.note ? <p className="break-words text-[11px] text-muted" data-private="">{slice.note}</p> : null}
          {slice.buildId ? (
            <div className="flex items-center justify-between gap-2 pt-1 text-[11px] text-muted">
              <span className="truncate font-mono">Build {slice.buildId}</span>
              {onCopyBuildId ? (
                <button className="iw-button-secondary shrink-0 px-1.5 py-0.5" onClick={onCopyBuildId} type="button">
                  Copy ID
                </button>
              ) : null}
            </div>
          ) : null}
        </div>
      </details>
    </div>
  );
}
