use super::*;

// ===========================================================================
// Engineering - Requirements  (raw NodeMaterialAllocation evidence)
// ===========================================================================

pub(super) const REQUIREMENTS_HEADERS: [&str; 27] = [
    "Op #",
    "Build ID",
    "Graph Node ID",
    "Tree Path",
    "Parent Path",
    "Depth",
    "Type ID",
    "Type Name",
    "Resolution",
    "Scope",
    "Required Quantity",
    "Planned Inventory Use",
    "Remaining / Shortage",
    "Child Runs",
    "Output Per Run",
    "Produced Quantity",
    "Surplus Quantity",
    "API Inventory Cost",
    "API Fresh Cost",
    "API Child Consumed Cost",
    "API Requirement Cost",
    "Child Operation",
    "API Child Produced",
    "API Child Consumed",
    "API Child Surplus",
    "API Retained Basis",
    "Cost Evidence",
];

/// Raw `NodeMaterialAllocation` evidence, plus the matching
/// `BuildCostProjection` boundary's cost evidence -- both **literal, never a
/// formula**: this sheet's whole purpose (and an existing hard invariant,
/// see `export_engineering_requirements_is_raw_evidence`) is to be a raw
/// dump untouched by any Excel calculation. The Excel-vs-API cost
/// *comparison* (formulas + Delta) lives on the owning operation's own OP
/// sheet row instead -- this sheet only needs to make the API numbers
/// inspectable row by row.
pub(super) fn write_engineering_requirements(
    sheet: &mut Worksheet,
    model: &VerificationExportModel,
    plan: &WorkbookPlan,
) -> Result<(), XlsxError> {
    sheet.set_name("Engineering - Requirements")?;
    write_bold_headers(sheet, &REQUIREMENTS_HEADERS)?;
    let allocations = &model.materials.node_allocations;
    if allocations.is_empty() {
        return Ok(());
    }

    // Join op_index by graph_node_id from the operation list.
    let op_by_node: BTreeMap<&str, u32> = model
        .materials
        .verification_operations
        .iter()
        .map(|op| (op.graph_node_id.as_str(), op.op_index))
        .collect();
    // Join `(graph_node_id, type_id)` -> `traversal_index`: `NodeMaterialAllocation`
    // (this sheet's raw evidence) carries no `traversal_index`, but every
    // boundary is uniquely identified by its owning node + type (the same
    // invariant `NodeMaterialAllocation`'s own doc comment states), so this
    // recovers the join to `model.cost.boundaries`.
    let traversal_by_node_type: BTreeMap<(&str, i64), u32> = model
        .materials
        .verification_inputs
        .iter()
        .map(|vi| ((vi.graph_node_id.as_str(), vi.type_id), vi.traversal_index))
        .collect();

    let int = int_format();
    let wrap = wrap_format();
    let money4 = money4_format();
    for (offset, alloc) in allocations.iter().enumerate() {
        let r = offset as u32 + 1;
        match op_by_node.get(alloc.graph_node_id.as_str()) {
            Some(op_index) => sheet.write_number_with_format(r, 0, *op_index as f64, &int)?,
            None => sheet.write_blank(r, 0, &Format::new())?,
        };
        sheet.write_string(r, 1, alloc.build_id.0.to_string())?;
        sheet.write_string(r, 2, alloc.graph_node_id.as_str())?;
        sheet.write_string_with_format(r, 3, tree_path_label(&alloc.tree_path), &wrap)?;
        sheet.write_string_with_format(r, 4, parent_path_label(&alloc.tree_path), &wrap)?;
        sheet.write_number_with_format(r, 5, alloc.tree_path.len() as f64, &int)?;
        sheet.write_number_with_format(r, 6, alloc.type_id as f64, &int)?;
        sheet.write_string_with_format(r, 7, sanitize_cell_text(&alloc.type_name), &wrap)?;
        sheet.write_string(r, 8, resolution_label(alloc.resolution))?;
        sheet.write_string(r, 9, scope_label(alloc.scope))?;
        sheet.write_number_with_format(r, 10, alloc.required_quantity as f64, &int)?;
        sheet.write_number_with_format(r, 11, alloc.allocated_quantity as f64, &int)?;
        sheet.write_number_with_format(r, 12, alloc.shortage_quantity as f64, &int)?;
        sheet.write_number_with_format(r, 13, alloc.child_runs as f64, &int)?;
        sheet.write_number_with_format(r, 14, alloc.output_per_run as f64, &int)?;
        sheet.write_number_with_format(r, 15, alloc.produced_quantity as f64, &int)?;
        sheet.write_number_with_format(r, 16, alloc.surplus_quantity as f64, &int)?;

        // --- Cost evidence, all literal ------------------------------------
        let bc: Option<&BoundaryCostProjection> = traversal_by_node_type
            .get(&(alloc.graph_node_id.as_str(), alloc.type_id))
            .and_then(|ti| plan.cost_by_traversal.get(ti))
            .map(|&i| &model.cost.boundaries[i]);

        write_opt_f64(
            sheet,
            r,
            17,
            bc.and_then(|b| opt_money_f64(b.inventory_cost)),
            &money4,
        )?;
        write_opt_f64(
            sheet,
            r,
            18,
            bc.and_then(|b| opt_money_f64(b.fresh_cost)),
            &money4,
        )?;
        write_opt_f64(
            sheet,
            r,
            19,
            bc.and_then(|b| opt_money_f64(b.child_consumed_cost)),
            &money4,
        )?;
        write_opt_f64(
            sheet,
            r,
            20,
            bc.and_then(|b| opt_money_f64(b.requirement_cost)),
            &money4,
        )?;
        match bc.and_then(|b| b.child_op_index) {
            Some(child_op_index) => {
                sheet.write_number_with_format(r, 21, child_op_index as f64, &int)?
            }
            None => sheet.write_blank(r, 21, &Format::new())?,
        };
        write_opt_f64(
            sheet,
            r,
            22,
            bc.map(|b| b.child_produced_quantity as f64),
            &int,
        )?;
        write_opt_f64(
            sheet,
            r,
            23,
            bc.map(|b| b.child_consumed_quantity as f64),
            &int,
        )?;
        write_opt_f64(
            sheet,
            r,
            24,
            bc.map(|b| b.child_surplus_quantity as f64),
            &int,
        )?;
        write_opt_f64(
            sheet,
            r,
            25,
            bc.and_then(|b| opt_money_f64(b.child_surplus_retained_basis)),
            &money4,
        )?;
        sheet.write_string(
            r,
            26,
            match bc {
                Some(b) if b.complete => "complete",
                Some(_) => "incomplete",
                None => "n/a",
            },
        )?;
    }

    add_plain_table(
        sheet,
        "EngineeringRequirements",
        &REQUIREMENTS_HEADERS,
        allocations.len(),
    )?;
    sheet.set_freeze_panes(1, 0)?;
    set_default_widths(sheet, &REQUIREMENTS_HEADERS);
    Ok(())
}
