//! Renders a [`VerificationExportModel`] into an `.xlsx` byte buffer -- a
//! relational calculation model (the Summary sheet's "Workbook Version"
//! cell reads `v4`).
//!
//! * **Quantity reconciliation** -- required / allocation / shortage /
//!   child runs / produced / surplus, Excel formulas built only from
//!   primitive inputs, compared cell-by-cell against ISKWorks.
//! * **Planning-cost reconciliation** on top of the same primitive-input
//!   -> quantity-formula spine: inventory basis consumed, fresh Buy cost,
//!   child production cost (proportional consumed + exact retained
//!   surplus), operation installation (EIV / system cost index / facility
//!   tax / SCC / alliance surcharges), operation and root totals -- every
//!   Excel cost formula built only from primitives and already-reconciled
//!   quantity cells, compared against
//!   [`iskworks_core::build_cost::BuildCostProjection`] evidence.
//! * **Money rounding** -- every Money boundary that can land on a genuine
//!   half-to-even/half-away-from-zero tie uses an Excel-side half-to-even
//!   reconstruction ([`money_round_half_even`],
//!   [`money_round_half_even_ratio`]) matching `rust_decimal::round_dp`'s
//!   `MidpointNearestEven` exactly. Plain `ROUND` is used only on
//!   boundaries where it is provably the correct rounding rule (see
//!   `money_round`'s doc comment). See `write_operation_sheet`'s Cost
//!   section and `write_summary`'s "MONEY ROUNDING" note.
//!
//! Sheets, in this fixed order:
//!
//! 1. **Summary** -- landing / index: build header, workbook counts, a
//!    verification roll-up (formulas over the ledger + operations), and a
//!    navigation table with an internal hyperlink per operation.
//! 2. **Types** -- one row per referenced `type_id`: name / group / category
//!    / packaged volume. `type_id` is the relational key; every other sheet
//!    derives human-readable values by `INDEX/MATCH` against the `Types_*`
//!    defined names.
//! 3. **Blueprints** -- one row per distinct blueprint / reaction-formula
//!    `type_id` -- static recipe reference only.
//! 4. **Operations** -- one row per walked production node: per-node state,
//!    `Runs (Excel)` chained from the parent operation, `Sheet` hyperlink.
//! 5. **Materials** -- one row per aggregate `type_id`: `Excel *` totals via
//!    `SUMIFS` over the ledger, inventory basis + volume from the single
//!    balance snapshot.
//! 6. **`<operation>` sheets** -- one per walked node: the recipe requirement
//!    table where the calculation *lives*. `Excel Required` is computed from
//!    this sheet's own primitive cells; the inventory-allocation cells are
//!    direct references into this boundary's Allocation Ledger row;
//!    `Excel Child Runs` drives the child sheet.
//! 7. **Allocation Ledger** -- one row per requirement boundary in DFS
//!    order: the Excel-side authority for global inventory *allocation*
//!    (prior use, available, planned use, shortage). Its `Excel Required`
//!    column is a direct reference back to the owning operation sheet's
//!    `Excel Required` -- it does not recompute the recipe. A separate
//!    `Ledger Recomputed Required` column keeps an independent recipe
//!    recompute purely as an audit cross-check.
//! 8. **Engineering - Requirements** -- the raw `NodeMaterialAllocation`
//!    evidence dump.
//!
//! One acyclic Excel calculation graph: OP primitive inputs -> OP Excel
//! Required -> ledger Excel Required -> ledger prior use / available /
//! planned use -> ledger Excel Shortage -> OP Excel Shortage (direct cell
//! ref) -> OP Excel Child Runs -> child operation Runs -> ... , compared
//! cell-by-cell against the `API *` authoritative values.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use iskworks_core::build_cost::{BoundaryCostProjection, OperationCostProjection};
use iskworks_core::build_materials::{
    MaterialActivity, MaterialBoundaryResolution, MaterialRowStrategy, VerificationBoundaryInput,
    VerificationOperationInput,
};
use iskworks_core::industry::Money;
use iskworks_core::FulfillmentScope;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_xlsxwriter::{
    ConditionalFormatCell, ConditionalFormatCellRule, ConditionalFormatFormula,
    ConditionalFormatText, ConditionalFormatTextRule, Format, FormatAlign, Formula, Table,
    TableColumn, Url, Workbook, Worksheet, XlsxError,
};

