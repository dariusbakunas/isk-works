import { Plus } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router";
import { createPriceSource, listPriceSources, type PriceSource } from "../../../api/industry";
import { ButtonLink, EmptyState, InlineAlert, PageHeader, Panel, StatusBadge } from "../../../components/primitives";
import { Private } from "../../../observability/private";
import { apiMessage } from "../shared/api-error";
import { Field } from "../shared/field";
import { formatDate } from "../shared/formatting";

type AsyncState<T> =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; data: T };

// Manual Price Lists are the only kind a user creates or browses here --
// `eveClientMarketExport`/`esiMarketOrders` rows are auto-provisioned
// internal plumbing for scope-based pricing with no detail page of their
// own, so they're filtered
// out rather than shown as dead links.
export function PricesPage() {
  const [state, setState] = useState<AsyncState<PriceSource[]>>({ status: "loading" });
  useEffect(() => {
    listPriceSources()
      .then((data) => setState({ status: "ready", data: data.filter((source) => source.kind === "manual") }))
      .catch((error) => setState({ status: "error", message: apiMessage(error) }));
  }, []);
  return (
    <>
      <PageHeader eyebrow="Planning assumptions" title="Price Overrides">
        Reusable manual values used to create immutable Build Price Snapshots.
      </PageHeader>
      <div className="mb-4 flex flex-wrap justify-end gap-2">
        <ButtonLink to="/prices/imports">Import Market Export</ButtonLink>
        <ButtonLink to="/prices/new" variant="primary"><Plus className="mr-2 h-4 w-4" /> New Price Override</ButtonLink>
      </div>
      {state.status === "loading" ? <Panel>Loading Price Overrides...</Panel> : null}
      {state.status === "error" ? <InlineAlert title="Price Overrides unavailable">{state.message}</InlineAlert> : null}
      {state.status === "ready" && state.data.length === 0 ? (
        <Panel><EmptyState title="No Price Overrides">Create a manual collection of prices.</EmptyState></Panel>
      ) : null}
      {state.status === "ready" ? (
        <div className="grid gap-2">
          {state.data.map((source) => (
            <Link className="iw-panel grid gap-2 p-4 hover:border-primary/60 md:grid-cols-[minmax(0,1fr)_auto]" key={source.id} to={`/prices/${source.id}`}>
              <div>
                <div className="flex flex-wrap items-center gap-2">
                  <Private as="strong">{source.name}</Private>
                  <StatusBadge>Manual</StatusBadge>
                </div>
                <Private as="p" className="iw-muted mt-1">{source.description || "No description"}</Private>
              </div>
              <div className="md:text-right">
                <strong>{source.itemCount.toLocaleString()} prices</strong>
                <p className="text-xs text-muted">Updated {formatDate(source.updatedAt)} · used by {source.recentBuildCount} Builds</p>
              </div>
            </Link>
          ))}
        </div>
      ) : null}
    </>
  );
}

export function NewPriceSourcePage() {
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [error, setError] = useState("");
  async function save() {
    try {
      const source = await createPriceSource({ name, description });
      navigate(`/prices/${source.id}`);
    } catch (requestError) {
      setError(apiMessage(requestError));
    }
  }
  return (
    <>
      <PageHeader eyebrow="Planning prices" title="New Price Override">
        Create a manually maintained collection of prices.
      </PageHeader>
      {error ? <InlineAlert title="Price Override not created">{error}</InlineAlert> : null}
      <Panel className="mt-4">
        <Field label="Name" value={name} onChange={setName} />
        <Field label="Description" value={description} onChange={setDescription} multiline />
        <div className="mt-4 flex gap-2">
          <ButtonLink to="/prices">Cancel</ButtonLink>
          <button className="iw-button-primary" onClick={() => void save()} type="button">Create Price Override</button>
        </div>
      </Panel>
    </>
  );
}

