import { MapPin, Receipt, TrendingDown, TrendingUp, User } from "lucide-react";

import type { Insight } from "../../../api/finance-analytics";
import { Private } from "../../../observability/private";
import { describeInsight, type InsightText } from "./insight-text";

const ICONS = { up: TrendingUp, down: TrendingDown, place: MapPin, person: User, fee: Receipt } as const;
const TONES: Record<InsightText["tone"], { border: string; icon: string }> = {
  good: { border: "border-l-income", icon: "text-income" },
  bad: { border: "border-l-expense", icon: "text-expense" },
  warn: { border: "border-l-warning", icon: "text-warning" },
  info: { border: "border-l-net", icon: "text-net" },
};

/** Rule-based findings computed by the server; nothing here is canned text. */
export function InsightsRow({ insights }: { insights: Insight[] }) {
  return (
    <section aria-label="Insights">
      <h2 className="mb-1.5 text-[0.625rem] font-semibold uppercase tracking-widest text-muted">Insights</h2>
      {insights.length === 0 ? (
        <p className="text-[0.6875rem] text-muted">Nothing stands out in this period.</p>
      ) : (
        <ul className="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
          {insights.map((insight) => {
            const text = describeInsight(insight);
            const tone = TONES[text.tone];
            const Icon = ICONS[text.icon];
            return (
              <Private as="li" className={`iw-panel flex gap-2.5 border-l-[3px] px-3 py-2 ${tone.border}`} key={`${insight.kind}-${insight.subject}`}>
                <Icon aria-hidden="true" className={`mt-0.5 h-4 w-4 shrink-0 ${tone.icon}`} />
                <div className="min-w-0">
                  <div className="text-[0.6875rem] font-bold text-foreground">{text.title}</div>
                  <div className="mt-0.5 text-[0.625rem] text-muted">{text.body}</div>
                </div>
              </Private>
            );
          })}
        </ul>
      )}
    </section>
  );
}
