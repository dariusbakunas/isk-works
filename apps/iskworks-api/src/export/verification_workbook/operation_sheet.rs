use super::*;

// ===========================================================================
// One sheet per operation
// ===========================================================================

pub(super) const OP_TYPE_ID: u16 = 0;
pub(super) const OP_MATERIAL: u16 = 1;
pub(super) const OP_RESOLUTION: u16 = 2;
pub(super) const OP_SCOPE: u16 = 3;
pub(super) const OP_BASE_QPR: u16 = 4;
pub(super) const OP_RUNS: u16 = 5;
pub(super) const OP_ME_FACTOR: u16 = 6;
pub(super) const OP_FAC_FACTOR: u16 = 7;
pub(super) const OP_EXCEL_REQUIRED: u16 = 8;
pub(super) const OP_API_REQUIRED: u16 = 9;
pub(super) const OP_REQUIRED_DIFF: u16 = 10;
pub(super) const OP_LEDGER_ROW: u16 = 11;
pub(super) const OP_STARTING_INV: u16 = 12;
pub(super) const OP_PRIOR_ALLOC: u16 = 13;
pub(super) const OP_AVAILABLE: u16 = 14;
pub(super) const OP_EXCEL_PLANNED_USE: u16 = 15;
pub(super) const OP_API_PLANNED_USE: u16 = 16;
pub(super) const OP_PLANNED_USE_DIFF: u16 = 17;
pub(super) const OP_EXCEL_SHORTAGE: u16 = 18;
pub(super) const OP_API_SHORTAGE: u16 = 19;
pub(super) const OP_SHORTAGE_DIFF: u16 = 20;
pub(super) const OP_OUTPUT_PER_RUN: u16 = 21;
pub(super) const OP_EXCEL_CHILD_RUNS: u16 = 22;
pub(super) const OP_API_CHILD_RUNS: u16 = 23;
pub(super) const OP_CHILD_RUNS_DIFF: u16 = 24;
pub(super) const OP_EXCEL_PRODUCED: u16 = 25;
pub(super) const OP_API_PRODUCED: u16 = 26;
pub(super) const OP_PRODUCED_DIFF: u16 = 27;
pub(super) const OP_EXCEL_SURPLUS: u16 = 28;
pub(super) const OP_API_SURPLUS: u16 = 29;
pub(super) const OP_SURPLUS_DIFF: u16 = 30;
pub(super) const OP_CHILD_OPERATION: u16 = 31;
pub(super) const OP_CHECK: u16 = 32;

// --- Cost section: per-boundary requirement-cost evidence + formulas -------
pub(super) const OP_INVENTORY_UNIT_BASIS: u16 = 33;
pub(super) const OP_EXCEL_INVENTORY_COST: u16 = 34;
pub(super) const OP_API_INVENTORY_COST: u16 = 35;
pub(super) const OP_INVENTORY_COST_DIFF: u16 = 36;
pub(super) const OP_FRESH_UNIT_PRICE: u16 = 37;
pub(super) const OP_EXCEL_FRESH_COST: u16 = 38;
pub(super) const OP_API_FRESH_COST: u16 = 39;
pub(super) const OP_FRESH_COST_DIFF: u16 = 40;
pub(super) const OP_FRESH_PRICE_STALE: u16 = 41;
pub(super) const OP_CHILD_TOTAL_PRODUCTION_COST: u16 = 42;
pub(super) const OP_CHILD_PRODUCED_QTY_COST: u16 = 43;
pub(super) const OP_EXCEL_CHILD_UNIT_COST: u16 = 44;
pub(super) const OP_EXCEL_CHILD_CONSUMED_COST: u16 = 45;
pub(super) const OP_API_CHILD_CONSUMED_COST: u16 = 46;
pub(super) const OP_CHILD_CONSUMED_COST_DIFF: u16 = 47;
pub(super) const OP_EXCEL_RETAINED_SURPLUS_BASIS: u16 = 48;
pub(super) const OP_API_RETAINED_SURPLUS_BASIS: u16 = 49;
pub(super) const OP_RETAINED_SURPLUS_BASIS_DIFF: u16 = 50;
pub(super) const OP_EXCEL_REQUIREMENT_COST: u16 = 51;
pub(super) const OP_API_REQUIREMENT_COST: u16 = 52;
pub(super) const OP_REQUIREMENT_COST_DIFF: u16 = 53;
pub(super) const OP_ADJUSTED_PRICE: u16 = 54;
pub(super) const OP_EIV_TERM: u16 = 55;
pub(super) const OP_COST_STATUS: u16 = 56;

