use super::*;

/// The "INSTALLATION & TOTAL" key/value(/API/Delta) block written two rows
/// below an operation sheet's own material table (see
/// [`installation_section_row`]). `nbound` is this operation's own boundary
/// count (its material table's row count) -- the caller's `boundaries.len()`.
///
/// Independently reproduces, from primitives only (never the API's
/// precomputed installation total): EIV, the facility job-cost chain,
/// Material/Component Cost as `SUM` of this op's own boundary rows'
/// `Excel Requirement Cost`, and Total Production Cost, which is Material
/// plus Installation. Every computed row shows Excel, API,
/// and Delta; primitive facility inputs (percentages, SCI, fixed
/// supplemental cost) are single-value evidence rows -- they are inputs,
/// not something to compare.
pub(super) fn write_operation_cost_section(
    sheet: &mut Worksheet,
    op: &VerificationOperationInput,
    op_cost: Option<&OperationCostProjection>,
    nbound: usize,
) -> Result<(), XlsxError> {
    let head = header_format();
    let money4 = money4_format();
    let int = int_format();
    let pct = Format::new().set_num_format("0.####");
    let sci_fmt = Format::new().set_num_format("0.########");

    let section = installation_section_row(nbound);
    let row = |offset: u32| section + offset;
    let cell = |r: u32, col: u16| format!("${}{}", col_letter(u32::from(col)), r + 1);

    sheet.write_string_with_format(section, 0, "INSTALLATION & TOTAL", &head)?;

    // --- primitive facility inputs (single Value column, B) -------------
    sheet.write_string_with_format(row(INST_ROW_SCI), 0, "System Cost Index", &head)?;
    write_opt_f64(
        sheet,
        row(INST_ROW_SCI),
        1,
        opt_decimal_f64(op.system_cost_index),
        &sci_fmt,
    )?;
    sheet.write_string_with_format(
        row(INST_ROW_JOB_REDUCTION_PCT),
        0,
        "Job Cost Reduction %",
        &head,
    )?;
    sheet.write_number_with_format(
        row(INST_ROW_JOB_REDUCTION_PCT),
        1,
        decimal_f64(op.job_cost_reduction_percent),
        &pct,
    )?;
    sheet.write_string_with_format(row(INST_ROW_FACILITY_TAX_PCT), 0, "Facility Tax %", &head)?;
    sheet.write_number_with_format(
        row(INST_ROW_FACILITY_TAX_PCT),
        1,
        decimal_f64(op.facility_tax_percent),
        &pct,
    )?;
    sheet.write_string_with_format(row(INST_ROW_SCC_PCT), 0, "SCC Surcharge %", &head)?;
    sheet.write_number_with_format(
        row(INST_ROW_SCC_PCT),
        1,
        decimal_f64(op.scc_surcharge_percent),
        &pct,
    )?;
    sheet.write_string_with_format(row(INST_ROW_ALLIANCE_PCT), 0, "Alliance Surcharge %", &head)?;
    sheet.write_number_with_format(
        row(INST_ROW_ALLIANCE_PCT),
        1,
        decimal_f64(op.alliance_surcharge_percent),
        &pct,
    )?;
    sheet.write_string_with_format(
        row(INST_ROW_FIXED_SUPPLEMENTAL),
        0,
        "Fixed Supplemental Cost",
        &head,
    )?;
    sheet.write_number_with_format(
        row(INST_ROW_FIXED_SUPPLEMENTAL),
        1,
        decimal_f64(op.fixed_supplemental_cost.0),
        &money4,
    )?;

    // --- computed (Excel | API | Delta) ----------------------------------
    for (col, label) in ["", "Excel", "API", "Delta"].iter().enumerate() {
        sheet.write_string_with_format(row(INST_ROW_TABLE_HEADER), col as u16, *label, &head)?;
    }

    let sci_cell = cell(row(INST_ROW_SCI), 1);
    let job_reduction_cell = cell(row(INST_ROW_JOB_REDUCTION_PCT), 1);
    let fac_tax_pct_cell = cell(row(INST_ROW_FACILITY_TAX_PCT), 1);
    let scc_pct_cell = cell(row(INST_ROW_SCC_PCT), 1);
    let alliance_pct_cell = cell(row(INST_ROW_ALLIANCE_PCT), 1);
    let fixed_supp_cell = cell(row(INST_ROW_FIXED_SUPPLEMENTAL), 1);

    // EIV: SUM of this op's own boundary rows' unrounded EIV Term cells,
    // rounded ONCE (never per-term -- matches `operation_installation_cost`
    // exactly; the two orders genuinely diverge on ties, see
    // `eiv_sums_unrounded_terms_before_rounding_once_not_the_other_way_round`).
    // Adjusted prices are of unconstrained precision, so this sum is a
    // genuine half-to-even boundary -- `money_round_half_even`. Blank
    // material table -> EIV is trivially 0 (complete).
    sheet.write_string_with_format(row(INST_ROW_EIV), 0, "EIV", &head)?;
    let eiv_excel_cell = cell(row(INST_ROW_EIV), INST_COL_EXCEL);
    if nbound == 0 {
        sheet.write_number_with_format(row(INST_ROW_EIV), INST_COL_EXCEL, 0.0, &money4)?;
    } else {
        let eiv_col = col_letter(u32::from(OP_EIV_TERM));
        let range = format!(
            "${eiv_col}${first}:${eiv_col}${last}",
            first = OP_FIRST_DATA_XR,
            last = OP_FIRST_DATA_XR + nbound as u32 - 1
        );
        sheet.write_formula_with_format(
            row(INST_ROW_EIV),
            INST_COL_EXCEL,
            Formula::new(format!(
                "=IF(COUNTIF({range},\"INCOMPLETE\")>0,\"INCOMPLETE\",{rounded})",
                rounded = money_round_half_even(&format!("SUM({range})")),
            )),
            &money4,
        )?;
    }
    write_opt_f64(
        sheet,
        row(INST_ROW_EIV),
        INST_COL_API,
        op_cost.and_then(|o| opt_money_f64(o.own_installation.eiv)),
        &money4,
    )?;
    sheet.write_formula(
        row(INST_ROW_EIV),
        INST_COL_DELTA,
        Formula::new(cost_delta(
            &cell(row(INST_ROW_EIV), INST_COL_EXCEL),
            &cell(row(INST_ROW_EIV), INST_COL_API),
        )),
    )?;

    sheet.write_string_with_format(
        row(INST_ROW_UNMOD_SYSTEM_INDEX_COST),
        0,
        "Unmodified System Index Cost",
        &head,
    )?;
    // `system_cost_index` is a manually configured Decimal of unconstrained
    // precision -- genuine half-to-even boundary.
    sheet.write_formula_with_format(
        row(INST_ROW_UNMOD_SYSTEM_INDEX_COST),
        INST_COL_EXCEL,
        Formula::new(format!(
            "=IF(AND(ISNUMBER({eiv_excel_cell}),ISNUMBER({sci_cell})),{rounded},\"INCOMPLETE\")",
            rounded = money_round_half_even(&format!("{eiv_excel_cell}*{sci_cell}")),
        )),
        &money4,
    )?;
    write_opt_f64(
        sheet,
        row(INST_ROW_UNMOD_SYSTEM_INDEX_COST),
        INST_COL_API,
        op_cost.and_then(|o| opt_money_f64(o.own_installation.unmodified_system_index_cost)),
        &money4,
    )?;
    sheet.write_formula(
        row(INST_ROW_UNMOD_SYSTEM_INDEX_COST),
        INST_COL_DELTA,
        Formula::new(cost_delta(
            &cell(row(INST_ROW_UNMOD_SYSTEM_INDEX_COST), INST_COL_EXCEL),
            &cell(row(INST_ROW_UNMOD_SYSTEM_INDEX_COST), INST_COL_API),
        )),
    )?;

    let unmod_excel_cell = cell(row(INST_ROW_UNMOD_SYSTEM_INDEX_COST), INST_COL_EXCEL);
    sheet.write_string_with_format(
        row(INST_ROW_SYSTEM_INDEX_COST),
        0,
        "System Index Cost",
        &head,
    )?;
    // `job_cost_reduction_percent` is likewise unconstrained precision --
    // genuine half-to-even boundary.
    sheet.write_formula_with_format(
        row(INST_ROW_SYSTEM_INDEX_COST),
        INST_COL_EXCEL,
        Formula::new(format!(
            "=IF(ISNUMBER({unmod_excel_cell}),{rounded},\"INCOMPLETE\")",
            rounded =
                money_round_half_even(&format!("{unmod_excel_cell}*(1-{job_reduction_cell}/100)")),
        )),
        &money4,
    )?;
    write_opt_f64(
        sheet,
        row(INST_ROW_SYSTEM_INDEX_COST),
        INST_COL_API,
        op_cost.and_then(|o| opt_money_f64(o.own_installation.system_index_cost)),
        &money4,
    )?;
    sheet.write_formula(
        row(INST_ROW_SYSTEM_INDEX_COST),
        INST_COL_DELTA,
        Formula::new(cost_delta(
            &cell(row(INST_ROW_SYSTEM_INDEX_COST), INST_COL_EXCEL),
            &cell(row(INST_ROW_SYSTEM_INDEX_COST), INST_COL_API),
        )),
    )?;

    sheet.write_string_with_format(row(INST_ROW_FACILITY_TAX), 0, "Facility Tax", &head)?;
    sheet.write_formula_with_format(
        row(INST_ROW_FACILITY_TAX),
        INST_COL_EXCEL,
        Formula::new(component_formula(&eiv_excel_cell, &fac_tax_pct_cell)),
        &money4,
    )?;
    write_opt_f64(
        sheet,
        row(INST_ROW_FACILITY_TAX),
        INST_COL_API,
        op_cost.and_then(|o| opt_money_f64(o.own_installation.facility_tax)),
        &money4,
    )?;
    sheet.write_formula(
        row(INST_ROW_FACILITY_TAX),
        INST_COL_DELTA,
        Formula::new(cost_delta(
            &cell(row(INST_ROW_FACILITY_TAX), INST_COL_EXCEL),
            &cell(row(INST_ROW_FACILITY_TAX), INST_COL_API),
        )),
    )?;

    sheet.write_string_with_format(row(INST_ROW_SCC_SURCHARGE), 0, "SCC Surcharge", &head)?;
    sheet.write_formula_with_format(
        row(INST_ROW_SCC_SURCHARGE),
        INST_COL_EXCEL,
        Formula::new(component_formula(&eiv_excel_cell, &scc_pct_cell)),
        &money4,
    )?;
    write_opt_f64(
        sheet,
        row(INST_ROW_SCC_SURCHARGE),
        INST_COL_API,
        op_cost.and_then(|o| opt_money_f64(o.own_installation.scc_surcharge)),
        &money4,
    )?;
    sheet.write_formula(
        row(INST_ROW_SCC_SURCHARGE),
        INST_COL_DELTA,
        Formula::new(cost_delta(
            &cell(row(INST_ROW_SCC_SURCHARGE), INST_COL_EXCEL),
            &cell(row(INST_ROW_SCC_SURCHARGE), INST_COL_API),
        )),
    )?;

    sheet.write_string_with_format(
        row(INST_ROW_ALLIANCE_SURCHARGE),
        0,
        "Alliance Surcharge",
        &head,
    )?;
    sheet.write_formula_with_format(
        row(INST_ROW_ALLIANCE_SURCHARGE),
        INST_COL_EXCEL,
        Formula::new(component_formula(&eiv_excel_cell, &alliance_pct_cell)),
        &money4,
    )?;
    write_opt_f64(
        sheet,
        row(INST_ROW_ALLIANCE_SURCHARGE),
        INST_COL_API,
        op_cost.and_then(|o| opt_money_f64(o.own_installation.alliance_surcharge)),
        &money4,
    )?;
    sheet.write_formula(
        row(INST_ROW_ALLIANCE_SURCHARGE),
        INST_COL_DELTA,
        Formula::new(cost_delta(
            &cell(row(INST_ROW_ALLIANCE_SURCHARGE), INST_COL_EXCEL),
            &cell(row(INST_ROW_ALLIANCE_SURCHARGE), INST_COL_API),
        )),
    )?;

    sheet.write_string_with_format(
        row(INST_ROW_OWN_INSTALL_TOTAL),
        0,
        "Own Installation Total",
        &head,
    )?;
    if op.facility_id.is_none() {
        sheet.write_string(
            row(INST_ROW_OWN_INSTALL_TOTAL),
            INST_COL_EXCEL,
            "INCOMPLETE",
        )?;
    } else {
        let sum = [
            fixed_supp_cell.clone(),
            cell(row(INST_ROW_SYSTEM_INDEX_COST), INST_COL_EXCEL),
            cell(row(INST_ROW_FACILITY_TAX), INST_COL_EXCEL),
            cell(row(INST_ROW_SCC_SURCHARGE), INST_COL_EXCEL),
            cell(row(INST_ROW_ALLIANCE_SURCHARGE), INST_COL_EXCEL),
        ]
        .join("+");
        sheet.write_formula_with_format(
            row(INST_ROW_OWN_INSTALL_TOTAL),
            INST_COL_EXCEL,
            Formula::new(format!(
                "=IF(AND(ISNUMBER({eiv_excel_cell}),ISNUMBER({sci_cell})),{sum},\"INCOMPLETE\")"
            )),
            &money4,
        )?;
    }
    write_opt_f64(
        sheet,
        row(INST_ROW_OWN_INSTALL_TOTAL),
        INST_COL_API,
        op_cost.and_then(|o| opt_money_f64(o.own_installation.total)),
        &money4,
    )?;
    sheet.write_formula(
        row(INST_ROW_OWN_INSTALL_TOTAL),
        INST_COL_DELTA,
        Formula::new(cost_delta(
            &cell(row(INST_ROW_OWN_INSTALL_TOTAL), INST_COL_EXCEL),
            &cell(row(INST_ROW_OWN_INSTALL_TOTAL), INST_COL_API),
        )),
    )?;

    // Material / Component Cost: SUM of this op's own boundary rows'
    // Excel Requirement Cost -- never a separate re-derivation, never adding
    // a child operation's total again (that value only ever reaches this op
    // through its own boundary's already-proportional consumed-cost cell).
    sheet.write_string_with_format(
        row(INST_ROW_MATERIAL_COST),
        0,
        "Material / Component Cost",
        &head,
    )?;
    if nbound == 0 {
        sheet.write_number_with_format(
            row(INST_ROW_MATERIAL_COST),
            INST_COL_EXCEL,
            0.0,
            &money4,
        )?;
    } else {
        // Every summand is already an exact multiple of `0.0001` (each
        // boundary's own Excel Requirement Cost cell), so their SUM is
        // exact too -- plain `money_round` (a no-op here) is correct;
        // `m()` re-rounds this sum in Rust too, for the same reason.
        let req_col = col_letter(u32::from(OP_EXCEL_REQUIREMENT_COST));
        let range = format!(
            "${req_col}${first}:${req_col}${last}",
            first = OP_FIRST_DATA_XR,
            last = OP_FIRST_DATA_XR + nbound as u32 - 1
        );
        sheet.write_formula_with_format(
            row(INST_ROW_MATERIAL_COST),
            INST_COL_EXCEL,
            Formula::new(format!(
                "=IF(COUNTIF({range},\"INCOMPLETE\")>0,\"INCOMPLETE\",{rounded})",
                rounded = money_round(&format!("SUM({range})")),
            )),
            &money4,
        )?;
    }
    write_opt_f64(
        sheet,
        row(INST_ROW_MATERIAL_COST),
        INST_COL_API,
        op_cost.and_then(|o| opt_money_f64(o.material_component_cost)),
        &money4,
    )?;
    sheet.write_formula(
        row(INST_ROW_MATERIAL_COST),
        INST_COL_DELTA,
        Formula::new(cost_delta(
            &cell(row(INST_ROW_MATERIAL_COST), INST_COL_EXCEL),
            &cell(row(INST_ROW_MATERIAL_COST), INST_COL_API),
        )),
    )?;

    // Total Production Cost = Material + Installation.
    sheet.write_string_with_format(
        row(INST_ROW_TOTAL_PRODUCTION_COST),
        0,
        "Total Production Cost",
        &head,
    )?;
    sheet.write_formula_with_format(
        row(INST_ROW_TOTAL_PRODUCTION_COST),
        INST_COL_EXCEL,
        Formula::new(incomplete_guard2(
            &cell(row(INST_ROW_MATERIAL_COST), INST_COL_EXCEL),
            &cell(row(INST_ROW_OWN_INSTALL_TOTAL), INST_COL_EXCEL),
            "+",
        )),
        &money4,
    )?;
    write_opt_f64(
        sheet,
        row(INST_ROW_TOTAL_PRODUCTION_COST),
        INST_COL_API,
        op_cost.and_then(|o| opt_money_f64(o.total_production_cost)),
        &money4,
    )?;
    sheet.write_formula(
        row(INST_ROW_TOTAL_PRODUCTION_COST),
        INST_COL_DELTA,
        Formula::new(cost_delta(
            &cell(row(INST_ROW_TOTAL_PRODUCTION_COST), INST_COL_EXCEL),
            &cell(row(INST_ROW_TOTAL_PRODUCTION_COST), INST_COL_API),
        )),
    )?;

    // Produced Quantity -- the same value the header's Total Output cell
    // already computes (`=$B$6*$B$9`); referenced, never recomputed.
    sheet.write_string_with_format(row(INST_ROW_PRODUCED_QTY), 0, "Produced Quantity", &head)?;
    sheet.write_formula_with_format(
        row(INST_ROW_PRODUCED_QTY),
        INST_COL_EXCEL,
        Formula::new("=$B$10"),
        &int,
    )?;
    write_opt_f64(
        sheet,
        row(INST_ROW_PRODUCED_QTY),
        INST_COL_API,
        op_cost.map(|o| o.produced_quantity as f64),
        &int,
    )?;
    sheet.write_formula(
        row(INST_ROW_PRODUCED_QTY),
        INST_COL_DELTA,
        Formula::new(cost_delta(
            &cell(row(INST_ROW_PRODUCED_QTY), INST_COL_EXCEL),
            &cell(row(INST_ROW_PRODUCED_QTY), INST_COL_API),
        )),
    )?;

    // Unit Production Cost -- display/evidence only, never fed back into
    // any conservation identity. Unlike every other cell on this sheet,
    // Rust rounds *this specific* figure via `Money::checked_div_quantity`
    // (`Decimal::rescale`), which is half-away-from-zero, not half-to-even
    // -- so plain `money_round` (Excel's native `ROUND`, also
    // half-away-from-zero) is the *correct* match here, not a leftover.
    // Using `money_round_half_even` on this cell would introduce a
    // divergence instead of avoiding one.
    sheet.write_string_with_format(
        row(INST_ROW_UNIT_PRODUCTION_COST),
        0,
        "Unit Production Cost",
        &head,
    )?;
    sheet.write_formula_with_format(
        row(INST_ROW_UNIT_PRODUCTION_COST),
        INST_COL_EXCEL,
        Formula::new(format!(
            "=IF(AND(ISNUMBER({total}),{qty}>0),{rounded},\"\")",
            total = cell(row(INST_ROW_TOTAL_PRODUCTION_COST), INST_COL_EXCEL),
            qty = cell(row(INST_ROW_PRODUCED_QTY), INST_COL_EXCEL),
            rounded = money_round(&format!(
                "{}/{}",
                cell(row(INST_ROW_TOTAL_PRODUCTION_COST), INST_COL_EXCEL),
                cell(row(INST_ROW_PRODUCED_QTY), INST_COL_EXCEL)
            )),
        )),
        &money4,
    )?;
    write_opt_f64(
        sheet,
        row(INST_ROW_UNIT_PRODUCTION_COST),
        INST_COL_API,
        op_cost.and_then(|o| opt_money_f64(o.unit_production_cost)),
        &money4,
    )?;
    sheet.write_formula(
        row(INST_ROW_UNIT_PRODUCTION_COST),
        INST_COL_DELTA,
        Formula::new(cost_delta(
            &cell(row(INST_ROW_UNIT_PRODUCTION_COST), INST_COL_EXCEL),
            &cell(row(INST_ROW_UNIT_PRODUCTION_COST), INST_COL_API),
        )),
    )?;

    // Cost Status: INCOMPLETE if Excel couldn't compute a Total
    // Production Cost or the API has none; MISMATCH if any of the three
    // headline deltas (material / installation / total) is a genuine
    // nonzero number; OK otherwise.
    sheet.write_string_with_format(row(INST_ROW_COST_STATUS), 0, "Cost Status", &head)?;
    let material_delta = cell(row(INST_ROW_MATERIAL_COST), INST_COL_DELTA);
    let install_delta = cell(row(INST_ROW_OWN_INSTALL_TOTAL), INST_COL_DELTA);
    let total_delta = cell(row(INST_ROW_TOTAL_PRODUCTION_COST), INST_COL_DELTA);
    let total_excel = cell(row(INST_ROW_TOTAL_PRODUCTION_COST), INST_COL_EXCEL);
    let total_api = cell(row(INST_ROW_TOTAL_PRODUCTION_COST), INST_COL_API);
    sheet.write_formula(
        row(INST_ROW_COST_STATUS),
        INST_COL_EXCEL,
        Formula::new(format!(
            "=IF(OR(NOT(ISNUMBER({total_excel})),{total_api}=\"\"),\"INCOMPLETE\",\
             IF(OR(AND(ISNUMBER({md}),{md}<>0),AND(ISNUMBER({id}),{id}<>0),\
             AND(ISNUMBER({td}),{td}<>0)),\"MISMATCH\",\"OK\"))",
            md = material_delta,
            id = install_delta,
            td = total_delta,
        )),
    )?;
    let mismatch_fill = Format::new()
        .set_background_color("FFC7CE")
        .set_font_color("9C0006");
    let ok_fill = Format::new()
        .set_background_color("C6EFCE")
        .set_font_color("006100");
    let incomplete_fill = Format::new()
        .set_background_color("FFEB9C")
        .set_font_color("9C6500");
    let status_row = row(INST_ROW_COST_STATUS);
    for (needle, fmt) in [
        ("MISMATCH", &mismatch_fill),
        ("INCOMPLETE", &incomplete_fill),
        ("OK", &ok_fill),
    ] {
        sheet.add_conditional_format(
            status_row,
            INST_COL_EXCEL,
            status_row,
            INST_COL_EXCEL,
            &ConditionalFormatText::new()
                .set_rule(ConditionalFormatTextRule::Contains(needle.to_string()))
                .set_format(fmt),
        )?;
    }

    Ok(())
}

pub(super) fn write_bold_headers_at(
    sheet: &mut Worksheet,
    row: u32,
    headers: &[&str],
) -> Result<(), XlsxError> {
    let head = header_format();
    for (col, label) in headers.iter().enumerate() {
        sheet.write_string_with_format(row, col as u16, *label, &head)?;
    }
    Ok(())
}
