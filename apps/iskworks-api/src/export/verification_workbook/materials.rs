use super::*;

// ===========================================================================
// Materials
// ===========================================================================

pub(super) const MATERIALS_HEADERS: [&str; 22] = [
    "Type ID",
    "Item",
    "Group",
    "Category",
    "Packaged Volume (m³)",
    "Strategy",
    "Starting Inventory",
    "Inventory Unit Basis",
    "Inventory Total Basis",
    "API Required",
    "Excel Required",
    "Required Diff",
    "API Planned Use",
    "Excel Planned Use",
    "Planned Use Diff",
    "API Shortage",
    "Excel Shortage",
    "Shortage Diff",
    "Excel Reused Value",
    "Total Required Volume",
    "Provisional",
    "Check",
];

pub(super) fn write_materials(
    sheet: &mut Worksheet,
    model: &VerificationExportModel,
    _plan: &WorkbookPlan,
) -> Result<(), XlsxError> {
    sheet.set_name("Materials")?;
    write_bold_headers(sheet, &MATERIALS_HEADERS)?;
    let rows = &model.materials.rows;
    if rows.is_empty() {
        return Ok(());
    }

    let basis_by_type: BTreeMap<i64, &iskworks_core::build_materials::InventoryBasisEntry> = model
        .materials
        .inventory_basis
        .iter()
        .map(|entry| (entry.type_id, entry))
        .collect();

    let int = int_format();
    let isk = isk_format();
    let vol = Format::new().set_num_format("0.####");
    let wrap = wrap_format();

    for (offset, line) in rows.iter().enumerate() {
        let r = offset as u32 + 1;
        let xr = r + 1;
        let key = format!("$A{xr}");
        sheet.write_number_with_format(r, 0, line.type_id as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            1,
            Formula::new(types_lookup("Types_Name", &key)),
            &wrap,
        )?;
        sheet.write_formula(r, 2, Formula::new(types_lookup("Types_Group", &key)))?;
        sheet.write_formula(r, 3, Formula::new(types_lookup("Types_Category", &key)))?;
        sheet.write_formula(
            r,
            4,
            Formula::new(format!(
                "=IFERROR(INDEX(Types_PackagedVolume,MATCH({key},Types_Id,0)),\"\")"
            )),
        )?;
        sheet.write_string(r, 5, strategy_label(line.strategy))?;
        sheet.write_number_with_format(r, 6, line.available_quantity as f64, &int)?;
        match basis_by_type.get(&line.type_id).and_then(|b| b.unit_basis) {
            Some(unit) => sheet.write_number_with_format(r, 7, decimal_f64(unit), &isk)?,
            None => sheet.write_blank(r, 7, &Format::new())?,
        };
        match basis_by_type.get(&line.type_id) {
            Some(basis) => {
                sheet.write_number_with_format(r, 8, decimal_f64(basis.total_basis), &isk)?
            }
            None => sheet.write_blank(r, 8, &Format::new())?,
        };
        sheet.write_number_with_format(r, 9, line.required_quantity as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            10,
            Formula::new(format!("=SUMIFS(Ledger_ExcelRequired,Ledger_TypeId,{key})")),
            &int,
        )?;
        sheet.write_formula_with_format(r, 11, Formula::new(format!("=$K{xr}-$J{xr}")), &int)?;
        sheet.write_number_with_format(r, 12, line.allocated_quantity as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            13,
            Formula::new(format!(
                "=SUMIFS(Ledger_ExcelPlannedUse,Ledger_TypeId,{key})"
            )),
            &int,
        )?;
        sheet.write_formula_with_format(r, 14, Formula::new(format!("=$N{xr}-$M{xr}")), &int)?;
        sheet.write_number_with_format(r, 15, line.shortage_quantity as f64, &int)?;
        sheet.write_formula_with_format(
            r,
            16,
            Formula::new(format!("=SUMIFS(Ledger_ExcelShortage,Ledger_TypeId,{key})")),
            &int,
        )?;
        sheet.write_formula_with_format(r, 17, Formula::new(format!("=$Q{xr}-$P{xr}")), &int)?;
        // Excel Reused Value = Excel Planned Use * Inventory Unit Basis.
        sheet.write_formula_with_format(r, 18, Formula::new(format!("=$N{xr}*$H{xr}")), &isk)?;
        // Total Required Volume = Excel Required * Packaged Volume.
        sheet.write_formula_with_format(
            r,
            19,
            Formula::new(format!("=IF($E{xr}=\"\",\"\",$K{xr}*$E{xr})")),
            &vol,
        )?;
        sheet.write_boolean(r, 20, line.provisional)?;
        sheet.write_formula(
            r,
            21,
            Formula::new(format!(
                "=IF(AND($L{xr}=0,$O{xr}=0,$R{xr}=0),\"OK\",\"MISMATCH\")"
            )),
        )?;
    }

    add_plain_table(sheet, "Materials", &MATERIALS_HEADERS, rows.len())?;
    apply_check_and_diff_formatting(sheet, 21, &[11, 14, 17], rows.len() as u32)?;
    sheet.set_freeze_panes(1, 2)?;
    set_default_widths(sheet, &MATERIALS_HEADERS);
    Ok(())
}
