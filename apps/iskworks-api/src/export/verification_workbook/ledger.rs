use super::*;

// ===========================================================================
// Allocation Ledger
// ===========================================================================

pub(super) const L_TRAVERSAL: u16 = 0;
pub(super) const L_PARENT: u16 = 1;
pub(super) const L_OP: u16 = 2;
pub(super) const L_BUILD_ID: u16 = 3;
pub(super) const L_GRAPH_NODE_ID: u16 = 4;
pub(super) const L_TREE_PATH: u16 = 5;
pub(super) const L_TYPE_ID: u16 = 6;
pub(super) const L_MATERIAL: u16 = 7;
pub(super) const L_ACTIVITY: u16 = 8;
pub(super) const L_RESOLUTION: u16 = 9;
pub(super) const L_SCOPE: u16 = 10;
pub(super) const L_BASE_QPR: u16 = 11;
pub(super) const L_BLUEPRINT_ME: u16 = 12;
pub(super) const L_FAC_FACTOR: u16 = 13;
pub(super) const L_OUTPUT_PER_RUN: u16 = 14;
pub(super) const L_STARTING_INV: u16 = 15;
pub(super) const L_RUNS: u16 = 16;
pub(super) const L_EXCEL_ME_FACTOR: u16 = 17;
/// The allocator input: a direct reference to the owning
/// operation sheet's `Excel Required` cell -- edits to an OP primitive flow
/// through here into the whole allocation chain. Never recomputed here, never
/// the API value.
pub(super) const L_EXCEL_REQUIRED: u16 = 18;
/// Independent ledger-side recompute of the recipe requirement from this
/// sheet's own primitive columns. Audit comparison only -- NOT the allocator
/// input. A non-zero `Recompute Diff` means the OP sheet and the ledger
/// disagree on the recipe formula.
pub(super) const L_RECOMPUTED_REQUIRED: u16 = 19;
pub(super) const L_API_REQUIRED: u16 = 20;
pub(super) const L_REQUIRED_DIFF: u16 = 21;
pub(super) const L_RECOMPUTE_DIFF: u16 = 22;
pub(super) const L_PRIOR_USE: u16 = 23;
pub(super) const L_EXCEL_AVAILABLE: u16 = 24;
pub(super) const L_EXCEL_PLANNED_USE: u16 = 25;
pub(super) const L_API_PLANNED_USE: u16 = 26;
pub(super) const L_PLANNED_USE_DIFF: u16 = 27;
pub(super) const L_EXCEL_SHORTAGE: u16 = 28;
pub(super) const L_API_SHORTAGE: u16 = 29;
pub(super) const L_SHORTAGE_DIFF: u16 = 30;
pub(super) const L_EXCEL_CHILD_RUNS: u16 = 31;
pub(super) const L_API_CHILD_RUNS: u16 = 32;
pub(super) const L_CHILD_RUNS_DIFF: u16 = 33;
pub(super) const L_EXCEL_PRODUCED: u16 = 34;
pub(super) const L_API_PRODUCED: u16 = 35;
pub(super) const L_PRODUCED_DIFF: u16 = 36;
pub(super) const L_EXCEL_SURPLUS: u16 = 37;
pub(super) const L_API_SURPLUS: u16 = 38;
pub(super) const L_SURPLUS_DIFF: u16 = 39;
pub(super) const L_CHECK: u16 = 40;

pub(super) const LEDGER_HEADERS: [&str; 41] = [
    "Traversal #",
    "Parent #",
    "Op #",
    "Build ID",
    "Graph Node ID",
    "Tree Path",
    "Type ID",
    "Material",
    "Activity",
    "Resolution",
    "Scope",
    "Base Qty / Run",
    "Blueprint ME",
    "Facility Material Factor",
    "Output Per Run",
    "Starting Inventory",
    "Runs",
    "Excel ME Factor",
    "Excel Required",
    "Ledger Recomputed Required",
    "API Required",
    "Required Diff",
    "Recompute Diff",
    "Prior Planned Use",
    "Excel Available",
    "Excel Planned Use",
    "API Planned Use",
    "Planned Use Diff",
    "Excel Shortage",
    "API Shortage",
    "Shortage Diff",
    "Excel Child Runs",
    "API Child Runs",
    "Child Runs Diff",
    "Excel Produced",
    "API Produced",
    "Produced Diff",
    "Excel Surplus",
    "API Surplus",
    "Surplus Diff",
    "Check",
];

pub(super) const LEDGER_DIFF_COLS: [u16; 7] = [
    L_REQUIRED_DIFF,
    L_RECOMPUTE_DIFF,
    L_PLANNED_USE_DIFF,
    L_SHORTAGE_DIFF,
    L_CHILD_RUNS_DIFF,
    L_PRODUCED_DIFF,
    L_SURPLUS_DIFF,
];

