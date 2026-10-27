import { ChevronLeft, CircleCheck, CircleHelp, CircleX, Copy, CopyPlus, Pencil, Trash2, type LucideIcon } from "lucide-react";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";

import {
  createFit,
  deleteFit,
  exportFitText,
  type CatalogueItem,
  type FitDetail,
  type FitPilot,
  type FitStatsView,
  type FlyabilityStatus,
  type SlotKind,
} from "../../api/fittings";
import { EveTypeImage } from "../../components/eve-type-image";
import { ConfirmDialog, InlineAlert } from "../../components/primitives";
import { PRIVATE_ATTR } from "../../observability/private";
import { EngineUnavailableHint, PilotPortrait, useEngineAvailable, useFittingsContext } from "./fit-shared";
import { capacitorLabel, formatPercent, formatStat, rackLabel } from "./fittings-format";
import {
  addStack,
  addToFreeSlot,
  cycleState,
  draftFrom,
  fillRack,
  placeAt,
  removeItem,
  replaceType,
  setCharge,
  setQuantity,
  setState,
  toFitDraft,
  type RackSizes,
} from "./fit-draft";
import { ModuleBrowser, type DraggedType } from "./module-browser";
import { VariationsDialog } from "./variations-dialog";
import { ModuleTable } from "./module-table";
import { PilotsPanel } from "./pilots-panel";
import { StatsRail } from "./stats-rail";
import { useFitEditor } from "./use-fit-editor";
import { useFit, useFitParams, useFitPilots, useFitStats } from "./use-fittings";

const STATUS: Record<FlyabilityStatus | "unknown", { label: string; icon: LucideIcon; className: string }> = {
  canUseAll: { label: "can use all", icon: CircleCheck, className: "text-positive" },
  canFlyHull: { label: "hull only", icon: CircleCheck, className: "text-warning" },
  cannot: { label: "can't fly", icon: CircleX, className: "text-danger" },
  unknown: { label: "skills unknown", icon: CircleHelp, className: "text-muted" },
};

export function statusOf(pilot: Pick<FitPilot, "status">) {
  return STATUS[pilot.status ?? "unknown"];
}

const chipClass = (selected: boolean) =>
  `flex h-8 shrink-0 items-center gap-1.5 rounded-md border px-1.5 text-xs ${
    selected ? "border-primary bg-primary/15 text-foreground" : "border-border bg-background text-muted hover:text-foreground"
  }`;

function PilotChips({ pilots, selected, onSelect }: { pilots: FitPilot[]; selected: string; onSelect: (pilot: string) => void }) {
  return (
    <div aria-label="Pilot" className="flex items-center gap-1 overflow-x-auto" role="group">
      {pilots.map((pilot) => {
        const status = statusOf(pilot);
        const Icon = status.icon;
        return (
          <button
            aria-label={`${pilot.name}: ${status.label}`}
            aria-pressed={selected === pilot.connectionId}
            className={chipClass(selected === pilot.connectionId)}
            key={pilot.connectionId}
            onClick={() => onSelect(pilot.connectionId)}
            type="button"
          >
            <PilotPortrait connectionId={pilot.connectionId} name={pilot.name} />
            <span {...PRIVATE_ATTR}>{pilot.name}</span>
            <Icon aria-hidden="true" className={`h-3 w-3 ${status.className}`} />
          </button>
        );
      })}
      {(["allV", "allZero"] as const).map((choice) => (
        <button
          aria-pressed={selected === choice}
          className={`${chipClass(selected === choice)} font-mono`}
          key={choice}
          onClick={() => onSelect(choice)}
          type="button"
        >
          {choice === "allV" ? "All V" : "All 0"}
        </button>
      ))}
    </div>
  );
}

