use super::*;

// ---------------------------------------------------------------------------
// Shared formatting helpers
// ---------------------------------------------------------------------------

pub(super) fn header_format() -> Format {
    Format::new().set_bold()
}
pub(super) fn int_format() -> Format {
    Format::new().set_num_format("#,##0")
}
pub(super) fn isk_format() -> Format {
    Format::new().set_num_format("#,##0")
}
/// Money at the canonical planning-cost scale (4 decimal places) -- used for
/// every cost-reconciliation cell so a 0.0001 mismatch is visible, not hidden
/// by display rounding (display format never performs the domain rounding
/// itself; every Money formula below rounds explicitly via [`money_round`]).
pub(super) fn money4_format() -> Format {
    Format::new().set_num_format("#,##0.0000")
}
pub(super) fn factor_format() -> Format {
    Format::new().set_num_format("0.########")
}
pub(super) fn wrap_format() -> Format {
    Format::new().set_text_wrap().set_align(FormatAlign::Top)
}

pub(super) fn resolution_label(resolution: MaterialBoundaryResolution) -> &'static str {
    // Lowercase and identical to the JSON wire value: the ledger / operation
    // sheet formulas branch on `="build"` / `"reaction"`.
    match resolution {
        MaterialBoundaryResolution::Buy => "buy",
        MaterialBoundaryResolution::Build => "build",
        MaterialBoundaryResolution::Reaction => "reaction",
        MaterialBoundaryResolution::Unresolved => "unresolved",
    }
}
pub(super) fn scope_label(scope: FulfillmentScope) -> &'static str {
    match scope {
        FulfillmentScope::Full => "Full",
        FulfillmentScope::Missing => "Missing",
    }
}
pub(super) fn activity_label(activity: MaterialActivity) -> &'static str {
    match activity {
        MaterialActivity::Manufacturing => "manufacturing",
        MaterialActivity::Reaction => "reaction",
    }
}
pub(super) fn strategy_label(strategy: MaterialRowStrategy) -> &'static str {
    match strategy {
        MaterialRowStrategy::Buy => "Buy",
        MaterialRowStrategy::Build => "Build",
        MaterialRowStrategy::Reaction => "Reaction",
        MaterialRowStrategy::Mixed => "Mixed",
    }
}

/// 0 -> "A", 25 -> "Z", 26 -> "AA", ...
pub(super) fn col_letter(mut index: u32) -> String {
    let mut out = String::new();
    loop {
        out.insert(0, (b'A' + (index % 26) as u8) as char);
        if index < 26 {
            return out;
        }
        index = index / 26 - 1;
    }
}

pub(super) fn tree_path_label(path: &[i64]) -> String {
    path.iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(" > ")
}

pub(super) fn parent_path_label(path: &[i64]) -> String {
    if path.is_empty() {
        String::new()
    } else {
        tree_path_label(&path[..path.len() - 1])
    }
}

/// The `Types` lookup formula for a `type_id` in cell `$<col><xr>` -> item
/// name (or "Unknown"). `name_range` is a `Types_*` defined name.
pub(super) fn types_lookup(name_range: &str, key_cell: &str) -> String {
    format!("=IFERROR(INDEX({name_range},MATCH({key_cell},Types_Id,0)),\"Unknown\")")
}

/// [`types_lookup`], but falls back to a blank string instead of `"Unknown"`
/// -- used for numeric evidence (e.g. `Types_AdjustedPrice`) where a missing
/// value must read as *absent* to a downstream `ISNUMBER`/`=""` guard, not as
/// a human-readable placeholder.
pub(super) fn types_lookup_blank(name_range: &str, key_cell: &str) -> String {
    format!("=IFERROR(INDEX({name_range},MATCH({key_cell},Types_Id,0)),\"\")")
}

pub(super) fn write_bold_headers(sheet: &mut Worksheet, headers: &[&str]) -> Result<(), XlsxError> {
    let head = header_format();
    for (col, label) in headers.iter().enumerate() {
        sheet.write_string_with_format(0, col as u16, *label, &head)?;
    }
    Ok(())
}

pub(super) fn add_plain_table(
    sheet: &mut Worksheet,
    name: &str,
    headers: &[&str],
    data_rows: usize,
) -> Result<(), XlsxError> {
    let columns: Vec<TableColumn> = headers
        .iter()
        .map(|h| TableColumn::new().set_header(*h))
        .collect();
    let table = Table::new().set_name(name).set_columns(&columns);
    sheet.add_table(0, 0, data_rows as u32, (headers.len() - 1) as u16, &table)?;
    Ok(())
}

