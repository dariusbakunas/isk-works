import type { PriceSourceItem } from "../../../api/industry";
import { formatIskCompact, formatIskSummary } from "../../../components/money";
import { EveTypeImage } from "../../../components/eve-type-image";
import {
  OperationalTable,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../components/operational-table";
import { formatDate } from "../shared/formatting";

const PRICE_COLUMNS: OperationalColumn[] = [
  { key: "item", label: "Item", width: "minmax(220px,1fr)", sticky: true },
  { key: "typeId", label: "Type ID", width: "100px", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "updated", label: "Updated", width: "140px", hideBelow: "desktop" },
  { key: "price", label: "Price", width: "130px", align: "right", numeric: true },
];

export function PriceItemsOperationalTable({
  items,
  onSelectItem,
  selectedTypeId = null,
}: {
  items: PriceSourceItem[];
  onSelectItem: (typeId: number) => void;
  selectedTypeId?: number | null;
}) {
  return (
    <OperationalTable
      ariaLabel="Prices"
      columns={PRICE_COLUMNS}
      onSelectRow={(key) => onSelectItem(typeIdFromRowKey(key))}
      selectedRowKey={selectedTypeId === null ? null : rowKeyForTypeId(selectedTypeId)}
    >
      <tbody>
        {items.map((item) => <PriceRow item={item} key={rowKey(item)} />)}
      </tbody>
    </OperationalTable>
  );
}

function PriceRow({ item }: { item: PriceSourceItem }) {
  const cells = {
    item: (
      <span className="flex min-w-0 items-center gap-2">
        <EveTypeImage size={24} typeId={item.typeId} typeName={item.typeName} />
        <span className="min-w-0 truncate font-medium">{item.typeName}</span>
      </span>
    ),
    typeId: <span className="text-muted">{item.typeId}</span>,
    updated: <span className="text-muted">{formatDate(item.updatedAt)}</span>,
    price: <Money value={item.price} />,
  };
  return <OperationalTableRow cells={cells} rowKey={rowKey(item)} />;
}

function Money({ value }: { value: string }) {
  return (
    <span className="whitespace-nowrap font-mono tabular-nums" title={formatIskSummary(value)}>
      {formatIskCompact(value)}
    </span>
  );
}

export function rowKey(item: PriceSourceItem): string {
  return rowKeyForTypeId(item.typeId);
}

export function rowKeyForTypeId(typeId: number): string {
  return `price:${typeId}`;
}

function typeIdFromRowKey(key: string): number {
  return Number(key.slice("price:".length));
}
