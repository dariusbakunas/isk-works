import type { PiPlan, PlanColony, PlanFlow } from "../../../../api/pi-planner";
import { formatIskCompact } from "../../../../components/money";
import { formatQuantity } from "../planetary-format";

const NODE_W = 150;
const NODE_H = 44;
const ROW_GAP = 10;
const BAND_GAP = 18;
const HEADER_H = 26;
const WIDTH = 900;

/** The column a colony sits in: P1 for extraction, else its top output tier. */
export function colonyTier(colony: PlanColony): number {
  if (colony.role === "extraction") return 1;
  return Math.max(2, ...colony.recipes.map((recipe) => recipe.tier));
}

interface Edge {
  from: number | null;
  to: number | null;
  names: string[];
  m3PerHour: number;
  unitsPerHour: number;
  taxPerMonth: number;
  crossCharacter: boolean;
}

/** Flows merged per (from, to) pair, so each link draws once. */
export function edges(flows: PlanFlow[]): Edge[] {
  const merged = new Map<string, Edge>();
  for (const flow of flows) {
    const key = `${flow.from ?? "m"}>${flow.to ?? "m"}`;
    const edge = merged.get(key) ?? {
      from: flow.from,
      to: flow.to,
      names: [],
      m3PerHour: 0,
      unitsPerHour: 0,
      taxPerMonth: 0,
      crossCharacter: false,
    };
    if (!edge.names.includes(flow.name)) edge.names.push(flow.name);
    edge.m3PerHour += Number(flow.m3PerHour);
    edge.unitsPerHour += Number(flow.unitsPerHour);
    edge.taxPerMonth += Number(flow.taxPerMonth);
    edge.crossCharacter ||= flow.crossCharacter;
    merged.set(key, edge);
  }
  return [...merged.values()];
}

/** Stroke width in three steps by m³/h, relative to the busiest link. */
export function edgeWeight(m3PerHour: number, busiest: number): 1.5 | 3 | 5 {
  if (busiest <= 0) return 1.5;
  const share = m3PerHour / busiest;
  if (share > 2 / 3) return 5;
  if (share > 1 / 3) return 3;
  return 1.5;
}

function edgeLabel(edge: Edge): string {
  return `${edge.names.join(", ")} · ${formatQuantity(edge.unitsPerHour)}/h · tax ${formatIskCompact(
    String(edge.taxPerMonth),
  )}/mo${edge.crossCharacter ? " · between characters, needs hauling" : ""}`;
}

function endName(plan: PiPlan, end: number | null): string {
  return end === null ? "Market" : plan.colonies[end]?.planetName ?? "?";
}

/**
 * Product flow, one band per character: P1 (extract + refine) · P2 · P3 ·
 * P4 · Market. Below 900 px it is a list.
 */