pub(super) fn set_default_widths(sheet: &mut Worksheet, headers: &[&str]) {
    for (col, label) in headers.iter().enumerate() {
        let width = match *label {
            "Build ID" | "Graph Node ID" => 40.0,
            "Type Name" | "Material" | "Item" | "Product" | "Blueprint/Formula" => 28.0,
            "Tree Path" | "Parent Path" => 18.0,
            _ => (label.len() as f64 + 2.0).max(12.0),
        };
        let _ = sheet.set_column_width(col as u16, width);
    }
}

pub(super) fn decimal_f64(value: rust_decimal::Decimal) -> f64 {
    value.to_f64().unwrap_or(0.0)
}

pub(super) fn opt_decimal_f64(value: Option<Decimal>) -> Option<f64> {
    value.and_then(|v| v.to_f64())
}
pub(super) fn opt_money_f64(value: Option<Money>) -> Option<f64> {
    value.and_then(|v| v.0.to_f64())
}
pub(super) fn write_opt_f64(
    sheet: &mut Worksheet,
    row: u32,
    col: u16,
    value: Option<f64>,
    fmt: &Format,
) -> Result<(), XlsxError> {
    match value {
        Some(v) => sheet.write_number_with_format(row, col, v, fmt)?,
        None => sheet.write_blank(row, col, &Format::new())?,
    };
    Ok(())
}

// ---------------------------------------------------------------------------
// Money-rounding contract (see the module doc + `write_summary`'s "MONEY
// ROUNDING" note)
// ---------------------------------------------------------------------------

/// `build_cost.rs`'s canonical `Money` rounding funnels
/// through its local `m()` helper -- `Decimal::round_dp(4)`, which is
/// `RoundingStrategy::MidpointNearestEven` ("banker's" rounding) by
/// construction (`rust_decimal`'s documented default for `round_dp`). Every
/// `inventory_cost`, `fresh_cost`, `consumed_child_cost`,
/// `material_component_cost`, `total_production_cost`, `eiv`, and every
/// installation component (`unmodified_system_index_cost`,
/// `system_index_cost`, `facility_tax`, `scc_surcharge`,
/// `alliance_surcharge`, `own_installation.total`) goes through `m()`, hence
/// half-to-even.
///
/// The **one** exception: `child_unit_production_cost` and
/// `unit_production_cost` (both explicitly display/evidence-only, never fed
/// back into a canonical figure) are rounded via
/// [`Money::checked_div_quantity`], which calls `Decimal::rescale` --
/// verified against `rust_decimal`'s own `rescale_internal` (which looks
/// only at the single digit immediately after the cut and rounds up whenever
/// it is `>= 5`, with no even/odd tie-break) to be **half-away-from-zero**,
/// not half-to-even. Plain Excel `ROUND` is *already* half-away-from-zero,
/// so [`money_round`] (this function) is the *correct*, exact match for
/// those two cells specifically -- using the half-to-even machinery there
/// would introduce a real divergence, not close one.
///
/// `money_round` remains the right choice for a handful of other cells too,
/// where it is applied to a value that is *already* an exact multiple of
/// `0.0001` before rounding (an integer quantity times an already-4dp
/// `Money` cell, or a `SUM` of already-4dp cells) -- rounding an
/// already-exact value to 4dp is a no-op under every rounding rule, so which
/// rule Excel uses cannot matter. Each call site below says which case it
/// is; see [`money_round_half_even`] for the boundaries that need the real
/// thing.
pub(super) fn money_round(expr: &str) -> String {
    format!("ROUND({expr},4)")
}

/// Excel-side half-to-even ("banker's") rounding to 4 decimal places for a
/// general expression whose exact decimal-digit count is not bounded ahead
/// of time (an externally-sourced adjusted price or a manually configured
/// percentage/system-cost-index, each of unconstrained precision) -- the
/// real match for every canonical-Money boundary [`money_round`] would get
/// wrong on a tie (EIV and every installation component; see that
/// function's doc for the ones where plain `ROUND` is already correct).
///
/// Excel has no native half-to-even rounding, so this reconstructs it:
/// scale the expression by `10^4`, split it into its integer part and
/// fractional remainder, and compare the remainder to exactly `0.5`. The
/// remainder is first collapsed to 9 decimal places (i.e. ties are resolved
/// down to `1e-13` of an ISK in the original units) so IEEE-754 double
/// representation noise from the preceding multiplication/division can
/// never be misread as a genuine mathematical tie -- while a real
/// difference at that scale, far finer than any realistic price or
/// percentage input carries, still reads as non-tied. On a genuine tie,
/// round to whichever of the two adjacent scaled integers is even.
///
/// `expr` is inlined multiple times (Excel has no `LET` in every supported
/// version -- see the module's Excel-compatibility note); it must be a
/// side-effect-free reference/expression, which every call site here is.
pub(super) fn money_round_half_even(expr: &str) -> String {
    let scaled = format!("(({expr})*10000)");
    let floor = format!("INT({scaled})");
    let frac = format!("ROUND({scaled}-{floor},9)");
    format!("(({floor})+IF({frac}<0.5,0,IF({frac}>0.5,1,IF(MOD({floor},2)=0,0,1))))/10000")
}

