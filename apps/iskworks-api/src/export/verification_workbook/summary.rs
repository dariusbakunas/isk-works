use super::*;

// ===========================================================================
// Summary
// ===========================================================================

pub(super) fn write_summary(
    sheet: &mut Worksheet,
    model: &VerificationExportModel,
    plan: &WorkbookPlan,
) -> Result<(), XlsxError> {
    sheet.set_name("Summary")?;
    sheet.set_column_width(0, 30)?;
    sheet.set_column_width(1, 44)?;
    let head = header_format();
    let int = int_format();

    let operations = &model.materials.verification_operations;
    let ledger_rows = model.materials.verification_inputs.len() as u32;
    let ledger_last = ledger_rows + 1;
    let ops_last = operations.len() as u32 + 1;
    let ledger_col = |col: u16| col_letter(u32::from(col));

    let mut row = 0u32;
    let kv_text =
        |sheet: &mut Worksheet, row: &mut u32, label: &str, value: &str| -> Result<(), XlsxError> {
            sheet.write_string(*row, 0, label)?;
            sheet.write_string(*row, 1, sanitize_cell_text(value))?;
            *row += 1;
            Ok(())
        };
    let kv_int =
        |sheet: &mut Worksheet, row: &mut u32, label: &str, value: i64| -> Result<(), XlsxError> {
            sheet.write_string(*row, 0, label)?;
            sheet.write_number_with_format(*row, 1, value as f64, &int_format())?;
            *row += 1;
            Ok(())
        };
    let kv_formula = |sheet: &mut Worksheet,
                      row: &mut u32,
                      label: &str,
                      formula: String|
     -> Result<(), XlsxError> {
        sheet.write_string(*row, 0, label)?;
        sheet.write_formula(*row, 1, Formula::new(formula))?;
        *row += 1;
        Ok(())
    };

    sheet.write_string_with_format(row, 0, "BUILD", &head)?;
    row += 1;
    // Literal workbook format/schema version marker, so a reopened workbook
    // (a future importer, a support ticket) can be checked against the
    // format it was written with. It changes only on a schema change, not
    // on rounding-precision corrections to the same schema.
    kv_text(sheet, &mut row, "Workbook Version", "v4")?;
    let root_product = operations
        .first()
        .map(|op| op.product_name.as_str())
        .unwrap_or(model.build_name.as_str());
    kv_text(sheet, &mut row, "Root product", root_product)?;
    kv_text(
        sheet,
        &mut row,
        "Root Build ID",
        &model.build_id.to_string(),
    )?;
    kv_int(
        sheet,
        &mut row,
        "Runs (editor overlay)",
        model.overlay_runs as i64,
    )?;
    kv_text(
        sheet,
        &mut row,
        "Root facility",
        operations
            .first()
            .and_then(|op| op.facility_name.as_deref())
            .unwrap_or("(none selected)"),
    )?;
    kv_text(
        sheet,
        &mut row,
        "Generated at (UTC)",
        &model.generated_at.format("%Y-%m-%d %H:%M:%SZ").to_string(),
    )?;
    kv_text(
        sheet,
        &mut row,
        "Adjusted prices observed at (UTC)",
        &model
            .cost
            .root
            .adjusted_price_observed_at
            .map(|at| at.format("%Y-%m-%d %H:%M:%SZ").to_string())
            .unwrap_or_else(|| "(not available)".to_string()),
    )?;

    row += 1;
    sheet.write_string_with_format(row, 0, "WORKBOOK", &head)?;
    row += 1;
    kv_int(sheet, &mut row, "Operations", operations.len() as i64)?;
    kv_int(
        sheet,
        &mut row,
        "Materials",
        model.materials.rows.len() as i64,
    )?;
    kv_int(
        sheet,
        &mut row,
        "Blueprints / formulas",
        plan.blueprint_type_ids.len() as i64,
    )?;

    row += 1;
    sheet.write_string_with_format(row, 0, "VERIFICATION", &head)?;
    row += 1;
    if ledger_rows == 0 {
        kv_int(sheet, &mut row, "Allocation Ledger rows", 0)?;
        return Ok(());
    }
    kv_formula(
        sheet,
        &mut row,
        "Allocation Ledger rows",
        format!("=COUNTA('Allocation Ledger'!$A$2:$A${ledger_last})"),
    )?;
    kv_formula(
        sheet,
        &mut row,
        "Required mismatches (Excel vs API)",
        format!(
            "=COUNTIF('Allocation Ledger'!${c}$2:${c}${ledger_last},\"<>0\")",
            c = ledger_col(L_REQUIRED_DIFF)
        ),
    )?;
    kv_formula(
        sheet,
        &mut row,
        "Recipe recompute mismatches",
        format!(
            "=COUNTIF('Allocation Ledger'!${c}$2:${c}${ledger_last},\"<>0\")",
            c = ledger_col(L_RECOMPUTE_DIFF)
        ),
    )?;
    kv_formula(
        sheet,
        &mut row,
        "Allocation mismatches",
        format!(
            "=COUNTIF('Allocation Ledger'!${c}$2:${c}${ledger_last},\"<>0\")",
            c = ledger_col(L_PLANNED_USE_DIFF)
        ),
    )?;
    kv_formula(
        sheet,
        &mut row,
        "Child-run mismatches",
        format!(
            "=COUNTIF('Allocation Ledger'!${c}$2:${c}${ledger_last},\"<>0\")",
            c = ledger_col(L_CHILD_RUNS_DIFF)
        ),
    )?;
    kv_formula(
        sheet,
        &mut row,
        "Operation mismatches",
        format!(
            "=COUNTIF(Operations!${c}$2:${c}${ops_last},\"MISMATCH\")",
            c = col_letter(u32::from(OPS_CHECK_COL))
        ),
    )?;
    kv_formula(
        sheet,
        &mut row,
        "Overall verification",
        format!(
            "=IF(COUNTIF('Allocation Ledger'!${lc}$2:${lc}${ledger_last},\"MISMATCH\")\
             +COUNTIF(Operations!${oc}$2:${oc}${ops_last},\"MISMATCH\")=0,\"OK\",\"MISMATCH\")",
            lc = ledger_col(L_CHECK),
            oc = col_letter(u32::from(OPS_CHECK_COL)),
        ),
    )?;

    // --- ROOT COST --------------------------------------------------------
    // The canonical Build planning cost is the *root operation's own*
    // Total Production Cost -- never the sum of every operation's total
    // (descendant cost is already folded in through consumed contributions).
    row += 1;
    sheet.write_string_with_format(row, 0, "ROOT COST", &head)?;
    row += 1;
    for (i, label) in ["", "Excel", "API", "Delta"].iter().enumerate() {
        sheet.write_string_with_format(row, i as u16, *label, &head)?;
    }
    row += 1;
    if !operations.is_empty() {
        let root_sheet = &plan.sheet_names[0];
        let root_nbound = plan.op_boundaries.first().map_or(0, |b| b.len());
        let money4 = money4_format();
        for (label, inst_row) in [
            ("Root Material / Component Cost", INST_ROW_MATERIAL_COST),
            ("Root Installation", INST_ROW_OWN_INSTALL_TOTAL),
            ("Root Total Production Cost", INST_ROW_TOTAL_PRODUCTION_COST),
        ] {
            sheet.write_string(row, 0, label)?;
            sheet.write_formula_with_format(
                row,
                1,
                Formula::new(format!(
                    "={}",
                    op_install_cell(root_sheet, root_nbound, inst_row, INST_COL_EXCEL)
                )),
                &money4,
            )?;
            sheet.write_formula_with_format(
                row,
                2,
                Formula::new(format!(
                    "={}",
                    op_install_cell(root_sheet, root_nbound, inst_row, INST_COL_API)
                )),
                &money4,
            )?;
            sheet.write_formula_with_format(
                row,
                3,
                Formula::new(format!(
                    "={}",
                    op_install_cell(root_sheet, root_nbound, inst_row, INST_COL_DELTA)
                )),
                &money4,
            )?;
            row += 1;
        }
    }

    // --- COST VERIFICATION ------------------------------------------------
    row += 1;
    sheet.write_string_with_format(row, 0, "COST VERIFICATION", &head)?;
    row += 1;
    kv_formula(
        sheet,
        &mut row,
        "Operations cost-incomplete",
        format!(
            "=COUNTIF(Operations!${c}$2:${c}${ops_last},\"INCOMPLETE\")",
            c = col_letter(u32::from(OPS_COST_CHECK_COL))
        ),
    )?;
    kv_formula(
        sheet,
        &mut row,
        "Operations cost mismatches",
        format!(
            "=COUNTIF(Operations!${c}$2:${c}${ops_last},\"MISMATCH\")",
            c = col_letter(u32::from(OPS_COST_CHECK_COL))
        ),
    )?;
    kv_formula(
        sheet,
        &mut row,
        "Cost verification",
        format!(
            "=IF(COUNTIF(Operations!${c}$2:${c}${ops_last},\"MISMATCH\")>0,\"MISMATCH\",\
             IF(COUNTIF(Operations!${c}$2:${c}${ops_last},\"INCOMPLETE\")>0,\"INCOMPLETE\",\"OK\"))",
            c = col_letter(u32::from(OPS_COST_CHECK_COL))
        ),
    )?;
    // Combined status: both the quantity reconciliation
    // ("Overall verification" above) and this cost
    // reconciliation must be clean. An incomplete cost (a genuinely missing
    // primitive -- market price, inventory basis, adjusted price, system
    // cost index, facility) is reported as INCOMPLETE, never folded into
    // MISMATCH.
    kv_formula(
        sheet,
        &mut row,
        "Overall verification (Quantity + Cost)",
        format!(
            "=IF(OR(COUNTIF('Allocation Ledger'!${lc}$2:${lc}${ledger_last},\"MISMATCH\")>0,\
             COUNTIF(Operations!${oc}$2:${oc}${ops_last},\"MISMATCH\")>0,\
             COUNTIF(Operations!${cc}$2:${cc}${ops_last},\"MISMATCH\")>0),\"MISMATCH\",\
             IF(COUNTIF(Operations!${cc}$2:${cc}${ops_last},\"INCOMPLETE\")>0,\"INCOMPLETE\",\"OK\"))",
            lc = ledger_col(L_CHECK),
            oc = col_letter(u32::from(OPS_CHECK_COL)),
            cc = col_letter(u32::from(OPS_COST_CHECK_COL)),
        ),
    )?;

    // --- MONEY ROUNDING ---------------------------------------------------
    // A small, static workbook-engineering note -- not a formula table,
    // since it documents a *policy*, not a per-row computed value. See
    // `money_round` / `money_round_half_even` / `money_round_half_even_ratio`'s
    // doc comments for the full per-function reasoning; this is the summary.
    row += 1;
    sheet.write_string_with_format(row, 0, "MONEY ROUNDING", &head)?;
    row += 1;
    for (i, label) in ["Boundary", "Rust Rounding", "Excel Implementation"]
        .iter()
        .enumerate()
    {
        sheet.write_string_with_format(row, i as u16, *label, &head)?;
    }
    row += 1;
    for (operation, rust_rounding, excel_impl) in [
        (
            "Inventory cost, fresh cost, material/component cost, total production cost",
            "round_dp(4) half-to-even, but the value is already an exact multiple of 0.0001 before rounding",
            "ROUND(x,4) -- any rounding rule is a no-op on an already-exact value",
        ),
        (
            "Consumed child cost (the critical boundary)",
            "round_dp(4) half-to-even on a genuine division: child_total * consumed / produced",
            "Exact-integer quotient/remainder half-to-even (never a floating-point midpoint guess) -- see money_round_half_even_ratio",
        ),
        (
            "EIV; unmodified/system index cost; facility tax, SCC, alliance surcharge",
            "round_dp(4) half-to-even on a multiplication by an unconstrained-precision adjusted price or percentage",
            "Scaled floating-point half-to-even with a 1e-9 tie tolerance -- see money_round_half_even",
        ),
        (
            "Retained surplus basis (child_total - consumed_cost)",
            "exact Decimal subtraction, no rounding (both operands already 4dp)",
            "Exact subtraction, no rounding",
        ),
        (
            "Unit production cost / child unit cost (display/evidence only)",
            "checked_div_quantity -- Decimal::rescale, half-AWAY-from-zero (verified against rescale_internal; NOT half-to-even, unlike every figure above)",
            "ROUND(x,4) -- half-away-from-zero, the correct match for this one boundary",
        ),
    ] {
        sheet.write_string(row, 0, operation)?;
        sheet.write_string(row, 1, rust_rounding)?;
        sheet.write_string(row, 2, excel_impl)?;
        row += 1;
    }
    sheet.write_string_with_format(
        row,
        0,
        "Strategy: every canonical Money boundary above is reproduced with Excel-side \
         half-to-even rounding, not plain ROUND -- there is no accepted rounding divergence in \
         this workbook. Two implementations are used: (1) an exact-integer quotient/remainder \
         construction wherever the pre-rounding value is already known to be an exact multiple \
         of 0.0001 (consumed child cost), which cannot misclassify a tie because it never \
         compares a floating-point remainder to 0.5 at all; (2) a scaled-floating-point \
         half-to-even reconstruction, with genuine mathematical ties resolved down to 1e-13 of \
         an ISK, for the boundaries whose input (an adjusted price or a manually configured \
         percentage) has no bounded decimal-digit count to exploit. Plain ROUND is kept only \
         where it is provably correct: on an already-exact value (rounding is a no-op under any \
         rule), or on the two display-only unit-cost figures, where Rust itself rounds \
         half-away-from-zero rather than half-to-even.",
        &wrap_format(),
    )?;
    row += 1;

    // --- NAVIGATION ------------------------------------------------------
    row += 1;
    sheet.write_string_with_format(row, 0, "NAVIGATION", &head)?;
    row += 1;
    for (i, label) in ["Product", "Activity", "Runs", "Check", "Open"]
        .iter()
        .enumerate()
    {
        sheet.write_string_with_format(row, i as u16, *label, &head)?;
    }
    row += 1;
    for (offset, op) in operations.iter().enumerate() {
        let sheet_name = &plan.sheet_names[offset];
        sheet.write_string(row, 0, sanitize_cell_text(&op.product_name))?;
        sheet.write_string(row, 1, activity_label(op.activity))?;
        sheet.write_formula_with_format(
            row,
            2,
            Formula::new(format!("='{}'!$B$6", sheet_name.replace('\'', "''"))),
            &int,
        )?;
        sheet.write_formula(
            row,
            3,
            Formula::new(format!(
                "=Operations!${c}${xr}",
                c = col_letter(u32::from(OPS_CHECK_COL)),
                xr = offset as u32 + 2
            )),
        )?;
        sheet.write_url(
            row,
            4,
            Url::new(format!("internal:'{}'!A1", sheet_name.replace('\'', "''"))).set_text("Open"),
        )?;
        row += 1;
    }

    sheet.set_freeze_panes(1, 0)?;
    Ok(())
}
