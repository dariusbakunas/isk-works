// Build Graph node cards. One shared shell,
// four card bodies. Visual hierarchy: ROOT strongest -> BUILD/REACTION
// medium -> ACTIONABLE BUY compact -> inline Buy rows quiet. Kind = tint +
// left accent bar; selection = ring; data issue = chip. No new colour
// tokens -- manufacturing reuses `--color-primary`, reaction
// `--color-reaction`.

import { ChevronDown, ChevronRight } from "lucide-react";
import type { KeyboardEvent, ReactNode } from "react";

import { Handle, Position, type NodeProps } from "@xyflow/react";

import type {
  AcquisitionNode,
  GraphWarning,
  ProductionKind,
  ProductionNode,
  RecipeSelection,
  UnresolvedBuildNode,
} from "../../../../api/industry";
import { EveTypeImage } from "../../../../components/eve-type-image";
import { formatIskCompact } from "../../../../components/money";
import { Badge } from "../../../../components/primitives";

import {
  coveragePercentLabel,
  coverageSplit,
} from "../../inspector/sourcing-coverage";
import { nodeSize } from "./graph-layout";
import { warningChipLabel } from "./graph-warnings";
import { recipeCurrencyChip, type RecipeCurrencyChip } from "./recipe-currency";
import type { BuildGraphNode, BuildGraphNodeData } from "./to-react-flow";

const NUMBER = new Intl.NumberFormat("en-US");

/** Everything the view merges onto a node's `data` for rendering. */
export interface GraphNodeExtraData {
  collapsible?: boolean;
  collapsed?: boolean;
  hiddenCount?: number;
  onToggleCollapse?: () => void;
  onSelect?: () => void;
  onBuyBuild?: () => void;
  warnings?: GraphWarning[];
  pending?: boolean;
  error?: string | null;
}

type Accent = "primary" | "reaction" | "muted" | "warning" | "danger" | "positive";

const ACCENT_BAR: Record<Accent, string> = {
  primary: "bg-primary",
  reaction: "bg-reaction",
  muted: "bg-border",
  warning: "bg-warning",
  danger: "bg-danger",
  positive: "bg-positive",
};
const ACCENT_RING: Record<Accent, string> = {
  primary: "ring-primary/60 border-primary",
  reaction: "ring-reaction/60 border-reaction",
  muted: "ring-foreground/50 border-foreground",
  warning: "ring-warning/60 border-warning",
  danger: "ring-danger/60 border-danger",
  positive: "ring-positive/60 border-positive",
};
// Opaque tints (a translucent `bg-*/[0.06]` let the canvas dot-grid show
// through the card). color-mix over --color-panel keeps them token-derived.
const ACCENT_TINT: Record<Accent, string> = {
  primary: "bg-[color-mix(in_oklch,var(--color-primary)_12%,var(--color-panel))]",
  reaction: "bg-[color-mix(in_oklch,var(--color-reaction)_12%,var(--color-panel))]",
  muted: "bg-panel",
  warning: "bg-[color-mix(in_oklch,var(--color-warning)_10%,var(--color-panel))]",
  danger: "bg-[color-mix(in_oklch,var(--color-danger)_10%,var(--color-panel))]",
  positive: "bg-[color-mix(in_oklch,var(--color-positive)_10%,var(--color-panel))]",
};

function kindLabel(kind: ProductionKind): "Manufacturing" | "Reaction" {
  return kind === "rootReaction" || kind === "reaction" ? "Reaction" : "Manufacturing";
}
function kindAccent(kind: ProductionKind): Accent {
  return kind === "rootReaction" || kind === "reaction" ? "reaction" : "primary";
}
function recipeKindOf(recipe: RecipeSelection): "Manufacturing" | "Reaction" {
  return recipe.mode === "reaction" ? "Reaction" : "Manufacturing";
}

function costOnCard(costState: string, estimatedCost: string | null): ReactNode {
  if (costState === "known" && estimatedCost != null) {
    return <span className="text-warning">{formatIskCompact(estimatedCost)} ISK</span>;
  }
  return <span className="text-muted">Cost incomplete</span>;
}

