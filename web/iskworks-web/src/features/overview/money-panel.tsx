import type { CharacterRosterEntry } from "../../api/characters";
import type { AnalyticsDelta, CashFlowBucket } from "../../api/finance-analytics";
import { IskCompact } from "../../components/money";
import { textToneClasses, type Tone } from "../../components/primitives";
import { sumDecimalStrings } from "../../lib/decimal";
import { ago } from "../jobs/jobs-format";
import { useNow } from "../jobs/use-jobs";
import { useOverviewData } from "./overview-data";
import { panelState, PanelFrame } from "./panel-frame";

/** Every character's wallet, added exactly; `null` when none is known. */
export function totalWallet(characters: CharacterRosterEntry[]): string | null {
  return sumDecimalStrings(characters.map((character) => character.walletBalance)).value;
}

export function deltaLabel(delta: AnalyticsDelta | null): { text: string; tone: Tone } | null {
  if (!delta) return null;
  if (delta.isNew) return { text: "new vs prev 30d", tone: "positive" };
  if (delta.percent === null) return null;
  const up = delta.percent >= 0;
  return { text: `${up ? "▲" : "▼"} ${Math.round(Math.abs(delta.percent))}% vs prev 30d`, tone: up ? "positive" : "danger" };
}

function isNegative(value: string): boolean {
  return value.trim().startsWith("-");
}

function Sparkline({ values }: { values: string[] }) {
  const points = values.map(Number).filter(Number.isFinite);
  if (points.length < 2) return null;
  const min = Math.min(...points);
  const span = Math.max(...points) - min || 1;
  const path = points.map((value, index) => `${(index / (points.length - 1)) * 200},${30 - ((value - min) / span) * 28}`).join(" ");
  return (
    <svg aria-hidden="true" className="mt-1 h-8 w-full" preserveAspectRatio="none" viewBox="0 0 200 32">
      <polyline fill="none" points={path} stroke="var(--color-primary)" strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

/** Net per day as bars around a zero line: gains up, losses down. */
function NetBars({ buckets }: { buckets: CashFlowBucket[] }) {
  const values = buckets.map((bucket) => Number(bucket.net));
  const peak = Math.max(...values.map(Math.abs), 1);
  const gains = values.filter((value) => value > 0).length;
  const losses = values.filter((value) => value < 0).length;
  return (
    <div
      aria-label={`Net per day: ${gains} up, ${losses} down`}
      className="mt-2 grid h-14 auto-cols-fr grid-flow-col gap-px border-y border-border/60"
      role="img"
    >
      {values.map((value, index) => (
        <span key={buckets[index]!.start} className="grid grid-rows-2" title={`${buckets[index]!.start}: ${buckets[index]!.net}`}>
          <span className="flex items-end">
            {value > 0 ? <i className="block w-full bg-positive/80" style={{ height: `${(value / peak) * 100}%` }} /> : null}
          </span>
          <span className="flex items-start">
            {value < 0 ? <i className="block w-full bg-danger/80" style={{ height: `${(-value / peak) * 100}%` }} /> : null}
          </span>
        </span>
      ))}
    </div>
  );
}

/** Wallets now, the last 30 days' net, and PI income (spec §3.5, §9.5). */
export function MoneyPanel({ className = "" }: { className?: string }) {
  const { sources } = useOverviewData();
  const nowMs = useNow();
  const characters = sources.characters.data ?? [];
  const finance = sources.finance;
  const wallet = totalWallet(characters);
  const synced = characters
    .filter((character) => character.walletBalance !== null && character.lastSyncedAt)
    .map((character) => character.lastSyncedAt!)
    .sort()[0];
  const netState = panelState(finance, () => false, null);
  const net = finance.data?.kpis.net;
  const delta = deltaLabel(net?.delta ?? null);
  const piPerMonth = sources.planetary.data?.summary.iskPerMonth;

  return (
    <PanelFrame
      className={className}
      link={{ to: "/finance/analytics" }}
      stamp={synced ? `wallets ${ago(synced, nowMs)}` : undefined}
      state={panelState(sources.characters, () => false, null)}
      title="Money"
    >
      <div className="border-b border-border px-3 py-2.5">
        <div className="text-[0.625rem] font-semibold uppercase tracking-wider text-muted">Total wallet</div>
        <div className="flex items-baseline gap-1.5">
          {wallet !== null ? <IskCompact className="text-2xl font-bold text-foreground" value={wallet} /> : <span className="text-2xl text-muted">—</span>}
          <span className="text-xs text-muted">ISK</span>
        </div>
        {finance.data ? <Sparkline values={finance.data.kpis.walletBalance.sparkline} /> : null}
      </div>
      <div className="flex flex-1 flex-col px-3 py-2.5">
        <div className="flex items-end gap-2">
          <div>
            <div className="text-[0.625rem] font-semibold uppercase tracking-wider text-muted">Net, 30 days</div>
            {net ? (
              <IskCompact className={`text-lg font-bold ${isNegative(net.value) ? "text-danger" : "text-positive"}`} value={net.value} />
            ) : null}
          </div>
          {delta ? (
            <span className={`ml-auto rounded-[2px] border border-current/40 px-1.5 py-0.5 text-[0.625rem] ${textToneClasses[delta.tone]}`}>
              {delta.text}
            </span>
          ) : null}
        </div>
        {netState.kind === "error" ? (
          <p className="mt-2 text-xs text-muted">
            {netState.message}{" "}
            <button className="text-primary hover:underline" onClick={netState.retry} type="button">Retry</button>
          </p>
        ) : netState.kind === "loading" ? (
          <span aria-label="Loading net per day" className="mt-2 block h-14 animate-pulse rounded-sm bg-panel-strong" role="status" />
        ) : finance.data && finance.data.cashFlow.length > 0 ? (
          <NetBars buckets={finance.data.cashFlow} />
        ) : null}
        {piPerMonth ? (
          <p className="mt-2 text-xs text-muted">
            PI: <IskCompact className="text-foreground" value={piPerMonth} /> / month
          </p>
        ) : null}
      </div>
    </PanelFrame>
  );
}