function Header({
  fit,
  pilots,
  onChanged,
  editing,
  name,
  onRename,
  onEdit,
}: {
  fit: FitDetail;
  pilots: FitPilot[];
  onChanged: () => void;
  editing: boolean;
  /** The name shown: the draft's while editing. */
  name: string;
  onRename: (name: string) => void;
  onEdit: () => void;
}) {
  const navigate = useNavigate();
  const { meta } = useFittingsContext();
  const engineAvailable = useEngineAvailable();
  const { pilot, profile, setPilot, setProfile } = useFitParams();
  const [copied, setCopied] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const shipName = fit.ship.name ?? `Type ${fit.ship.typeId}`;

  const run = async (action: () => Promise<void>) => {
    setError(null);
    try {
      await action();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "That didn't work.");
    }
  };

  return (
    <div className="space-y-2" data-testid="fit-header">
      <div className="flex flex-wrap items-center gap-3">
        <Link className="flex items-center gap-0.5 text-xs text-muted hover:text-foreground" to="/fittings">
          <ChevronLeft aria-hidden="true" className="h-3.5 w-3.5" />
          Fits
        </Link>
        <EveTypeImage size={64} typeId={fit.ship.typeId} typeName={shipName} variation="render" />
        <div className="min-w-0">
          <p className="text-[0.6875rem] text-muted">{shipName}</p>
          {editing ? (
            <label className="block">
              <span className="sr-only">Fit name</span>
              <input
                {...PRIVATE_ATTR}
                className="iw-input w-72 text-base font-semibold"
                onChange={(event) => onRename(event.target.value)}
                value={name}
              />
            </label>
          ) : (
            <h1 {...PRIVATE_ATTR} className="iw-title">{fit.name}</h1>
          )}
          <p className="text-[0.625rem] uppercase tracking-wide text-muted">{fit.ship.groupName}</p>
        </div>
        <div className="ml-auto flex flex-wrap items-center gap-2">
          {editing ? null : (
            <button className="iw-button-primary gap-1.5" disabled={!engineAvailable} onClick={onEdit} type="button">
              <Pencil aria-hidden="true" className="h-3.5 w-3.5" />
              Edit
            </button>
          )}
          <button
            className="iw-button-secondary gap-1.5"
            disabled={!engineAvailable}
            onClick={() =>
              void run(async () => {
                await navigator.clipboard.writeText(await exportFitText(fit.id));
                setCopied(true);
                window.setTimeout(() => setCopied(false), 2000);
              })
            }
            type="button"
          >
            <Copy aria-hidden="true" className="h-3.5 w-3.5" />
            Copy for in-game import
          </button>
          {copied ? <span className="text-xs text-positive" role="status">Copied</span> : null}
          <button
            className="iw-button-secondary gap-1.5"
            onClick={() =>
              void run(async () => {
                const copy = await createFit({ ...toFitDraft(draftFrom(fit), fit.ship.typeId), name: `${fit.name} (copy)` });
                onChanged();
                navigate(`/fittings/${copy.id}`);
              })
            }
            type="button"
          >
            <CopyPlus aria-hidden="true" className="h-3.5 w-3.5" />
            Duplicate
          </button>
          <button className="iw-button-danger gap-1.5" onClick={() => setConfirmDelete(true)} type="button">
            <Trash2 aria-hidden="true" className="h-3.5 w-3.5" />
            Delete
          </button>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <PilotChips onSelect={setPilot} pilots={pilots} selected={pilot} />
        <label className="ml-auto flex items-center gap-1.5 text-xs text-muted">
          Damage profile
          <select className="iw-input" onChange={(event) => setProfile(event.target.value)} value={profile}>
            {(meta?.damageProfiles ?? []).map((preset) => (
              <option key={preset.id} value={preset.id}>{preset.name}</option>
            ))}
          </select>
        </label>
      </div>
      {error ? <InlineAlert title="That didn't work">{error}</InlineAlert> : null}
      <ConfirmDialog
        confirmLabel="Delete fit"
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() =>
          void run(async () => {
            setConfirmDelete(false);
            await deleteFit(fit.id);
            onChanged();
            navigate("/fittings");
          })
        }
        open={confirmDelete}
        title={`Delete ${fit.name}?`}
      >
        The fit is removed for everyone in the workspace. Skill plans made from it stay.
      </ConfirmDialog>
    </div>
  );
}

