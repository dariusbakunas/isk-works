import type { Insight } from "../../../api/finance-analytics";
import { formatIskAbbreviated } from "../../../components/money";

export interface InsightText {
  title: string;
  body: string;
  icon: "up" | "down" | "place" | "person" | "fee";
  tone: "good" | "bad" | "warn" | "info";
}

const percent = (value: number) => `${Math.round(Math.abs(value))}%`;
const money = (value: string) => formatIskAbbreviated(value);

/**
 * Wording for a server-computed insight. Every number comes from the insight;
 * there is no advice text, only what the data says.
 */
export function describeInsight(insight: Insight): InsightText {
  const sideWord = insight.side === "income" ? "income" : "spending";
  switch (insight.kind) {
    case "categoryChange": {
      const change = insight.changePercent;
      if (change === null || insight.previous === null) {
        return {
          title: `${insight.subject} ${sideWord} changed`,
          body: `${money(insight.amount)} this period.`,
          icon: "up",
          tone: "info",
        };
      }
      const rising = change > 0;
      // More income is good; more spending is not.
      const good = insight.side === "income" ? rising : !rising;
      return {
        title: `${insight.subject} ${sideWord} ${rising ? "up" : "down"} ${percent(change)}`,
        body: `${money(insight.amount)} vs ${money(insight.previous)} in the previous period.`,
        icon: rising ? "up" : "down",
        tone: good ? "good" : "bad",
      };
    }
    case "locationConcentration":
      return {
        title: `${percent(insight.sharePercent ?? 0)} of ${sideWord} at ${insight.subject}`,
        body: `${money(insight.amount)} of ${money(insight.total ?? insight.amount)} total ${sideWord}.`,
        icon: "place",
        tone: "warn",
      };
    case "feeBurden": {
      const change = insight.changePercent;
      const trend = change === null ? "" : `, ${change >= 0 ? "up" : "down"} ${percent(change)} on the previous period`;
      return {
        title: `Taxes & fees cost ${money(insight.amount)}`,
        body: `${percent(insight.sharePercent ?? 0)} of ${money(insight.total ?? insight.amount)} income${trend}.`,
        icon: "fee",
        tone: "warn",
      };
    }
    case "characterConcentration":
      return {
        title: `${insight.subject} earned ${percent(insight.sharePercent ?? 0)} of ${sideWord}`,
        body: `${money(insight.amount)} of ${money(insight.total ?? insight.amount)} total ${sideWord}.`,
        icon: "person",
        tone: "info",
      };
  }
}
