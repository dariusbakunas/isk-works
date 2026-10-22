import { jsonRequest } from "./json-request";

export type MarketOrderSide = "buy" | "sell";
export type StandingStatus = "best" | "tied" | "outbid" | "unknown";
export type BookVisibility = "full" | "partial" | "none";
export type FreshSource = "book" | "characterSync";
export type MarketOrderLocationKind = "station" | "structure" | "unknown";

/** A buy order's reach. Sell orders always read `region` but only sell at
 * their own location. */
export type OrderRange =
  | { kind: "station" }
  | { kind: "solarSystem" }
  | { kind: "jumps"; jumps: number }
  | { kind: "region" };

/** Money values are decimal strings; never do arithmetic on them in floats. */
export interface BookOrder {
  orderId: number;
  side: MarketOrderSide;
  price: string;
  volumeRemain: number;
  range: OrderRange;
  locationId: number;
  solarSystemId: number;
  issuedAt: string;
}

export interface OrderStanding {
  status: StandingStatus;
  visibility: BookVisibility;
  /** The freshest known price: the order's copy in the book when present. */
  price: string;
  volumeRemain: number;
  issuedAt: string;
  freshSource: FreshSource;
  bestCompetitor: BookOrder | null;
  /** How far the best competitor beats this order: positive when outbid,
   * negative when this order leads. */
  gap: string | null;
  gapPercent: string | null;
  competitorsAhead: number;
  volumeAhead: number;
  /** One valid price step past the best competitor. */
  suggestedPrice: string | null;
  modifiableAt: string;
  outbidSince: string | null;
}

export interface SellMargin {
  netUnit: string;
  marginUnit: string;
  marginPercent: string | null;
  remainingProfit: string;
}

export interface DepthOrder {
  orderId: number;
  price: string;
  volumeRemain: number;
  range: OrderRange;
  locationId: number;
  locationName: string | null;
  isOwn: boolean;
  isAlt: boolean;
  /** `null` when a jump range can't be checked (no stargates imported). */
  tradesHere: boolean | null;
}

export interface MarketOrderRow {
  orderId: number;
  typeId: number;
  typeName: string | null;
  side: MarketOrderSide;
  connectionId: string;
  eveCharacterId: number;
  characterName: string;
  range: OrderRange;
  volumeTotal: number;
  minVolume: number;
  durationDays: number;
  expiresAt: string;
  escrow: string | null;
  firstSeenAt: string;
  standing: OrderStanding;
  unitCost: string | null;
  stockQuantity: number;
  salesTaxRate: string;
  salesTaxEstimated: boolean;
  margin: SellMargin | null;
  marginAtSuggested: SellMargin | null;
  depth: { sameSide: DepthOrder[]; oppositeSide: DepthOrder[] };
}

export type BookUnavailable =
  | { reason: "unknownLocation" }
  | { reason: "refreshFailed"; detail: string }
  | { reason: "noDataYet" };

export interface MarketOrderLocation {
  locationId: number;
  locationName: string | null;
  regionId: number;
  kind: MarketOrderLocationKind;
  unavailable: BookUnavailable | null;
  bookObservedAt: string | null;
  orders: MarketOrderRow[];
}

export interface TraderCharacter {
  connectionId: string;
  eveCharacterId: number;
  characterName: string;
  ordersScopeMissing: boolean;
  ordersSyncedAt: string | null;
  ordersSyncError: string | null;
  accountingLevel: number | null;
}

export interface MarketOrdersOverview {
  generatedAt: string;
  characters: TraderCharacter[];
  stargatesMissing: boolean;
  summary: {
    outbid: number;
    tied: number;
    best: number;
    unknown: number;
    activeSellValue: string;
    buyEscrow: string;
    oldestBookAt: string | null;
  };
  locations: MarketOrderLocation[];
}

const request = jsonRequest({
  unavailable: "ISK Works API is unavailable.",
  failed: "Market orders request failed.",
});

export function getMarketOrders(signal?: AbortSignal): Promise<MarketOrdersOverview> {
  return request("/api/market-orders", { signal });
}

/** Moves the page's books to the front of the refresh queue. */
export function refreshMarketOrderBooks(): Promise<void> {
  return request("/api/market-orders/refresh", { method: "POST" });
}
