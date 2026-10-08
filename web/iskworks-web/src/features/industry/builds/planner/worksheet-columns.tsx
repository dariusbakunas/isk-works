import type { OperationalColumn } from "../../../../components/operational-table";

export const PRODUCTION_WORKSHEET_COLUMNS: OperationalColumn[] = [
  { key: "item", label: "Item", width: "minmax(210px,1.8fr)", sticky: true },
  { key: "sourcing", label: "Sourcing", width: "132px" },
  { key: "required", label: "Required", width: "112px", align: "right", numeric: true },
  { key: "available", label: "Available", width: "112px", align: "right", numeric: true },
  { key: "covered", label: "Covered", width: "112px", align: "right", numeric: true },
  { key: "shortage", label: "Shortage", width: "112px", align: "right", numeric: true },
  { key: "coverage", label: "Coverage", width: "116px", align: "right", numeric: true },
  { key: "pricing", label: "Pricing", width: "82px", align: "right" },
  // Named "Unit Cost", not "Unit Price": a material row's value here is the
  // *effective planning cost per unit* (Buy: blended inventory+fresh price;
  // Build/Reaction: blended inventory+production cost; fully inventory: the
  // historical basis) -- never a synthesized market price for a
  // self-produced row. The output row is the one exception, still showing a
  // sale-side unit price in this same column (see `production-worksheet.tsx`).
  { key: "unitPrice", label: "Unit Cost", width: "88px", align: "right", numeric: true },
  { key: "totalValue", label: "Total Value", width: "98px", align: "right", numeric: true },
];
