import { Link } from "react-router";

import type { ConsumedMaterial } from "../../../../api/inventory-analytics";
import { formatIskCompact, formatIskSummary } from "../../../../components/money";
import { OperationalTable, OperationalTableRow, type OperationalColumn } from "../../../../components/operational-table";
import { textToneClasses } from "../../../../components/primitives";
import { Sparkline } from "../../../../components/sparkline";
import { categoryColor } from "../material-colors";
import { formatQuantity } from "../runway/runway-format";
import { BURN_RATE_HISTORY_WEEKS, coverTone, formatCover } from "./consumption-format";

const COLUMNS: OperationalColumn[] = [
  { key: "material", label: "Material", width: "minmax(13rem, 1fr)", sticky: true },
  { key: "quantity", label: "Qty used", width: "6.5rem", align: "right", numeric: true },
  { key: "value", label: "ISK value", title: "Cost basis at consumption", width: "6.5rem", align: "right", numeric: true },
  { key: "share", label: "Share", width: "4.5rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "trend", label: "8 weeks", title: "Quantity per week", width: "5.5rem", hideBelow: "tablet" },
  { key: "average", label: "Avg / week", width: "6.5rem", align: "right", numeric: true, hideBelow: "desktop" },
  { key: "onHand", label: "On hand", width: "6.5rem", align: "right", numeric: true, hideBelow: "desktop" },
  { key: "cover", label: "Days of cover", title: "On hand at the average burn rate", width: "6.5rem", align: "right", numeric: true },
];

const NOT_ENOUGH_HISTORY = `Needs ${BURN_RATE_HISTORY_WEEKS} weeks of history`;

export function ConsumedMaterialsTable({ materials }: { materials: ConsumedMaterial[] }) {
  return (
    <OperationalTable ariaLabel="Consumed materials" columns={COLUMNS} onSelectRow={() => undefined} selectedRowKey={null}>
      <tbody>
        {materials.map((material) => (
          <OperationalTableRow
            cells={{
              material: (
                <span className="flex min-w-0 flex-col">
                  <Link className="truncate hover:text-primary hover:underline" title="Open in Stock" to={`/inventory?item=${material.typeId}`}>
                    {material.name}
                  </Link>
                  <span className="text-[0.625rem]" style={{ color: categoryColor(material.category) }}>{material.category}</span>
                </span>
              ),
              quantity: formatQuantity(material.quantity),
              value: <span title={formatIskSummary(material.value)}>{formatIskCompact(material.value)}</span>,
              share: <span className="text-muted">{material.sharePercent.toFixed(1)}%</span>,
              trend: <Sparkline color={categoryColor(material.category)} height={16} values={material.weeklyQuantity} width={56} />,
              average: material.averagePerWeek === null
                ? <span className="text-muted" title={NOT_ENOUGH_HISTORY}>—</span>
                : <span className="text-muted">{formatQuantity(material.averagePerWeek)}/wk</span>,
              onHand: formatQuantity(material.onHand),
              cover: material.daysOfCover === null
                ? <span className="text-muted" title={NOT_ENOUGH_HISTORY}>—</span>
                : <span className={textToneClasses[coverTone(material.daysOfCover)]}>{formatCover(material.daysOfCover)}</span>,
            }}
            interactive={false}
            key={material.typeId}
            rowKey={String(material.typeId)}
          />
        ))}
      </tbody>
    </OperationalTable>
  );
}