// ─── shared shell ────────────────────────────────────────────────────────

function NodeShell({
  size,
  accent,
  selected,
  ariaLabel,
  onSelect,
  withTargetHandle = true,
  children,
}: {
  size: { width: number; height: number };
  accent: Accent;
  selected: boolean;
  ariaLabel: string;
  onSelect?: () => void;
  withTargetHandle?: boolean;
  children: ReactNode;
}) {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onSelect?.();
    }
  };
  return (
    <div
      aria-label={ariaLabel}
      aria-pressed={selected}
      className={`relative overflow-hidden rounded-md border text-sm transition focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-primary ${ACCENT_TINT[accent]} ${
        selected ? `ring-2 ${ACCENT_RING[accent]}` : "border-border hover:border-foreground/40"
      }`}
      onKeyDown={onSelect ? onKeyDown : undefined}
      role="button"
      style={{ width: size.width, minHeight: size.height }}
      tabIndex={0}
    >
      <span
        aria-hidden="true"
        className={`absolute inset-y-0 left-0 w-[3px] ${ACCENT_BAR[accent]}`}
      />
      {withTargetHandle ? <Handle position={Position.Top} type="target" /> : null}
      <div className="pl-3 pr-2 py-2">{children}</div>
      <Handle position={Position.Bottom} type="source" />
    </div>
  );
}

function NodeHeader({
  typeId,
  typeName,
  eyebrow,
  accent,
  imageSize,
  imageVariation = "icon",
  right,
}: {
  typeId: number;
  typeName: string;
  eyebrow: string;
  accent: Accent;
  imageSize?: 24 | 32 | 40 | 48;
  imageVariation?: "icon" | "render";
  right?: ReactNode;
}) {
  const accentText =
    accent === "reaction"
      ? "text-reaction"
      : accent === "warning"
        ? "text-warning"
        : accent === "danger"
          ? "text-danger"
          : accent === "positive"
            ? "text-positive"
            : accent === "muted"
              ? "text-muted"
              : "text-primary";
  return (
    <div className="flex items-start gap-2">
      {imageSize ? (
        <EveTypeImage
          size={imageSize}
          typeId={typeId}
          typeName={typeName}
          variation={imageVariation}
        />
      ) : null}
      <div className="min-w-0 flex-1">
        <div className={`text-[10px] font-bold uppercase tracking-widest ${accentText}`}>
          {eyebrow}
        </div>
        <div className="truncate font-semibold text-foreground" title={typeName}>
          {typeName}
        </div>
      </div>
      {right ? <div className="flex shrink-0 items-center gap-1">{right}</div> : null}
    </div>
  );
}

function CollapseControl({
  title,
  collapsed,
  hiddenCount,
  onToggle,
}: {
  title: string;
  collapsed: boolean;
  hiddenCount: number;
  onToggle?: () => void;
}) {
  return (
    <button
      aria-expanded={!collapsed}
      aria-label={`${collapsed ? "Expand" : "Collapse"} dependencies for ${title}`}
      className={`nodrag flex items-center gap-1 rounded text-[11px] text-muted hover:bg-panel-strong hover:text-foreground ${
        collapsed ? "border border-border px-1.5 py-0.5" : "h-6 w-6 justify-center"
      }`}
      onClick={(event) => {
        event.stopPropagation();
        event.preventDefault();
        onToggle?.();
      }}
      onMouseDown={(event) => event.stopPropagation()}
      type="button"
    >
      {collapsed ? (
        <ChevronRight aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
      ) : (
        <ChevronDown aria-hidden="true" className="h-4 w-4" />
      )}
      {collapsed && hiddenCount > 0 ? (
        <span>
          {NUMBER.format(hiddenCount)} {hiddenCount === 1 ? "dependency" : "dependencies"} hidden
        </span>
      ) : null}
    </button>
  );
}

