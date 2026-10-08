import type { HTMLAttributes } from "react";

export type MoneyAmountMode = "summary" | "detail" | "exact";
export type MoneySignDisplay = "auto" | "always" | "never";

export interface MoneyParts {
  sign: "" | "+" | "-";
  integerPart: string;
  fractionalPart: string;
  currency: "ISK";
  exactText: string;
  displayText: string;
  rounded: boolean;
}

interface ParsedDecimal {
  negative: boolean;
  integer: string;
  fraction: string;
}

const decimalPattern = /^([+-]?)(\d+)(?:\.(\d+))?$/;

function parseDecimal(value: string): ParsedDecimal {
  const match = decimalPattern.exec(value.trim());
  if (!match) throw new Error(`Invalid decimal value: ${value}`);
  const integer = match[2].replace(/^0+(?=\d)/, "");
  const fraction = match[3] ?? "";
  const isZero = /^0+$/.test(integer) && (fraction === "" || /^0+$/.test(fraction));
  return {
    negative: match[1] === "-" && !isZero,
    integer,
    fraction,
  };
}

function groupInteger(value: string): string {
  return value.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

function canonicalDecimal(parsed: ParsedDecimal): string {
  const fraction = parsed.fraction.replace(/0+$/, "");
  return `${parsed.negative ? "-" : ""}${parsed.integer}${fraction ? `.${fraction}` : ""}`;
}

function incrementDigits(value: string): string {
  const digits = value.split("");
  for (let index = digits.length - 1; index >= 0; index -= 1) {
    if (digits[index] !== "9") {
      digits[index] = String(Number(digits[index]) + 1);
      return digits.join("");
    }
    digits[index] = "0";
  }
  return `1${digits.join("")}`;
}

function roundForSummary(parsed: ParsedDecimal, maximumFractionDigits: number): ParsedDecimal {
  if (!Number.isInteger(maximumFractionDigits) || maximumFractionDigits < 0) {
    throw new Error("maximumSummaryFractionDigits must be a non-negative integer");
  }
  if (parsed.fraction.length <= maximumFractionDigits) return parsed;

  const keptFraction = parsed.fraction.slice(0, maximumFractionDigits);
  const shouldRoundUp = parsed.fraction[maximumFractionDigits] >= "5";
  if (!shouldRoundUp) return { ...parsed, fraction: keptFraction };

  const combined = incrementDigits(`${parsed.integer}${keptFraction}`);
  if (maximumFractionDigits === 0) {
    return { ...parsed, integer: combined, fraction: "" };
  }
  return {
    ...parsed,
    integer: combined.slice(0, -maximumFractionDigits) || "0",
    fraction: combined.slice(-maximumFractionDigits).padStart(maximumFractionDigits, "0"),
  };
}

function resolveSign(parsed: ParsedDecimal, signDisplay: MoneySignDisplay): "" | "+" | "-" {
  if (signDisplay === "never") return "";
  if (parsed.negative) return "-";
  if (signDisplay === "always" && canonicalDecimal(parsed) !== "0") return "+";
  return "";
}

export function splitExactDecimal(
  value: string,
  options: {
    mode?: MoneyAmountMode;
    signDisplay?: MoneySignDisplay;
    maximumSummaryFractionDigits?: number;
    preserveScale?: number;
  } = {},
): MoneyParts {
  const mode = options.mode ?? "summary";
  const original = parseDecimal(value);
  let displayed = original;

  if (mode === "summary") {
    displayed = roundForSummary(original, options.maximumSummaryFractionDigits ?? 2);
    displayed = { ...displayed, fraction: displayed.fraction.replace(/0+$/, "") };
  } else if (options.preserveScale !== undefined) {
    displayed = {
      ...displayed,
      fraction: displayed.fraction
        .slice(0, options.preserveScale)
        .padEnd(options.preserveScale, "0"),
    };
  }

  const sign = resolveSign(displayed, options.signDisplay ?? "auto");
  const integerPart = groupInteger(displayed.integer);
  const fractionalPart = displayed.fraction;
  const displayNumber = `${sign}${integerPart}${fractionalPart ? `.${fractionalPart}` : ""}`;
  const exactSign = original.negative ? "-" : "";
  const exactNumber = `${exactSign}${groupInteger(original.integer)}${original.fraction ? `.${original.fraction}` : ""}`;

  return {
    sign,
    integerPart,
    fractionalPart,
    currency: "ISK",
    exactText: `${exactNumber} ISK`,
    displayText: `${displayNumber} ISK`,
    rounded: canonicalDecimal(displayed) !== canonicalDecimal(original),
  };
}

export function formatIskSummary(
  value: string,
  options: { signDisplay?: MoneySignDisplay; maximumFractionDigits?: number } = {},
): string {
  return splitExactDecimal(value, {
    mode: "summary",
    signDisplay: options.signDisplay,
    maximumSummaryFractionDigits: options.maximumFractionDigits,
  }).displayText;
}

export function formatSignedIsk(value: string): string {
  return formatIskSummary(value, { signDisplay: "always" });
}

export function formatIskCompact(value: string): string {
  const amount = Number(value);
  if (!Number.isFinite(amount)) return value;
  if (Math.abs(amount) < 1_000) {
    return new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 }).format(amount);
  }
  return new Intl.NumberFormat("en-US", {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(amount);
}

const ABBREVIATION_UNITS: Array<[number, string, number]> = [
  [1e12, "T", 2],
  [1e9, "B", 2],
  [1e6, "M", 1],
  [1e3, "K", 0],
];

/**
 * The one abbreviated ISK format for charts and cards: 934K, 498.1M, 1.53B.
 * K is whole, M has one decimal, B and T two; trailing zeros are dropped and
 * a value that rounds up to the next unit (999,999 -> 1M) moves to it. Add
 * `currency` for the " ISK" suffix. Exact values belong in tooltips and tables
 * (`formatIskSummary` / `MoneyAmount`), not here.
 */
export function formatIskAbbreviated(
  value: string,
  options: { currency?: boolean; signDisplay?: "auto" | "always" } = {},
): string {
  const amount = Number(value);
  if (!Number.isFinite(amount)) return value;
  const magnitude = Math.abs(amount);
  const sign = amount < 0 ? "-" : options.signDisplay === "always" && amount > 0 ? "+" : "";
  const trim = (number: number, digits: number) =>
    new Intl.NumberFormat("en-US", { maximumFractionDigits: digits, useGrouping: false }).format(number);
  let text: string;
  for (let index = 0; ; index += 1) {
    if (index === ABBREVIATION_UNITS.length) {
      text = trim(magnitude, 2);
      break;
    }
    const [size, suffix, digits] = ABBREVIATION_UNITS[index];
    if (magnitude < size) continue;
    const scaled = Number(trim(magnitude / size, digits));
    // Rounding may reach 1000 of this unit; express it in the next one up.
    if (scaled >= 1000 && index > 0) {
      const [nextSize, nextSuffix, nextDigits] = ABBREVIATION_UNITS[index - 1];
      text = `${trim(magnitude / nextSize, nextDigits)}${nextSuffix}`;
    } else {
      text = `${trim(magnitude / size, digits)}${suffix}`;
    }
    break;
  }
  return `${sign}${text}${options.currency ? " ISK" : ""}`;
}

export function formatIskForSentence(value: string, signed = false): string {
  return signed ? formatSignedIsk(value) : formatIskSummary(value);
}

export interface MoneyAmountProps extends Omit<HTMLAttributes<HTMLSpanElement>, "children"> {
  value: string;
  mode?: MoneyAmountMode;
  currency?: "ISK";
  signDisplay?: MoneySignDisplay;
  showCurrency?: boolean;
  preserveScale?: number;
  maximumSummaryFractionDigits?: number;
}

export function MoneyAmount({
  value,
  mode = "summary",
  currency = "ISK",
  signDisplay = "auto",
  showCurrency = true,
  preserveScale,
  maximumSummaryFractionDigits = 2,
  className = "",
  title,
  ...props
}: MoneyAmountProps) {
  const parts = splitExactDecimal(value, {
    mode,
    signDisplay,
    preserveScale,
    maximumSummaryFractionDigits,
  });
  const visibleText = showCurrency ? parts.displayText : parts.displayText.replace(` ${currency}`, "");
  const exactTitle = mode === "summary" && parts.rounded ? `Exact: ${parts.exactText}` : undefined;

  if (mode === "summary") {
    return (
      <span
        className={`iw-money ${className}`}
        title={title ?? exactTitle}
        {...props}
      >
        {visibleText}
      </span>
    );
  }

  return (
    <span
      aria-label={visibleText}
      className={`iw-money ${className}`}
      title={title}
      {...props}
    >
      <span aria-hidden="true">
        <span className="iw-money-integer">{parts.sign}{parts.integerPart}</span>
        {parts.fractionalPart ? <span className={mode === "detail" ? "iw-money-fraction" : ""}>.{parts.fractionalPart}</span> : null}
        {showCurrency ? <span className="iw-money-currency"> {currency}</span> : null}
      </span>
    </span>
  );
}