export function FlowDiagram({
  characterNames,
  hovered,
  onHover,
  plan,
}: {
  characterNames: string[];
  hovered: number | null;
  onHover: (colony: number | null) => void;
  plan: PiPlan;
}) {
  const links = edges(plan.flows);
  const busiest = Math.max(0, ...links.map((edge) => edge.m3PerHour));
  const tiers = [...new Set(plan.colonies.map(colonyTier))].sort();
  const columns = [...tiers, "market" as const];
  const columnX = (index: number) =>
    columns.length === 1 ? 20 : 20 + (index * (WIDTH - 40 - NODE_W)) / (columns.length - 1);

  // Positions: bands by character, rows within each tier column.
  const position = new Map<number, { x: number; y: number }>();
  const bands: { character: number; top: number; height: number }[] = [];
  let top = HEADER_H;
  const characters = [...new Set(plan.colonies.map((colony) => colony.character))].sort((a, b) => a - b);
  for (const character of characters) {
    const mine = plan.colonies.filter((colony) => colony.character === character);
    let rows = 1;
    tiers.forEach((tier, column) => {
      const inColumn = mine.filter((colony) => colonyTier(colony) === tier);
      rows = Math.max(rows, inColumn.length);
      inColumn.forEach((colony, row) =>
        position.set(colony.index, { x: columnX(column), y: top + row * (NODE_H + ROW_GAP) }),
      );
    });
    const height = rows * (NODE_H + ROW_GAP) - ROW_GAP;
    bands.push({ character, top, height });
    top += height + BAND_GAP;
  }
  const height = Math.max(top - BAND_GAP + 10, HEADER_H + NODE_H + 10);
  const market = { x: columnX(columns.length - 1), y: (HEADER_H + height) / 2 - NODE_H / 2 };
  const at = (end: number | null) => (end === null ? market : position.get(end));

  return (
    <>
      <div className="hidden overflow-x-auto rounded-md border border-border bg-background lg:block" data-testid="flow-diagram">
        <svg aria-label="Product flow" className="block min-w-[900px]" role="img" viewBox={`0 0 ${WIDTH} ${height}`} width="100%">
          {columns.map((column, index) => (
            <text className="fill-muted text-[10px] uppercase" key={String(column)} x={columnX(index)} y={16}>
              {column === "market" ? "Market" : column === 1 ? "P1 · extract + refine" : `P${column}`}
            </text>
          ))}
          {bands.map((band) => (
            <text className="fill-faint text-[9px] uppercase" key={band.character} x={4} y={band.top - 4}>
              {characterNames[band.character] ?? `Character ${band.character + 1}`}
            </text>
          ))}
          {links.map((edge) => {
            const from = at(edge.from);
            const to = at(edge.to);
            if (!from || !to) return null;
            const x1 = from.x + NODE_W;
            const y1 = from.y + NODE_H / 2;
            const x2 = to.x;
            const y2 = to.y + NODE_H / 2;
            const bend = Math.max(30, (x2 - x1) / 2);
            const lit = hovered !== null && (edge.from === hovered || edge.to === hovered);
            return (
              <path
                className={lit ? "stroke-primary" : "stroke-primary/50"}
                d={`M${x1} ${y1} C${x1 + bend} ${y1} ${x2 - bend} ${y2} ${x2} ${y2}`}
                data-cross={edge.crossCharacter || undefined}
                fill="none"
                key={`${edge.from}-${edge.to}`}
                strokeDasharray={edge.crossCharacter ? "6 4" : undefined}
                strokeWidth={edgeWeight(edge.m3PerHour, busiest)}
              >
                <title>{edgeLabel(edge)}</title>
              </path>
            );
          })}
          {plan.colonies.map((colony) => {
            const spot = position.get(colony.index);
            if (!spot) return null;
            const steps = colony.role === "extraction"
              ? colony.exports.map((rate) => `${rate.name} · ${formatQuantity(Number(rate.unitsPerHour))}/h`)
              : colony.recipes.map((recipe) => `${recipe.name} (P${recipe.tier})`);
            return (
              <g
                data-colony={colony.index}
                key={colony.index}
                onMouseEnter={() => onHover(colony.index)}
                onMouseLeave={() => onHover(null)}
              >
                <rect
                  className={`fill-panel-strong ${hovered === colony.index ? "stroke-primary" : "stroke-border"}`}
                  height={NODE_H}
                  rx={3}
                  strokeWidth={hovered === colony.index ? 2 : 1}
                  width={NODE_W}
                  x={spot.x}
                  y={spot.y}
                />
                <text className="fill-foreground text-[10px] font-semibold" x={spot.x + 6} y={spot.y + 14}>
                  {colony.planetName}
                </text>
                {steps.slice(0, 2).map((step, line) => (
                  <text className="fill-muted text-[8px]" key={step} x={spot.x + 6} y={spot.y + 26 + line * 10}>
                    {step.length > 30 ? `${step.slice(0, 29)}…` : step}
                  </text>
                ))}
                <title>{steps.join("\n")}</title>
              </g>
            );
          })}
          <g>
            <rect className="fill-positive/10 stroke-positive/60" height={NODE_H} rx={NODE_H / 2} width={NODE_W} x={market.x} y={market.y} />
            <text className="fill-positive text-[10px]" x={market.x + 14} y={market.y + 26}>
              Sell at best buy
            </text>
          </g>
        </svg>
      </div>
      <ul className="divide-y divide-border rounded-md border border-border lg:hidden" aria-label="Product flow">
        {links.map((edge) => (
          <li className="flex flex-wrap items-center gap-x-2 px-2 py-1.5 text-xs" key={`${edge.from}-${edge.to}`}>
            <span className="text-foreground">
              {endName(plan, edge.from)} → {endName(plan, edge.to)}
            </span>
            <span className="text-muted">{edge.names.join(", ")}</span>
            <span className="ml-auto font-mono text-muted">{formatQuantity(edge.unitsPerHour)}/h</span>
            {edge.crossCharacter ? <span className="text-warning">between characters</span> : null}
          </li>
        ))}
      </ul>
    </>
  );
}
