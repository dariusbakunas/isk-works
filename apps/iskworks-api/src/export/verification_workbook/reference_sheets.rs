use super::*;

// ===========================================================================
// Types
// ===========================================================================

pub(super) const TYPES_HEADERS: [&str; 8] = [
    "Type ID",
    "Type Name",
    "Group ID",
    "Group Name",
    "Category ID",
    "Category Name",
    "Packaged Volume (m³)",
    "Adjusted Price (EIV)",
];

pub(super) fn write_types(
    sheet: &mut Worksheet,
    model: &VerificationExportModel,
    plan: &WorkbookPlan,
) -> Result<(), XlsxError> {
    sheet.set_name("Types")?;
    write_bold_headers(sheet, &TYPES_HEADERS)?;
    if plan.type_ids.is_empty() {
        return Ok(());
    }

    let int = int_format();
    let vol = Format::new().set_num_format("0.####");
    let money4 = money4_format();
    for (offset, type_id) in plan.type_ids.iter().enumerate() {
        let r = offset as u32 + 1;
        let reference = model.type_reference.get(type_id);
        sheet.write_number_with_format(r, 0, *type_id as f64, &int)?;
        match reference.and_then(|reference| reference.type_name.as_deref()) {
            Some(name) => sheet.write_string(r, 1, sanitize_cell_text(name))?,
            None => sheet.write_string(r, 1, "(unknown)")?,
        };
        write_opt_int(
            sheet,
            r,
            2,
            reference.and_then(|reference| reference.group_id),
            &int,
        )?;
        write_opt_str(
            sheet,
            r,
            3,
            reference.and_then(|reference| reference.group_name.as_deref()),
        )?;
        write_opt_int(
            sheet,
            r,
            4,
            reference.and_then(|reference| reference.category_id),
            &int,
        )?;
        write_opt_str(
            sheet,
            r,
            5,
            reference.and_then(|reference| reference.category_name.as_deref()),
        )?;
        match reference.and_then(|reference| reference.packaged_volume_m3) {
            Some(volume) => sheet.write_number_with_format(r, 6, decimal_f64(volume), &vol)?,
            None => sheet.write_blank(r, 6, &Format::new())?,
        };
        // Adjusted price used for every operation's EIV -- primitive
        // evidence, never re-fetched. Excel's EIV formula (on each operation
        // sheet) reads this column directly via `Types_AdjustedPrice`; blank
        // means "no adjusted price resolved", which the EIV formula must
        // treat as incomplete, never as zero.
        write_opt_f64(
            sheet,
            r,
            7,
            opt_decimal_f64(model.adjusted_prices.get(type_id).copied()),
            &money4,
        )?;
    }

    add_plain_table(sheet, "Types", &TYPES_HEADERS, plan.type_ids.len())?;
    sheet.set_freeze_panes(1, 0)?;
    set_default_widths(sheet, &TYPES_HEADERS);
    Ok(())
}

pub(super) fn write_opt_int(
    sheet: &mut Worksheet,
    row: u32,
    col: u16,
    value: Option<i64>,
    fmt: &Format,
) -> Result<(), XlsxError> {
    match value {
        Some(v) => sheet.write_number_with_format(row, col, v as f64, fmt)?,
        None => sheet.write_blank(row, col, &Format::new())?,
    };
    Ok(())
}
pub(super) fn write_opt_str(
    sheet: &mut Worksheet,
    row: u32,
    col: u16,
    value: Option<&str>,
) -> Result<(), XlsxError> {
    match value {
        Some(v) => sheet.write_string(row, col, sanitize_cell_text(v))?,
        None => sheet.write_blank(row, col, &Format::new())?,
    };
    Ok(())
}

// ===========================================================================
// Blueprints
// ===========================================================================

pub(super) const BLUEPRINTS_HEADERS: [&str; 7] = [
    "Blueprint/Formula Type ID",
    "Blueprint/Formula Name",
    "Activity",
    "Product Type ID",
    "Product",
    "Output per Run",
    "Base Material Count",
];

pub(super) fn write_blueprints(
    sheet: &mut Worksheet,
    model: &VerificationExportModel,
    plan: &WorkbookPlan,
) -> Result<(), XlsxError> {
    sheet.set_name("Blueprints")?;
    write_bold_headers(sheet, &BLUEPRINTS_HEADERS)?;
    if plan.blueprint_type_ids.is_empty() {
        return Ok(());
    }

    // One representative operation per distinct blueprint/formula type_id.
    let mut representative: BTreeMap<i64, &VerificationOperationInput> = BTreeMap::new();
    for op in &model.materials.verification_operations {
        representative
            .entry(op.blueprint_or_formula_type_id)
            .or_insert(op);
    }

    let int = int_format();
    let wrap = wrap_format();
    let mut rows = 0usize;
    for (offset, type_id) in plan.blueprint_type_ids.iter().enumerate() {
        let Some(op) = representative.get(type_id) else {
            continue;
        };
        let r = offset as u32 + 1;
        let xr = r + 1;
        sheet.write_number_with_format(r, 0, *type_id as f64, &int)?;
        sheet.write_string_with_format(
            r,
            1,
            sanitize_cell_text(&op.blueprint_or_formula_name),
            &wrap,
        )?;
        sheet.write_string(r, 2, activity_label(op.activity))?;
        sheet.write_number_with_format(r, 3, op.product_type_id as f64, &int)?;
        sheet.write_formula(
            r,
            4,
            Formula::new(types_lookup("Types_Name", &format!("$D{xr}"))),
        )?;
        sheet.write_number_with_format(r, 5, op.output_per_run as f64, &int)?;
        sheet.write_number_with_format(r, 6, op.base_material_count as f64, &int)?;
        rows += 1;
    }

    if rows > 0 {
        add_plain_table(sheet, "Blueprints", &BLUEPRINTS_HEADERS, rows)?;
    }
    sheet.set_freeze_panes(1, 0)?;
    set_default_widths(sheet, &BLUEPRINTS_HEADERS);
    Ok(())
}
