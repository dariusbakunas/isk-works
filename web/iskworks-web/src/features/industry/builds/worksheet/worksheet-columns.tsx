import type { OperationalColumn } from "../../../../components/operational-table";

export const WORKSHEET_COLUMNS: OperationalColumn[] = [
  { key: "item", label: "Item", width: "minmax(220px,1fr)", sticky: true },
  { key: "sourcing", label: "Sourcing", width: "110px" },
  { key: "required", label: "Required", width: "96px", align: "right", numeric: true },
  { key: "covered", label: "Covered", width: "96px", align: "right", numeric: true },
  { key: "shortage", label: "Shortage", width: "96px", align: "right", numeric: true },
  { key: "coverage", label: "Coverage", width: "96px", align: "right", numeric: true },
  { key: "pricing", label: "Pricing", width: "110px" },
  { key: "unitCost", label: "Unit Cost", width: "110px", align: "right", numeric: true },
  { key: "totalValue", label: "Total Value ⓘ", title: "Estimated cost contribution at this level. Values across nested components are not additive.", width: "120px", align: "right", numeric: true },
];