use super::{sanitize_cell_text, VerificationExportModel};

mod cells;
mod engineering;
mod ledger;
mod materials;
mod operation_cost;
mod operation_sheet;
mod operations;
mod reference_sheets;
mod summary;
use cells::*;
use engineering::*;
use ledger::*;
use materials::*;
use operation_cost::*;
use operation_sheet::*;
use operations::*;
use reference_sheets::*;
use summary::*;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Build the verification workbook for `model` and return the `.xlsx` bytes.
pub(crate) fn build_verification_workbook(
    model: &VerificationExportModel,
) -> Result<Vec<u8>, XlsxError> {
    let plan = WorkbookPlan::new(model);
    let mut workbook = Workbook::new();

    write_summary(workbook.add_worksheet(), model, &plan)?;
    write_types(workbook.add_worksheet(), model, &plan)?;
    write_blueprints(workbook.add_worksheet(), model, &plan)?;
    write_operations(workbook.add_worksheet(), model, &plan)?;
    write_materials(workbook.add_worksheet(), model, &plan)?;
    for op in &model.materials.verification_operations {
        write_operation_sheet(workbook.add_worksheet(), model, &plan, op)?;
    }
    write_allocation_ledger(workbook.add_worksheet(), model, &plan)?;
    write_engineering_requirements(workbook.add_worksheet(), model, &plan)?;

    define_workbook_names(&mut workbook, model, &plan)?;

    workbook.save_to_buffer()
}

// ---------------------------------------------------------------------------
// Precomputed layout the writers and defined names both consume
// ---------------------------------------------------------------------------

/// 0-based worksheet row of an operation sheet's material-table header.
const OP_TABLE_HEADER_ROW: u32 = 18;
/// 1-based Excel row of an operation sheet's first material-table data row.
const OP_FIRST_DATA_XR: u32 = OP_TABLE_HEADER_ROW + 2;

struct WorkbookPlan {
    /// Distinct referenced `type_id`s, ascending -- the `Types` sheet rows.
    type_ids: Vec<i64>,
    /// Per operation (`op_index`), its sanitized+unique sheet name.
    sheet_names: Vec<String>,
    /// Per operation, indices into `model.materials.verification_inputs` for
    /// that operation's boundaries, in ledger / traversal order.
    op_boundaries: Vec<Vec<usize>>,
    /// Per operation, the `=OpRuns_<i>` formula source: `Some(formula)` for a
    /// child operation (points at the parent sheet's Excel-Child-Runs cell),
    /// `None` for the root (literal runs).
    op_runs_source: Vec<Option<String>>,
    /// Per non-root operation, the 1-based Excel row (`xr`) of the boundary
    /// in the *parent's* sheet that spawned it -- `None` for the root. The
    /// cost sheets read "this op's produced / consumed / surplus quantity"
    /// and "this op's retained surplus basis" straight off that
    /// already-quantity-reconciled parent row instead of recomputing them.
    spawn_xr: Vec<Option<u32>>,
    /// Distinct blueprint/formula `type_id`s referenced by an operation.
    blueprint_type_ids: Vec<i64>,
    /// `traversal_index` -> index into `model.cost.boundaries`.
    cost_by_traversal: BTreeMap<u32, usize>,
    /// `op_index` -> index into `model.cost.operations`.
    cost_by_op: BTreeMap<u32, usize>,
    /// Per operation, every consuming boundary row it serves (one canonical
    /// producer, many demand edges): `(sheet name,
    /// 1-based OP-sheet row, traversal_index)`, in the operation's
    /// `incoming` order. One row for a single-consumer child, none for the
    /// root.
    incoming_rows: Vec<Vec<(String, u32, u32)>>,
    /// Consuming boundary `traversal_index` -> the shared operation it
    /// feeds, when that operation serves **two or more** demand edges.
    shared_by_traversal: BTreeMap<u32, SharedIncoming>,
}

