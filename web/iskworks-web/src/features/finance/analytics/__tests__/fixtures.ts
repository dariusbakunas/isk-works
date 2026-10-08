import type { FinanceAnalytics } from "../../../../api/finance-analytics";

const zero = "0.0000";

/** A valid, empty analytics payload; override the pieces a test cares about. */
export function emptyAnalytics(overrides: { transactionCount?: number } & Partial<FinanceAnalytics> = {}): FinanceAnalytics {
  const { transactionCount = 0, ...rest } = overrides;
  const kpi = { value: zero, delta: null, sparkline: [] };
  return {
    range: {
      dateFrom: "2026-09-01",
      dateTo: "2026-09-30",
      previousDateFrom: "2026-08-02",
      previousDateTo: "2026-08-31",
      granularity: "week",
    },
    earliestObservedAt: null,
    availableCharacters: [],
    excludedIntraAccount: { transactionCount: 0, totalIsk: zero },
    excludedInventoryBuys: { transactionCount: 0, totalIsk: zero },
    kpis: {
      income: kpi,
      expenses: kpi,
      net: kpi,
      margin: { percent: null, previousPercent: null, sparkline: [] },
      walletBalance: { value: null, delta: null, sparkline: [] },
      fees: null,
      transactionCount,
    },
    cashFlow: [],
    spendingByCategory: [],
    incomeByCategory: [],
    byCharacter: [],
    byLocation: [],
    topExpenses: [],
    topEarners: [],
    heatmap: [],
    insights: [],
    ...rest,
  };
}

export const CHARACTER_A = "0f8fad5b-d9cb-469f-a165-70867728950e";
export const CHARACTER_B = "1a2b3c4d-d9cb-469f-a165-70867728950e";

/** A populated payload: 1.5B income, 300M expenses, two characters, two weeks. */
export function sampleAnalytics(): FinanceAnalytics {
  const base = emptyAnalytics({ transactionCount: 12 });
  return {
    ...base,
    earliestObservedAt: "2026-07-05T17:47:54Z",
    availableCharacters: [
      { connectionId: CHARACTER_A, characterName: "Aura Valex", walletBalance: "2622608840.1500", balanceObservedAt: null },
      { connectionId: CHARACTER_B, characterName: "Valka", walletBalance: "100.0000", balanceObservedAt: null },
    ],
    excludedIntraAccount: { transactionCount: 3, totalIsk: "200000000.0000" },
    kpis: {
      income: { value: "1500000000.0000", delta: { previous: "400000000.0000", percent: 275, isNew: false }, sparkline: ["1", "2", "3"] },
      expenses: { value: "300000000.0000", delta: { previous: "250000000.0000", percent: 20, isNew: false }, sparkline: ["3", "2", "1"] },
      net: { value: "1200000000.0000", delta: { previous: "150000000.0000", percent: 700, isNew: false }, sparkline: ["1", "2", "4"] },
      margin: { percent: 80, previousPercent: 37.5, sparkline: [30, 50, 80] },
      walletBalance: { value: "2622608840.1500", delta: { previous: "2000000000.0000", percent: 31.1, isNew: false }, sparkline: ["1", "2", "3"] },
      fees: {
        value: "312000000.0000",
        delta: { previous: "300000000.0000", percent: 4, isNew: false },
        sparkline: ["1", "2"],
        brokersFee: "100000000.0000",
        transactionTax: "200000000.0000",
        marketProviderTax: "12000000.0000",
        availableFrom: "2026-07-05",
      },
      transactionCount: 12,
    },
    spendingByCategory: [
      { category: "Ships", total: "180000000.0000", previous: "100000000.0000" },
      { category: "Modules", total: "100000000.0000", previous: "150000000.0000" },
      { category: "Other", total: "20000000.0000", previous: "0.0000" },
    ],
    incomeByCategory: [
      { category: "Ships", total: "1200000000.0000", previous: "300000000.0000" },
      { category: "Modules", total: "300000000.0000", previous: null },
    ],
    byCharacter: [
      { connectionId: CHARACTER_A, characterName: "Aura Valex", income: "1000000000.0000", expenses: "100000000.0000", net: "900000000.0000" },
      { connectionId: CHARACTER_B, characterName: "Valka", income: "500000000.0000", expenses: "200000000.0000", net: "300000000.0000" },
    ],
    byLocation: [
      { locationId: 60003760, locationName: "Jita IV - Moon 4", regionName: "The Forge", income: "900000000.0000", expenses: "200000000.0000", net: "700000000.0000", transactionCount: 847 },
      { locationId: 1050000000001, locationName: "Structure 1050000000001", regionName: null, income: "600000000.0000", expenses: "100000000.0000", net: "500000000.0000", transactionCount: 12 },
    ],
    topExpenses: [
      { typeId: 34, typeName: "Tritanium", category: "Modules", quantity: 45000, averageUnitPrice: "8420.0000", total: "379000000.0000", sharePercent: 11.5, trend: ["0", "5", "1", "0", "0", "0", "0", "0"] },
    ],
    topEarners: [
      { typeId: 587, typeName: "Rifter", category: "Ships", quantity: 3, averageUnitPrice: "500000000.0000", total: "1500000000.0000", sharePercent: 42.5, trend: ["1", "2", "3", "0", "0", "0", "0", "0"] },
    ],
    heatmap: [
      { date: "2026-09-25", net: "0.0000" },
      { date: "2026-09-26", net: "42000000.0000" },
      { date: "2026-09-27", net: "-21000000.0000" },
      { date: "2026-09-28", net: "0.0000" },
      { date: "2026-09-29", net: "10500000.0000" },
    ],
    insights: [
      { kind: "categoryChange", side: "spending", subject: "Ships", amount: "180000000.0000", previous: "100000000.0000", total: "300000000.0000", sharePercent: 60, changePercent: 80 },
      { kind: "locationConcentration", side: "spending", subject: "Jita IV - Moon 4", amount: "200000000.0000", total: "300000000.0000", previous: null, sharePercent: 66.7, changePercent: null },
      { kind: "characterConcentration", side: "income", subject: "Aura Valex", amount: "1000000000.0000", total: "1500000000.0000", previous: null, sharePercent: 66.7, changePercent: null },
    ],
    cashFlow: [
      { start: "2026-08-31", income: "1000000000.0000", expenses: "100000000.0000", net: "900000000.0000", cumulativeNet: "900000000.0000" },
      { start: "2026-09-07", income: "500000000.0000", expenses: "200000000.0000", net: "300000000.0000", cumulativeNet: "1200000000.0000" },
    ],
  };
}
