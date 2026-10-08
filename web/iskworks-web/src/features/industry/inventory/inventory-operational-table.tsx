import { Check, Search } from "lucide-react";

import type { InventoryItem } from "../../../api/inventory";
import { formatIskCompact, formatIskSummary } from "../../../components/money";
import { EveTypeImage } from "../../../components/eve-type-image";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../components/operational-table";
import { hasEsiDiscrepancy } from "./inventory-filters";
import { sumDecimalStrings } from "../shared/decimal";

const INVENTORY_COLUMNS: OperationalColumn[] = [
  { key: "item", label: "Item", width: "minmax(220px,1fr)", sticky: true },
  { key: "owned", label: "Owned", width: "100px", align: "right", numeric: true },
  { key: "reserved", label: "Reserved", width: "100px", align: "right", numeric: true },
  { key: "available", label: "Available", width: "100px", align: "right", numeric: true },
  { key: "averageCost", label: "Avg Cost", width: "120px", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "historicalValue", label: "Historical Value", width: "130px", align: "right", numeric: true, hideBelow: "desktop" },
  { key: "marketPrice", label: "Market Price", width: "120px", align: "right", numeric: true, hideBelow: "desktop" },
  { key: "marketValue", label: "Market Value", width: "130px", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "esi", label: "ESI", width: "112px", align: "right" },
];

const OTHER_GROUP = "Other";

export interface InventoryGroup {
  key: string;
  label: string;
  items: InventoryItem[];
}

export function InventoryOperationalTable({
  expandedGroups,
  items,
  onSelectItem,
  onToggleGroup,
  onViewEsiHoldings,
  selectedTypeId = null,
}: {
  expandedGroups: Set<string>;
  items: InventoryItem[];
  onSelectItem: (typeId: number) => void;
  onToggleGroup: (groupKey: string, expanded: boolean) => void;
  onViewEsiHoldings: (item: InventoryItem) => void;
  selectedTypeId?: number | null;
}) {
  const groups = groupInventoryItems(items);
  return (
    <OperationalTable
      ariaLabel="Inventory"
      columns={INVENTORY_COLUMNS}
      onSelectRow={(key) => onSelectItem(typeIdFromRowKey(key))}
      selectedRowKey={selectedTypeId === null ? null : rowKeyForTypeId(selectedTypeId)}
    >
      {groups.map((group) => (
        <OperationalTableGroup
          expanded={expandedGroups.has(group.key)}
          groupKey={group.key}
          itemCount={group.items.length}
          key={group.key}
          label={group.label}
          onExpandedChange={(expanded) => onToggleGroup(group.key, expanded)}
          summary={<InventoryGroupSummary group={group} />}
        >
          {group.items.map((item) => (
            <InventoryRow item={item} key={rowKey(item)} onViewEsiHoldings={onViewEsiHoldings} />
          ))}
        </OperationalTableGroup>
      ))}
    </OperationalTable>
  );
}

function InventoryRow({
  item,
  onViewEsiHoldings,
}: {
  item: InventoryItem;
  onViewEsiHoldings: (item: InventoryItem) => void;
}) {
  const missingPrice = item.currentPrice === null;
  const status = missingPrice || !item.historicalComparisonComplete || hasEsiDiscrepancy(item)
    ? "warning" as const
    : "neutral" as const;
  const cells = {
    item: (
      <span className="flex min-w-0 items-center gap-2">
        <EveTypeImage size={24} typeId={item.balance.key.typeId} typeName={item.balance.typeName} />
        <span className="min-w-0 truncate font-medium">{item.balance.typeName}</span>
      </span>
    ),
    owned: <Quantity value={item.balance.quantity} />,
    reserved: <Quantity value={item.reservedQuantity} />,
    available: <Quantity value={item.availableQuantity} />,
    averageCost: item.balance.averageUnitCost
      ? <Money value={item.balance.averageUnitCost} />
      : <span className="text-xs text-warning">Unavailable</span>,
    historicalValue: <Money value={item.balance.totalHistoricalCost} />,
    marketPrice: item.currentPrice
      ? <Money value={item.currentPrice} />
      : <span className="text-xs text-warning">No price</span>,
    marketValue: item.currentValue
      ? <Money value={item.currentValue} />
      : <span className="text-xs text-warning">No price</span>,
    esi: <EsiBadge item={item} onViewHoldings={onViewEsiHoldings} />,
  };
  return <OperationalTableRow cells={cells} rowKey={rowKey(item)} status={status} />;
}