/// Excel-side half-to-even rounding of `total_cell * num_cell / den_cell`
/// where `total_cell` is already a canonical `Money` value (an exact
/// multiple of `0.0001`) and `num_cell`/`den_cell` are exact integer
/// quantities -- the [`consumed_child_cost`](m) boundary specifically
/// (`total.0.checked_mul(consumed).checked_div(produced)` then `m()` in
/// `build_cost.rs`).
///
/// This is the one boundary where floating-point midpoint detection isn't
/// necessary *or* the most defensible choice: because `total_cell` is
/// already known to be an exact multiple of `0.0001`, `total_cell * 10000`
/// is mathematically an exact integer, so the whole computation can be
/// pushed into exact-integer arithmetic (`INT`/`MOD`), never comparing a
/// floating-point remainder to `0.5` at all. `ROUND(total_cell*10000, 0)`
/// recovers that integer exactly, discarding only the double-precision
/// storage noise from `total_cell` having been written as an `f64` (never
/// discarding a genuine fractional digit, because there isn't one to
/// discard). From there, rounding the rational `scaled_total * num / den`
/// to the nearest integer with a tie going to the even one is done by
/// comparing `2 * remainder` against `den` -- an exact integer comparison,
/// not a floating-point one -- which is identical, digit for digit, to
/// `round_dp(4)` on the original quotient (rounding to 4 decimal places is
/// rounding the value scaled by `10^4` to the nearest integer).
///
/// Exact as long as `scaled_total * num` stays within a `f64`'s
/// exactly-representable integer range (`2^53`, roughly `9×10^15`) --
/// comfortably enough headroom for any realistic Build's ISK/quantity
/// magnitudes; see the module's Excel-compatibility note for this bound.
pub(super) fn money_round_half_even_ratio(
    total_cell: &str,
    num_cell: &str,
    den_cell: &str,
) -> String {
    let scaled_total = format!("ROUND(({total_cell})*10000,0)");
    let prod = format!("(({scaled_total})*({num_cell}))");
    let q = format!("INT({prod}/({den_cell}))");
    let r = format!("(({prod})-({q})*({den_cell}))");
    let twice_r = format!("(2*({r}))");
    format!(
        "((({q})+IF({twice_r}<({den_cell}),0,IF({twice_r}>({den_cell}),1,IF(MOD({q},2)=0,0,1)))))/10000"
    )
}

/// `a + b` (or any binary op via `op`) where either operand cell may instead
/// hold the `"INCOMPLETE"` text marker -- a primitive that is genuinely
/// missing (a missing fresh price, inventory basis, adjusted price, ...),
/// never silently substituted with zero. If both operands are numeric the
/// arithmetic runs normally; if either is the marker, the result is
/// `"INCOMPLETE"` too, so incompleteness propagates upward through every
/// dependent cost formula instead of being swallowed into a false `0`.
pub(super) fn incomplete_guard2(a: &str, b: &str, op: &str) -> String {
    format!("IF(AND(ISNUMBER({a}),ISNUMBER({b})),{a}{op}{b},\"INCOMPLETE\")")
}
/// Single-operand form of [`incomplete_guard2`]: `expr` passed through
/// unchanged if numeric, `"INCOMPLETE"` otherwise.
pub(super) fn incomplete_guard1(a: &str) -> String {
    format!("IF(ISNUMBER({a}),{a},\"INCOMPLETE\")")
}
/// A Delta cell: `excel - api` only when both sides are numeric, blank
/// otherwise (an incomplete/missing side has no meaningful delta).
pub(super) fn cost_delta(excel: &str, api: &str) -> String {
    format!("IF(AND(ISNUMBER({excel}),ISNUMBER({api})),{excel}-{api},\"\")")
}

/// One installation-cost percentage component (facility tax / SCC / alliance
/// surcharge): half-to-even `ROUND(eiv * pct/100, 4)`, `"INCOMPLETE"` when
/// EIV itself is not numeric -- matches `operation_installation_cost`'s
/// shared `component` closure exactly (each percentage only ever needs
/// `eiv`, never `system_cost_index`). `pct` is a manually configured
/// percentage of unconstrained precision, so this is a genuine
/// half-to-even boundary -- [`money_round_half_even`], never plain
/// [`money_round`].
pub(super) fn component_formula(eiv_excel_cell: &str, pct_cell: &str) -> String {
    format!(
        "=IF(ISNUMBER({eiv_excel_cell}),{rounded},\"INCOMPLETE\")",
        rounded = money_round_half_even(&format!("{eiv_excel_cell}*({pct_cell}/100)")),
    )
}
