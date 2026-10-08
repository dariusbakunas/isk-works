# Context-Aware ISK Formatting

## Scope

Monetary presentation is standardized across Build planning, Inventory accounting, ESI wallet review, market imports, prices, and facility estimates. Authoritative Rust, persistence, and API decimal values are unchanged.

## Problem Statement

Fixed four-decimal rendering made prominent whole-ISK values such as `245,080.0000 ISK` harder to scan. The same precision remains valuable in calculation and audit views, so one display style was not appropriate for every context.

## Accounting Versus Presentation Precision

The API continues to return exact decimal strings. Summary, detail, and exact formatting parse, group, and round decimal digits as strings, never through JavaScript `number`; formatting cannot mutate inventory events or historical-cost calculations. Only the compact and abbreviated display formatters (below) go through `number`, and they are never used where the exact value matters.

## Summary Mode

Summary mode groups thousands, hides trailing fractional zeros, omits the decimal point for whole ISK, and displays at most two fractional digits. It is used for cards, page-level metrics, comparison summaries, and explanatory prose.

Examples:

- `245080.0000` becomes `245,080 ISK`
- `245080.5000` becomes `245,080.5 ISK`
- `245080.1250` becomes `245,080.13 ISK`

## Detail Mode

Detail mode preserves the supplied fractional scale. The integer uses normal emphasis; the decimal point and fraction use the existing muted token at `0.76em`. All parts use tabular numerals and the amount does not wrap internally.

Detail mode is used for material calculations, wallet transactions, inventory formulas, and other itemized accounting rows.

## Exact Mode

Exact mode preserves the parsed decimal scale without trimming or rounding and uses tabular numerals. It is available for explicit audit, verification, and raw-value contexts.

## Rounding Policy

Summary values use decimal half-up rounding on the absolute value. This is display-only:

- `0.0049` becomes `0 ISK`
- `0.0050` becomes `0.01 ISK`
- `-1.1050` becomes `-1.11 ISK`

No binary floating-point operation participates in summary, detail, or exact formatting.

## Trailing-Zero Policy

Summary mode trims all insignificant fractional zeros. Detail and exact modes preserve the supplied authoritative scale unless a caller explicitly requests a display scale.

## Compact And Abbreviated Display

- `formatIskCompact` renders dense table cells with `Intl` compact notation (values under 1,000 keep up to two fractional digits). Cells show the compact value and put the exact `formatIskSummary` value in the `title` attribute.
- `formatIskAbbreviated` is the single abbreviated format for charts and cards (`934K`, `498.1M`, `1.53B`): K is whole, M has one decimal, B and T two, trailing zeros are dropped, and a value that rounds up to the next unit moves to it. An option adds the ` ISK` suffix.

## Signed-Money Behavior

Comparisons use `signDisplay="always"`: positive values receive `+`, negative values retain `-`, and zero has no sign. Ordinary costs and revenue use automatic signs. Meaning remains in the metric label or sentence rather than color alone.

## Shared Component API

`MoneyAmount` accepts:

- `value`
- `mode`: `summary`, `detail`, or `exact`
- `signDisplay`: `auto`, `always`, or `never`
- `showCurrency`
- `preserveScale`
- `maximumSummaryFractionDigits`
- normal span attributes such as `className`, `title`, and test or ARIA metadata

## Shared Formatter API

`components/money.tsx` exports:

- `splitExactDecimal`
- `formatIskSummary`
- `formatSignedIsk`
- `formatIskCompact`
- `formatIskAbbreviated`
- `formatIskForSentence`
- `MoneyAmount`

Structured parts include sign, grouped integer, fraction, exact text, display text, currency, and whether display rounding occurred.

## Exact-Value Accessibility

Detail and exact modes expose one complete accessible label while visual child spans are hidden from the accessibility tree, preventing duplicate announcements. A rounded summary receives a native title containing the exact value. Trimming zeroes alone does not create tooltip noise.

## Typography Treatment

All money uses tabular numerals. Detail fractions use `--color-muted` and `0.76em`; the ISK suffix remains attached to the amount. No new font or semantic color convention was introduced.

## Table Behavior

Existing table and grid sorting continues to use authoritative DTO values. Formatting occurs only during rendering. Monetary cells retain their existing right-side placement and stable tabular alignment.

## Mobile Behavior

Amounts remain internally unbroken so fractions and the ISK suffix cannot become detached. Summary grids continue to stack at existing responsive breakpoints. Abbreviations are limited to `formatIskCompact` and `formatIskAbbreviated`.

## Accessibility

Screen readers receive a complete value in natural order. Styled fractions do not create duplicate announcements. Signs are textual, contrast uses existing tokens, and native title text provides exact rounded values without adding a separate focus target to every card.

## Backend Boundary

Formatting never changes money domain types, decimal scales, database columns, API serialization, or inventory events. Server-authored explanatory sentences are formatted at render time without changing their source DTOs.

## Non-Money Values

Percentages, quantities, efficiency values, cost indices, rates, durations, and IDs remain on their existing non-money formatters.

## Tests

Formatter tests cover zero, whole and fractional values, trailing zeros, half-up rounding, negatives, explicit positive signs, leading zeros, very large values, sub-ISK values, invalid input, grouping, exact scale, and detail splitting.

Component tests cover summary, detail, exact, rounded-value titles, non-rounded summaries, accessible labels, signed values, hidden currency, and custom classes.

## Browser Verification

Verify a Build decision summary, expanded material calculation, and Inventory detail at desktop and approximately 390x844. Confirm concise summary values, muted exact fractions, signed differences, no duplicate suffixes, no horizontal overflow, and a clean console.