/** The headline stats in one line, for screens too narrow for the rail
 * beside the table (the rail follows below there). */
function SummaryStrip({ view }: { view: FitStatsView }) {
  const { defense, offense, capacitor, navigation } = view.stats;
  const cap = capacitor.state.kind === "stable"
    ? formatPercent(capacitor.state.level)
    : capacitorLabel(capacitor.state).replace("Depletes in ", "");
  return (
    <p
      className="iw-panel flex flex-wrap gap-x-3 gap-y-1 px-3 py-2 font-mono text-xs text-muted xl:hidden"
      data-testid="stats-summary"
    >
      <span>EHP <span className="text-foreground">{formatStat(defense.ehp)}</span></span>
      <span>DPS <span className="text-foreground">{formatStat(offense.totalDps, 1)}</span></span>
      <span>Cap <span className="text-foreground">{cap}</span></span>
      <span className="text-foreground">{formatStat(navigation.maxVelocity, 1)} m/s</span>
    </p>
  );
}

/** The bar under an edited fit: how much changed, discard and save. */
function EditBar({ editor }: { editor: ReturnType<typeof useFitEditor> }) {
  const { changes, restored, saving, error } = editor;
  return (
    <section
      aria-label="Unsaved changes"
      className="sticky bottom-3 z-30 flex flex-wrap items-center gap-3 rounded-md border border-primary/60 bg-panel px-3 py-2 shadow-2xl"
    >
      <span className={`text-xs ${changes ? "text-warning" : "text-muted"}`}>
        {restored ? "Unsaved changes restored · " : ""}
        {changes === 0 ? "No unsaved changes" : `${changes} unsaved ${changes === 1 ? "change" : "changes"}`}
      </span>
      {error ? <span className="text-xs text-danger">{error}</span> : null}
      <span className="ml-auto flex gap-2">
        <button className="iw-button-secondary" onClick={editor.discard} type="button">Discard</button>
        <button className="iw-button-primary" disabled={changes === 0 || saving} onClick={() => void editor.save()} type="button">
          {saving ? "Saving…" : "Save fit"}
        </button>
      </span>
    </section>
  );
}

export function FitView({ onChanged }: { onChanged: () => void }) {
  const { fitId = "" } = useParams();
  const fit = useFit(fitId);

  if (fit.error) return <InlineAlert title="Couldn't load the fit">{fit.error.message}</InlineAlert>;
  if (!fit.data) return <p className="text-sm text-muted">Loading fit…</p>;
  return <LoadedFit fit={fit.data} key={fit.data.id} onChanged={onChanged} reload={fit.retry} />;
}

