import { EveTypeImage } from "../../../../../components/eve-type-image";
import type {
  RelatedBuildsSlice,
} from "../inspector-model";

export function RelatedBuildsBody({ slice }: { slice: RelatedBuildsSlice }) {
  return (
    <div className="grid gap-1.5">
      {slice.entries.map((entry) => (
        <div
          className="flex items-center justify-between gap-3 py-0.5 text-xs"
          key={entry.typeId ?? "root"}
        >
          <span className="flex min-w-0 items-center gap-1.5 text-muted">
            <EveTypeImage
              size={24}
              typeId={entry.typeId ?? slice.fallbackTypeId ?? 0}
              typeName={entry.name}
            />
            <span className="truncate">{entry.name}</span>
          </span>
          <strong className="shrink-0 text-right font-mono">{entry.quantity.toLocaleString()}</strong>
        </div>
      ))}
    </div>
  );
}
