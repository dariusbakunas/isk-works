import { AlertTriangle } from "lucide-react";
import { Link } from "react-router";

import type { MarketOrderRow, MarketOrderSide, OrderStanding } from "../../api/market-orders";
import { CopyValueButton } from "../../components/copy-value-button";
import { EveTypeImage } from "../../components/eve-type-image";
import { formatIskCompact, formatIskSummary } from "../../components/money";
import { Badge, ProgressBar } from "../../components/primitives";
import { CharacterName } from "../../observability/private";
import { countdown, expiresIn, formatPrice, isNegative, LIKELY_FILLED_HINT, percentLabel, statusLabel, statusTone } from "./market-orders-format";
import { TypeInfoButton } from "../../components/type-info/type-info-button";

const muted = <span className="text-muted">—</span>;

export function SideChip({ side }: { side: MarketOrderSide }) {
  return <Badge square tone={side === "sell" ? "positive" : "primary"}>{side === "sell" ? "SELL" : "BUY"}</Badge>;
}

export function StatusCell({ standing }: { standing: OrderStanding }) {
  return (
    <span className="inline-flex items-center gap-1">
      <span title={standing.status === "likelyFilled" ? LIKELY_FILLED_HINT : undefined}>
        <Badge square tone={statusTone(standing.status)}>{statusLabel(standing.status).toUpperCase()}</Badge>
      </span>
      {standing.visibility === "partial" && standing.status !== "likelyFilled" ? (
        <span
          aria-label="Partial view"
          className="h-1.5 w-1.5 cursor-help rounded-full bg-muted"
          role="img"
          title="Buy orders inside other structures can't be seen"
        />
      ) : null}
    </span>
  );
}

export function ItemCell({ row }: { row: MarketOrderRow }) {
  const name = row.typeName ?? `Type ${row.typeId}`;
  return (
    <span className="flex min-w-0 items-center gap-2">
      <EveTypeImage size={24} typeId={row.typeId} typeName={name} />
      <span className="flex min-w-0 flex-col leading-tight">
        <span className="flex min-w-0 items-center gap-1">
          <span className="truncate text-xs font-semibold text-foreground">{name}</span>
          <TypeInfoButton name={name} typeId={row.typeId} />
        </span>
        <span className="flex items-center gap-1 text-[0.625rem] text-muted">
          <SideChip side={row.side} />
          <CharacterName className="truncate" name={row.characterName} />
        </span>
      </span>
    </span>
  );
}

export function PriceCell({ value, tone }: { value: string | null; tone?: "danger" }) {
  if (value === null) return muted;
  return (
    <span className={`font-mono text-xs ${tone === "danger" ? "text-danger" : ""}`} title={formatIskSummary(value)}>
      {formatPrice(value)}
    </span>
  );
}

export function BestCompetitorCell({ standing }: { standing: OrderStanding }) {
  if (!standing.bestCompetitor) return muted;
  return (
    <span className="flex flex-col items-end leading-tight">
      <PriceCell tone={standing.status === "outbid" ? "danger" : undefined} value={standing.bestCompetitor.price} />
      <span className="text-[0.625rem] text-muted">
        {standing.competitorsAhead} ahead · {standing.volumeAhead.toLocaleString("en-US")} units
      </span>
    </span>
  );
}

export function GapCell({ standing }: { standing: OrderStanding }) {
  if (standing.gap === null) return muted;
  const leading = isNegative(standing.gap);
  const amount = leading ? standing.gap.replace("-", "") : standing.gap;
  const outbid = standing.status === "outbid";
  return (
    <span className={`flex flex-col items-end leading-tight ${outbid ? "text-danger" : leading ? "text-positive" : "text-muted"}`}>
      <span className="font-mono text-xs" title={formatIskSummary(amount)}>
        {leading ? "lead " : ""}
        {formatPrice(amount)}
      </span>
      <span className="font-mono text-[0.625rem]">{percentLabel(standing.gapPercent?.replace("-", "") ?? null)}</span>
    </span>
  );
}

export function CopyPriceButton({ price }: { price: string }) {
  return <CopyValueButton label={`Copy ${formatIskSummary(price)}`} value={price} />;
}

export function SuggestedCell({ standing }: { standing: OrderStanding }) {
  if (standing.suggestedPrice === null) return muted;
  return (
    <span className="inline-flex items-center gap-1 text-primary">
      <PriceCell value={standing.suggestedPrice} />
      <CopyPriceButton price={standing.suggestedPrice} />
    </span>
  );
}

export function ModifyCell({ standing, nowMs }: { standing: OrderStanding; nowMs: number }) {
  const left = countdown(standing.modifiableAt, nowMs);
  return left ? <span className="font-mono text-xs text-warning">{left}</span> : <span className="text-xs text-positive">now</span>;
}

export function VolumeCell({ row }: { row: MarketOrderRow }) {
  const remaining = row.standing.volumeRemain;
  const percent = row.volumeTotal > 0 ? (remaining / row.volumeTotal) * 100 : 0;
  return (
    <span className="flex w-full flex-col gap-1">
      <span className="text-right font-mono text-xs">
        {formatIskCompact(String(remaining))}/{formatIskCompact(String(row.volumeTotal))}
      </span>
      <ProgressBar percent={percent} />
    </span>
  );
}

export function CostBasisCell({ row }: { row: MarketOrderRow }) {
  if (row.unitCost === null) {
    return (
      <Link className="whitespace-nowrap text-xs text-muted underline-offset-2 hover:underline" onClick={(event) => event.stopPropagation()} to="/inventory">
        no cost basis
      </Link>
    );
  }
  return <span className="font-mono text-xs" title={formatIskSummary(row.unitCost)}>{formatIskCompact(row.unitCost)}</span>;
}

export function MarginCell({ row }: { row: MarketOrderRow }) {
  if (row.side === "buy" || !row.margin) return muted;
  const losing = isNegative(row.margin.marginUnit);
  const suggested = row.marginAtSuggested;
  const undercutLoses = suggested !== null && isNegative(suggested.marginUnit);
  return (
    <span className="flex flex-col items-end leading-tight">
      <span className={`font-mono text-xs ${losing ? "text-danger" : "text-positive"}`} title={formatIskSummary(row.margin.marginUnit)}>
        {formatIskCompact(row.margin.marginUnit)} · {percentLabel(row.margin.marginPercent, true)}
      </span>
      {suggested ? (
        <span className={`inline-flex items-center gap-1 text-[0.625rem] ${undercutLoses ? "text-danger" : "text-muted"}`}>
          {undercutLoses ? <AlertTriangle aria-label="Undercut loses money" className="h-3 w-3" role="img" /> : null}
          at suggested {percentLabel(suggested.marginPercent, true)}
        </span>
      ) : null}
    </span>
  );
}

export function ExpiresCell({ row, nowMs }: { row: MarketOrderRow; nowMs: number }) {
  return <span className="font-mono text-xs text-muted">{expiresIn(row.expiresAt, nowMs)}</span>;
}
