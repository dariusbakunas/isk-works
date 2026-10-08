import type { CategoryTotal } from "../../../api/finance-analytics";
import { numeric } from "./analytics-format";

/** Column geometry in SVG units (the prototype's proportions). */
export const SANKEY = { leftX: 0, leftW: 18, centerX: 432, centerW: 22, rightX: 878, rightW: 18, curve: 130 } as const;

export interface SankeyNode {
  key: string;
  label: string;
  value: number;
  /** `net` is money kept; `drawdown` is money drawn from the wallet to cover spending. */
  kind: "category" | "net" | "drawdown";
  column: "left" | "right";
  /** Set for category nodes so a click can filter by it. */
  category?: string;
  x: number;
  w: number;
  y: number;
  h: number;
}

export interface SankeyLink {
  nodeKey: string;
  value: number;
  sourceY: number;
  sourceH: number;
  targetY: number;
  targetH: number;
  path: string;
}

export interface SankeyLayout {
  left: SankeyNode[];
  right: SankeyNode[];
  /** Ribbons from each income source into the wallet bar. */
  leftLinks: SankeyLink[];
  /** Ribbons from the wallet bar out to each spending node. */
  rightLinks: SankeyLink[];
  /** Larger of income and spending; both columns are scaled to it. */
  total: number;
  height: number;
}

interface Options {
  height?: number;
  gap?: number;
  minNodeHeight?: number;
}

function ribbon(x0: number, y0: number, h0: number, x1: number, y1: number, h1: number): string {
  const c = SANKEY.curve;
  const f = (n: number) => n.toFixed(1);
  return [
    `M${f(x0)},${f(y0)}`,
    `C${f(x0 + c)},${f(y0)} ${f(x1 - c)},${f(y1)} ${f(x1)},${f(y1)}`,
    `L${f(x1)},${f(y1 + h1)}`,
    `C${f(x1 - c)},${f(y1 + h1)} ${f(x0 + c)},${f(y0 + h0)} ${f(x0)},${f(y0 + h0)}`,
    "Z",
  ].join(" ");
}

/**
 * Income sources flow into the wallet, and the wallet flows out to spending
 * plus whatever was kept. When spending exceeds income the shortfall enters as
 * a Drawdown source, so both columns always total the same amount. Pure: the
 * component only draws what this returns.
 */
export function computeSankey(
  income: CategoryTotal[],
  spending: CategoryTotal[],
  { height = 260, gap = 2, minNodeHeight = 3 }: Options = {},
): SankeyLayout | null {
  const positive = (rows: CategoryTotal[]) =>
    rows.map((row) => ({ category: row.category, value: numeric(row.total) })).filter((row) => row.value > 0);
  const incomeRows = positive(income);
  const spendingRows = positive(spending);
  const incomeTotal = incomeRows.reduce((sum, row) => sum + row.value, 0);
  const spendingTotal = spendingRows.reduce((sum, row) => sum + row.value, 0);
  const total = Math.max(incomeTotal, spendingTotal);
  if (total <= 0) return null;

  type Draft = Pick<SankeyNode, "key" | "label" | "value" | "kind" | "category">;
  const leftDrafts: Draft[] = incomeRows.map((row) => ({ key: `income:${row.category}`, label: row.category, value: row.value, kind: "category", category: row.category }));
  const rightDrafts: Draft[] = spendingRows.map((row) => ({ key: `spending:${row.category}`, label: row.category, value: row.value, kind: "category", category: row.category }));
  if (spendingTotal > incomeTotal) {
    leftDrafts.push({ key: "drawdown", label: "Drawdown", value: spendingTotal - incomeTotal, kind: "drawdown" });
  } else if (incomeTotal > spendingTotal) {
    rightDrafts.push({ key: "net", label: "Net saved", value: incomeTotal - spendingTotal, kind: "net" });
  }

  const stack = (drafts: Draft[], column: "left" | "right"): SankeyNode[] => {
    const usable = height - gap * Math.max(drafts.length - 1, 0);
    const x = column === "left" ? SANKEY.leftX : SANKEY.rightX;
    const w = column === "left" ? SANKEY.leftW : SANKEY.rightW;
    let y = 0;
    return drafts.map((draft) => {
      const h = Math.max(minNodeHeight, (draft.value / total) * usable);
      const node: SankeyNode = { ...draft, column, x, w, y, h };
      y += h + gap;
      return node;
    });
  };
  const left = stack(leftDrafts, "left");
  const right = stack(rightDrafts, "right");

  let leftCursor = 0;
  const leftLinks = left.map((node): SankeyLink => {
    const slice = (node.value / total) * height;
    const link: SankeyLink = {
      nodeKey: node.key,
      value: node.value,
      sourceY: node.y,
      sourceH: node.h,
      targetY: leftCursor,
      targetH: slice,
      path: ribbon(node.x + node.w, node.y, node.h, SANKEY.centerX, leftCursor, slice),
    };
    leftCursor += slice;
    return link;
  });
  let rightCursor = 0;
  const rightLinks = right.map((node): SankeyLink => {
    const slice = (node.value / total) * height;
    const link: SankeyLink = {
      nodeKey: node.key,
      value: node.value,
      sourceY: rightCursor,
      sourceH: slice,
      targetY: node.y,
      targetH: node.h,
      path: ribbon(SANKEY.centerX + SANKEY.centerW, rightCursor, slice, node.x, node.y, node.h),
    };
    rightCursor += slice;
    return link;
  });

  return { left, right, leftLinks, rightLinks, total, height };
}