function ChipRow({
  extra,
}: {
  extra: GraphNodeExtraData & { recipeCurrencyChip?: RecipeCurrencyChip | null };
}) {
  const chipLabel = extra.warnings ? warningChipLabel(extra.warnings) : null;
  const recipeChip = extra.recipeCurrencyChip ?? null;
  if (!chipLabel && !recipeChip) return null;
  return (
    <div className="mt-1.5 flex flex-wrap gap-1">
      {recipeChip ? (
        <Badge square tone={recipeChip.tone}>
          {recipeChip.label}
        </Badge>
      ) : null}
      {chipLabel ? (
        <span title={extra.warnings?.map((w) => w.message).join("\n")}>
          <Badge square tone="warning">
            {chipLabel}
          </Badge>
        </span>
      ) : null}
    </div>
  );
}

/** How many direct dependency child nodes a Production node has. */
function dependencyCount(node: ProductionNode): number {
  return node.children.length;
}

// ─── card bodies ─────────────────────────────────────────────────────────

export function RootGraphNode({ data, selected }: NodeProps<BuildGraphNode>) {
  const node = data.node as ProductionNode;
  const extra = data as GraphNodeExtraData;
  const accent = kindAccent(node.kind);
  const recipeChip = recipeCurrencyChip(node.recipeCurrency);
  return (
    <NodeShell
      accent={accent}
      ariaLabel={`Root ${kindLabel(node.kind)} — ${node.typeName}`}
      onSelect={extra.onSelect}
      selected={Boolean(selected)}
      size={nodeSize({ data: data as BuildGraphNodeData, warnings: extra.warnings })}
      withTargetHandle={false}
    >
      <NodeHeader
        accent={accent}
        eyebrow={node.kind === "rootReaction" ? "React" : "Manufacture"}
        imageSize={48}
        imageVariation="render"
        typeId={node.typeId}
        typeName={node.typeName}
      />
      <div className="mt-1.5 flex items-center gap-2 text-xs text-muted">
        <span>
          {NUMBER.format(node.runs)} run{node.runs === 1 ? "" : "s"}
        </span>
        <span aria-hidden="true">·</span>
        <span>{costOnCard(node.costState, node.estimatedCost)}</span>
      </div>
      <ChipRow extra={{ ...extra, recipeCurrencyChip: recipeChip }} />
    </NodeShell>
  );
}

