// What a Shipping calculator window keeps, and its arithmetic: the items to
// haul, the hold they go in, and how the load splits into shipments.
// JSON-serialisable: tool windows persist it. Volumes are counted in whole
// millionths of a m³, so sums and splits are exact.

/** The tool window kind the Shipping calculator registers as. */
export const SHIPPING_KIND = "shipping-calculator";
/** Past this many shipments the hold size is a typo, not a plan. */
export const MAX_SHIPMENTS = 200;
const MAX_QUANTITY = 1_000_000_000_000;
const MICRO = 1_000_000;

export interface ShippingItem {
  typeId: number;
  name: string;
  quantity: number;
  /** Packaged volume of one unit; `null` when the SDE has none. */
  unitVolumeM3: string | null;
}

/** A named line not yet looked up: pasted, picked, or sent from a page. */
export interface PendingLine {
  name: string;
  quantity: number;
  /** The whole pasted line, when the quantity was split off its end: the
   * name to try if `name` isn't an item ("Hull Upgrades 5"). */
  alt?: string;
}

export interface ShippingState {
  items: ShippingItem[];
  /** The hold size as typed, in m³. */
  holdM3: string;
  pending: PendingLine[];
  /** Pasted lines no item matched. */
  unmatched: string[];
}

/** A type as `/api/types/resolve` returns it. */
export interface ResolvedType {
  typeId: number;
  typeName: string;
  packagedVolumeM3: string | null;
}

export function emptyShippingState(): ShippingState {
  return { items: [], holdM3: "", pending: [], unmatched: [] };
}

/** The state to open the calculator with so it adds `lines`. */
export function shippingStateWith(lines: PendingLine[]): ShippingState {
  return { ...emptyShippingState(), pending: lines };
}

/** Stored or foreign state, made safe to render. */
export function normalizeShipping(state: unknown): ShippingState {
  const value = (state ?? {}) as Partial<ShippingState>;
  return {
    items: Array.isArray(value.items) ? value.items : [],
    holdM3: typeof value.holdM3 === "string" ? value.holdM3 : "",
    pending: Array.isArray(value.pending) ? value.pending : [],
    unmatched: Array.isArray(value.unmatched) ? value.unmatched : [],
  };
}

/** Opening the calculator again adds to the window already in front. */
export function mergeShipping(current: ShippingState, incoming: ShippingState): ShippingState {
  const base = normalizeShipping(current);
  return { ...base, pending: [...base.pending, ...normalizeShipping(incoming).pending] };
}

export function clampQuantity(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(MAX_QUANTITY, Math.max(0, Math.floor(value)));
}

// "1,000", "1.000", "1 000" (plain, no-break or narrow space) or "1000".
const QUANTITY = String.raw`\d{1,3}(?:[,.   ]\d{3})+|\d+`;
const NAME_THEN_QUANTITY = new RegExp(String.raw`^(.+?)\s+x?\s*(${QUANTITY})$`, "i");
const QUANTITY_THEN_NAME = new RegExp(String.raw`^(${QUANTITY})\s*[x×]\s+(.+)$`, "i");
const QUANTITY_ONLY = new RegExp(`^(?:${QUANTITY})$`);

function quantityOf(raw: string): number {
  return Math.max(1, clampQuantity(Number(raw.replace(/\D/g, ""))));
}

/**
 * Pasted text as lines to look up. Reads EVE's multibuy shape ("Name 10"),
 * "Name x10", "10 x Name", a bare name (one unit), and rows copied from an
 * inventory or contract window (tab-separated: name, then quantity).
 */
export function parseItemList(text: string): PendingLine[] {
  const lines: PendingLine[] = [];
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    if (raw.includes("\t")) {
      const [name, quantity = ""] = raw.split("\t").map((cell) => cell.trim());
      if (name) lines.push({ name, quantity: QUANTITY_ONLY.test(quantity) ? quantityOf(quantity) : 1 });
      continue;
    }
    const leading = QUANTITY_THEN_NAME.exec(line);
    if (leading) {
      lines.push({ name: leading[2].trim(), quantity: quantityOf(leading[1]) });
      continue;
    }
    const trailing = NAME_THEN_QUANTITY.exec(line);
    if (trailing) {
      lines.push({ name: trailing[1].trim(), quantity: quantityOf(trailing[2]), alt: line });
      continue;
    }
    lines.push({ name: line, quantity: 1 });
  }
  return lines;
}

/** Every name worth asking the API about for `lines`. */
export function namesToResolve(lines: PendingLine[]): string[] {
  return [...new Set(lines.flatMap((line) => (line.alt ? [line.name, line.alt] : [line.name])))];
}

/** Folds looked-up `lines` into the items; lines nothing matched are kept in
 * `unmatched`. The lines leave `pending` either way. */
