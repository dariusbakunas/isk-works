import { Globe2, MoreHorizontal, Pencil, Trash2 } from "lucide-react";
import { useState } from "react";
import { Link, useNavigate } from "react-router";

import { apiMessage } from "../../../../api/api-error";
import { deleteSavedPlan, listSavedPlans, updateSavedPlan, type SavedPlanSummary } from "../../../../api/pi-plans";
import { DropdownMenu } from "../../../../components/dropdown-menu";
import { IskCompact } from "../../../../components/money";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../../components/operational-table";
import { ConfirmDialog, InlineAlert } from "../../../../components/primitives";
import { useLoad } from "../../../../hooks/use-load";
import { formatDate } from "../../../../lib/formatting";
import { PlanetarySectionTabs } from "../planetary-section-tabs";
import { PlanNameDialog } from "./plan-name-dialog";

const COLUMNS: OperationalColumn[] = [
  { key: "name", label: "Plan", width: "minmax(14rem, 2fr)" },
  { key: "home", label: "Home", width: "8rem", hideBelow: "tablet" },
  { key: "isk", label: "ISK/mo", width: "6rem", align: "right", numeric: true },
  { key: "progress", label: "Progress", width: "8rem" },
  { key: "saved", label: "Saved", width: "10rem", hideBelow: "desktop" },
  { key: "prices", label: "Prices as of", width: "10rem", hideBelow: "desktop" },
  { key: "actions", label: "", width: "2.5rem", align: "right" },
];

/** Planetary › Plans: PI plans saved from the Planner, and how far along each is. */
export function SavedPlansPage() {
  const navigate = useNavigate();
  const loaded = useLoad(() => listSavedPlans(), "plans");
  const [edits, setEdits] = useState<Record<string, SavedPlanSummary | null>>({});
  const [renaming, setRenaming] = useState<SavedPlanSummary | null>(null);
  const [deleting, setDeleting] = useState<SavedPlanSummary | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  // Renamed plans replace their row, deleted ones (null) drop out.
  const plans = (loaded.data ?? [])
    .map((plan) => (plan.id in edits ? edits[plan.id] : plan))
    .filter((plan): plan is SavedPlanSummary => plan !== null);

  async function rename(plan: SavedPlanSummary, name: string) {
    const saved = await updateSavedPlan(plan.id, { name });
    setEdits((current) => ({ ...current, [plan.id]: { ...plan, name: saved.name, updatedAt: saved.updatedAt } }));
    setRenaming(null);
  }

  async function remove(plan: SavedPlanSummary) {
    setDeleting(null);
    setFailure(null);
    try {
      await deleteSavedPlan(plan.id);
      setEdits((current) => ({ ...current, [plan.id]: null }));
    } catch (error) {
      setFailure(apiMessage(error));
    }
  }

  return (
    <>
      <header className="mb-3 flex flex-wrap items-center gap-3">
        <h1 className="iw-title flex items-center gap-2">
          <Globe2 aria-hidden="true" className="h-4 w-4 text-primary" />
          Planetary Interaction
        </h1>
      </header>
      <PlanetarySectionTabs />
      {failure ? <InlineAlert title="The plan could not be deleted">{failure}</InlineAlert> : null}
      {loaded.error ? (
        <InlineAlert title="Saved plans could not be loaded">{loaded.errorMessage}</InlineAlert>
      ) : loaded.data === null ? (
        <p className="text-sm text-muted">Loading saved plans…</p>
      ) : plans.length === 0 ? (
        <p className="text-sm text-muted">
          No saved plans yet. Solve one in the Planner and save it from the Plan tab to come back to it while you build
          it. <Link className="text-primary hover:underline" to="/planetary/planner">Open the Planner</Link>
        </p>
      ) : (
        <OperationalTable ariaLabel="Saved plans" columns={COLUMNS} onSelectRow={() => undefined} selectedRowKey={null}>
          <OperationalTableGroup groupKey="saved-plans" itemCount={plans.length} label="Saved plans">
            {plans.map((plan) => (
              <OperationalTableRow
                cells={{
                  name: (
                    <b className="block truncate text-xs" data-private="">
                      {plan.name}
                    </b>
                  ),
                  home: <span className="text-xs">{plan.homeSystemName ?? "—"}</span>,
                  isk: (
                    <span className="font-mono text-xs">
                      <IskCompact value={plan.iskPerMonth} />
                    </span>
                  ),
                  progress: (
                    <span className={`text-xs ${plan.setUp === plan.colonies ? "text-success" : ""}`}>
                      {plan.setUp} of {plan.colonies} set up
                    </span>
                  ),
                  saved: <span className="text-xs text-muted">{formatDate(plan.createdAt)}</span>,
                  prices: (
                    <span className="text-xs text-muted">{plan.priceObservedAt ? formatDate(plan.priceObservedAt) : "—"}</span>
                  ),
                  actions: (
                    <span onClick={(event) => event.stopPropagation()} onKeyDown={(event) => event.stopPropagation()}>
                      <DropdownMenu
                        align="end"
                        icon={<MoreHorizontal aria-hidden="true" className="h-4 w-4" />}
                        items={[
                          {
                            key: "rename",
                            label: "Rename",
                            icon: <Pencil aria-hidden="true" className="h-4 w-4" />,
                            onSelect: () => setRenaming(plan),
                          },
                          {
                            key: "delete",
                            label: "Delete",
                            icon: <Trash2 aria-hidden="true" className="h-4 w-4" />,
                            onSelect: () => setDeleting(plan),
                          },
                        ]}
                        label={`Actions for ${plan.name}`}
                        portal
                        variant="icon"
                      />
                    </span>
                  ),
                }}
                key={plan.id}
                onActivate={() => navigate(`/planetary/plans/${plan.id}`)}
                rowKey={plan.id}
              />
            ))}
          </OperationalTableGroup>
        </OperationalTable>
      )}
      {renaming ? (
        <PlanNameDialog
          initialName={renaming.name}
          onCancel={() => setRenaming(null)}
          onSubmit={(name) => rename(renaming, name)}
          submitLabel="Save"
          title="Rename plan"
        />
      ) : null}
      <ConfirmDialog
        confirmLabel="Delete"
        onCancel={() => setDeleting(null)}
        onConfirm={() => deleting && void remove(deleting)}
        open={deleting !== null}
        title={`Delete ${deleting?.name ?? "plan"}?`}
      >
        Its colonies and set-up ticks are gone for good. The colonies in game stay as they are.
      </ConfirmDialog>
    </>
  );
}
