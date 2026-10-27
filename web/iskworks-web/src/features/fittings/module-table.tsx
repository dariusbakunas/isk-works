import { Circle, CircleCheck, CircleDot, Flame, Lock, MoreHorizontal } from "lucide-react";

import type {
  FitItemView,
  FitStatsView,
  FitViolation,
  ItemStats,
  ModuleState,
  SlotKind,
} from "../../api/fittings";
import { DropdownMenu } from "../../components/dropdown-menu";
import { EveTypeImage } from "../../components/eve-type-image";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "../../components/operational-table";
import { formatDistance, formatHeadline, formatStat, rackLabel, stateLabel } from "./fittings-format";
import { DRAG_TYPE, type DraggedType } from "./module-browser";

const COLUMNS: OperationalColumn[] = [
  { key: "state", label: "", width: "28px" },
  { key: "module", label: "Module", width: "minmax(220px, 2fr)" },
  { key: "charge", label: "Charge", width: "minmax(120px, 1fr)", hideBelow: "tablet" },
  { key: "cpu", label: "CPU", width: "56px", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "pg", label: "PG", width: "64px", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "range", label: "Optimal + falloff", width: "128px", align: "right", hideBelow: "desktop" },
  { key: "cap", label: "Cap/s", width: "64px", align: "right", numeric: true, hideBelow: "desktop" },
  { key: "effect", label: "Effect", width: "128px", align: "right", hideBelow: "tablet" },
];

/** Racks with positions, shown with their empty slots up to the ship's count. */
const RACKS: Array<{ kind: SlotKind; total: (stats: FitStatsView["stats"]) => number }> = [
  { kind: "high", total: (stats) => stats.resources.highSlots.total },
  { kind: "med", total: (stats) => stats.resources.medSlots.total },
  { kind: "low", total: (stats) => stats.resources.lowSlots.total },
  { kind: "rig", total: (stats) => stats.resources.rigSlots.total },
];

const OTHER_GROUPS: SlotKind[] = ["subsystem", "service", "fighter_tube", "implant", "booster", "drone_bay", "fighter_bay", "cargo"];

const EMPTY_LABELS: Partial<Record<SlotKind, string>> = {
  high: "Empty high slot",
  med: "Empty med slot",
  low: "Empty low slot",
  rig: "Empty rig slot",
};

const STATE_ICONS: Record<ModuleState, { icon: typeof Circle; className: string }> = {
  offline: { icon: Circle, className: "text-muted" },
  online: { icon: CircleDot, className: "text-foreground" },
  active: { icon: CircleCheck, className: "text-positive" },
  overload: { icon: Flame, className: "text-warning" },
};

/** The state a module reached (or, without stats, the one it asks for). */
function StateGlyph({ requested, reached }: { requested: ModuleState; reached: ModuleState | null }) {
  const shown = reached ?? requested;
  const { icon: Icon, className } = STATE_ICONS[shown];
  const lower = reached !== null && reached !== requested;
  const label = lower ? `${stateLabel(shown)} (requested ${stateLabel(requested)})` : stateLabel(shown);
  return (
    <span aria-label={label} className={`inline-flex ${lower ? "opacity-60" : ""}`} role="img" title={label}>
      <Icon aria-hidden="true" className={`h-3.5 w-3.5 ${className}`} />
    </span>
  );
}

/** A short name for a fitting rule an item breaks. */
export function violationLabel(violation: FitViolation): string {
  const detail = violation.detail as { resource?: string };
  switch (violation.rule) {
    case "resource": {
      const resource = { cpu: "CPU", powergrid: "PG", calibration: "calibration" }[detail.resource ?? ""] ?? detail.resource;
      return `Over ${resource}`;
    }
    case "slots":
      return "No free slot";
    case "wrong_slot":
      return "Wrong rack";
    case "slot_taken":
      return "Slot taken";
    case "max_group":
    case "max_type":
      return "Limit reached";
    case "skill":
      return "Missing skill";
    case "charge_group":
    case "charge_size":
      return "Wrong charge";
    case "rig_size":
      return "Wrong rig size";
    case "ship_restricted":
    case "capital_item":
      return "Can't fit this ship";
    default:
      return violation.rule.replaceAll("_", " ");
  }
}

function Chip({ children, tone }: { children: string; tone: "danger" | "muted" }) {
  const classes = tone === "danger" ? "border-danger/60 text-danger" : "border-border text-muted";
  return <span className={`shrink-0 rounded-[2px] border px-1 py-px text-[0.625rem] ${classes}`}>{children}</span>;
}

