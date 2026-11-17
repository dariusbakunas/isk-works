import {
  BriefcaseBusiness,
  CheckCircle2,
  Clock3,
  Factory,
  Globe2,
  GraduationCap,
  Package,
  PackageX,
  RefreshCcwDot,
  ShoppingCart,
  Store,
  UserRoundX,
  type LucideIcon,
} from "lucide-react";
import { Link } from "react-router";

import { useNow } from "../jobs/use-jobs";
import { attentionChips, type AttentionChipId, type AttentionTone } from "./attention";
import { useOverviewData } from "./overview-data";
import { PanelFrame } from "./panel-frame";

const CHIP_ICONS: Record<AttentionChipId, LucideIcon> = {
  reconnect: UserRoundX,
  extractorsExpired: Globe2,
  queueEmpty: GraduationCap,
  outbid: ShoppingCart,
  epicsBlocked: BriefcaseBusiness,
  storageFull: Package,
  starved: PackageX,
  queueEnding: Clock3,
  syncIssues: RefreshCcwDot,
  jobsReady: CheckCircle2,
  idleSlots: Factory,
  likelyFilled: Store,
};

const TONE_CLASSES: Record<AttentionTone, string> = {
  critical: "border-danger/50 bg-danger/10 text-danger",
  warn: "border-warning/50 bg-warning/10 text-warning",
  info: "border-primary/40 bg-primary/10 text-primary",
};

/** The top strip: one chip per thing that needs the player, each a link to
 * the page that resolves it (spec §3.1). Chips are the links; there is no
 * View all. */
export function AttentionPanel({ className = "" }: { className?: string }) {
  const { sources } = useOverviewData();
  const nowMs = useNow();
  const attention = attentionChips(sources, nowMs);
  const { chips, unavailable, pending } = attention;

  if (chips.length === 0 && pending) return <PanelFrame className={className} state={{ kind: "loading" }} title="Attention" />;

  return (
    <PanelFrame className={className} state={{ kind: "ready" }} title="Attention">
      <div className="flex min-h-12 flex-wrap items-center gap-1.5 p-2">
        {chips.map((chip) => {
          const Icon = CHIP_ICONS[chip.id];
          return (
            <Link
              key={chip.id}
              className={`inline-flex h-7 items-center gap-1.5 rounded-[2px] border px-2 text-xs transition hover:brightness-125 ${TONE_CLASSES[chip.tone]}`}
              to={chip.to}
            >
              <Icon aria-hidden="true" className="h-3 w-3" />
              <b className="font-mono font-semibold">{chip.count}</b>
              <span>{chip.label}</span>
            </Link>
          );
        })}
        {chips.length === 0 && unavailable === 0 ? (
          <p className="flex items-center gap-2 px-1 text-xs text-muted">
            <CheckCircle2 aria-hidden="true" className="h-3.5 w-3.5 text-positive" />
            All clear: nothing needs you right now.
          </p>
        ) : null}
        {unavailable > 0 ? (
          <span className="ml-auto px-1 text-xs text-muted" title="These sources failed to load; their alerts aren't shown.">
            {unavailable} {unavailable === 1 ? "source" : "sources"} unavailable
          </span>
        ) : null}
      </div>
    </PanelFrame>
  );
}
