use super::*;

// ===========================================================================
// Operations
// ===========================================================================

pub(super) const OPS_HEADERS: [&str; 41] = [
    "Op #",
    "Parent Op #",
    "Graph Node ID",
    "Build ID",
    "Tree Path",
    "Product Type ID",
    "Product",
    "Activity",
    "Blueprint/Formula Type ID",
    "Blueprint/Formula",
    "Runs (Excel)",
    "Runs (ISKWorks)",
    "ME",
    "TE",
    "Output per Run",
    "Total Output (Excel)",
    "Facility ID",
    "Facility",
    "Structure Type",
    "System",
    "Structure Material %",
    "Effective Material Factor",
    "Structure Time %",
    "Sheet",
    "Check",
    "Excel Material Cost",
    "API Material Cost",
    "Material Cost Delta",
    "Excel Installation",
    "API Installation",
    "Installation Delta",
    "Excel Total Cost",
    "API Total Cost",
    "Total Cost Delta",
    "Produced Qty",
    "Parent Consumed Qty",
    "Surplus Qty",
    "Excel Retained Surplus Basis",
    "API Retained Surplus Basis",
    "Surplus Basis Delta",
    "Cost Check",
];

/// The 1-based Excel cell reference (with sheet qualifier) of `col` on the
/// boundary row in `op`'s *parent* sheet that spawned it -- `None` for the
/// root (nothing spawned it) or if the spawn row could not be located.
pub(super) fn parent_cell(
    plan: &WorkbookPlan,
    op: &VerificationOperationInput,
    col: u16,
) -> Option<String> {
    // An operation serving several demand
    // edges reads the sum over every consuming row (surplus / retained
    // basis are non-zero on the owner row only, so the sum is exact).
    let refs = plan.incoming_refs(op.op_index, col);
    if refs.len() > 1 {
        return Some(format!("SUM({})", refs.join(",")));
    }
    let parent_op_index = op.parent_op_index?;
    let xr = plan.spawn_xr[op.op_index as usize]?;
    let parent_sheet = &plan.sheet_names[parent_op_index as usize];
    Some(format!(
        "'{}'!${}${}",
        parent_sheet.replace('\'', "''"),
        col_letter(u32::from(col)),
        xr
    ))
}

/// The 1-based Excel cell reference of an operation sheet's own Installation
/// section cell at `row_offset`/`col` (see [`installation_section_row`]).
pub(super) fn op_install_cell(
    sheet_name: &str,
    nbound: usize,
    row_offset: u32,
    col: u16,
) -> String {
    let xr = installation_section_row(nbound) + row_offset + 1;
    format!(
        "'{}'!${}${}",
        sheet_name.replace('\'', "''"),
        col_letter(u32::from(col)),
        xr
    )
}

