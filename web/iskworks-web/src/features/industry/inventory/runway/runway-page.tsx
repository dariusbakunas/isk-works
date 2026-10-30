import { ClipboardCopy, Download } from "lucide-react";
import { useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router";

import { MATERIAL_CATEGORIES } from "../../../../api/inventory-analytics";
import { DropdownMenu } from "../../../../components/dropdown-menu";
import { EmptyState, InlineAlert, PageHeader, Panel } from "../../../../components/primitives";
import { Private } from "../../../../observability/private";
import { InventorySectionTabs } from "../inventory-section-tabs";
import { RunwaySkeleton } from "./runway-skeleton";
import { EpicReadinessStrip, RunwayKpiStrip } from "./runway-summary";
import { categoryFromParam, RunwayTable } from "./runway-table";
import { downloadText } from "../../../../components/csv";
import { shoppingListCsv, shoppingListMultibuy } from "./shopping-list-export";
import { useInventoryRunway } from "./use-inventory-runway";

// Inventory › Runway: what committed Epics still have to buy, against stock.
// Filters live in the URL (?category=, ?epic=) so a view can be linked.
export function RunwayPage() {
  const [params, setParams] = useSearchParams();
  const category = categoryFromParam(params.get("category"));
  const epicFilter = params.get("epic");
  const { data, loading, error, retry } = useInventoryRunway(category);
  const [exportNotice, setExportNotice] = useState<{ tone: "success" | "error"; text: string } | null>(null);

  function setParam(key: string, value: string | null) {
    const next = new URLSearchParams(params);
    if (value) next.set(key, value);
    else next.delete(key);
    setParams(next, { replace: true });
  }

  const activeEpic = data?.epics.find((epic) => epic.orderId === epicFilter) ?? null;
  const materials = useMemo(() => {
    const rows = data?.materials ?? [];
    return activeEpic ? rows.filter((row) => row.epics.some((epic) => epic.orderId === activeEpic.orderId)) : rows;
  }, [data, activeEpic]);

  async function copyMultibuy() {
    const text = shoppingListMultibuy(materials);
    if (!text) {
      setExportNotice({ tone: "success", text: "Nothing is short, so there is nothing to buy." });
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      setExportNotice({ tone: "success", text: "Copied. Paste it into EVE's Multibuy window." });
    } catch {
      setExportNotice({ tone: "error", text: "Couldn't reach the clipboard." });
    }
  }

  function downloadCsv() {
    const date = new Date().toISOString().slice(0, 10);
    downloadText(shoppingListCsv(materials), `iskworks-shopping-list-${date}.csv`, "text/csv");
  }

  return (
    <>
      <PageHeader eyebrow="Accounting inventory" title="Inventory">
        What your committed Epics still have to buy, against the stock you hold. Drafts are not counted.
      </PageHeader>
      <InventorySectionTabs />
      <div className="mb-3 flex flex-wrap items-end justify-between gap-2">
        <label className="min-w-48 text-sm font-semibold">
          Category
          <select
            className="iw-input mt-1"
            onChange={(event) => setParam("category", event.target.value || null)}
            value={category ?? ""}
          >
            <option value="">All categories</option>
            {MATERIAL_CATEGORIES.map((option) => <option key={option} value={option}>{option}</option>)}
          </select>
        </label>
        <DropdownMenu
          align="end"
          icon={<Download aria-hidden="true" className="h-4 w-4" />}
          items={[
            {
              key: "csv",
              label: "Export CSV",
              icon: <Download aria-hidden="true" className="h-4 w-4" />,
              onSelect: downloadCsv,
            },
            {
              key: "multibuy",
              label: "Copy as EVE multibuy",
              icon: <ClipboardCopy aria-hidden="true" className="h-4 w-4" />,
              onSelect: () => void copyMultibuy(),
            },
          ]}
          label="Shopping list"
        />
      </div>
      {exportNotice ? (
        <div className="mb-3">
          <InlineAlert title={exportNotice.tone === "error" ? "Copy failed" : "Shopping list"} tone={exportNotice.tone}>
            {exportNotice.text}
          </InlineAlert>
        </div>
      ) : null}
      {error ? (
        <div className="mb-3">
          <InlineAlert title="Runway unavailable">
            {error.message}{" "}
            <button className="underline" onClick={retry} type="button">Retry</button>
          </InlineAlert>
        </div>
      ) : null}
      {loading && !data ? <RunwaySkeleton /> : null}
      {data && data.epics.length === 0 ? (
        <Panel>
          <EmptyState
            action={<Link className="iw-button-primary" to="/builds">Open Builds</Link>}
            title="No committed Epics"
          >
            Create an Epic from a Build plan to see what it still has to buy, and what you are short of before you start.
          </EmptyState>
        </Panel>
      ) : null}
      {data && data.epics.length > 0 ? (
        <div aria-busy={loading} className={`space-y-3 ${loading ? "opacity-60" : ""}`}>
          <RunwayKpiStrip kpis={data.kpis} />
          {data.kpis.materialsShort === 0 && data.materials.length > 0 ? (
            <InlineAlert title={`All ${data.kpis.materialsTotal} inputs covered by stock`} tone="success">
              {data.kpis.epicsTotal} open Epic{data.kpis.epicsTotal === 1 ? "" : "s"}, nothing to buy
              {category ? ` in ${category}` : ""}.
            </InlineAlert>
          ) : null}
          {data.missingPriceCount > 0 ? (
            <InlineAlert title="Some shortfalls have no price" tone="warning">
              {data.missingPriceCount} short material{data.missingPriceCount === 1 ? " has" : "s have"} no frozen unit
              cost on {data.missingPriceCount === 1 ? "its" : "their"} Epic, so the shortfall value leaves{" "}
              {data.missingPriceCount === 1 ? "it" : "them"} out.
            </InlineAlert>
          ) : null}
          <EpicReadinessStrip
            epics={data.epics}
            onSelectEpic={(orderId) => setParam("epic", orderId)}
            selectedEpicId={activeEpic?.orderId ?? null}
          />
          {activeEpic ? (
            <div className="flex items-center gap-2 text-xs text-primary">
              <span>
                Showing materials for <Private className="font-semibold">{activeEpic.displayName}</Private>
              </span>
              <button className="underline" onClick={() => setParam("epic", null)} type="button">Show all</button>
            </div>
          ) : null}
          {materials.length > 0 ? (
            <RunwayTable epics={data.epics} materials={materials} />
          ) : (
            <Panel>
              <EmptyState title="Nothing left to buy">
                {category
                  ? `The open Epics need nothing more in ${category}.`
                  : "Every input of the open Epics has already been used."}
              </EmptyState>
            </Panel>
          )}
        </div>
      ) : null}
    </>
  );
}