pub(super) fn write_allocation_ledger(
    sheet: &mut Worksheet,
    model: &VerificationExportModel,
    plan: &WorkbookPlan,
) -> Result<(), XlsxError> {
    sheet.set_name("Allocation Ledger")?;
    let inputs = &model.materials.verification_inputs;
    write_bold_headers(sheet, &LEDGER_HEADERS)?;

    if inputs.is_empty() {
        return Ok(());
    }

    let int = int_format();
    let wrap = wrap_format();
    let factor_fmt = factor_format();
    let last = inputs.len() as u32 + 1;

    let cell = |col: u16, xr: u32| format!("${}{}", col_letter(u32::from(col)), xr);
    let range = |col: u16, from: u32, to: u32| {
        let c = col_letter(u32::from(col));
        format!("${c}${from}:${c}${to}")
    };

    for (offset, vi) in inputs.iter().enumerate() {
        let r = offset as u32 + 1;
        let xr = r + 1;

        sheet.write_number_with_format(r, L_TRAVERSAL, vi.traversal_index as f64, &int)?;
        match vi.parent_traversal_index {
            Some(parent) => {
                sheet.write_number_with_format(r, L_PARENT, parent as f64, &int)?;
            }
            None => {
                sheet.write_blank(r, L_PARENT, &Format::new())?;
            }
        };
        sheet.write_number_with_format(r, L_OP, vi.op_index as f64, &int)?;
        sheet.write_string(r, L_BUILD_ID, vi.build_id.0.to_string())?;
        sheet.write_string(r, L_GRAPH_NODE_ID, vi.graph_node_id.as_str())?;
        sheet.write_string_with_format(r, L_TREE_PATH, tree_path_label(&vi.tree_path), &wrap)?;
        sheet.write_number_with_format(r, L_TYPE_ID, vi.type_id as f64, &int)?;
        sheet.write_string_with_format(r, L_MATERIAL, sanitize_cell_text(&vi.type_name), &wrap)?;
        sheet.write_string(r, L_ACTIVITY, activity_label(vi.activity))?;
        sheet.write_string(r, L_RESOLUTION, resolution_label(vi.resolution))?;
        sheet.write_string(r, L_SCOPE, scope_label(vi.scope))?;
        sheet.write_number_with_format(r, L_BASE_QPR, vi.base_quantity_per_run as f64, &int)?;
        sheet.write_number_with_format(r, L_BLUEPRINT_ME, vi.blueprint_me as f64, &int)?;
        sheet.write_number_with_format(
            r,
            L_FAC_FACTOR,
            decimal_f64(vi.facility_material_factor),
            &factor_fmt,
        )?;
        sheet.write_number_with_format(r, L_OUTPUT_PER_RUN, vi.output_per_run as f64, &int)?;
        sheet.write_number_with_format(r, L_STARTING_INV, vi.starting_inventory as f64, &int)?;

        match vi.parent_traversal_index {
            Some(_) => {
                sheet.write_formula_with_format(
                    r,
                    L_RUNS,
                    Formula::new(format!(
                        "=INDEX({child_runs},MATCH({parent},{traversal},0))",
                        child_runs = range(L_EXCEL_CHILD_RUNS, 2, last),
                        parent = cell(L_PARENT, xr),
                        traversal = range(L_TRAVERSAL, 2, last),
                    )),
                    &int,
                )?;
            }
            None => {
                sheet.write_number_with_format(r, L_RUNS, vi.node_runs as f64, &int)?;
            }
        };

        sheet.write_formula_with_format(
            r,
            L_EXCEL_ME_FACTOR,
            Formula::new(format!(
                "=IF({activity}=\"reaction\",1,1-{me}/100)",
                activity = cell(L_ACTIVITY, xr),
                me = cell(L_BLUEPRINT_ME, xr),
            )),
            &factor_fmt,
        )?;
        // The independent ledger-side recompute (audit column). Uses this
        // sheet's own primitive cells -- the same math the OP sheet performs.
        let recompute = format!(
            "=MAX({runs},ROUNDUP(ROUND({base}*{runs}*{me}*{fac},6),0))",
            runs = cell(L_RUNS, xr),
            base = cell(L_BASE_QPR, xr),
            me = cell(L_EXCEL_ME_FACTOR, xr),
            fac = cell(L_FAC_FACTOR, xr),
        );
        // The allocator input: a direct reference to the owning operation
        // sheet's `Excel Required` cell for this exact boundary. Falls back to
        // the local recompute only if the OP-sheet row cannot be located
        // (should never happen -- every boundary has a recorded operation).
        let op_required_ref = {
            let op_index = vi.op_index as usize;
            let position = plan
                .op_boundaries
                .get(op_index)
                .and_then(|bucket| bucket.iter().position(|ix| *ix == offset));
            match (plan.sheet_names.get(op_index), position) {
                (Some(sheet_name), Some(k)) => Some(format!(
                    "='{}'!${}${}",
                    sheet_name.replace('\'', "''"),
                    col_letter(u32::from(OP_EXCEL_REQUIRED)),
                    OP_FIRST_DATA_XR + k as u32,
                )),
                _ => None,
            }
        };
        sheet.write_formula_with_format(
            r,
            L_EXCEL_REQUIRED,
            Formula::new(op_required_ref.unwrap_or_else(|| recompute.clone())),
            &int,
        )?;
        sheet.write_formula_with_format(r, L_RECOMPUTED_REQUIRED, Formula::new(recompute), &int)?;
        sheet.write_number_with_format(r, L_API_REQUIRED, vi.api_required as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            L_REQUIRED_DIFF,
            Formula::new(format!(
                "={}-{}",
                cell(L_EXCEL_REQUIRED, xr),
                cell(L_API_REQUIRED, xr)
            )),
            &int,
        )?;
        sheet.write_formula_with_format(
            r,
            L_RECOMPUTE_DIFF,
            Formula::new(format!(
                "={}-{}",
                cell(L_EXCEL_REQUIRED, xr),
                cell(L_RECOMPUTED_REQUIRED, xr)
            )),
            &int,
        )?;

        if offset == 0 {
            sheet.write_number_with_format(r, L_PRIOR_USE, 0.0, &int)?;
        } else {
            sheet.write_formula_with_format(
                r,
                L_PRIOR_USE,
                Formula::new(format!(
                    "=SUMIFS({use_above},{type_above},{ty})",
                    use_above = range(L_EXCEL_PLANNED_USE, 2, xr - 1),
                    type_above = range(L_TYPE_ID, 2, xr - 1),
                    ty = cell(L_TYPE_ID, xr),
                )),
                &int,
            )?;
        }
        sheet.write_formula_with_format(
            r,
            L_EXCEL_AVAILABLE,
            Formula::new(format!(
                "=MAX({inv}-{prior},0)",
                inv = cell(L_STARTING_INV, xr),
                prior = cell(L_PRIOR_USE, xr),
            )),
            &int,
        )?;
        sheet.write_formula_with_format(
            r,
            L_EXCEL_PLANNED_USE,
            Formula::new(format!(
                "=IF({scope}=\"Full\",0,MIN({required},{available}))",
                scope = cell(L_SCOPE, xr),
                required = cell(L_EXCEL_REQUIRED, xr),
                available = cell(L_EXCEL_AVAILABLE, xr),
            )),
            &int,
        )?;
        sheet.write_number_with_format(r, L_API_PLANNED_USE, vi.api_planned_use as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            L_PLANNED_USE_DIFF,
            Formula::new(format!(
                "={}-{}",
                cell(L_EXCEL_PLANNED_USE, xr),
                cell(L_API_PLANNED_USE, xr)
            )),
            &int,
        )?;

        sheet.write_formula_with_format(
            r,
            L_EXCEL_SHORTAGE,
            Formula::new(format!(
                "=MAX({required}-{planned},0)",
                required = cell(L_EXCEL_REQUIRED, xr),
                planned = cell(L_EXCEL_PLANNED_USE, xr),
            )),
            &int,
        )?;
        sheet.write_number_with_format(r, L_API_SHORTAGE, vi.api_shortage as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            L_SHORTAGE_DIFF,
            Formula::new(format!(
                "={}-{}",
                cell(L_EXCEL_SHORTAGE, xr),
                cell(L_API_SHORTAGE, xr)
            )),
            &int,
        )?;

        let shared = plan.shared_by_traversal.get(&vi.traversal_index);
        let aggregate_shortage = shared.map(|shared| {
            plan.incoming_ledger_refs(shared.producer_op, L_EXCEL_SHORTAGE)
                .join(",")
        });
        sheet.write_formula_with_format(
            r,
            L_EXCEL_CHILD_RUNS,
            Formula::new(match &aggregate_shortage {
                // Sized once from every demand edge of the shared producer.
                Some(all) => format!(
                    "=IF(AND({short}>0,{opr}>0),ROUNDUP(SUM({all})/{opr},0),0)",
                    short = cell(L_EXCEL_SHORTAGE, xr),
                    opr = cell(L_OUTPUT_PER_RUN, xr),
                ),
                None => format!(
                    "=IF(AND(OR({res}=\"build\",{res}=\"reaction\"),{short}>0,{opr}>0),\
                     ROUNDUP({short}/{opr},0),0)",
                    res = cell(L_RESOLUTION, xr),
                    short = cell(L_EXCEL_SHORTAGE, xr),
                    opr = cell(L_OUTPUT_PER_RUN, xr),
                ),
            }),
            &int,
        )?;
        sheet.write_number_with_format(r, L_API_CHILD_RUNS, vi.api_child_runs as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            L_CHILD_RUNS_DIFF,
            Formula::new(format!(
                "={}-{}",
                cell(L_EXCEL_CHILD_RUNS, xr),
                cell(L_API_CHILD_RUNS, xr)
            )),
            &int,
        )?;

        sheet.write_formula_with_format(
            r,
            L_EXCEL_PRODUCED,
            Formula::new(format!(
                "={}*{}",
                cell(L_EXCEL_CHILD_RUNS, xr),
                cell(L_OUTPUT_PER_RUN, xr)
            )),
            &int,
        )?;
        sheet.write_number_with_format(r, L_API_PRODUCED, vi.api_produced as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            L_PRODUCED_DIFF,
            Formula::new(format!(
                "={}-{}",
                cell(L_EXCEL_PRODUCED, xr),
                cell(L_API_PRODUCED, xr)
            )),
            &int,
        )?;

        sheet.write_formula_with_format(
            r,
            L_EXCEL_SURPLUS,
            Formula::new(match (shared, &aggregate_shortage) {
                (Some(shared), Some(all)) if shared.is_owner => format!(
                    "=MAX({produced}-SUM({all}),0)",
                    produced = cell(L_EXCEL_PRODUCED, xr),
                ),
                (Some(_), _) => "=0".to_string(),
                (None, _) => format!(
                    "=MAX({produced}-{short},0)",
                    produced = cell(L_EXCEL_PRODUCED, xr),
                    short = cell(L_EXCEL_SHORTAGE, xr),
                ),
            }),
            &int,
        )?;
        sheet.write_number_with_format(r, L_API_SURPLUS, vi.api_surplus as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            L_SURPLUS_DIFF,
            Formula::new(format!(
                "={}-{}",
                cell(L_EXCEL_SURPLUS, xr),
                cell(L_API_SURPLUS, xr)
            )),
            &int,
        )?;

        let diff_terms = LEDGER_DIFF_COLS
            .iter()
            .map(|col| format!("{}=0", cell(*col, xr)))
            .collect::<Vec<_>>()
            .join(",");
        sheet.write_formula(
            r,
            L_CHECK,
            Formula::new(format!("=IF(AND({diff_terms}),\"OK\",\"MISMATCH\")")),
        )?;
    }

    apply_check_and_diff_formatting(sheet, L_CHECK, &LEDGER_DIFF_COLS, last - 1)?;
    sheet.set_freeze_panes(1, 3)?;
    set_default_widths(sheet, &LEDGER_HEADERS);
    Ok(())
}