/** What the table offers while the fit is being edited. */
export interface ModuleTableEditor {
  onCycleState: (index: number) => void;
  /** The row picked (for its charges in the browser). */
  selectedIndex: number | null;
  onSelect: (index: number) => void;
  /** A browser module dropped on a slot (a module's or an empty one). */
  onDrop: (type: DraggedType, kind: SlotKind, slotIndex: number) => void;
  onRemove: (index: number) => void;
  onFillRack: (index: number) => void;
  onUnloadCharge: (index: number) => void;
  onVariations: (index: number) => void;
  onQuantity: (index: number, quantity: number) => void;
}

function ModuleRow({
  index,
  item,
  stats,
  blocked,
  unknown,
  violations,
  editor,
}: {
  index: number;
  item: FitItemView;
  stats: ItemStats | null;
  blocked: boolean;
  unknown: boolean;
  violations: FitViolation[];
  editor?: ModuleTableEditor;
}) {
  const name = item.name ?? `Type ${item.typeId}`;
  const stack = item.slotKind === "drone_bay" || item.slotKind === "cargo" || item.slotKind === "fighter_bay";
  const dash = <span className="text-muted">—</span>;
  return (
    <OperationalTableRow
      accent={violations.length ? "blocking" : undefined}
      cells={{
        state: editor && !stack ? (
          <button
            aria-label={`State: ${stateLabel(item.state)} — change`}
            className="rounded p-0.5 hover:bg-panel-strong"
            onClick={() => editor.onCycleState(index)}
            type="button"
          >
            <StateGlyph reached={stats?.reachedState ?? null} requested={item.state} />
          </button>
        ) : (
          <StateGlyph reached={stats?.reachedState ?? null} requested={item.state} />
        ),
        module: (
          <span className="flex min-w-0 items-center gap-1.5">
            <EveTypeImage size={24} typeId={item.typeId} typeName={name} />
            {stack && editor ? (
              <span className="flex min-w-0 items-center gap-1">
                <input
                  aria-label={`Quantity of ${name}`}
                  className="iw-input w-16 py-0 text-right font-mono text-xs"
                  min={1}
                  onChange={(event) => {
                    const quantity = Number(event.target.value);
                    if (Number.isFinite(quantity) && quantity >= 1) editor.onQuantity(index, quantity);
                  }}
                  onClick={(event) => event.stopPropagation()}
                  type="number"
                  value={item.quantity}
                />
                <span className="truncate">× {name}</span>
              </span>
            ) : (
              <span className="truncate">{stack ? `${item.quantity.toLocaleString("en-US")} × ${name}` : name}</span>
            )}
            {blocked ? (
              <span aria-label="This pilot can't use it" className="text-warning" role="img" title="This pilot can't use it">
                <Lock aria-hidden="true" className="h-3 w-3" />
              </span>
            ) : null}
            {violations.map((violation, position) => (
              <Chip key={`${violation.rule}-${position}`} tone="danger">{violationLabel(violation)}</Chip>
            ))}
            {unknown ? <Chip tone="muted">Unknown to stats engine</Chip> : null}
            {editor ? (
              <span className="ml-auto shrink-0">
                <DropdownMenu
                  icon={<MoreHorizontal aria-hidden="true" className="h-3.5 w-3.5" />}
                  items={[
                    ...(stack
                      ? []
                      : [
                          { key: "variations", label: "Variations…", onSelect: () => editor.onVariations(index) },
                          { key: "fill", label: "Fill rack with this module", onSelect: () => editor.onFillRack(index) },
                        ]),
                    ...(item.chargeTypeId !== null
                      ? [{ key: "unload", label: "Unload charge", onSelect: () => editor.onUnloadCharge(index) }]
                      : []),
                    { key: "remove", label: "Remove", onSelect: () => editor.onRemove(index) },
                  ]}
                  label={`Actions for ${name}`}
                  portal
                  variant="icon"
                />
              </span>
            ) : null}
          </span>
        ),
        charge: item.chargeName ? <span className="truncate text-muted">{item.chargeName}</span> : null,
        cpu: stats && stats.cpu ? formatStat(stats.cpu, 1) : dash,
        pg: stats && stats.powergrid ? formatStat(stats.powergrid, 1) : dash,
        range:
          stats && (stats.optimal || stats.falloff)
            ? `${formatDistance(stats.optimal)} + ${formatDistance(stats.falloff)}`
            : dash,
        cap: stats && stats.capacitorPerSecond ? `−${formatStat(stats.capacitorPerSecond, 1)}` : dash,
        effect: stats ? formatHeadline(stats.headline) : dash,
      }}
      interactive={Boolean(editor)}
      rowKey={`item-${index}`}
    />
  );
}

/** The fit's racks (with empty slots), drones and cargo, with each item's
 * stats when the engine calculated them. */