pub(super) fn write_operations(
    sheet: &mut Worksheet,
    model: &VerificationExportModel,
    plan: &WorkbookPlan,
) -> Result<(), XlsxError> {
    sheet.set_name("Operations")?;
    write_bold_headers(sheet, &OPS_HEADERS)?;
    let operations = &model.materials.verification_operations;
    if operations.is_empty() {
        return Ok(());
    }

    let int = int_format();
    let wrap = wrap_format();
    let factor_fmt = factor_format();
    let pct = Format::new().set_num_format("0.####");
    let money4 = money4_format();

    for (offset, op) in operations.iter().enumerate() {
        let r = offset as u32 + 1;
        let xr = r + 1;
        let sheet_name = &plan.sheet_names[offset];
        let nbound = plan.op_boundaries.get(offset).map_or(0, |b| b.len());

        sheet.write_number_with_format(r, 0, op.op_index as f64, &int)?;
        match op.parent_op_index {
            Some(parent) => sheet.write_number_with_format(r, 1, parent as f64, &int)?,
            None => sheet.write_blank(r, 1, &Format::new())?,
        };
        sheet.write_string(r, 2, op.graph_node_id.as_str())?;
        sheet.write_string(r, 3, op.build_id.0.to_string())?;
        sheet.write_string_with_format(r, 4, tree_path_label(&op.tree_path), &wrap)?;
        sheet.write_number_with_format(r, 5, op.product_type_id as f64, &int)?;
        sheet.write_formula(
            r,
            6,
            Formula::new(types_lookup("Types_Name", &format!("$F{xr}"))),
        )?;
        sheet.write_string(r, 7, activity_label(op.activity))?;
        sheet.write_number_with_format(r, 8, op.blueprint_or_formula_type_id as f64, &int)?;
        sheet.write_string_with_format(
            r,
            9,
            sanitize_cell_text(&op.blueprint_or_formula_name),
            &wrap,
        )?;
        // Runs (Excel): root literal; child -> defined name -> parent sheet.
        match &plan.op_runs_source[offset] {
            Some(_) => sheet.write_formula_with_format(
                r,
                10,
                Formula::new(format!("=OpRuns_{}", op.op_index)),
                &int,
            )?,
            None => sheet.write_number_with_format(r, 10, op.node_runs as f64, &int)?,
        };
        sheet.write_number_with_format(r, 11, op.node_runs as f64, &int)?;
        match op.me {
            Some(me) => sheet.write_number_with_format(r, 12, me as f64, &int)?,
            None => sheet.write_string(r, 12, "n/a")?,
        };
        match op.te {
            Some(te) => sheet.write_number_with_format(r, 13, te as f64, &int)?,
            None => sheet.write_string(r, 13, "n/a")?,
        };
        sheet.write_number_with_format(r, 14, op.output_per_run as f64, &int)?;
        sheet.write_formula_with_format(r, 15, Formula::new(format!("=$K{xr}*$O{xr}")), &int)?;
        write_opt_str(
            sheet,
            r,
            16,
            op.facility_id.map(|id| id.to_string()).as_deref(),
        )?;
        write_opt_str(sheet, r, 17, op.facility_name.as_deref())?;
        write_opt_str(sheet, r, 18, op.structure_type.as_deref())?;
        write_opt_str(sheet, r, 19, op.solar_system.as_deref())?;
        sheet.write_number_with_format(
            r,
            20,
            decimal_f64(op.structure_material_reduction_percent),
            &pct,
        )?;
        sheet.write_number_with_format(
            r,
            21,
            decimal_f64(op.effective_material_factor),
            &factor_fmt,
        )?;
        sheet.write_number_with_format(
            r,
            22,
            decimal_f64(op.structure_time_reduction_percent),
            &pct,
        )?;
        sheet.write_url(
            r,
            23,
            Url::new(format!("internal:'{}'!A1", sheet_name.replace('\'', "''")))
                .set_text(sheet_name.clone()),
        )?;
        let check_col = col_letter(u32::from(OP_CHECK));
        if nbound == 0 {
            sheet.write_string(r, 24, "OK")?;
        } else {
            sheet.write_formula(
                r,
                24,
                Formula::new(format!(
                    "=IF(COUNTIF('{sheet}'!${col}${first}:${col}${last},\"MISMATCH\")=0,\"OK\",\"MISMATCH\")",
                    sheet = sheet_name.replace('\'', "''"),
                    col = check_col,
                    first = OP_FIRST_DATA_XR,
                    last = OP_FIRST_DATA_XR + nbound as u32 - 1,
                )),
            )?;
        }

        // --- Cost columns -- every cell here is a direct reference
        // into this op's own Installation section, or (for
        // produced/consumed/surplus) into the parent's already-reconciled
        // boundary row. Never a re-derivation. ----------------------------
        let material = |col: u16| op_install_cell(sheet_name, nbound, INST_ROW_MATERIAL_COST, col);
        sheet.write_formula_with_format(
            r,
            25,
            Formula::new(format!("={}", material(INST_COL_EXCEL))),
            &money4,
        )?;
        sheet.write_formula_with_format(
            r,
            26,
            Formula::new(format!("={}", material(INST_COL_API))),
            &money4,
        )?;
        sheet.write_formula_with_format(
            r,
            27,
            Formula::new(format!("={}", material(INST_COL_DELTA))),
            &money4,
        )?;

        let install =
            |col: u16| op_install_cell(sheet_name, nbound, INST_ROW_OWN_INSTALL_TOTAL, col);
        sheet.write_formula_with_format(
            r,
            28,
            Formula::new(format!("={}", install(INST_COL_EXCEL))),
            &money4,
        )?;
        sheet.write_formula_with_format(
            r,
            29,
            Formula::new(format!("={}", install(INST_COL_API))),
            &money4,
        )?;
        sheet.write_formula_with_format(
            r,
            30,
            Formula::new(format!("={}", install(INST_COL_DELTA))),
            &money4,
        )?;

        let total =
            |col: u16| op_install_cell(sheet_name, nbound, INST_ROW_TOTAL_PRODUCTION_COST, col);
        sheet.write_formula_with_format(
            r,
            31,
            Formula::new(format!("={}", total(INST_COL_EXCEL))),
            &money4,
        )?;
        sheet.write_formula_with_format(
            r,
            32,
            Formula::new(format!("={}", total(INST_COL_API))),
            &money4,
        )?;
        sheet.write_formula_with_format(
            r,
            33,
            Formula::new(format!("={}", total(INST_COL_DELTA))),
            &money4,
        )?;

        sheet.write_formula_with_format(
            r,
            34,
            Formula::new(format!("='{}'!$B$10", sheet_name.replace('\'', "''"))),
            &int,
        )?;
        match parent_cell(plan, op, OP_EXCEL_SHORTAGE) {
            Some(refstr) => {
                sheet.write_formula_with_format(r, 35, Formula::new(format!("={refstr}")), &int)?
            }
            None => sheet.write_blank(r, 35, &Format::new())?,
        };
        match parent_cell(plan, op, OP_EXCEL_SURPLUS) {
            Some(refstr) => {
                sheet.write_formula_with_format(r, 36, Formula::new(format!("={refstr}")), &int)?
            }
            None => sheet.write_blank(r, 36, &Format::new())?,
        };
        match parent_cell(plan, op, OP_EXCEL_RETAINED_SURPLUS_BASIS) {
            Some(refstr) => sheet.write_formula_with_format(
                r,
                37,
                Formula::new(format!("={refstr}")),
                &money4,
            )?,
            None => sheet.write_blank(r, 37, &Format::new())?,
        };
        match parent_cell(plan, op, OP_API_RETAINED_SURPLUS_BASIS) {
            Some(refstr) => sheet.write_formula_with_format(
                r,
                38,
                Formula::new(format!("={refstr}")),
                &money4,
            )?,
            None => sheet.write_blank(r, 38, &Format::new())?,
        };
        match parent_cell(plan, op, OP_RETAINED_SURPLUS_BASIS_DIFF) {
            Some(refstr) => sheet.write_formula_with_format(
                r,
                39,
                Formula::new(format!("={refstr}")),
                &money4,
            )?,
            None => sheet.write_blank(r, 39, &Format::new())?,
        };
        sheet.write_formula(
            r,
            40,
            Formula::new(format!(
                "={}",
                op_install_cell(sheet_name, nbound, INST_ROW_COST_STATUS, INST_COL_EXCEL)
            )),
        )?;
    }

    add_plain_table(sheet, "Operations", &OPS_HEADERS, operations.len())?;
    apply_check_and_diff_formatting(sheet, OPS_CHECK_COL, &[], operations.len() as u32)?;
    let mismatch_fill = Format::new()
        .set_background_color("FFC7CE")
        .set_font_color("9C0006");
    let ok_fill = Format::new()
        .set_background_color("C6EFCE")
        .set_font_color("006100");
    let incomplete_fill = Format::new()
        .set_background_color("FFEB9C")
        .set_font_color("9C6500");
    for (needle, fmt) in [
        ("MISMATCH", &mismatch_fill),
        ("INCOMPLETE", &incomplete_fill),
        ("OK", &ok_fill),
    ] {
        sheet.add_conditional_format(
            1,
            OPS_COST_CHECK_COL,
            operations.len() as u32,
            OPS_COST_CHECK_COL,
            &ConditionalFormatText::new()
                .set_rule(ConditionalFormatTextRule::Contains(needle.to_string()))
                .set_format(fmt),
        )?;
    }
    sheet.set_freeze_panes(1, 1)?;
    set_default_widths(sheet, &OPS_HEADERS);
    Ok(())
}

pub(super) const OPS_CHECK_COL: u16 = 24;
pub(super) const OPS_COST_CHECK_COL: u16 = 40;
