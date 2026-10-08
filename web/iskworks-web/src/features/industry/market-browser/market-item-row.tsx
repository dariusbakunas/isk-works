import type { MarketItemSummary } from "../../../api/industry";
import { EveTypeImage } from "../../../components/eve-type-image";

// Icon + name only, deliberately no pricing -- used both as a tree leaf
// and, unchanged, as a row in the catalog-wide search results list.
// Indentation is the
// caller's concern (a wrapping <li> with its own padding), not this
// component's, so the same row works at any depth or with none at all.
export function MarketItemRow({
  item,
  selected,
  onSelect,
}: {
  item: Pick<MarketItemSummary, "typeId" | "typeName">;
  selected: boolean;
  onSelect: (typeId: number, typeName: string) => void;
}) {
  return (
    <button
      className={`flex w-full min-w-0 items-center gap-2 rounded px-1 py-1 text-left text-sm ${
        selected ? "bg-primary/10 text-primary" : "hover:bg-panel-strong"
      }`}
      onClick={() => onSelect(item.typeId, item.typeName)}
      type="button"
    >
      <EveTypeImage size={24} typeId={item.typeId} typeName={item.typeName} />
      <span className="min-w-0 flex-1 truncate">{item.typeName}</span>
    </button>
  );
}