function EsiBadge({ item, onViewHoldings }: { item: InventoryItem; onViewHoldings: (item: InventoryItem) => void }) {
  if (item.esiObservedQuantity == null) {
    return <span className="text-xs text-muted" title="No ESI observation for this item">—</span>;
  }
  const difference = item.reconciliationDifference ?? 0;
  const isMatch = difference === 0;
  return (
    <span className="flex items-center justify-end gap-1">
      {isMatch ? (
        <span className="flex text-positive" title="ESI observation matches accounting inventory">
          <Check aria-hidden="true" className="h-4 w-4" />
        </span>
      ) : (
        <span
          className="whitespace-nowrap font-mono text-xs font-semibold text-warning"
          title={`ESI observed ${item.esiObservedQuantity.toLocaleString()}, accounting owns ${item.balance.quantity.toLocaleString()}`}
        >
          {difference > 0 ? "+" : ""}
          {difference.toLocaleString()}
        </span>
      )}
      <button
        aria-label="View ESI holdings"
        className="grid h-5 w-5 shrink-0 place-items-center rounded text-muted transition hover:bg-panel-strong hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
        onClick={(event) => { event.stopPropagation(); onViewHoldings(item); }}
        title="View ESI holdings"
        type="button"
      >
        <Search aria-hidden="true" className="h-3.5 w-3.5" />
      </button>
    </span>
  );
}

function InventoryGroupSummary({ group }: { group: InventoryGroup }) {
  const currentValue = sumDecimalStrings(group.items.map((item) => item.currentValue));
  return (
    <span title={groupSummaryTitle(group, currentValue)}>
      {group.items.length} items · {formatCurrentValueSummary(currentValue.value)}{currentValue.partial ? " partial" : ""}
    </span>
  );
}

function Quantity({ value }: { value: number }) {
  const fullValue = value.toLocaleString();
  return (
    <span className="whitespace-nowrap font-mono tabular-nums" title={fullValue}>
      {fullValue}
    </span>
  );
}

function Money({ value }: { value: string }) {
  return (
    <span className="whitespace-nowrap font-mono tabular-nums" title={formatIskSummary(value)}>
      {formatIskCompact(value)}
    </span>
  );
}

function formatCurrentValueSummary(value: string | null): string {
  return value === null ? "Value unavailable" : `${formatIskCompact(value)} ISK`;
}

function groupSummaryTitle(
  group: InventoryGroup,
  currentValue: ReturnType<typeof sumDecimalStrings>,
): string {
  return `${group.items.length} distinct items; ${currentValue.value ?? "unknown"} ISK current value${currentValue.partial ? " (partial)" : ""}`;
}

export function rowKey(item: InventoryItem): string {
  return rowKeyForTypeId(item.balance.key.typeId);
}

export function rowKeyForTypeId(typeId: number): string {
  return `inventory:${typeId}`;
}

function typeIdFromRowKey(key: string): number {
  return Number(key.slice("inventory:".length));
}

export function groupInventoryItems(items: InventoryItem[]): InventoryGroup[] {
  const byGroup = new Map<string, InventoryItem[]>();
  for (const item of items) {
    const label = item.groupName?.trim() || OTHER_GROUP;
    const existing = byGroup.get(label);
    if (existing) existing.push(item);
    else byGroup.set(label, [item]);
  }
  return [...byGroup.entries()]
    .sort(([left], [right]) => compareGroupLabels(left, right))
    .map(([label, groupItems]) => ({
      key: label,
      label,
      items: [...groupItems].sort((left, right) => left.balance.typeName.localeCompare(right.balance.typeName)),
    }));
}

function compareGroupLabels(left: string, right: string): number {
  if (left === OTHER_GROUP) return right === OTHER_GROUP ? 0 : 1;
  if (right === OTHER_GROUP) return -1;
  return left.localeCompare(right);
}