function LoadedFit({ fit, onChanged, reload }: { fit: FitDetail; onChanged: () => void; reload: () => void }) {
  const engineAvailable = useEngineAvailable();
  const { pilot, profile, setPilot } = useFitParams();
  const pilots = useFitPilots(fit.id, profile);
  const stats = useFitStats(fit.id, pilot, profile, engineAvailable);
  const statsView: FitStatsView | null = engineAvailable ? stats.data : null;
  const editor = useFitEditor({
    fit,
    pilot,
    profile,
    engineAvailable,
    onSaved: () => {
      reload();
      stats.retry();
      pilots.retry();
      onChanged();
    },
  });

  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [variationsFor, setVariationsFor] = useState<number | null>(null);
  const items = editor.draft?.items ?? fit.items;
  const racks: RackSizes | null = statsView
    ? {
        high: statsView.stats.resources.highSlots.total,
        med: statsView.stats.resources.medSlots.total,
        low: statsView.stats.resources.lowSlots.total,
        rig: statsView.stats.resources.rigSlots.total,
      }
    : null;

  const addModule = (entry: CatalogueItem) => {
    if (!editor.draft || !entry.slot || !racks) return;
    const result = addToFreeSlot(editor.draft, entry, entry.slot, racks);
    if (result.placed) {
      editor.setDraft(() => result.draft);
      setMessage(null);
    } else {
      setMessage(`No free ${rackLabel(entry.slot).replace(/ slots$/, "").toLowerCase()} slot — drop it on a slot to replace one.`);
    }
  };
  const drop = (type: DraggedType, kind: SlotKind, slotIndex: number) => {
    if (type.slot !== kind) {
      setMessage(`${type.name} doesn't go in ${rackLabel(kind).toLowerCase()}.`);
      return;
    }
    editor.setDraft((draft) => placeAt(draft, type, kind, slotIndex));
    setMessage(null);
  };
  // While the draft differs, rows show its calculation only once it is
  // current; the rail keeps the last one, against the saved fit's.
  const tableStats = editor.differs ? editor.current : statsView;
  const railStats = editor.differs ? (editor.latest ?? statsView) : statsView;

  return (
    <div className="space-y-3">
      <Header
        editing={editor.editing}
        fit={fit}
        name={editor.draft?.name ?? fit.name}
        onChanged={onChanged}
        onEdit={editor.start}
        onRename={(name) => editor.setDraft((draft) => ({ ...draft, name }))}
        pilots={pilots.data ?? []}
      />
      {engineAvailable ? null : <EngineUnavailableHint />}
      {stats.error ? <InlineAlert title="Couldn't calculate the fit">{stats.error.message}</InlineAlert> : null}
      {railStats ? <SummaryStrip view={railStats} /> : null}
      <div
        className={`grid gap-3 ${editor.editing ? "xl:grid-cols-[260px_minmax(0,1fr)_320px]" : "xl:grid-cols-[minmax(0,1fr)_320px]"}`}
      >
        {editor.editing ? (
          <ModuleBrowser
            message={message}
            onAddModule={addModule}
            onAddStack={(type, kind) => editor.setDraft((draft) => addStack(draft, type, kind, 1))}
            onLoadCharge={(charge) => {
              if (selectedIndex !== null) editor.setDraft((draft) => setCharge(draft, selectedIndex, charge));
            }}
            pilot={pilot}
            selected={selectedIndex !== null ? (items[selectedIndex] ?? null) : null}
            shipTypeId={fit.ship.typeId}
          />
        ) : null}
        <div className="min-w-0 space-y-3">
          <ModuleTable
            editor={
              editor.editing
                ? {
                    selectedIndex,
                    onSelect: setSelectedIndex,
                    onDrop: drop,
                    onRemove: (index) => {
                      editor.setDraft((draft) => removeItem(draft, index));
                      setSelectedIndex(null);
                    },
                    onFillRack: (index) => {
                      const item = items[index];
                      if (racks && item) editor.setDraft((draft) => fillRack(draft, { typeId: item.typeId, name: item.name ?? "" }, item.slotKind, racks));
                    },
                    onUnloadCharge: (index) => editor.setDraft((draft) => setCharge(draft, index, null)),
                    onVariations: setVariationsFor,
                    onQuantity: (index, quantity) => editor.setDraft((draft) => setQuantity(draft, index, quantity)),
                    onCycleState: (index) =>
                      editor.setDraft((draft) =>
                        setState(
                          draft,
                          index,
                          cycleState(draft.items[index].state, tableStats?.items[index]?.maxState ?? "overload"),
                        ),
                      ),
                  }
                : undefined
            }
            items={items}
            rackStats={statsView?.stats ?? null}
            stats={tableStats}
          />
          <PilotsPanel fit={fit} onSelect={setPilot} pilots={pilots.data ?? []} selected={pilot} />
        </div>
        <StatsRail
          baseline={editor.differs ? (statsView?.stats ?? null) : null}
          fitId={fit.id}
          recalculating={editor.calculating}
          stats={railStats}
        />
      </div>
      {editor.editing ? <EditBar editor={editor} /> : null}
      {variationsFor !== null && items[variationsFor] ? (
        <VariationsDialog
          item={items[variationsFor]}
          onClose={() => setVariationsFor(null)}
          onPick={(entry) => {
            const index = variationsFor;
            editor.setDraft((draft) => replaceType(draft, index, entry));
            setVariationsFor(null);
          }}
          pilot={pilot}
        />
      ) : null}
    </div>
  );
}