export function ModuleTable({
  items,
  stats,
  rackStats = null,
  editor,
}: {
  items: FitItemView[];
  /** Index-parallel to `items`; `null` hides calculated values. */
  stats: FitStatsView | null;
  /** Rack sizes to use while `stats` is out of date (the saved fit's). */
  rackStats?: FitStatsView["stats"] | null;
  editor?: ModuleTableEditor;
}) {
  const indexed = items.map((item, index) => ({ item, index }));
  const blocked = new Set(stats?.flyability?.blockedItems ?? []);
  const unknown = new Set(stats?.unknownItems ?? []);
  const violationsOf = (index: number) =>
    (stats?.violations ?? []).filter(
      (violation) => violation.target.kind !== "ship" && violation.target.index === index,
    );
  const row = ({ item, index }: { item: FitItemView; index: number }) => (
    <ModuleRow
      blocked={blocked.has(index)}
      editor={editor}
      index={index}
      item={item}
      key={`item-${index}`}
      stats={stats?.items[index] ?? null}
      unknown={unknown.has(index)}
      violations={violationsOf(index)}
    />
  );

  const rackGroups = RACKS.map(({ kind, total }) => {
    const items = indexed
      .filter(({ item }) => item.slotKind === kind)
      .sort((a, b) => (a.item.slotIndex ?? 0) - (b.item.slotIndex ?? 0));
    const sizes = stats?.stats ?? rackStats;
    const slots = sizes ? total(sizes) : items.length;
    if (!items.length && !slots) return null;
    const taken = new Set(items.map(({ item }) => item.slotIndex));
    const empty = Array.from({ length: Math.max(0, slots) }, (_, position) => position).filter(
      (position) => !taken.has(position),
    );
    return (
      <OperationalTableGroup
        groupKey={kind}
        itemCount={items.length}
        key={kind}
        label={rackLabel(kind)}
        labelCase="normal"
        showCount={false}
        summary={
          <span className="font-mono text-muted" data-testid={`rack-usage-${kind}`}>
            {items.length} / {slots}
          </span>
        }
      >
        {items.map(row)}
        {empty.map((position) => (
          <OperationalTableRow
            cells={{ module: <span className="text-muted/70">{EMPTY_LABELS[kind]}</span> }}
            interactive={false}
            key={`empty-${kind}-${position}`}
            rowKey={`empty-${kind}-${position}`}
          />
        ))}
      </OperationalTableGroup>
    );
  });

  const otherGroups = OTHER_GROUPS.map((kind) => {
    const items = indexed.filter(({ item }) => item.slotKind === kind);
    if (!items.length) return null;
    const count = items.reduce((sum, { item }) => sum + item.quantity, 0);
    return (
      <OperationalTableGroup
        groupKey={kind}
        itemCount={count}
        key={kind}
        label={rackLabel(kind)}
        labelCase="normal"
      >
        {items.map(row)}
      </OperationalTableGroup>
    );
  });

  // Rows can't take drag handlers, so the wrapper finds the row dropped on.
  const dropTarget = (target: EventTarget): { kind: SlotKind; slotIndex: number } | null => {
    const key = (target as Element).closest?.("tr[data-row-key]")?.getAttribute("data-row-key") ?? "";
    const empty = /^empty-([a-z_]+)-(\d+)$/.exec(key);
    if (empty) return { kind: empty[1] as SlotKind, slotIndex: Number(empty[2]) };
    const filled = /^item-(\d+)$/.exec(key);
    const item = filled ? items[Number(filled[1])] : undefined;
    return item && item.slotIndex !== null ? { kind: item.slotKind, slotIndex: item.slotIndex } : null;
  };

  return (
    <div
      className="iw-panel"
      onDragOver={(event) => {
        if (editor && [...event.dataTransfer.types].includes(DRAG_TYPE)) event.preventDefault();
      }}
      onDrop={(event) => {
        if (!editor) return;
        const raw = event.dataTransfer.getData(DRAG_TYPE);
        const target = dropTarget(event.target);
        if (!raw || !target) return;
        event.preventDefault();
        editor.onDrop(JSON.parse(raw) as DraggedType, target.kind, target.slotIndex);
      }}
    >
      <OperationalTable
        ariaLabel="Modules"
        columns={COLUMNS}
        onSelectRow={(key) => {
          const match = /^item-(\d+)$/.exec(key);
          if (match && editor) editor.onSelect(Number(match[1]));
        }}
        selectedRowKey={editor && editor.selectedIndex !== null ? `item-${editor.selectedIndex}` : null}
      >
        {rackGroups}
        {otherGroups}
      </OperationalTable>
    </div>
  );
}