export function ProductionGraphNode({ data, selected }: NodeProps<BuildGraphNode>) {
  const node = data.node as ProductionNode;
  const extra = data as GraphNodeExtraData;
  const recipeChip = recipeCurrencyChip(node.recipeCurrency);
  const need = node.requiredQuantity;
  const surplusTone = node.surplus < 0 ? "text-danger" : node.surplus > 0 ? "text-positive" : "text-muted";
  const deps = dependencyCount(node);
  const collapsedSummary = Boolean(extra.collapsible && extra.collapsed);
  const isReaction = kindLabel(node.kind) === "Reaction";
  const verb = isReaction ? "React" : "Manufacture";
  // Live coverage after inventory: `netRequiredQuantity` already reflects
  // fulfillment scope (a `Full`-scoped component arrives with `net === required`).
  const cov = coverageSplit(node.requiredQuantity, node.netRequiredQuantity);
  const accent: Accent = cov.fullyCovered ? "positive" : kindAccent(node.kind);
  const stateBadge = cov.fullyCovered ? (
    <div className="mt-1.5">
      <Badge square tone="positive">
        INVENTORY
      </Badge>
    </div>
  ) : cov.partiallyCovered ? (
    <div className="mt-1.5">
      <Badge square tone={isReaction ? "reaction" : "primary"}>
        {isReaction ? "REACT" : "MANUFACTURE"}
      </Badge>
    </div>
  ) : null;
  return (
    <NodeShell
      accent={accent}
      ariaLabel={
        cov.fullyCovered
          ? `${kindLabel(node.kind)} ${node.typeName}. Required ${
              need != null ? NUMBER.format(need) : "—"
            }, fully covered by inventory.`
          : `${kindLabel(node.kind)} ${node.typeName}. Need ${
              need != null ? NUMBER.format(need) : "—"
            }, making ${NUMBER.format(node.producingQuantity)}.`
      }
      onSelect={extra.onSelect}
      selected={Boolean(selected)}
      size={nodeSize({ data: data as BuildGraphNodeData, warnings: extra.warnings, collapsedSummary })}
    >
      <NodeHeader
        accent={accent}
        eyebrow={cov.fullyCovered ? "Inventory" : isReaction ? "Reaction" : "Build"}
        imageSize={32}
        right={
          extra.collapsible && !extra.collapsed ? (
            <CollapseControl
              collapsed={false}
              hiddenCount={extra.hiddenCount ?? 0}
              onToggle={extra.onToggleCollapse}
              title={node.typeName}
            />
          ) : undefined
        }
        typeId={node.typeId}
        typeName={node.typeName}
      />
      {stateBadge}
      {cov.fullyCovered ? (
        <div className="mt-1.5 text-[11px] text-muted">
          {need != null ? (
            <span>
              Required <span className="text-foreground">{NUMBER.format(need)}</span>
            </span>
          ) : null}
          <span className="ml-2 text-positive">{coveragePercentLabel(cov)}</span>
        </div>
      ) : (
        <>
          {cov.partiallyCovered ? (
            <div className="mt-1.5 grid grid-cols-[auto_1fr] gap-x-2 text-[11px] text-muted">
              <span>Need</span>
              <span className="font-mono text-foreground">{NUMBER.format(cov.required)}</span>
              <span>Inventory</span>
              <span className="font-mono text-positive">{NUMBER.format(cov.inventory)}</span>
              <span>{verb}</span>
              <span className="font-mono text-foreground">{NUMBER.format(cov.remaining)}</span>
            </div>
          ) : null}
          <div className="mt-1.5 flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-[11px] text-muted">
            {!cov.partiallyCovered && need != null ? (
              <>
                <span>
                  Need <span className="text-foreground">{NUMBER.format(need)}</span>
                </span>
                <span aria-hidden="true">·</span>
              </>
            ) : null}
            <span>
              Making <span className="text-foreground">{NUMBER.format(node.producingQuantity)}</span>
            </span>
            <span className={`font-medium ${surplusTone}`}>
              {node.surplus > 0 ? "+" : ""}
              {NUMBER.format(node.surplus)}
            </span>
          </div>
          <div className="mt-0.5 text-[11px]">{costOnCard(node.costState, node.estimatedCost)}</div>
        </>
      )}
      {collapsedSummary ? (
        <div className="mt-1.5 border-t border-border pt-1.5">
          <CollapseControl
            collapsed
            hiddenCount={extra.hiddenCount ?? deps}
            onToggle={extra.onToggleCollapse}
            title={node.typeName}
          />
        </div>
      ) : null}
      <ChipRow extra={{ ...extra, recipeCurrencyChip: recipeChip }} />
    </NodeShell>
  );
}

