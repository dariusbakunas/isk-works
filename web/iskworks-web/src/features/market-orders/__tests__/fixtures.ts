import type {
  DepthOrder,
  MarketOrderLocation,
  MarketOrderRow,
  MarketOrdersOverview,
  OrderStanding,
  TraderCharacter,
} from "../../../api/market-orders";

export const NOW = new Date("2026-10-22T12:00:00Z");
export const KAEL = "11111111-1111-1111-1111-111111111111";
export const ILSA = "22222222-2222-2222-2222-222222222222";
export const JITA_4_4 = 60_003_760;
export const FORTIZAR = 1_035_466_617_946;

const minutes = (count: number) => new Date(NOW.getTime() + count * 60_000).toISOString();

export function character(overrides: Partial<TraderCharacter> = {}): TraderCharacter {
  return {
    connectionId: KAEL,
    eveCharacterId: 90_000_001,
    characterName: "Kael Varon",
    ordersScopeMissing: false,
    ordersSyncedAt: minutes(-12),
    ordersSyncError: null,
    accountingLevel: 5,
    ...overrides,
  };
}

function standing(overrides: Partial<OrderStanding> = {}): OrderStanding {
  return {
    status: "best",
    visibility: "full",
    price: "312400000",
    volumeRemain: 5,
    issuedAt: minutes(-120),
    freshSource: "book",
    bestCompetitor: null,
    gap: null,
    gapPercent: null,
    competitorsAhead: 0,
    volumeAhead: 0,
    suggestedPrice: null,
    modifiableAt: minutes(-115),
    outbidSince: null,
    ...overrides,
  };
}

function depth(orderId: number, price: string, overrides: Partial<DepthOrder> = {}): DepthOrder {
  return {
    orderId,
    price,
    volumeRemain: 2,
    range: { kind: "region" },
    locationId: JITA_4_4,
    locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant (0.9)",
    isOwn: false,
    isAlt: false,
    tradesHere: true,
    ...overrides,
  };
}

export function row(overrides: Partial<MarketOrderRow> = {}): MarketOrderRow {
  return {
    orderId: 1,
    typeId: 12_005,
    typeName: "Ishtar",
    side: "sell",
    connectionId: KAEL,
    eveCharacterId: 90_000_001,
    characterName: "Kael Varon",
    range: { kind: "region" },
    volumeTotal: 5,
    minVolume: 1,
    durationDays: 90,
    expiresAt: minutes(60 * 24 * 44 + 60 * 20),
    escrow: null,
    firstSeenAt: minutes(-60 * 24),
    standing: standing(),
    unitCost: "305000000.0000",
    stockQuantity: 5,
    salesTaxRate: "0.033750",
    salesTaxEstimated: false,
    margin: { netUnit: "301856500", marginUnit: "-3143500", marginPercent: "-1.03", remainingProfit: "-15717500" },
    marginAtSuggested: null,
    depth: { sameSide: [], oppositeSide: [] },
    ...overrides,
  };
}

/** An Ishtar sell order outbid by 500k, whose undercut would sell below cost. */
export function outbidIshtar(): MarketOrderRow {
  return row({
    standing: standing({
      status: "outbid",
      bestCompetitor: {
        orderId: 2,
        side: "sell",
        price: "311900000",
        volumeRemain: 5,
        range: { kind: "region" },
        locationId: JITA_4_4,
        solarSystemId: 30_000_142,
        issuedAt: minutes(-12),
      },
      gap: "500000",
      gapPercent: "0.16",
      competitorsAhead: 2,
      volumeAhead: 5,
      suggestedPrice: "311800000",
      modifiableAt: minutes(3),
      outbidSince: minutes(-12),
    }),
    margin: { netUnit: "301856500", marginUnit: "-3143500", marginPercent: "-1.03", remainingProfit: "-15717500" },
    marginAtSuggested: { netUnit: "301276500", marginUnit: "-3723500", marginPercent: "-1.22", remainingProfit: "-18617500" },
    depth: {
      sameSide: [
        depth(2, "311900000"),
        depth(9, "312000000", { isAlt: true }),
        depth(1, "312400000", { isOwn: true, volumeRemain: 5 }),
        depth(3, "315000000"),
      ],
      oppositeSide: [depth(4, "298000000")],
    },
  });
}

/** A Megacyte buy order in Jita, best price, with a ranged order out of reach. */
export function megacyteBuy(): MarketOrderRow {
  return row({
    orderId: 5,
    typeId: 40,
    typeName: "Megacyte",
    side: "buy",
    range: { kind: "jumps", jumps: 5 },
    volumeTotal: 120_000,
    escrow: "1105200000",
    unitCost: null,
    stockQuantity: 0,
    margin: null,
    standing: standing({
      price: "9210",
      volumeRemain: 120_000,
      bestCompetitor: {
        orderId: 6,
        side: "buy",
        price: "9200",
        volumeRemain: 50_000,
        range: { kind: "region" },
        locationId: JITA_4_4,
        solarSystemId: 30_000_142,
        issuedAt: minutes(-30),
      },
      gap: "-10",
      gapPercent: "-0.11",
      suggestedPrice: "9201",
    }),
    depth: {
      sameSide: [
        depth(7, "9300", { range: { kind: "jumps", jumps: 5 }, locationName: "Amarr VIII (Oris) - Emperor Family Academy (1.0)", tradesHere: false }),
        depth(5, "9210", { isOwn: true, range: { kind: "jumps", jumps: 5 } }),
        depth(6, "9200"),
      ],
      oppositeSide: [],
    },
  });
}

export function location(overrides: Partial<MarketOrderLocation> = {}): MarketOrderLocation {
  return {
    locationId: JITA_4_4,
    locationName: "Jita IV - Moon 4 - Caldari Navy Assembly Plant (0.9)",
    regionId: 10_000_002,
    kind: "station",
    unavailable: null,
    bookObservedAt: minutes(-3),
    orders: [outbidIshtar(), megacyteBuy()],
    ...overrides,
  };
}

export function overview(overrides: Partial<MarketOrdersOverview> = {}): MarketOrdersOverview {
  return {
    generatedAt: NOW.toISOString(),
    characters: [character(), character({ connectionId: ILSA, eveCharacterId: 90_000_002, characterName: "Ilsa Morrow" })],
    stargatesMissing: false,
    summary: {
      outbid: 1,
      tied: 0,
      best: 2,
      unknown: 1,
      activeSellValue: "7639600000",
      buyEscrow: "1359950000",
      oldestBookAt: minutes(-3),
    },
    locations: [
      location(),
      location({
        locationId: FORTIZAR,
        locationName: "K-6K16 - Fortizar Northern Market",
        kind: "structure",
        unavailable: { reason: "refreshFailed", detail: "every connected character with market access was denied docking access to this structure" },
        bookObservedAt: null,
        orders: [
          row({
            orderId: 10,
            typeId: 40_340,
            typeName: "Large Skill Injector",
            connectionId: ILSA,
            characterName: "Ilsa Morrow",
            unitCost: null,
            margin: null,
            standing: standing({ status: "unknown", visibility: "none", price: "875000000" }),
          }),
        ],
      }),
    ],
    ...overrides,
  };
}
