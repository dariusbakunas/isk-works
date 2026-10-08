import { Plus, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router";
import { deletePriceSource, getPriceSource, removePriceItem, updatePriceSource, upsertPriceItem, type PriceSource, type PriceSourceItem } from "../../../api/industry";
import { searchTypes, type TypeSearchResult } from "../../../api/sde";
import { EveTypeImage } from "../../../components/eve-type-image";
import { MoneyInput, type MoneyInputResult } from "../../../components/money-input";
import { ConfirmDialog, EmptyState, InlineAlert, PageHeader, Panel } from "../../../components/primitives";
import { useDebouncedLookup } from "../../../hooks/use-debounced-lookup";
import { apiMessage } from "../shared/api-error";
import { Field } from "../shared/field";
import { PriceItemsOperationalTable } from "./price-items-operational-table";

export function PriceSourceDetailPage() {
  const { priceSourceId = "" } = useParams();
  const navigate = useNavigate();
  const [source, setSource] = useState<PriceSource | null>(null);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [search, setSearch] = useState("");
  const [itemSearch, setItemSearch] = useState("");
  const [typeId, setTypeId] = useState("");
  const [typeName, setTypeName] = useState("");
  const [price, setPrice] = useState("");
  // The live classification of the Unit price field. The domain rejects an
  // empty price, so Save stays blocked until this reaches "valid".
  const [priceResult, setPriceResult] = useState<MoneyInputResult>({ status: "empty" });
  const [note, setNote] = useState("");
  const [selectedTypeId, setSelectedTypeId] = useState<number | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState("");
  const itemLookup = useDebouncedLookup<TypeSearchResult>(
    itemSearch,
    searchTypes,
    (requestError) => setError(apiMessage(requestError)),
  );
  useEffect(() => {
    getPriceSource(priceSourceId)
      .then((loaded) => {
        setSource(loaded);
        setName(loaded.name);
        setDescription(loaded.description);
      })
      .catch((requestError) => setError(apiMessage(requestError)));
  }, [priceSourceId]);
  const filtered = useMemo(
    () => source?.items.filter((item) => item.typeName.toLowerCase().includes(search.toLowerCase()) || String(item.typeId).includes(search)) ?? [],
    [source, search],
  );
  if (!source) return error ? <InlineAlert title="Price Override unavailable">{error}</InlineAlert> : <Panel>Loading Price Override...</Panel>;
  const currentSource = source;
  async function saveDetails() {
    try {
      const updated = await updatePriceSource(currentSource.id, { expectedRevision: currentSource.revision, name, description });
      setSource(updated);
    } catch (requestError) { setError(apiMessage(requestError)); }
  }
  function clearForm() {
    setSelectedTypeId(null);
    setTypeId(""); setTypeName(""); setPrice(""); setNote("");
  }
  function selectItem(item: PriceSourceItem) {
    setError("");
    setSelectedTypeId(item.typeId);
    setTypeId(String(item.typeId));
    setTypeName(item.typeName);
    setPrice(item.price);
    setNote(item.note);
    setItemSearch("");
  }
  async function addItem() {
    const parsedId = Number(typeId);
    if (!Number.isSafeInteger(parsedId) || parsedId <= 0) {
      setError("Enter a valid EVE type ID.");
      return;
    }
    if (priceResult.status !== "valid" || priceResult.canonical === undefined) return;
    try {
      const updated = await upsertPriceItem(currentSource.id, parsedId, {
        expectedRevision: currentSource.revision,
        typeName,
        price: priceResult.canonical,
        note,
      });
      setSource(updated);
      clearForm();
    } catch (requestError) { setError(apiMessage(requestError)); }
  }
  async function removeItem(type: number) {
    try {
      setSource(await removePriceItem(currentSource.id, type, currentSource.revision));
      if (selectedTypeId === type) clearForm();
    } catch (requestError) { setError(apiMessage(requestError)); }
  }
  async function removeSource() {
    try { await deletePriceSource(currentSource.id, currentSource.revision); navigate("/prices"); }
    catch (requestError) { setError(apiMessage(requestError)); setConfirmDelete(false); }
  }
  return (
    <>
      <PageHeader eyebrow="Price Override" title={source.name} titlePrivate>
        Manually maintained assumptions. Historical Build snapshots remain unchanged when these values change.
      </PageHeader>
      {error ? <InlineAlert title={error.includes("changed") ? "Reload required" : "Change not saved"}>{error}</InlineAlert> : null}
      <div className="mt-4 grid gap-4">
        <Panel>
          <div className="grid gap-3 md:grid-cols-2"><Field label="Name" value={name} onChange={setName} /><Field label="Description" value={description} onChange={setDescription} /></div>
          <div className="mt-3 flex flex-wrap gap-2">
            <button className="iw-button-primary" onClick={() => void saveDetails()} type="button">Save details</button>
            <button className="iw-button-danger ml-auto" onClick={() => setConfirmDelete(true)} type="button"><Trash2 className="mr-2 h-4 w-4" /> Delete</button>
          </div>
        </Panel>
        <Panel>
          <h2 className="text-base font-semibold">{selectedTypeId === null ? "Add a price" : `Editing ${typeName || `type ${typeId}`}`}</h2>
          <p className="iw-muted mt-1">
            {selectedTypeId === null
              ? "Use the stable EVE type ID. Names are captured for display only."
              : "Update the price or note, then save. Click another row or cancel to add a different item instead."}
          </p>
          {selectedTypeId === null ? (
            <>
              <Field label="Find an EVE item" value={itemSearch} onChange={setItemSearch} className="mt-3" />
              {itemLookup.searching ? <p className="iw-muted mt-2 text-xs" role="status">Searching active SDE...</p> : null}
              {itemLookup.results.length > 0 ? (
                <div className="mt-2 max-h-48 divide-y divide-border overflow-y-auto border-y border-border">
                  {itemLookup.results.map((result) => (
                    <button
                      className="flex w-full items-center justify-between gap-3 px-2 py-2 text-left text-sm hover:bg-panel-strong"
                      key={result.typeId}
                      onClick={() => {
                        setTypeId(String(result.typeId));
                        setTypeName(result.typeName);
                        setItemSearch("");
                      }}
                      type="button"
                    >
                      <span className="flex min-w-0 items-center gap-3">
                        <EveTypeImage size={32} typeId={result.typeId} typeName={result.typeName} />
                        <strong className="truncate">{result.typeName}</strong>
                      </span>
                      <small className="text-muted">{result.groupName ?? "Unclassified"} · {result.typeId}</small>
                    </button>
                  ))}
                </div>
              ) : null}
            </>
          ) : null}
          <div className="mt-3 grid gap-2 sm:grid-cols-2 xl:grid-cols-4">
            <Field disabled={selectedTypeId !== null} label="EVE type ID" value={typeId} onChange={setTypeId} inputMode="numeric" />
            <Field disabled={selectedTypeId !== null} label="Item name" value={typeName} onChange={setTypeName} />
            <label className="block">
              <span className="mb-1 block text-sm font-semibold">Unit price (ISK)</span>
              <MoneyInput
                aria-label="Unit price (ISK)"
                onClear={() => setPrice("")}
                onCommit={setPrice}
                onValueChange={setPriceResult}
                value={price}
              />
            </label>
            <Field label="Note" value={note} onChange={setNote} />
          </div>
          <div className="mt-3 flex flex-wrap gap-2">
            <button
              className="iw-button-primary"
              disabled={priceResult.status !== "valid"}
              onClick={() => void addItem()}
              type="button"
            >
              <Plus className="mr-2 h-4 w-4" /> {selectedTypeId === null ? "Save price" : "Update price"}
            </button>
            {selectedTypeId !== null ? (
              <>
                <button className="iw-button-secondary" onClick={clearForm} type="button">Cancel</button>
                <button className="iw-button-danger ml-auto" onClick={() => void removeItem(selectedTypeId)} type="button">
                  <Trash2 className="mr-2 h-4 w-4" /> Remove this price
                </button>
              </>
            ) : null}
          </div>
        </Panel>
        <Panel>
          <div className="flex flex-wrap items-end justify-between gap-3">
            <h2 className="text-base font-semibold">{source.itemCount.toLocaleString()} prices</h2>
            <Field label="Search prices" value={search} onChange={setSearch} />
          </div>
          {filtered.length === 0 ? <EmptyState title="No matching prices">Add a price or change the search.</EmptyState> : (
            <div className="mt-3">
              <PriceItemsOperationalTable
                items={filtered}
                onSelectItem={(itemTypeId) => {
                  const item = filtered.find((candidate) => candidate.typeId === itemTypeId);
                  if (item) selectItem(item);
                }}
                selectedTypeId={selectedTypeId}
              />
            </div>
          )}
        </Panel>
      </div>
      <ConfirmDialog open={confirmDelete} title="Delete this Price Override?" confirmLabel="Delete Price Override" onCancel={() => setConfirmDelete(false)} onConfirm={() => void removeSource()}>
        Historical Build snapshots retain their captured source name and prices. No default override will be selected automatically.
      </ConfirmDialog>
    </>
  );
}

