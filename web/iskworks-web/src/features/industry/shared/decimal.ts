interface ParsedDecimal {
  coefficient: bigint;
  scale: number;
}

export interface DecimalSum {
  value: string | null;
  partial: boolean;
}

export function sumDecimalStrings(values: Array<string | null | undefined>): DecimalSum {
  const known = values.filter((value): value is string => value != null).map(parseDecimal);
  if (known.length === 0) return { value: null, partial: values.length > 0 };

  const scale = Math.max(...known.map((value) => value.scale));
  const coefficient = known.reduce(
    (sum, value) => sum + value.coefficient * 10n ** BigInt(scale - value.scale),
    0n,
  );
  return {
    value: formatDecimal({ coefficient, scale }),
    partial: known.length !== values.length,
  };
}

/**
 * `lineTotal / requiredQuantity` -- the Worksheet's "effective planning unit
 * cost" for a row whose requirement was met by a blend of sources (partial
 * inventory + fresh buy, partial inventory + a Build/Reaction child, or any
 * mix), and the only meaningful per-unit figure for a self-produced
 * Build/Reaction row (which has no market-buy `unitPrice` at all -- see
 * `PlannedMaterialLine.unit_price`'s doc comment on the Rust side).
 *
 * Exact `BigInt` arithmetic at Money's 4-decimal-place scale, half up --
 * never floating point. This is a display-only derivation (never fed back
 * into further arithmetic), so it deliberately does not reproduce the
 * backend's banker's-rounding convention exactly; half-up is simpler to get
 * right and the difference is never visible at 4 decimal places for
 * ISK-scale quantities.
 *
 * Returns `null` when `requiredQuantity` isn't a positive integer -- never
 * divides by zero, never fabricates a value for a non-sensical quantity.
 */
export function effectiveUnitCost(lineTotal: string, requiredQuantity: number): string | null {
  if (!Number.isInteger(requiredQuantity) || requiredQuantity <= 0) return null;
  const { coefficient, scale } = parseDecimal(lineTotal);
  const targetScale = 4;
  // One extra ("guard") digit beyond the target scale so the final digit can
  // be rounded correctly rather than merely truncated.
  const scaleAdjustment = targetScale + 1 - scale;
  const scaledNumerator = scaleAdjustment >= 0
    ? coefficient * 10n ** BigInt(scaleAdjustment)
    // `lineTotal` is always Money-scale (4) in practice, so this branch
    // (asked to divide at a *coarser* scale than the input) is defensive
    // only -- it truncates rather than rounds the digits it drops.
    : coefficient / 10n ** BigInt(-scaleAdjustment);
  const divisor = BigInt(requiredQuantity);
  const quotientWithGuard = scaledNumerator / divisor;
  const guardDigit = quotientWithGuard % 10n;
  const rounded = guardDigit >= 5n ? quotientWithGuard / 10n + 1n : quotientWithGuard / 10n;
  return formatDecimal({ coefficient: rounded, scale: targetScale });
}

function parseDecimal(value: string): ParsedDecimal {
  const match = /^(-?)(\d+)(?:\.(\d+))?$/.exec(value.trim());
  if (!match) throw new Error(`Invalid decimal: ${value}`);
  const fraction = match[3] ?? "";
  const coefficient = BigInt(`${match[1]}${match[2]}${fraction}`);
  return { coefficient, scale: fraction.length };
}

function formatDecimal({ coefficient, scale }: ParsedDecimal): string {
  const sign = coefficient < 0n ? "-" : "";
  const digits = (coefficient < 0n ? -coefficient : coefficient).toString().padStart(scale + 1, "0");
  if (scale === 0) return `${sign}${digits}`;
  return `${sign}${digits.slice(0, -scale)}.${digits.slice(-scale)}`;
}