/// Red-fill `Check == MISMATCH`, green-fill `OK`, bold-red any non-zero
/// `Diff` cell -- shared by the ledger and the operation sheets.
pub(super) fn apply_check_and_diff_formatting(
    sheet: &mut Worksheet,
    check_col: u16,
    diff_cols: &[u16],
    last_data_row: u32,
) -> Result<(), XlsxError> {
    let mismatch_fill = Format::new()
        .set_background_color("FFC7CE")
        .set_font_color("9C0006");
    let ok_fill = Format::new()
        .set_background_color("C6EFCE")
        .set_font_color("006100");
    sheet.add_conditional_format(
        1,
        check_col,
        last_data_row,
        check_col,
        &ConditionalFormatText::new()
            .set_rule(ConditionalFormatTextRule::Contains("MISMATCH".to_string()))
            .set_format(&mismatch_fill),
    )?;
    sheet.add_conditional_format(
        1,
        check_col,
        last_data_row,
        check_col,
        &ConditionalFormatText::new()
            .set_rule(ConditionalFormatTextRule::Contains("OK".to_string()))
            .set_format(&ok_fill),
    )?;
    let nonzero_font = Format::new().set_font_color("9C0006").set_bold();
    for col in diff_cols {
        sheet.add_conditional_format(
            1,
            *col,
            last_data_row,
            *col,
            &ConditionalFormatCell::new()
                .set_rule(ConditionalFormatCellRule::NotEqualTo(0))
                .set_format(&nonzero_font),
        )?;
    }
    Ok(())
}
