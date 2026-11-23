import { Search } from "lucide-react";
import { useEffect, useId, useState } from "react";

import { searchInventableProducts, type InventableProduct } from "../../../api/invention";
import { EveTypeImage } from "../../../components/eve-type-image";
import { useLoad } from "../../../hooks/use-load";

/** A search box over inventable products. While it isn't being typed in it
 * shows the chosen product. */
export function ProductSearch({
  onPick,
  selectedName,
  selectedTypeId,
}: {
  onPick: (product: InventableProduct) => void;
  selectedName: string | null;
  selectedTypeId: number | null;
}) {
  const listId = useId();
  const [text, setText] = useState("");
  const [editing, setEditing] = useState(false);
  const [query, setQuery] = useState("");

  // Only typing schedules a search.
  useEffect(() => {
    const timer = window.setTimeout(() => setQuery(text.trim()), 200);
    return () => window.clearTimeout(timer);
  }, [text]);

  const searching = editing && query.length >= 2;
  const results = useLoad(searching ? (signal) => searchInventableProducts(query, signal) : null, `products:${query}`);
  const products = searching ? results.data?.products ?? [] : [];

  function pick(product: InventableProduct) {
    setEditing(false);
    setText("");
    setQuery("");
    onPick(product);
  }

  return (
    <div className="relative w-64 max-w-full">
      {selectedTypeId !== null && selectedName && !editing ? (
        <span className="pointer-events-none absolute left-1.5 top-1/2 -translate-y-1/2">
          <EveTypeImage size={20} typeId={selectedTypeId} typeName={selectedName} />
        </span>
      ) : (
        <Search aria-hidden="true" className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
      )}
      <input
        aria-autocomplete="list"
        aria-controls={listId}
        aria-expanded={searching}
        aria-label="Product"
        className="iw-input min-h-7 py-1 pl-8 text-xs"
        onBlur={() => setEditing(false)}
        onChange={(event) => {
          setEditing(true);
          setText(event.target.value);
        }}
        onFocus={() => setEditing(true)}
        onKeyDown={(event) => {
          if (event.key === "Escape") event.currentTarget.blur();
          if (event.key === "Enter" && products.length > 0) pick(products[0]);
        }}
        placeholder="Search inventable product…"
        role="combobox"
        value={editing ? text : selectedName ?? ""}
      />
      {searching ? (
        <ul
          className="absolute left-0 right-0 top-full z-30 mt-1 max-h-72 overflow-y-auto border border-border bg-panel shadow-2xl"
          id={listId}
          role="listbox"
        >
          {results.loading ? (
            <li className="px-2 py-1.5 text-muted">Searching…</li>
          ) : results.errorMessage ? (
            <li className="px-2 py-1.5 text-danger" role="alert">{results.errorMessage}</li>
          ) : products.length === 0 ? (
            <li className="px-2 py-1.5 text-muted">No inventable products match</li>
          ) : products.map((product) => (
            <li aria-selected={product.productTypeId === selectedTypeId} key={product.productTypeId} role="option">
              <button
                className="flex w-full items-center gap-2 px-2 py-1.5 text-left hover:bg-panel-strong"
                // Before the input's blur, which would close the list.
                onMouseDown={(event) => {
                  event.preventDefault();
                  pick(product);
                }}
                type="button"
              >
                <EveTypeImage size={24} typeId={product.productTypeId} typeName={product.productName} />
                <span className="min-w-0">
                  <span className="block truncate font-medium text-foreground">{product.productName}</span>
                  <span className="block truncate text-[0.625rem] text-muted">from {product.sourceBlueprintName}</span>
                </span>
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