pub(super) const OP_TABLE_HEADERS: [&str; 57] = [
    "Type ID",
    "Material",
    "Resolution",
    "Scope",
    "Base Qty / Run",
    "Runs",
    "ME Factor",
    "Facility Factor",
    "Excel Required",
    "ISKWorks Required",
    "Required Diff",
    "Ledger Row",
    "Starting Inventory",
    "Prior Allocation",
    "Available At Boundary",
    "Excel Planned Use",
    "ISKWorks Planned Use",
    "Planned Use Diff",
    "Excel Shortage",
    "ISKWorks Shortage",
    "Shortage Diff",
    "Output per Run",
    "Excel Child Runs",
    "ISKWorks Child Runs",
    "Child Runs Diff",
    "Excel Produced",
    "ISKWorks Produced",
    "Produced Diff",
    "Excel Surplus",
    "ISKWorks Surplus",
    "Surplus Diff",
    "Child Operation",
    "Check",
    "Inventory Unit Basis",
    "Excel Inventory Cost",
    "API Inventory Cost",
    "Inventory Cost Diff",
    "Fresh Unit Price",
    "Excel Fresh Cost",
    "API Fresh Cost",
    "Fresh Cost Diff",
    "Fresh Price Stale",
    "Child Total Production Cost",
    "Child Produced Qty",
    "Excel Child Unit Cost",
    "Excel Child Consumed Cost",
    "API Child Consumed Cost",
    "Child Consumed Cost Diff",
    "Excel Retained Surplus Basis",
    "API Retained Surplus Basis",
    "Retained Surplus Basis Diff",
    "Excel Requirement Cost",
    "API Requirement Cost",
    "Requirement Cost Diff",
    "Adjusted Price",
    "EIV Term",
    "Cost Status",
];

/// Every Excel-vs-API cost difference column on an operation sheet's
/// per-boundary table -- mirrors [`OP_ALL_DIFF_COLS`] for the quantity
/// columns. Used for conditional formatting only (the `Cost Status` column
/// is its own formula, not a pure "all zero" AND like the quantity `Check`,
/// because a boundary can be legitimately cost-incomplete without being a
/// quantity mismatch).
pub(super) const OP_COST_DIFF_COLS: [u16; 5] = [
    OP_INVENTORY_COST_DIFF,
    OP_FRESH_COST_DIFF,
    OP_CHILD_CONSUMED_COST_DIFF,
    OP_RETAINED_SURPLUS_BASIS_DIFF,
    OP_REQUIREMENT_COST_DIFF,
];

// --- Installation & Total section -------------------------------------------
// A key/value(/API/Delta) block written two rows below this sheet's own
// material table (`OP_TABLE_HEADER_ROW + boundaries.len() + 2`), so its
// absolute row depends on this operation's own boundary count. Row offsets
// below are *relative* to that section's first row; a caller elsewhere in
// the workbook (Operations sheet, a parent operation's Child columns) that
// needs to reference one of these cells computes the section's absolute
// start row itself from the target operation's own boundary count (known at
// generation time via `WorkbookPlan::op_boundaries`) and adds the offset.
pub(super) const INST_ROW_SCI: u32 = 1;
pub(super) const INST_ROW_JOB_REDUCTION_PCT: u32 = 2;
pub(super) const INST_ROW_FACILITY_TAX_PCT: u32 = 3;
pub(super) const INST_ROW_SCC_PCT: u32 = 4;
pub(super) const INST_ROW_ALLIANCE_PCT: u32 = 5;
pub(super) const INST_ROW_FIXED_SUPPLEMENTAL: u32 = 6;
pub(super) const INST_ROW_TABLE_HEADER: u32 = 8;
pub(super) const INST_ROW_EIV: u32 = 9;
pub(super) const INST_ROW_UNMOD_SYSTEM_INDEX_COST: u32 = 10;
pub(super) const INST_ROW_SYSTEM_INDEX_COST: u32 = 11;
pub(super) const INST_ROW_FACILITY_TAX: u32 = 12;
pub(super) const INST_ROW_SCC_SURCHARGE: u32 = 13;
pub(super) const INST_ROW_ALLIANCE_SURCHARGE: u32 = 14;
pub(super) const INST_ROW_OWN_INSTALL_TOTAL: u32 = 15;
pub(super) const INST_ROW_MATERIAL_COST: u32 = 16;
pub(super) const INST_ROW_TOTAL_PRODUCTION_COST: u32 = 17;
pub(super) const INST_ROW_PRODUCED_QTY: u32 = 18;
pub(super) const INST_ROW_UNIT_PRODUCTION_COST: u32 = 19;
pub(super) const INST_ROW_COST_STATUS: u32 = 20;
/// Column letters within the Installation section's key/value block.
pub(super) const INST_COL_EXCEL: u16 = 1; // B
pub(super) const INST_COL_API: u16 = 2; // C
pub(super) const INST_COL_DELTA: u16 = 3; // D

/// The Installation section's absolute 0-based start row for an operation
/// with `nbound` boundaries in its own material table.
pub(super) fn installation_section_row(nbound: usize) -> u32 {
    OP_TABLE_HEADER_ROW + nbound as u32 + 2
}

/// Every Excel-vs-API difference column on an operation sheet. The `Check`
/// formula requires all of these to be zero, and each gets non-zero
/// conditional formatting. Keep this list and the `Check` formula in lockstep.
pub(super) const OP_ALL_DIFF_COLS: [u16; 6] = [
    OP_REQUIRED_DIFF,
    OP_PLANNED_USE_DIFF,
    OP_SHORTAGE_DIFF,
    OP_CHILD_RUNS_DIFF,
    OP_PRODUCED_DIFF,
    OP_SURPLUS_DIFF,
];

