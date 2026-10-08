import type { KeyboardEvent } from "react";

import type { CategoryTotal } from "../../../api/finance-analytics";
import { formatIskAbbreviated, formatIskSummary } from "../../../components/money";
import { Private } from "../../../observability/private";
import { NoData } from "./category-donuts";
import { computeSankey, SANKEY, type SankeyNode } from "./sankey-layout";

const HEIGHT = 260;
const PAD = { left: 200, right: 200, top: 24, bottom: 8 } as const;
/** Least vertical distance between two labels in one column; closer ones are skipped. */
const LABEL_SPACING = 13;
const WIDTH = SANKEY.rightX + SANKEY.rightW;

/**
 * Income sources -> wallet -> spending and what was kept. Drawn from the same
 * category totals as the donuts; a selected category is highlighted and the
 * rest dimmed, and clicking a category node filters the page like the donuts.
 */
export function SankeyDiagram({
  income,
  spending,
  colors,
  selected,
  onSelect,
}: {
  income: CategoryTotal[];
  spending: CategoryTotal[];
  colors: Map<string, string>;
  selected: string | null;
  onSelect: (category: string | null) => void;
}) {
  const layout = computeSankey(income, spending, { height: HEIGHT });
  if (!layout) return <NoData>No money moved in this period.</NoData>;

  const colorOf = (node: SankeyNode) =>
    node.kind === "net" ? "var(--color-income)" : node.kind === "drawdown" ? "var(--color-expense)" : (colors.get(node.label) ?? "var(--color-faint)");
  const matches = (node: SankeyNode) => selected === null || node.category === selected;
  const selectable = (node: SankeyNode) => node.kind === "category" && node.category !== "Other";
  const toggle = (node: SankeyNode) => selectable(node) && onSelect(selected === node.category ? null : (node.category ?? null));
  const onKey = (node: SankeyNode) => (event: KeyboardEvent) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      toggle(node);
    }
  };
  const nodes = [...layout.left, ...layout.right];
  // Thin neighbouring nodes would print over each other, so a label that would
  // collide with the previous shown one is left out (its tooltip remains).
  const labelled = new Set<string>();
  for (const column of [layout.left, layout.right]) {
    let last = -Infinity;
    for (const node of column) {
      const y = node.y + node.h / 2;
      if (y - last >= LABEL_SPACING) {
        labelled.add(node.key);
        last = y;
      }
    }
  }
  const nodeByKey = new Map(nodes.map((node) => [node.key, node]));
  const summary = `Money flow: ${layout.left.length} sources into the wallet, ${layout.right.length} destinations out of it.`;

  return (
    <div className="overflow-x-auto">
      <svg
        aria-label={summary}
        className="min-w-[40rem]"
        role="group"
        viewBox={`${-PAD.left} ${-PAD.top} ${WIDTH + PAD.left + PAD.right} ${HEIGHT + PAD.top + PAD.bottom}`}
        width="100%"
      >
        {[...layout.leftLinks, ...layout.rightLinks].map((link) => {
          const node = nodeByKey.get(link.nodeKey)!;
          return (
            <path
              d={link.path}
              fill={colorOf(node)}
              fillOpacity={selected === null ? 0.22 : matches(node) ? 0.4 : 0.05}
              key={`${node.column}-${link.nodeKey}`}
            >
              <title>{`${node.label}${node.kind === "category" ? ` (${node.column === "left" ? "income" : "spending"})` : ""}: ${formatIskSummary(String(link.value))}`}</title>
            </path>
          );
        })}
        {/* A category can be earned and spent, so say which column is which. */}
        {(["left", "right"] as const).map((column) => (
          <text
            fill="var(--color-muted)"
            fontSize="10"
            fontWeight={600}
            key={column}
            letterSpacing="1.5"
            textAnchor={column === "left" ? "end" : "start"}
            x={column === "left" ? SANKEY.leftX - 6 : SANKEY.rightX + SANKEY.rightW + 6}
            y={-10}
          >
            {column === "left" ? "INCOME SOURCES" : "SPENDING"}
          </text>
        ))}
        <rect fill="var(--color-panel-strong)" height={HEIGHT} stroke="var(--color-border)" width={SANKEY.centerW} x={SANKEY.centerX} y={0} />
        <text
          fill="var(--color-muted)"
          fontSize="9"
          letterSpacing="2"
          textAnchor="middle"
          transform={`translate(${SANKEY.centerX + SANKEY.centerW / 2},${HEIGHT / 2}) rotate(-90)`}
        >
          WALLET
        </text>
        {nodes.map((node) => {
          const left = node.column === "left";
          const dim = selected !== null && !matches(node);
          const interactive = selectable(node);
          return (
            <Private
              aria-label={interactive ? `${node.label}, ${node.column === "left" ? "income" : "spending"}, ${formatIskAbbreviated(String(node.value), { currency: true })}` : undefined}
              aria-pressed={interactive ? selected === node.category : undefined}
              as="g"
              cursor={interactive ? "pointer" : undefined}
              key={node.key}
              onClick={interactive ? () => toggle(node) : undefined}
              onKeyDown={interactive ? onKey(node) : undefined}
              role={interactive ? "button" : undefined}
              tabIndex={interactive ? 0 : undefined}
            >
              <title>{`${node.label}${node.kind === "category" ? ` (${node.column === "left" ? "income" : "spending"})` : ""}: ${formatIskSummary(String(node.value))}`}</title>
              <rect fill={colorOf(node)} fillOpacity={dim ? 0.2 : 0.9} height={node.h} width={node.w} x={node.x} y={node.y} />
              {labelled.has(node.key) ? (
              <text
                fill={dim ? "var(--color-faint)" : "var(--color-foreground)"}
                fontSize="11"
                fontWeight={selected !== null && !dim ? 600 : 400}
                textAnchor={left ? "end" : "start"}
                x={left ? node.x - 6 : node.x + node.w + 6}
                y={node.y + node.h / 2 + (node.h >= 24 ? -1 : 4)}
              >
                {node.label}
              </text>
              ) : null}
              {labelled.has(node.key) && node.h >= 24 ? (
                <text
                  fill="var(--color-muted)"
                  fontFamily="var(--font-mono)"
                  fontSize="9.5"
                  textAnchor={left ? "end" : "start"}
                  x={left ? node.x - 6 : node.x + node.w + 6}
                  y={node.y + node.h / 2 + 11}
                >
                  {formatIskAbbreviated(String(node.value))}
                </text>
              ) : null}
            </Private>
          );
        })}
      </svg>
    </div>
  );
}
