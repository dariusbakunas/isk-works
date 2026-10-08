/**
 * Presentation-layer parser for human-typed ISK amounts.
 *
 * This is the ONLY place raw editing text is turned into a canonical,
 * server-ready monetary string. It mirrors the domain invariant enforced by
 * `Money::parse` in `crates/iskworks-core/src/industry/money.rs`
 * (non-negative, at most four *significant* fractional digits) so that an
 * invalid value never leaves the input and reaches an API request. It does
 * NOT replace the backend check -- `Money::parse` stays authoritative.
 *
 * The canonical form is unformatted (grouping commas stripped); the API DTO
 * and the Rust `Money` type are unchanged.
 */

export const MONEY_INPUT_MESSAGES = {
  // `Money::parse` accepts zero and rejects only negatives -- the copy must
  // describe that non-negative invariant, not imply zero is disallowed.
  negative: "Enter zero or a positive amount.",
  characters: "Use only digits, commas, and a decimal point.",
  decimals: "At most 4 decimal places.",
  decimalPoint: "Enter a single decimal point.",
  grouping: "Use commas only every 3 digits, e.g. 1,000.",
} as const;

export type MoneyInputStatus = "empty" | "incomplete" | "valid" | "invalid";

export interface MoneyInputResult {
  status: MoneyInputStatus;
  /** Present iff `status === "valid"`: unformatted, server-ready string. */
  canonical?: string;
  /** Present iff `status === "invalid"`: inline validation message. */
  message?: string;
}

/** A run of digits with no grouping. */
const UNGROUPED = /^\d+$/;
/** A fully-formed grouped integer: `1,234` / `12,345,678`. */
const GROUPED_COMPLETE = /^\d{1,3}(?:,\d{3})+$/;
/**
 * A grouped integer still being typed: a trailing comma, or a final group
 * with only one or two digits so far (`1,` / `12,3` / `1,234,5`).
 */
const GROUPED_TYPING = /^\d{1,3}(?:,\d{3})*,\d{0,2}$/;

const INCOMPLETE: MoneyInputResult = { status: "incomplete" };
const EMPTY: MoneyInputResult = { status: "empty" };

function invalid(message: string): MoneyInputResult {
  return { status: "invalid", message };
}

/**
 * Classify raw editing text.
 *
 * @param options.final `true` for a blur/Enter commit: a trailing decimal
 *   point is dropped (`1,000.` -> `1000`) and an unfinished comma group
 *   becomes invalid instead of merely incomplete.
 */
export function parseMoneyInput(
  raw: string,
  options: { final?: boolean } = {},
): MoneyInputResult {
  const final = options.final ?? false;
  const text = raw.trim();
  if (text === "") return EMPTY;

  if (text.startsWith("-")) return invalid(MONEY_INPUT_MESSAGES.negative);
  if (/[^\d.,]/.test(text)) return invalid(MONEY_INPUT_MESSAGES.characters);

  const dotCount = (text.match(/\./g) ?? []).length;
  if (dotCount > 1) return invalid(MONEY_INPUT_MESSAGES.decimalPoint);

  let hadDot = text.includes(".");
  const [intText, fracText = ""] = text.split(".");

  // A lone "." (or ".") with nothing on either side: just keep editing.
  if (intText === "" && fracText === "") return INCOMPLETE;

  // ----- fractional part -----
  if (hadDot && fracText !== "") {
    if (!UNGROUPED.test(fracText)) return invalid(MONEY_INPUT_MESSAGES.grouping);
    const significant = fracText.replace(/0+$/, "");
    if (significant.length > 4) return invalid(MONEY_INPUT_MESSAGES.decimals);
  }

  // ----- integer part -----
  let intKind: "complete" | "typing" | "bad";
  if (intText === "") {
    // ".5" -> treat as 0.5; "." handled above.
    intKind = "complete";
  } else if (UNGROUPED.test(intText) || GROUPED_COMPLETE.test(intText)) {
    intKind = "complete";
  } else if (GROUPED_TYPING.test(intText)) {
    intKind = "typing";
  } else {
    intKind = "bad";
  }

  if (intKind === "bad") return invalid(MONEY_INPUT_MESSAGES.grouping);
  if (intKind === "typing") {
    // A half-typed group (`1,00`) is only a repairable prefix while nothing
    // has been appended after it. Once a decimal point exists the integer
    // part is frozen, so `1,00.` / `1,00.5` can never become valid.
    return final || hadDot
      ? invalid(MONEY_INPUT_MESSAGES.grouping)
      : INCOMPLETE;
  }

  // Trailing decimal point with no fractional digits yet.
  if (hadDot && fracText === "") {
    if (!final) return INCOMPLETE;
    hadDot = false;
  }

  const intDigits = (intText === "" ? "0" : intText)
    .replace(/,/g, "")
    .replace(/^0+(?=\d)/, "");
  const canonical = hadDot && fracText !== "" ? `${intDigits}.${fracText}` : intDigits;
  return { status: "valid", canonical };
}

/**
 * Re-group a canonical amount with en-US thousands separators for display.
 * Insignificant trailing fractional zeros are dropped so the server's
 * 4dp-rescaled echo (`1000000.0000`) shows the same as what the user typed
 * (`1,000,000`).
 */
export function formatMoneyInputDisplay(canonical: string): string {
  if (canonical === "") return "";
  const [intPart, rawFrac] = canonical.split(".");
  const grouped = intPart.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  const frac = (rawFrac ?? "").replace(/0+$/, "");
  return frac ? `${grouped}.${frac}` : grouped;
}