pub(super) fn write_operation_sheet(
    sheet: &mut Worksheet,
    model: &VerificationExportModel,
    plan: &WorkbookPlan,
    op: &VerificationOperationInput,
) -> Result<(), XlsxError> {
    let op_index = op.op_index as usize;
    sheet.set_name(&plan.sheet_names[op_index])?;
    sheet.set_column_width(0, 30)?;
    sheet.set_column_width(1, 30)?;

    let head = header_format();
    let int = int_format();
    let factor_fmt = factor_format();
    let pct = Format::new().set_num_format("0.####");
    let money4 = money4_format();

    // --- HEADER block (key/value, col A/B) ------------------------------
    let put = |sheet: &mut Worksheet, row: u32, label: &str| -> Result<(), XlsxError> {
        sheet.write_string_with_format(row, 0, label, &head)?;
        Ok(())
    };
    put(sheet, 0, "Product")?;
    sheet.write_string(0, 1, sanitize_cell_text(&op.product_name))?;
    put(sheet, 1, "Product Type ID")?;
    sheet.write_number_with_format(1, 1, op.product_type_id as f64, &int)?;
    put(sheet, 2, "Blueprint/Formula")?;
    sheet.write_string(2, 1, sanitize_cell_text(&op.blueprint_or_formula_name))?;
    put(sheet, 3, "Blueprint/Formula Type ID")?;
    sheet.write_number_with_format(3, 1, op.blueprint_or_formula_type_id as f64, &int)?;
    put(sheet, 4, "Activity")?;
    sheet.write_string(4, 1, activity_label(op.activity))?;
    put(sheet, 5, "Runs")?;
    match &plan.op_runs_source[op_index] {
        Some(_) => sheet.write_formula_with_format(
            5,
            1,
            Formula::new(format!("=OpRuns_{}", op.op_index)),
            &int,
        )?,
        None => sheet.write_number_with_format(5, 1, op.node_runs as f64, &int)?,
    };
    put(sheet, 6, "ME")?;
    match op.me {
        Some(me) => sheet.write_number_with_format(6, 1, me as f64, &int)?,
        None => sheet.write_string(6, 1, "n/a")?,
    };
    put(sheet, 7, "TE")?;
    match op.te {
        Some(te) => sheet.write_number_with_format(7, 1, te as f64, &int)?,
        None => sheet.write_string(7, 1, "n/a")?,
    };
    put(sheet, 8, "Output per Run")?;
    sheet.write_number_with_format(8, 1, op.output_per_run as f64, &int)?;
    put(sheet, 9, "Total Output")?;
    sheet.write_formula_with_format(9, 1, Formula::new("=$B$6*$B$9"), &int)?;
    put(sheet, 10, "Facility")?;
    sheet.write_string(
        10,
        1,
        sanitize_cell_text(op.facility_name.as_deref().unwrap_or("(none selected)")),
    )?;
    put(sheet, 11, "Structure Type")?;
    sheet.write_string(
        11,
        1,
        sanitize_cell_text(op.structure_type.as_deref().unwrap_or("")),
    )?;
    put(sheet, 12, "System")?;
    sheet.write_string(
        12,
        1,
        sanitize_cell_text(op.solar_system.as_deref().unwrap_or("")),
    )?;
    put(sheet, 13, "Structure Material %")?;
    sheet.write_number_with_format(
        13,
        1,
        decimal_f64(op.structure_material_reduction_percent),
        &pct,
    )?;
    put(sheet, 14, "Effective Material Factor")?;
    sheet.write_number_with_format(
        14,
        1,
        decimal_f64(op.effective_material_factor),
        &factor_fmt,
    )?;
    put(sheet, 15, "Structure Time %")?;
    sheet.write_number_with_format(
        15,
        1,
        decimal_f64(op.structure_time_reduction_percent),
        &pct,
    )?;
    sheet.write_url(
        16,
        0,
        Url::new("internal:Summary!A1").set_text("Back to Summary"),
    )?;

    // --- MATERIAL CALCULATION TABLE ----------------------------------
    write_bold_headers_at(sheet, OP_TABLE_HEADER_ROW, &OP_TABLE_HEADERS)?;
    let boundaries = plan
        .op_boundaries
        .get(op_index)
        .cloned()
        .unwrap_or_default();
    let inputs = &model.materials.verification_inputs;

    // child spawn traversal_index -> child sheet name (for the Child
    // Operation link column).
    let mut child_by_spawn: BTreeMap<u32, String> = BTreeMap::new();
    for (other_ix, other) in model.materials.verification_operations.iter().enumerate() {
        if !other.incoming.is_empty() {
            for demand in &other.incoming {
                if demand.consumer_op_index == op.op_index {
                    child_by_spawn
                        .insert(demand.traversal_index, plan.sheet_names[other_ix].clone());
                }
            }
            continue;
        }
        if other.parent_op_index == Some(op.op_index) {
            if let Some(&first) = plan.op_boundaries.get(other_ix).and_then(|b| b.first()) {
                if let Some(spawn_ti) = inputs[first].parent_traversal_index {
                    child_by_spawn.insert(spawn_ti, plan.sheet_names[other_ix].clone());
                }
            }
        }
    }

    for (k, boundary_ix) in boundaries.iter().enumerate() {
        let vi: &VerificationBoundaryInput = &inputs[*boundary_ix];
        let r = OP_TABLE_HEADER_ROW + 1 + k as u32;
        let xr = r + 1;
        let c = |col: u16| format!("${}{}", col_letter(u32::from(col)), xr);

        sheet.write_number_with_format(r, OP_TYPE_ID, vi.type_id as f64, &int)?;
        sheet.write_formula(
            r,
            OP_MATERIAL,
            Formula::new(types_lookup("Types_Name", &c(OP_TYPE_ID))),
        )?;
        sheet.write_string(r, OP_RESOLUTION, resolution_label(vi.resolution))?;
        sheet.write_string(r, OP_SCOPE, scope_label(vi.scope))?;
        sheet.write_number_with_format(r, OP_BASE_QPR, vi.base_quantity_per_run as f64, &int)?;
        sheet.write_formula_with_format(r, OP_RUNS, Formula::new("=$B$6"), &int)?;
        sheet.write_formula_with_format(
            r,
            OP_ME_FACTOR,
            Formula::new("=IF($B$5=\"reaction\",1,1-$B$7/100)"),
            &factor_fmt,
        )?;
        sheet.write_formula_with_format(r, OP_FAC_FACTOR, Formula::new("=$B$15"), &factor_fmt)?;
        sheet.write_formula_with_format(
            r,
            OP_EXCEL_REQUIRED,
            Formula::new(format!(
                "=MAX({runs},ROUNDUP(ROUND({base}*{runs}*{me}*{fac},6),0))",
                runs = c(OP_RUNS),
                base = c(OP_BASE_QPR),
                me = c(OP_ME_FACTOR),
                fac = c(OP_FAC_FACTOR),
            )),
            &int,
        )?;
        sheet.write_number_with_format(r, OP_API_REQUIRED, vi.api_required as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            OP_REQUIRED_DIFF,
            Formula::new(format!("={}-{}", c(OP_EXCEL_REQUIRED), c(OP_API_REQUIRED))),
            &int,
        )?;
        // The 1-based ledger data-row for this boundary (== traversal_index +
        // 1), shown for human cross-reference. The allocation cells below use
        // a direct cell reference into that ledger row rather than a
        // whole-column INDEX: a whole-column lookup would make this OP sheet
        // depend on *every* ledger `Excel Required` cell, and since each of
        // those points back at an OP sheet's `Excel Required`, that would
        // form a circular reference. A single-cell reference keeps the graph
        // acyclic: OP primitives -> OP Excel Required -> ledger Excel Required
        // -> ledger allocation -> ledger Shortage -> (here) OP Excel Shortage
        // -> OP Excel Child Runs -> child OP Runs.
        sheet.write_number_with_format(r, OP_LEDGER_ROW, (vi.traversal_index + 1) as f64, &int)?;
        let ledger_xr = vi.traversal_index + 2;
        let lref = |col: u16| {
            format!(
                "='Allocation Ledger'!${}${}",
                col_letter(u32::from(col)),
                ledger_xr
            )
        };
        sheet.write_formula_with_format(
            r,
            OP_STARTING_INV,
            Formula::new(lref(L_STARTING_INV)),
            &int,
        )?;
        sheet.write_formula_with_format(
            r,
            OP_PRIOR_ALLOC,
            Formula::new(lref(L_PRIOR_USE)),
            &int,
        )?;
        sheet.write_formula_with_format(
            r,
            OP_AVAILABLE,
            Formula::new(lref(L_EXCEL_AVAILABLE)),
            &int,
        )?;
        sheet.write_formula_with_format(
            r,
            OP_EXCEL_PLANNED_USE,
            Formula::new(lref(L_EXCEL_PLANNED_USE)),
            &int,
        )?;
        sheet.write_number_with_format(r, OP_API_PLANNED_USE, vi.api_planned_use as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            OP_PLANNED_USE_DIFF,
            Formula::new(format!(
                "={}-{}",
                c(OP_EXCEL_PLANNED_USE),
                c(OP_API_PLANNED_USE)
            )),
            &int,
        )?;
        sheet.write_formula_with_format(
            r,
            OP_EXCEL_SHORTAGE,
            Formula::new(lref(L_EXCEL_SHORTAGE)),
            &int,
        )?;
        sheet.write_number_with_format(r, OP_API_SHORTAGE, vi.api_shortage as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            OP_SHORTAGE_DIFF,
            Formula::new(format!("={}-{}", c(OP_EXCEL_SHORTAGE), c(OP_API_SHORTAGE))),
            &int,
        )?;
        sheet.write_number_with_format(r, OP_OUTPUT_PER_RUN, vi.output_per_run as f64, &int)?;
        let shared = plan.shared_by_traversal.get(&vi.traversal_index);
        let child_runs_formula = match shared {
            // A demand edge into a producer serving several consumers: the
            // producer's one aggregate run count (never this row's own).
            Some(shared) => format!(
                "=IF({short}>0,OpRuns_{op},0)",
                short = c(OP_EXCEL_SHORTAGE),
                op = shared.producer_op,
            ),
            None => format!(
                "=IF(AND(OR({res}=\"build\",{res}=\"reaction\"),{short}>0,{opr}>0),\
                 ROUNDUP({short}/{opr},0),0)",
                res = c(OP_RESOLUTION),
                short = c(OP_EXCEL_SHORTAGE),
                opr = c(OP_OUTPUT_PER_RUN),
            ),
        };
        sheet.write_formula_with_format(
            r,
            OP_EXCEL_CHILD_RUNS,
            Formula::new(child_runs_formula),
            &int,
        )?;
        sheet.write_number_with_format(r, OP_API_CHILD_RUNS, vi.api_child_runs as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            OP_CHILD_RUNS_DIFF,
            Formula::new(format!(
                "={}-{}",
                c(OP_EXCEL_CHILD_RUNS),
                c(OP_API_CHILD_RUNS)
            )),
            &int,
        )?;
        sheet.write_formula_with_format(
            r,
            OP_EXCEL_PRODUCED,
            Formula::new(format!(
                "={}*{}",
                c(OP_EXCEL_CHILD_RUNS),
                c(OP_OUTPUT_PER_RUN)
            )),
            &int,
        )?;
        sheet.write_number_with_format(r, OP_API_PRODUCED, vi.api_produced as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            OP_PRODUCED_DIFF,
            Formula::new(format!("={}-{}", c(OP_EXCEL_PRODUCED), c(OP_API_PRODUCED))),
            &int,
        )?;
        let surplus_formula = match shared {
            // The operation's one surplus, on its owner edge only.
            Some(shared) if shared.is_owner => format!(
                "=MAX({}-SUM({}),0)",
                c(OP_EXCEL_PRODUCED),
                plan.incoming_refs(shared.producer_op, OP_EXCEL_SHORTAGE)
                    .join(",")
            ),
            Some(_) => "=0".to_string(),
            None => format!("=MAX({}-{},0)", c(OP_EXCEL_PRODUCED), c(OP_EXCEL_SHORTAGE)),
        };
        sheet.write_formula_with_format(
            r,
            OP_EXCEL_SURPLUS,
            Formula::new(surplus_formula),
            &int,
        )?;
        sheet.write_number_with_format(r, OP_API_SURPLUS, vi.api_surplus as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            OP_SURPLUS_DIFF,
            Formula::new(format!("={}-{}", c(OP_EXCEL_SURPLUS), c(OP_API_SURPLUS))),
            &int,
        )?;

        // Child Operation
        match vi.resolution {
            MaterialBoundaryResolution::Build | MaterialBoundaryResolution::Reaction => {
                match child_by_spawn.get(&vi.traversal_index) {
                    Some(child_sheet) => sheet.write_url(
                        r,
                        OP_CHILD_OPERATION,
                        Url::new(format!("internal:'{}'!A1", child_sheet.replace('\'', "''")))
                            .set_text(child_sheet.clone()),
                    )?,
                    None => sheet.write_string(r, OP_CHILD_OPERATION, "Covered - not expanded")?,
                };
            }
            _ => {
                sheet.write_blank(r, OP_CHILD_OPERATION, &Format::new())?;
            }
        };

        // --- Cost section ------------------------------------------------
        // Primitive/API evidence for this boundary, if any (should always be
        // present -- `model.cost.boundaries` covers every verification
        // input 1:1 -- looked up defensively rather than assumed).
        let bc: Option<&BoundaryCostProjection> = plan
            .cost_by_traversal
            .get(&vi.traversal_index)
            .map(|&i| &model.cost.boundaries[i]);
        let res = c(OP_RESOLUTION);

        // Inventory (reused) portion: reused_quantity * inventory_unit_basis,
        // zero when nothing was reused, INCOMPLETE (never zero) when the
        // basis is missing for a nonzero reuse. `unit_basis` is itself
        // an already-4dp `Money` value (the weighted-average historical
        // cost) times an integer quantity, so the product is *already* an
        // exact multiple of `0.0001` -- rounding it is a no-op under any
        // rule, so plain `money_round` is correct here, not a shortcut.
        write_opt_f64(
            sheet,
            r,
            OP_INVENTORY_UNIT_BASIS,
            bc.and_then(|b| opt_decimal_f64(b.inventory_unit_basis)),
            &money4,
        )?;
        sheet.write_formula_with_format(
            r,
            OP_EXCEL_INVENTORY_COST,
            Formula::new(format!(
                "=IF({planned}=0,0,IF({basis}=\"\",\"INCOMPLETE\",{rounded}))",
                planned = c(OP_EXCEL_PLANNED_USE),
                basis = c(OP_INVENTORY_UNIT_BASIS),
                rounded = money_round(&format!(
                    "{}*{}",
                    c(OP_EXCEL_PLANNED_USE),
                    c(OP_INVENTORY_UNIT_BASIS)
                )),
            )),
            &money4,
        )?;
        write_opt_f64(
            sheet,
            r,
            OP_API_INVENTORY_COST,
            bc.and_then(|b| opt_money_f64(b.inventory_cost)),
            &money4,
        )?;
        sheet.write_formula(
            r,
            OP_INVENTORY_COST_DIFF,
            Formula::new(cost_delta(
                &c(OP_EXCEL_INVENTORY_COST),
                &c(OP_API_INVENTORY_COST),
            )),
        )?;

        // Fresh Buy portion: fresh_quantity (== Excel Shortage) *
        // fresh_unit_price, zero when nothing needs buying, INCOMPLETE when
        // a nonzero shortage has no resolved price. `fresh_unit_price`
        // is itself an already-4dp `Money`, so (as with inventory cost
        // above) this product is already exact -- plain `money_round`.
        write_opt_f64(
            sheet,
            r,
            OP_FRESH_UNIT_PRICE,
            bc.and_then(|b| opt_money_f64(b.fresh_unit_price)),
            &money4,
        )?;
        sheet.write_formula_with_format(
            r,
            OP_EXCEL_FRESH_COST,
            Formula::new(format!(
                "=IF({short}=0,0,IF({price}=\"\",\"INCOMPLETE\",{rounded}))",
                short = c(OP_EXCEL_SHORTAGE),
                price = c(OP_FRESH_UNIT_PRICE),
                rounded = money_round(&format!(
                    "{}*{}",
                    c(OP_EXCEL_SHORTAGE),
                    c(OP_FRESH_UNIT_PRICE)
                )),
            )),
            &money4,
        )?;
        write_opt_f64(
            sheet,
            r,
            OP_API_FRESH_COST,
            bc.and_then(|b| opt_money_f64(b.fresh_cost)),
            &money4,
        )?;
        sheet.write_formula(
            r,
            OP_FRESH_COST_DIFF,
            Formula::new(cost_delta(&c(OP_EXCEL_FRESH_COST), &c(OP_API_FRESH_COST))),
        )?;
        sheet.write_boolean(r, OP_FRESH_PRICE_STALE, vi.fresh_price_stale)?;

        // Build/Reaction child production portion: reference the spawned
        // child operation's *own* independently-computed Total Production
        // Cost / Produced Quantity cells (never the API's) -- blank when
        // this boundary has no child (Buy, Unresolved, or FullyCovered).
        match bc.and_then(|b| b.child_op_index) {
            Some(child_op_index) => {
                let child_nbound = plan
                    .op_boundaries
                    .get(child_op_index as usize)
                    .map_or(0, |b| b.len());
                let child_sheet = plan.sheet_names[child_op_index as usize].replace('\'', "''");
                let child_section = installation_section_row(child_nbound);
                let child_ref = |offset: u32| {
                    format!(
                        "'{child_sheet}'!${}${}",
                        col_letter(u32::from(INST_COL_EXCEL)),
                        child_section + offset + 1,
                    )
                };
                sheet.write_formula_with_format(
                    r,
                    OP_CHILD_TOTAL_PRODUCTION_COST,
                    Formula::new(format!("={}", child_ref(INST_ROW_TOTAL_PRODUCTION_COST))),
                    &money4,
                )?;
                sheet.write_formula_with_format(
                    r,
                    OP_CHILD_PRODUCED_QTY_COST,
                    Formula::new(format!("={}", child_ref(INST_ROW_PRODUCED_QTY))),
                    &int,
                )?;
            }
            None => {
                sheet.write_blank(r, OP_CHILD_TOTAL_PRODUCTION_COST, &Format::new())?;
                sheet.write_blank(r, OP_CHILD_PRODUCED_QTY_COST, &Format::new())?;
            }
        }
        // Display/evidence only -- never fed back into the consumed-cost
        // arithmetic. Matches Rust's own `checked_div_quantity`
        // rounding for this figure (half-away-from-zero, via `rescale` --
        // see `money_round`'s doc), so plain `money_round` here too.
        sheet.write_formula_with_format(
            r,
            OP_EXCEL_CHILD_UNIT_COST,
            Formula::new(format!(
                "=IF(AND(ISNUMBER({total}),{produced}>0),{rounded},\"\")",
                total = c(OP_CHILD_TOTAL_PRODUCTION_COST),
                produced = c(OP_CHILD_PRODUCED_QTY_COST),
                rounded = money_round(&format!(
                    "{}/{}",
                    c(OP_CHILD_TOTAL_PRODUCTION_COST),
                    c(OP_CHILD_PRODUCED_QTY_COST)
                )),
            )),
            &money4,
        )?;
        // The canonical, conservation-exact consumed-cost formula:
        // half-to-even ROUND(child_total * consumed_quantity /
        // child_produced_quantity, 4) -- never `child_unit_cost *
        // consumed_quantity`. This is THE critical midpoint boundary:
        // `child_total` is already an exact multiple of `0.0001`, so
        // it's rounded via exact-integer quotient/remainder arithmetic
        // (`money_round_half_even_ratio`), never the floating-point
        // midpoint guess -- see that function's doc.
        sheet.write_formula_with_format(
            r,
            OP_EXCEL_CHILD_CONSUMED_COST,
            Formula::new(format!(
                "=IF(OR({res}=\"buy\",{res}=\"unresolved\",{short}=0),0,\
                 IF(AND(ISNUMBER({total}),{produced}>0),{rounded},\"INCOMPLETE\"))",
                short = c(OP_EXCEL_SHORTAGE),
                total = c(OP_CHILD_TOTAL_PRODUCTION_COST),
                produced = c(OP_CHILD_PRODUCED_QTY_COST),
                rounded = money_round_half_even_ratio(
                    &c(OP_CHILD_TOTAL_PRODUCTION_COST),
                    &c(OP_EXCEL_SHORTAGE),
                    &c(OP_CHILD_PRODUCED_QTY_COST),
                ),
            )),
            &money4,
        )?;
        write_opt_f64(
            sheet,
            r,
            OP_API_CHILD_CONSUMED_COST,
            bc.and_then(|b| opt_money_f64(b.child_consumed_cost)),
            &money4,
        )?;
        sheet.write_formula(
            r,
            OP_CHILD_CONSUMED_COST_DIFF,
            Formula::new(cost_delta(
                &c(OP_EXCEL_CHILD_CONSUMED_COST),
                &c(OP_API_CHILD_CONSUMED_COST),
            )),
        )?;

        // Retained surplus basis: child_total - consumed_cost, an
        // *exact* subtraction of two already-rounded Money cells (never
        // independently re-rounded) -- this is what makes
        // consumed + retained == total hold exactly.
        let retained_formula = match shared {
            // Owner edge of a producer serving several consumers: the exact
            // remainder of its one total after EVERY consumer's consumed
            // share -- conservation holds across all of them.
            Some(shared) if shared.is_owner => {
                let consumed = plan.incoming_refs(shared.producer_op, OP_EXCEL_CHILD_CONSUMED_COST);
                let numeric = consumed
                    .iter()
                    .map(|cell| format!("ISNUMBER({cell})"))
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "=IF(AND(ISNUMBER({total}),{numeric}),{total}-SUM({consumed}),\"\")",
                    total = c(OP_CHILD_TOTAL_PRODUCTION_COST),
                    consumed = consumed.join(","),
                )
            }
            Some(_) => format!(
                "=IF(ISNUMBER({total}),0,\"\")",
                total = c(OP_CHILD_TOTAL_PRODUCTION_COST),
            ),
            None => format!(
                "=IF(AND(ISNUMBER({total}),ISNUMBER({consumed})),{total}-{consumed},\"\")",
                total = c(OP_CHILD_TOTAL_PRODUCTION_COST),
                consumed = c(OP_EXCEL_CHILD_CONSUMED_COST),
            ),
        };
        sheet.write_formula_with_format(
            r,
            OP_EXCEL_RETAINED_SURPLUS_BASIS,
            Formula::new(retained_formula),
            &money4,
        )?;
        write_opt_f64(
            sheet,
            r,
            OP_API_RETAINED_SURPLUS_BASIS,
            bc.and_then(|b| opt_money_f64(b.child_surplus_retained_basis)),
            &money4,
        )?;
        sheet.write_formula(
            r,
            OP_RETAINED_SURPLUS_BASIS_DIFF,
            Formula::new(cost_delta(
                &c(OP_EXCEL_RETAINED_SURPLUS_BASIS),
                &c(OP_API_RETAINED_SURPLUS_BASIS),
            )),
        )?;

        // Requirement Cost: Buy -> inventory + fresh; fully covered
        // (build/reaction, shortage 0) -> inventory alone, retained surplus
        // excluded; build/reaction with a child -> inventory + consumed
        // child cost. Unresolved -> INCOMPLETE, never fabricated.
        sheet.write_formula_with_format(
            r,
            OP_EXCEL_REQUIREMENT_COST,
            Formula::new(format!(
                "=IF({res}=\"unresolved\",\"INCOMPLETE\",\
                 IF({res}=\"buy\",{buy},IF({short}=0,{inv_only},{child})))",
                buy = incomplete_guard2(&c(OP_EXCEL_INVENTORY_COST), &c(OP_EXCEL_FRESH_COST), "+"),
                short = c(OP_EXCEL_SHORTAGE),
                inv_only = incomplete_guard1(&c(OP_EXCEL_INVENTORY_COST)),
                child = incomplete_guard2(
                    &c(OP_EXCEL_INVENTORY_COST),
                    &c(OP_EXCEL_CHILD_CONSUMED_COST),
                    "+"
                ),
            )),
            &money4,
        )?;
        write_opt_f64(
            sheet,
            r,
            OP_API_REQUIREMENT_COST,
            bc.and_then(|b| opt_money_f64(b.requirement_cost)),
            &money4,
        )?;
        sheet.write_formula(
            r,
            OP_REQUIREMENT_COST_DIFF,
            Formula::new(cost_delta(
                &c(OP_EXCEL_REQUIREMENT_COST),
                &c(OP_API_REQUIREMENT_COST),
            )),
        )?;

        // EIV term: this boundary's contribution to the *operation's*
        // EIV -- base recipe quantity * runs * adjusted price, no ME, no
        // facility/rig material reduction (unlike Excel Required above).
        // Rust accumulates every term *unrounded* and rounds the EIV total
        // once (see `operation_installation_cost` in `build_cost.rs`) -- this
        // term is therefore left unrounded too; only the op-level EIV sum
        // (Installation section) applies `money_round`.
        sheet.write_formula(
            r,
            OP_ADJUSTED_PRICE,
            Formula::new(types_lookup_blank("Types_AdjustedPrice", &c(OP_TYPE_ID))),
        )?;
        sheet.write_formula_with_format(
            r,
            OP_EIV_TERM,
            Formula::new(format!(
                "=IF({base}=0,0,IF({price}=\"\",\"INCOMPLETE\",{price}*{base}*{runs}))",
                base = c(OP_BASE_QPR),
                price = c(OP_ADJUSTED_PRICE),
                runs = c(OP_RUNS),
            )),
            &money4,
        )?;

        // Cost Status: distinguishes a genuinely missing
        // primitive (INCOMPLETE) from a computed disagreement (MISMATCH)
        // from a clean match, noting stale-but-usable fresh-price evidence.
        sheet.write_formula(
            r,
            OP_COST_STATUS,
            Formula::new(format!(
                "=IF(OR(NOT(ISNUMBER({excel})),{api}=\"\"),\"INCOMPLETE\",\
                 IF({diff}<>0,\"MISMATCH\",\
                 IF(AND({stale},{res}=\"buy\"),\"OK (STALE EVIDENCE)\",\"OK\")))",
                excel = c(OP_EXCEL_REQUIREMENT_COST),
                api = c(OP_API_REQUIREMENT_COST),
                diff = c(OP_REQUIREMENT_COST_DIFF),
                stale = c(OP_FRESH_PRICE_STALE),
            )),
        )?;

        // Check -- every Excel-vs-API difference (all six of OP_ALL_DIFF_COLS)
        // is zero AND this sheet's self-computed Excel Required agrees with the
        // ledger's independent recompute for the same boundary. The ledger's
        // *allocator* Excel Required is, by construction, a reference straight
        // back to this cell, so it is the ledger's `Recomputed Required` (the
        // audit column) that provides a genuine second opinion here.
        let diff_terms = OP_ALL_DIFF_COLS
            .iter()
            .map(|col| format!("{}=0", c(*col)))
            .collect::<Vec<_>>()
            .join(",");
        sheet.write_formula(
            r,
            OP_CHECK,
            Formula::new(format!(
                "=IF(AND({diff_terms},{er}={recomp}),\"OK\",\"MISMATCH\")",
                er = c(OP_EXCEL_REQUIRED),
                recomp = lref(L_RECOMPUTED_REQUIRED).trim_start_matches('='),
            )),
        )?;
    }

    if !boundaries.is_empty() {
        let last_data_row = OP_TABLE_HEADER_ROW + boundaries.len() as u32;
        // Conditional formatting is relative to the table header row.
        let mismatch_fill = Format::new()
            .set_background_color("FFC7CE")
            .set_font_color("9C0006");
        let ok_fill = Format::new()
            .set_background_color("C6EFCE")
            .set_font_color("006100");
        sheet.add_conditional_format(
            OP_TABLE_HEADER_ROW + 1,
            OP_CHECK,
            last_data_row,
            OP_CHECK,
            &ConditionalFormatText::new()
                .set_rule(ConditionalFormatTextRule::Contains("MISMATCH".to_string()))
                .set_format(&mismatch_fill),
        )?;
        sheet.add_conditional_format(
            OP_TABLE_HEADER_ROW + 1,
            OP_CHECK,
            last_data_row,
            OP_CHECK,
            &ConditionalFormatText::new()
                .set_rule(ConditionalFormatTextRule::Contains("OK".to_string()))
                .set_format(&ok_fill),
        )?;
        let nonzero_font = Format::new().set_font_color("9C0006").set_bold();
        for col in OP_ALL_DIFF_COLS.iter() {
            sheet.add_conditional_format(
                OP_TABLE_HEADER_ROW + 1,
                *col,
                last_data_row,
                *col,
                &ConditionalFormatCell::new()
                    .set_rule(ConditionalFormatCellRule::NotEqualTo(0))
                    .set_format(&nonzero_font),
            )?;
        }
        // Cost diff columns can legitimately hold the blank "" text when a
        // side is incomplete -- a plain `NotEqualTo(0)` cell rule would
        // false-flag those as mismatches. A formula rule keeps INCOMPLETE
        // visually distinct from a genuine (numeric, nonzero) MISMATCH.
        let first_xr = OP_TABLE_HEADER_ROW + 2;
        for col in OP_COST_DIFF_COLS.iter() {
            let cell = format!("${}{}", col_letter(u32::from(*col)), first_xr);
            sheet.add_conditional_format(
                OP_TABLE_HEADER_ROW + 1,
                *col,
                last_data_row,
                *col,
                &ConditionalFormatFormula::new()
                    .set_rule(Formula::new(format!("=AND(ISNUMBER({cell}),{cell}<>0)")))
                    .set_format(&nonzero_font),
            )?;
        }
        let incomplete_fill = Format::new()
            .set_background_color("FFEB9C")
            .set_font_color("9C6500");
        sheet.add_conditional_format(
            OP_TABLE_HEADER_ROW + 1,
            OP_COST_STATUS,
            last_data_row,
            OP_COST_STATUS,
            &ConditionalFormatText::new()
                .set_rule(ConditionalFormatTextRule::Contains("MISMATCH".to_string()))
                .set_format(&mismatch_fill),
        )?;
        sheet.add_conditional_format(
            OP_TABLE_HEADER_ROW + 1,
            OP_COST_STATUS,
            last_data_row,
            OP_COST_STATUS,
            &ConditionalFormatText::new()
                .set_rule(ConditionalFormatTextRule::Contains(
                    "INCOMPLETE".to_string(),
                ))
                .set_format(&incomplete_fill),
        )?;
        sheet.add_conditional_format(
            OP_TABLE_HEADER_ROW + 1,
            OP_COST_STATUS,
            last_data_row,
            OP_COST_STATUS,
            &ConditionalFormatText::new()
                .set_rule(ConditionalFormatTextRule::Contains("OK".to_string()))
                .set_format(&ok_fill),
        )?;
    }

    let op_cost = plan
        .cost_by_op
        .get(&op.op_index)
        .map(|&i| &model.cost.operations[i]);
    write_operation_cost_section(sheet, op, op_cost, boundaries.len())?;

    sheet.set_freeze_panes(OP_TABLE_HEADER_ROW + 1, 2)?;
    Ok(())
}