export function applyResolved(state: ShippingState, lines: PendingLine[], types: ResolvedType[]): ShippingState {
  const byName = new Map(types.map((type) => [type.typeName.toLowerCase(), type]));
  const items = state.items.map((item) => ({ ...item }));
  const unmatched = [...state.unmatched];
  for (const line of lines) {
    const exact = byName.get(line.name.toLowerCase());
    const whole = exact ? undefined : line.alt ? byName.get(line.alt.toLowerCase()) : undefined;
    const type = exact ?? whole;
    if (!type) {
      const shown = line.alt ?? line.name;
      if (!unmatched.includes(shown)) unmatched.push(shown);
      continue;
    }
    const quantity = exact ? line.quantity : 1;
    const listed = items.find((item) => item.typeId === type.typeId);
    if (listed) listed.quantity = clampQuantity(listed.quantity + quantity);
    else items.push({ typeId: type.typeId, name: type.typeName, quantity, unitVolumeM3: type.packagedVolumeM3 });
  }
  const done = new Set(lines);
  return { ...state, items, unmatched, pending: state.pending.filter((line) => !done.has(line)) };
}

/** A decimal m³ string in millionths of a m³; `null` if it isn't a number. */
export function toMicro(value: string | null): number | null {
  if (value == null) return null;
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 0) return null;
  return Math.round(parsed * MICRO);
}

/** The typed hold size in millionths of a m³; `null` unless it's positive. */
export function parseHold(value: string): number | null {
  const cleaned = value.replace(/[,\s  ]/g, "");
  if (!cleaned || !/^\d*\.?\d*$/.test(cleaned)) return null;
  const micro = toMicro(cleaned);
  return micro && micro > 0 ? micro : null;
}

export function formatM3(micro: number): string {
  return (micro / MICRO).toLocaleString("en-US", { maximumFractionDigits: 2 });
}

export interface ShipmentLine {
  typeId: number;
  name: string;
  quantity: number;
  volumeMicro: number;
}

export interface Shipment {
  lines: ShipmentLine[];
  usedMicro: number;
}

export interface ShippingPlan {
  /** Everything with a known volume, whether or not it fits the hold. */
  totalMicro: number;
  shipments: Shipment[];
  /** Items whose single unit is bigger than the hold. */
  oversized: ShippingItem[];
  /** Items the SDE has no volume for. */
  unknown: ShippingItem[];
  /** Set instead of `shipments` when there would be more than `MAX_SHIPMENTS`. */
  tooMany?: number;
}

/**
 * Splits `items` into hold-sized shipments. Each shipment is filled as full
 * as it goes, biggest units first, so stacks split across shipments where
 * that saves a trip; smaller items top up the space bulky ones leave.
 */
export function planShipments(items: ShippingItem[], holdMicro: number | null): ShippingPlan {
  const unknown: ShippingItem[] = [];
  const oversized: ShippingItem[] = [];
  const loads: { item: ShippingItem; unit: number; left: number }[] = [];
  let totalMicro = 0;
  for (const item of items) {
    if (item.quantity <= 0) continue;
    const unit = toMicro(item.unitVolumeM3);
    if (unit == null) {
      unknown.push(item);
      continue;
    }
    totalMicro += unit * item.quantity;
    if (holdMicro != null && unit > holdMicro) oversized.push(item);
    else loads.push({ item, unit, left: item.quantity });
  }
  const plan: ShippingPlan = { totalMicro, shipments: [], oversized, unknown };
  if (holdMicro == null) return plan;

  const fitting = loads.reduce((sum, load) => sum + load.unit * load.left, 0);
  if (Math.ceil(fitting / holdMicro) > MAX_SHIPMENTS) return { ...plan, tooMany: Math.ceil(fitting / holdMicro) };

  loads.sort((a, b) => b.unit - a.unit || a.item.name.localeCompare(b.item.name));
  while (loads.some((load) => load.left > 0)) {
    let space = holdMicro;
    const lines: ShipmentLine[] = [];
    for (const load of loads) {
      if (load.left === 0 || load.unit > space) continue;
      const quantity = load.unit === 0 ? load.left : Math.min(load.left, Math.floor(space / load.unit));
      lines.push({ typeId: load.item.typeId, name: load.item.name, quantity, volumeMicro: quantity * load.unit });
      space -= quantity * load.unit;
      load.left -= quantity;
    }
    plan.shipments.push({ lines, usedMicro: holdMicro - space });
  }
  return plan;
}

/** A shipment as "name quantity" lines, the shape EVE's multibuy accepts. */
export function shipmentText(shipment: Shipment): string {
  return [...shipment.lines]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((line) => `${line.name} ${line.quantity}`)
    .join("\n");
}