export function AcquisitionGraphNode({ data, selected }: NodeProps<BuildGraphNode>) {
  const node = data.node as AcquisitionNode;
  const extra = data as GraphNodeExtraData;
  const buildable = node.buildableRecipe != null;
  // Live coverage: `missingQuantity` already reflects fulfillment scope
  // (a `Full`-scoped node arrives with `missing === required`).
  const cov = coverageSplit(node.requiredQuantity, node.missingQuantity);
  const accent: Accent = cov.fullyCovered ? "positive" : "muted";
  const cost =
    node.costState === "known" && node.estimatedCost != null ? (
      <span className="ml-2 text-warning">{formatIskCompact(node.estimatedCost)} ISK</span>
    ) : null;
  return (
    <NodeShell
      accent={accent}
      ariaLabel={
        cov.fullyCovered
          ? `${node.typeName} from inventory, required ${NUMBER.format(cov.required)}, fully covered`
          : `Buy ${node.typeName}, ${cov.partiallyCovered ? `buy ${NUMBER.format(cov.remaining)} of ${NUMBER.format(cov.required)}` : `buy ${NUMBER.format(cov.remaining)}`}`
      }
      onSelect={extra.onSelect}
      selected={Boolean(selected)}
      size={nodeSize({ data: data as BuildGraphNodeData })}
    >
      <NodeHeader
        accent={accent}
        eyebrow={cov.fullyCovered ? "Inventory" : "Buy"}
        right={
          <Badge square tone={cov.fullyCovered ? "positive" : "muted"}>
            {cov.fullyCovered ? "INVENTORY" : "BUY"}
          </Badge>
        }
        typeId={0}
        typeName={node.typeName}
      />
      {cov.fullyCovered ? (
        <div className="mt-1.5 flex items-baseline gap-x-2 text-[11px] text-muted">
          <span>Required</span>
          <span className="font-mono text-foreground">{NUMBER.format(cov.required)}</span>
          <span className="text-positive">{coveragePercentLabel(cov)}</span>
          {cost}
        </div>
      ) : cov.partiallyCovered ? (
        <div className="mt-1.5 grid grid-cols-[auto_1fr] gap-x-2 text-[11px] text-muted">
          <span>Need</span>
          <span className="font-mono text-foreground">{NUMBER.format(cov.required)}</span>
          <span>Inventory</span>
          <span className="font-mono text-positive">{NUMBER.format(cov.inventory)}</span>
          <span>Buy</span>
          <span className="font-mono text-foreground">
            {NUMBER.format(cov.remaining)}
            {cost}
          </span>
        </div>
      ) : (
        <div className="mt-1.5 text-[11px] text-muted">
          Buy <span className="font-mono text-foreground">{NUMBER.format(cov.remaining)}</span>
          {cost}
        </div>
      )}
      {buildable && extra.onBuyBuild ? (
        <div className="mt-2">
          <button
            aria-label={`Build ${node.typeName}`}
            className="nodrag inline-flex items-center gap-1 rounded border border-primary/60 px-1.5 py-0.5 text-[11px] font-semibold text-primary hover:bg-primary/10"
            onClick={(event) => {
              event.stopPropagation();
              event.preventDefault();
              extra.onBuyBuild?.();
            }}
            onMouseDown={(event) => event.stopPropagation()}
            type="button"
          >
            → Switch to BUILD
          </button>
        </div>
      ) : null}
    </NodeShell>
  );
}

export function UnresolvedBuildGraphNode({ data, selected }: NodeProps<BuildGraphNode>) {
  const node = data.node as UnresolvedBuildNode;
  const extra = data as GraphNodeExtraData;
  const status = extra.error
    ? "Linked build creation failed"
    : extra.pending
      ? "Creating linked build…"
      : "Resolving linked build…";
  return (
    <NodeShell
      accent={extra.error ? "danger" : "warning"}
      ariaLabel={`Build ${node.typeName} — ${status}`}
      onSelect={extra.onSelect}
      selected={Boolean(selected)}
      size={nodeSize({ data: data as BuildGraphNodeData, error: extra.error })}
    >
      <NodeHeader
        accent={extra.error ? "danger" : "warning"}
        eyebrow="Build"
        right={
          <Badge square tone={extra.error ? "danger" : "warning"}>
            {recipeKindOf(node.recipe) === "Reaction" ? "RXN" : "BUILD"}
          </Badge>
        }
        typeId={0}
        typeName={node.typeName}
      />
      <div className={`mt-1.5 text-[11px] ${extra.error ? "text-danger" : "text-warning"}`}>
        {status}
      </div>
      <div className="mt-0.5 text-[11px] text-muted">
        Required <span className="font-mono text-foreground">{NUMBER.format(node.requiredQuantity)}</span>
      </div>
      {extra.error ? (
        <div className="mt-1 text-[10px] text-muted">{extra.error}</div>
      ) : null}
    </NodeShell>
  );
}

export const buildGraphNodeTypes = {
  root: RootGraphNode,
  production: ProductionGraphNode,
  acquisition: AcquisitionGraphNode,
  unresolvedBuild: UnresolvedBuildGraphNode,
} as const;