/// One consuming boundary of an operation that serves several demand edges:
/// its run count is `ROUNDUP(SUM(every incoming shortage) / output_per_run)`
/// -- sized once from the aggregate, never per row -- and its surplus /
/// retained basis belong to the first (owner) row only.
#[derive(Clone)]
struct SharedIncoming {
    producer_op: u32,
    is_owner: bool,
}

impl WorkbookPlan {
    /// `'<sheet>'!$<col>$<row>` for every consuming boundary row of `op`.
    fn incoming_refs(&self, op_index: u32, col: u16) -> Vec<String> {
        self.incoming_rows
            .get(op_index as usize)
            .map(|rows| {
                rows.iter()
                    .map(|(sheet, xr, _)| {
                        format!(
                            "'{}'!${}${}",
                            sheet.replace('\'', "''"),
                            col_letter(u32::from(col)),
                            xr
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The Allocation Ledger's `col` cell for every consuming boundary of
    /// `op` (ledger data row == traversal_index + 2).
    fn incoming_ledger_refs(&self, op_index: u32, col: u16) -> Vec<String> {
        self.incoming_rows
            .get(op_index as usize)
            .map(|rows| {
                rows.iter()
                    .map(|(_, _, traversal)| {
                        format!("${}${}", col_letter(u32::from(col)), traversal + 2)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl WorkbookPlan {
    fn new(model: &VerificationExportModel) -> Self {
        let materials = &model.materials;

        // --- referenced type set ---------------------------------------
        let mut type_set: BTreeSet<i64> = BTreeSet::new();
        for row in &materials.rows {
            type_set.insert(row.type_id);
        }
        for boundary in &materials.verification_inputs {
            type_set.insert(boundary.type_id);
        }
        for op in &materials.verification_operations {
            type_set.insert(op.product_type_id);
            if op.blueprint_or_formula_type_id > 0 {
                type_set.insert(op.blueprint_or_formula_type_id);
            }
        }
        let type_ids: Vec<i64> = type_set.into_iter().filter(|id| *id > 0).collect();

        // --- boundaries grouped by operation, in ledger order --------
        let mut op_boundaries: Vec<Vec<usize>> =
            vec![Vec::new(); materials.verification_operations.len()];
        for (ix, boundary) in materials.verification_inputs.iter().enumerate() {
            if let Some(bucket) = op_boundaries.get_mut(boundary.op_index as usize) {
                bucket.push(ix);
            }
        }
        for bucket in &mut op_boundaries {
            bucket.sort_by_key(|ix| materials.verification_inputs[*ix].traversal_index);
        }

        // --- deterministic, collision-safe operation sheet names -----
        let mut sheet_names: Vec<String> =
            Vec::with_capacity(materials.verification_operations.len());
        let mut seen: HashSet<String> = [
            "summary",
            "types",
            "blueprints",
            "operations",
            "materials",
            "allocation ledger",
            "engineering - requirements",
        ]
        .iter()
        .map(|name| (*name).to_string())
        .collect();
        for op in &materials.verification_operations {
            let name = operation_sheet_name(&op.product_name, &op.graph_node_id, &mut seen);
            sheet_names.push(name);
        }

        // --- parent -> child Runs source formula per operation -------
        let mut op_runs_source: Vec<Option<String>> =
            vec![None; materials.verification_operations.len()];
        let mut spawn_xr: Vec<Option<u32>> = vec![None; materials.verification_operations.len()];
        let mut incoming_rows: Vec<Vec<(String, u32, u32)>> =
            vec![Vec::new(); materials.verification_operations.len()];
        let mut shared_by_traversal: BTreeMap<u32, SharedIncoming> = BTreeMap::new();
        for (op_index, op) in materials.verification_operations.iter().enumerate() {
            if !op.incoming.is_empty() {
                // Every demand edge this operation serves, located on its
                // consumer's sheet.
                for demand in &op.incoming {
                    let consumer = demand.consumer_op_index as usize;
                    let Some(k) = op_boundaries.get(consumer).and_then(|bucket| {
                        bucket.iter().position(|ix| {
                            materials.verification_inputs[*ix].traversal_index
                                == demand.traversal_index
                        })
                    }) else {
                        continue;
                    };
                    incoming_rows[op_index].push((
                        sheet_names[consumer].clone(),
                        OP_FIRST_DATA_XR + k as u32,
                        demand.traversal_index,
                    ));
                }
                let rows = &incoming_rows[op_index];
                let Some((first_sheet, first_xr, _)) = rows.first() else {
                    continue;
                };
                spawn_xr[op_index] = Some(*first_xr);
                if rows.len() == 1 {
                    // Column W == Excel Child Runs on an operation sheet.
                    op_runs_source[op_index] = Some(format!(
                        "='{}'!$W${}",
                        first_sheet.replace('\'', "''"),
                        first_xr
                    ));
                } else {
                    // One producer, many demand edges: sized ONCE from the
                    // aggregate of every incoming Excel shortage, at this
                    // operation's own Output per Run ($B$9).
                    let shortages = rows
                        .iter()
                        .map(|(sheet, xr, _)| {
                            format!(
                                "'{}'!${}${}",
                                sheet.replace('\'', "''"),
                                col_letter(u32::from(OP_EXCEL_SHORTAGE)),
                                xr
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    let own = sheet_names[op_index].replace('\'', "''");
                    op_runs_source[op_index] = Some(format!(
                        "=IF(SUM({shortages})>0,ROUNDUP(SUM({shortages})/'{own}'!$B$9,0),0)"
                    ));
                    for (position, (_, _, traversal)) in rows.iter().enumerate() {
                        shared_by_traversal.insert(
                            *traversal,
                            SharedIncoming {
                                producer_op: op.op_index,
                                is_owner: position == 0,
                            },
                        );
                    }
                }
                continue;
            }
            let Some(parent_op_index) = op.parent_op_index else {
                continue; // root: literal runs
            };
            // The spawning boundary's traversal_index is shared by every
            // boundary of this node.
            let Some(&first_ix) = op_boundaries.get(op_index).and_then(|b| b.first()) else {
                continue;
            };
            let Some(spawn_ti) = materials.verification_inputs[first_ix].parent_traversal_index
            else {
                continue;
            };
            let Some(parent_bucket) = op_boundaries.get(parent_op_index as usize) else {
                continue;
            };
            let Some(k) = parent_bucket
                .iter()
                .position(|ix| materials.verification_inputs[*ix].traversal_index == spawn_ti)
            else {
                continue;
            };
            let parent_sheet = &sheet_names[parent_op_index as usize];
            let xr = OP_FIRST_DATA_XR + k as u32;
            spawn_xr[op_index] = Some(xr);
            // Column W == Excel Child Runs on an operation sheet (index 22).
            op_runs_source[op_index] =
                Some(format!("='{}'!$W${}", parent_sheet.replace('\'', "''"), xr));
        }

        // --- cost evidence lookups -----------------------------------
        let cost_by_traversal: BTreeMap<u32, usize> = model
            .cost
            .boundaries
            .iter()
            .enumerate()
            .map(|(i, b)| (b.traversal_index, i))
            .collect();
        let cost_by_op: BTreeMap<u32, usize> = model
            .cost
            .operations
            .iter()
            .enumerate()
            .map(|(i, o)| (o.op_index, i))
            .collect();

        // --- distinct blueprint/formula types -----------------------
        let mut bp_set: BTreeSet<i64> = BTreeSet::new();
        for op in &materials.verification_operations {
            if op.blueprint_or_formula_type_id > 0 {
                bp_set.insert(op.blueprint_or_formula_type_id);
            }
        }
        let blueprint_type_ids: Vec<i64> = bp_set.into_iter().collect();

        Self {
            type_ids,
            sheet_names,
            op_boundaries,
            op_runs_source,
            spawn_xr,
            blueprint_type_ids,
            cost_by_traversal,
            cost_by_op,
            incoming_rows,
            shared_by_traversal,
        }
    }
}

/// FNV-1a 32-bit -- a stable, dependency-free hash of a `graph_node_id` for
/// the operation-sheet-name disambiguation suffix.
fn fnv1a(input: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in input.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Strip characters Excel forbids in a sheet name (`[ ] : * ? / \`), the
/// apostrophe, and control chars; collapse internal whitespace; trim.
fn sanitize_sheet_fragment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut pending_space = false;
    for ch in value.chars() {
        if matches!(ch, '[' | ']' | ':' | '*' | '?' | '/' | '\\' | '\'') || ch.is_control() {
            continue;
        }
        if ch.is_whitespace() {
            if !out.is_empty() {
                pending_space = true;
            }
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }
    out
}

/// `product_name` + a stable `(hhhhh)` suffix derived from `graph_node_id`,
/// clamped to Excel's 31-char / valid-character / case-insensitive-unique
/// rules. `seen` accumulates every used name (lowercased), pre-seeded with
/// the fixed sheet names.
fn operation_sheet_name(
    product_name: &str,
    graph_node_id: &str,
    seen: &mut HashSet<String>,
) -> String {
    let hash = fnv1a(graph_node_id) & 0x000F_FFFF; // 20 bits -> 5 hex
    let mut base: String = sanitize_sheet_fragment(product_name)
        .chars()
        .take(21)
        .collect();
    base = base.trim().to_string();
    if base.is_empty() {
        base = "Op".to_string();
    }
    let mut name = format!("{base} ({hash:05x})");
    let mut counter = 2u32;
    while seen.contains(&name.to_lowercase()) {
        let short: String = base.chars().take(16).collect();
        name = format!("{} ({hash:05x}-{counter})", short.trim());
        name = name.chars().take(31).collect();
        counter += 1;
    }
    seen.insert(name.to_lowercase());
    name
}

// ===========================================================================
// Defined names
// ===========================================================================

fn define_workbook_names(
    workbook: &mut Workbook,
    model: &VerificationExportModel,
    plan: &WorkbookPlan,
) -> Result<(), XlsxError> {
    let types_last = (plan.type_ids.len().max(1) as u32) + 1;
    let ledger_last = (model.materials.verification_inputs.len().max(1) as u32) + 1;

    let types_col = |col: u16| col_letter(u32::from(col));
    let ledger_col = |col: u16| col_letter(u32::from(col));

    let type_range = |col: u16| {
        format!(
            "='Types'!${c}$2:${c}${types_last}",
            c = types_col(col),
            types_last = types_last
        )
    };
    let ledger_range = |col: u16| {
        format!(
            "='Allocation Ledger'!${c}$2:${c}${ledger_last}",
            c = ledger_col(col),
            ledger_last = ledger_last
        )
    };

    workbook.define_name("Types_Id", &type_range(0))?;
    workbook.define_name("Types_Name", &type_range(1))?;
    workbook.define_name("Types_Group", &type_range(3))?;
    workbook.define_name("Types_Category", &type_range(5))?;
    workbook.define_name("Types_PackagedVolume", &type_range(6))?;
    workbook.define_name("Types_AdjustedPrice", &type_range(7))?;

    // Only the Materials roll-up (`SUMIFS`) consumes ledger ranges by name.
    // The operation sheets reference their own ledger row by direct cell
    // address (see `write_operation_sheet`), so no `Ledger_StartingInventory`
    // / `Ledger_PriorUse` / `Ledger_Available` names are needed.
    workbook.define_name("Ledger_TypeId", &ledger_range(L_TYPE_ID))?;
    workbook.define_name("Ledger_ExcelRequired", &ledger_range(L_EXCEL_REQUIRED))?;
    workbook.define_name("Ledger_ExcelPlannedUse", &ledger_range(L_EXCEL_PLANNED_USE))?;
    workbook.define_name("Ledger_ExcelShortage", &ledger_range(L_EXCEL_SHORTAGE))?;

    // One name per child operation -> the parent sheet's Excel-Child-Runs
    // cell that drives this operation's Runs.
    for (op_index, source) in plan.op_runs_source.iter().enumerate() {
        if let Some(formula) = source {
            workbook.define_name(format!("OpRuns_{op_index}"), formula)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
