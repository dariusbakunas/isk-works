use super::*;
use std::collections::HashSet;

#[test]
fn operation_sheet_name_is_valid_stable_and_unique() {
    let invalid = ['[', ']', ':', '*', '?', '/', '\\'];
    let mut seen: HashSet<String> = HashSet::new();

    // Deterministic: same (name, node id) -> same sheet name.
    let a = operation_sheet_name("Ishtar", "build:abc", &mut seen.clone());
    let b = operation_sheet_name("Ishtar", "build:abc", &mut seen.clone());
    assert_eq!(a, b);
    assert!(a.chars().count() <= 31);
    assert!(a.ends_with(')') && a.contains('('));
    assert!(!a.contains(invalid));

    // Adversarial names: invalid chars stripped, control chars gone,
    // whitespace collapsed, never blank, never > 31.
    for raw in [
        "=cmd|' /c calc'!A1",
        "Ba[d]:name*?/\\slash",
        "\t\ttabs\r\nand\nnewlines",
        "   ",
        "",
        "Nanomekaniki Mikroepexergastis with a very very long trailing name",
        "Fernite Carbide Composite Armor Plate Extended Variant Mark IV",
    ] {
        let name = operation_sheet_name(raw, "build:xyz", &mut seen);
        assert!(!name.is_empty(), "{raw:?} -> non-blank");
        assert!(name.chars().count() <= 31, "{raw:?} -> {name:?} <= 31");
        assert!(
            !name.contains(invalid),
            "{raw:?} -> {name:?} no invalid char"
        );
        assert!(!name.starts_with('\'') && !name.ends_with('\''));
        assert!(!name.chars().any(|c| c.is_control()));
    }

    // Case-insensitive collisions get disambiguated.
    let mut seen2: HashSet<String> = HashSet::new();
    let n1 = operation_sheet_name("Thrasher", "build:1", &mut seen2);
    // Force a collision by seeding the lowercase of a would-be name.
    seen2.insert(operation_sheet_name(
        "Thrasher",
        "build:2",
        &mut seen2.clone(),
    ));
    let n3 = operation_sheet_name("Thrasher", "build:2", &mut seen2);
    assert_ne!(n1, n3);

    // Fixed sheet names are reserved.
    let mut reserved: HashSet<String> = ["summary", "types", "operations"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let clash = operation_sheet_name("Summary", "build:q", &mut reserved);
    assert_ne!(clash.to_lowercase(), "summary");
}

#[test]
fn col_letter_matches_excel() {
    assert_eq!(col_letter(0), "A");
    assert_eq!(col_letter(25), "Z");
    assert_eq!(col_letter(26), "AA");
    assert_eq!(col_letter(38), "AM");
}

// =======================================================================
// Cost reconciliation: hand-built
// `BuildCostProjection` fixtures generated straight through
// `build_verification_workbook`, no HTTP/coordinator/repository layer --
// this is the same seam `crates/iskworks-core/src/build_cost/tests.rs`
// uses for the Rust-side arithmetic; here we additionally inspect the
// *rendered workbook*. calamine cannot evaluate a formula (rust_xlsxwriter
// writes no cached result), so -- exactly like the quantity tests
// above -- every assertion is either a literal (primitive / API evidence)
// cell value, read via calamine, or a formula's *text*, read via the raw
// sheet XML.
// =======================================================================
mod cost {
    use super::*;
    use calamine::{Data, Reader, Xlsx};
    use iskworks_app::BuildMaterialsSummary;
    use iskworks_core::build_cost::{project_build_cost, BoundaryCostKind, BuildCostProjection};
    use iskworks_core::build_materials::InventoryBasisEntry;
    use iskworks_core::{
        Build, BuildId, BuildRecipe, CapturedRecipe, OwnerId, PricingSelectionKind, RecipeCurrency,
        WorkspaceId,
    };
    use std::collections::HashMap;
    use std::io::{Cursor, Read};

    // ---- fixture builders (mirrors build_cost::tests) -------------------

    fn money(value: &str) -> Money {
        Money::parse(value).unwrap()
    }
    fn dec(value: &str) -> Decimal {
        value.parse().unwrap()
    }

    fn test_build(name: &str) -> Build {
        let now = chrono::Utc::now();
        let recipe = BuildRecipe::Manufacturing(
            CapturedRecipe::capture(
                uuid::Uuid::new_v4(),
                "test".to_string(),
                iskworks_sde::ManufacturingRecipe {
                    blueprint_type_id: 1,
                    blueprint_name: "Test Blueprint".to_string(),
                    duration_seconds: Some(1),
                    materials: vec![iskworks_sde::RecipeLine {
                        type_id: 34,
                        type_name: "Tritanium".to_string(),
                        quantity: 1,
                    }],
                    products: vec![iskworks_sde::RecipeLine {
                        type_id: 90_000,
                        type_name: "Product".to_string(),
                        quantity: 1,
                    }],
                },
            )
            .unwrap(),
        );
        Build {
            id: BuildId::new(),
            workspace_id: WorkspaceId::new(),
            owner_id: OwnerId::new(),
            name: name.to_string(),
            recipe,
            runs: 1,
            notes: String::new(),
            revision: 1,
            created_at: now,
            updated_at: now,
            draft_planning: None,
            recipe_currency: RecipeCurrency::Current,
            active_sde_version: None,
            product_category_name: None,
            product_group_name: None,
            selected_blueprint_origin: None,
            has_owned_blueprint: false,
        }
    }

    fn op(
        op_index: u32,
        parent: Option<u32>,
        node_runs: u64,
        output_per_run: u64,
    ) -> VerificationOperationInput {
        VerificationOperationInput {
            op_index,
            parent_op_index: parent,
            parent_traversal_index: None,
            incoming: Vec::new(),
            graph_node_id: if parent.is_none() {
                "root:00000000-0000-0000-0000-000000000000".to_string()
            } else {
                format!("build:00000000-0000-0000-0000-{op_index:012}")
            },
            build_id: BuildId::default(),
            revision: 1,
            tree_path: Vec::new(),
            activity: MaterialActivity::Manufacturing,
            product_type_id: 90_000 + i64::from(op_index),
            product_name: format!("Product {op_index}"),
            output_per_run,
            base_material_count: 1,
            blueprint_or_formula_type_id: 10_000 + i64::from(op_index),
            blueprint_or_formula_name: format!("Blueprint {op_index}"),
            node_runs,
            persisted_runs: node_runs,
            recipe_currency: RecipeCurrency::Current,
            me: Some(0),
            te: Some(0),
            blueprint_selection: None,
            facility_id: None,
            facility_name: None,
            structure_type: None,
            solar_system: None,
            structure_material_reduction_percent: Decimal::ZERO,
            structure_time_reduction_percent: Decimal::ZERO,
            effective_material_factor: Decimal::ONE,
            facility_profile_revision: None,
            system_cost_index: None,
            job_cost_reduction_percent: Decimal::ZERO,
            facility_tax_percent: Decimal::ZERO,
            scc_surcharge_percent: Decimal::ZERO,
            alliance_surcharge_percent: Decimal::ZERO,
            fixed_supplemental_cost: Money::zero(),
            job_count: 1,
            installation_formula_version: String::new(),
        }
    }

    fn with_facility(
        mut operation: VerificationOperationInput,
        system_cost_index: Option<&str>,
        job_cost_reduction_percent: &str,
        facility_tax_percent: &str,
        scc_surcharge_percent: &str,
        alliance_surcharge_percent: &str,
        fixed_supplemental_cost: &str,
    ) -> VerificationOperationInput {
        operation.facility_id = Some(uuid::Uuid::from_u128(u128::from(operation.op_index) + 1));
        operation.facility_name = Some("Test Facility".to_string());
        operation.facility_profile_revision = Some(1);
        operation.system_cost_index = system_cost_index.map(dec);
        operation.job_cost_reduction_percent = dec(job_cost_reduction_percent);
        operation.facility_tax_percent = dec(facility_tax_percent);
        operation.scc_surcharge_percent = dec(scc_surcharge_percent);
        operation.alliance_surcharge_percent = dec(alliance_surcharge_percent);
        operation.fixed_supplemental_cost = money(fixed_supplemental_cost);
        operation.installation_formula_version = "eve-manufacturing-facility-v1".to_string();
        operation
    }

    #[allow(clippy::too_many_arguments)]
    fn boundary(
        traversal_index: u32,
        op_index: u32,
        parent_traversal_index: Option<u32>,
        type_id: i64,
        resolution: MaterialBoundaryResolution,
        scope: FulfillmentScope,
        base_quantity_per_run: u64,
        api_required: u64,
        api_planned_use: u64,
        api_shortage: u64,
    ) -> VerificationBoundaryInput {
        VerificationBoundaryInput {
            traversal_index,
            parent_traversal_index,
            op_index,
            build_id: BuildId::default(),
            graph_node_id: "root:00000000-0000-0000-0000-000000000000".to_string(),
            tree_path: Vec::new(),
            type_id,
            type_name: format!("Type {type_id}"),
            activity: MaterialActivity::Manufacturing,
            resolution,
            scope,
            node_runs: 1,
            base_quantity_per_run,
            blueprint_me: 0,
            facility_material_factor: Decimal::ONE,
            output_per_run: 0,
            starting_inventory: api_planned_use,
            api_required,
            api_planned_use,
            api_shortage,
            api_child_runs: 0,
            api_produced: 0,
            api_surplus: 0,
            fresh_unit_price: None,
            fresh_price_selection: PricingSelectionKind::Default,
            fresh_pricing_policy: None,
            fresh_price_note: String::new(),
            fresh_price_stale: false,
            market_region_id: Some(10_000_002),
            market_location_id: Some(60_003_760),
            intended_recipe: None,
            dependency_id: String::new(),
            producer_build_id: None,
        }
    }

    fn with_price(
        mut boundary: VerificationBoundaryInput,
        unit_price: &str,
    ) -> VerificationBoundaryInput {
        boundary.fresh_unit_price = Some(money(unit_price));
        boundary
    }

    /// Marks a Build/Reaction boundary as the one that spawned a real
    /// child operation: `output_per_run` is the child's own recipe
    /// yield (needed both for the Excel Child Runs formula and for
    /// `project_build_cost`'s child-quantity bookkeeping); the API
    /// evidence fields are set to the values consistent with that
    /// child's `node_runs`/`output_per_run`, matching what a correct
    /// independent Excel recompute of `ROUNDUP(shortage/output_per_run,0)`
    /// would produce.
    fn spawn(
        mut boundary: VerificationBoundaryInput,
        output_per_run: u64,
        api_child_runs: u64,
        api_produced: u64,
        api_surplus: u64,
    ) -> VerificationBoundaryInput {
        boundary.output_per_run = output_per_run;
        boundary.api_child_runs = api_child_runs;
        boundary.api_produced = api_produced;
        boundary.api_surplus = api_surplus;
        boundary
    }

    fn basis(type_id: i64, unit_basis: Option<&str>) -> InventoryBasisEntry {
        InventoryBasisEntry {
            type_id,
            quantity: 0,
            unit_basis: unit_basis.map(dec),
            total_basis: Decimal::ZERO,
        }
    }

    fn model_from(
        build: &Build,
        operations: Vec<VerificationOperationInput>,
        boundaries: Vec<VerificationBoundaryInput>,
        inventory_basis: Vec<InventoryBasisEntry>,
        adjusted_prices: &[(i64, &str)],
    ) -> VerificationExportModel {
        let adjusted: BTreeMap<i64, Decimal> = adjusted_prices
            .iter()
            .map(|(id, v)| (*id, dec(v)))
            .collect();
        let cost = project_build_cost(&operations, &boundaries, &inventory_basis, &adjusted, None);
        let overlay_runs = operations
            .iter()
            .find(|o| o.parent_op_index.is_none())
            .map_or(0, |o| o.node_runs);
        let materials = BuildMaterialsSummary {
            build_id: build.id,
            generated_at: chrono::Utc::now(),
            rows: Vec::new(),
            node_allocations: Vec::new(),
            sources: Vec::new(),
            warnings: Vec::new(),
            verification_inputs: boundaries,
            verification_operations: operations,
            inventory_basis,
            market_evidence: iskworks_core::GraphMarketEvidence::new(Vec::new()),
            metrics: iskworks_core::industry::PlannerMetrics::default(),
        };
        VerificationExportModel::assemble(
            build,
            overlay_runs,
            materials,
            cost,
            adjusted,
            BTreeMap::new(),
            chrono::Utc::now(),
        )
    }

    fn generate(model: &VerificationExportModel) -> Vec<u8> {
        build_verification_workbook(model).expect("workbook generation succeeds")
    }

    fn op_cost(cost: &BuildCostProjection, op_index: u32) -> &OperationCostProjection {
        cost.operations
            .iter()
            .find(|o| o.op_index == op_index)
            .unwrap_or_else(|| panic!("no operation {op_index}"))
    }
    fn boundary_cost(cost: &BuildCostProjection, traversal_index: u32) -> &BoundaryCostProjection {
        cost.boundaries
            .iter()
            .find(|b| b.traversal_index == traversal_index)
            .unwrap_or_else(|| panic!("no boundary {traversal_index}"))
    }

    // ---- xlsx inspection helpers (mirrors apps/iskworks-api/tests/industry_api.rs) --

    fn open_wb(bytes: &[u8]) -> Xlsx<Cursor<Vec<u8>>> {
        calamine::open_workbook_from_rs(Cursor::new(bytes.to_vec()))
            .expect("generated bytes parse as a valid .xlsx workbook")
    }
    fn cell_text(data: &Data) -> String {
        match data {
            Data::String(s) => s.clone(),
            Data::Float(f) => f.to_string(),
            Data::Int(i) => i.to_string(),
            Data::Bool(b) => b.to_string(),
            _ => String::new(),
        }
    }
    fn cell_f64(data: &Data) -> Option<f64> {
        match data {
            Data::Float(f) => Some(*f),
            Data::Int(i) => Some(*i as f64),
            Data::String(s) => s.parse().ok(),
            Data::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }
    /// header -> value for the boundary row of `type_id` on an operation
    /// sheet (material table header at 0-based row 18).
    fn op_table_row(
        wb: &mut Xlsx<Cursor<Vec<u8>>>,
        sheet: &str,
        type_id: &str,
    ) -> HashMap<String, Data> {
        let range = wb.worksheet_range(sheet).unwrap();
        let rows: Vec<&[Data]> = range.rows().collect();
        let headers: Vec<String> = rows[18].iter().map(cell_text).collect();
        for row in &rows[19..] {
            if row.first().map(cell_text).as_deref() == Some(type_id) {
                return headers.iter().cloned().zip(row.iter().cloned()).collect();
            }
        }
        panic!("no material row on {sheet} for type {type_id}");
    }
    fn headers_of(wb: &mut Xlsx<Cursor<Vec<u8>>>, sheet: &str) -> Vec<String> {
        wb.worksheet_range(sheet)
            .unwrap()
            .rows()
            .next()
            .unwrap()
            .iter()
            .map(cell_text)
            .collect()
    }
    fn attr(chunk: &str, key: &str) -> Option<String> {
        chunk
            .split(&format!("{key}=\""))
            .nth(1)
            .and_then(|s| s.split('"').next())
            .map(str::to_string)
    }
    fn unescape_xml(s: &str) -> String {
        s.replace("&gt;", ">")
            .replace("&lt;", "<")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&")
    }
    fn sheet_xml_map(bytes: &[u8]) -> HashMap<String, String> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
        let read = |archive: &mut zip::ZipArchive<Cursor<Vec<u8>>>, name: &str| -> String {
            let mut f = archive.by_name(name).unwrap();
            let mut s = String::new();
            f.read_to_string(&mut s).unwrap();
            s
        };
        let workbook_xml = read(&mut archive, "xl/workbook.xml");
        let rels_xml = read(&mut archive, "xl/_rels/workbook.xml.rels");
        let mut rid_target: HashMap<String, String> = HashMap::new();
        for chunk in rels_xml.split("<Relationship ").skip(1) {
            let id = attr(chunk, "Id");
            let target = attr(chunk, "Target");
            if let (Some(id), Some(target)) = (id, target) {
                rid_target.insert(id, target.trim_start_matches('/').to_string());
            }
        }
        let mut out = HashMap::new();
        for chunk in workbook_xml.split("<sheet ").skip(1) {
            let name = attr(chunk, "name");
            let rid = chunk
                .split("r:id=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .map(str::to_string);
            if let (Some(name), Some(rid)) = (name, rid) {
                if let Some(target) = rid_target.get(&rid) {
                    let path = if target.starts_with("xl/") {
                        target.clone()
                    } else {
                        format!("xl/{target}")
                    };
                    let xml = read(&mut archive, &path);
                    out.insert(name, unescape_xml(&xml));
                }
            }
        }
        out
    }
    fn formula_cell(bytes: &[u8], sheet: &str, cell_ref: &str) -> String {
        let xml = sheet_xml_map(bytes).remove(sheet).unwrap_or_default();
        let needle = format!("<c r=\"{cell_ref}\"");
        let Some(rest) = xml.split(&needle).nth(1) else {
            return String::new();
        };
        let Some(cell) = rest.split("</c>").next() else {
            return String::new();
        };
        cell.split("<f>")
            .nth(1)
            .and_then(|s| s.split("</f>").next())
            .unwrap_or("")
            .to_string()
    }
    fn op_sheet_names(bytes: &[u8]) -> Vec<String> {
        let wb = open_wb(bytes);
        let names: Vec<String> = wb.sheet_names().iter().map(|s| s.to_string()).collect();
        let start = names.iter().position(|n| n == "Materials").unwrap() + 1;
        let end = names.iter().position(|n| n == "Allocation Ledger").unwrap();
        names[start..end].to_vec()
    }

    // ---- tests -----------------------------------------------------------

    // An all-Buy boundary with partial inventory reuse + fresh
    // purchase. Exact parity, and the Excel formula chain never touches an
    // API cell.
    #[test]
    fn all_buy_parity_and_formula_independence() {
        let build = test_build("All-Buy");
        let operations = vec![with_facility(
            op(0, None, 1, 1),
            Some("0"),
            "0",
            "0",
            "0",
            "0",
            "0",
        )];
        let boundaries = vec![with_price(
            boundary(
                0,
                0,
                None,
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                100,
                100,
                60,
                40,
            ),
            "5",
        )];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(34, Some("3"))],
            &[(34, "0")],
        );
        let bytes = generate(&model);
        let sheet = op_sheet_names(&bytes)[0].clone();
        let mut wb = open_wb(&bytes);
        let row = op_table_row(&mut wb, &sheet, "34");

        assert_eq!(cell_f64(&row["Inventory Unit Basis"]), Some(3.0));
        assert_eq!(cell_f64(&row["API Inventory Cost"]), Some(180.0)); // 60*3
        assert_eq!(cell_f64(&row["Fresh Unit Price"]), Some(5.0));
        assert_eq!(cell_f64(&row["API Fresh Cost"]), Some(200.0)); // 40*5
        assert_eq!(cell_f64(&row["API Requirement Cost"]), Some(380.0)); // 180+200

        // Excel Requirement Cost (row 20 -- the first/only boundary) is a
        // pure function of this sheet's own Excel Inventory/Fresh Cost
        // cells, never the API Requirement Cost cell.
        let req = formula_cell(&bytes, &sheet, "AZ20");
        assert!(
            req.contains("$AI20"),
            "references Excel Inventory Cost: {req}"
        );
        assert!(req.contains("$AM20"), "references Excel Fresh Cost: {req}");
        assert!(
            !req.contains("$BA20"),
            "never references API Requirement Cost: {req}"
        );

        let inv = formula_cell(&bytes, &sheet, "AI20");
        assert!(
            inv.contains("$P20"),
            "inventory cost uses Excel Planned Use: {inv}"
        );
        assert!(
            inv.contains("$AH20"),
            "inventory cost uses its own unit-basis cell: {inv}"
        );
        assert!(
            !inv.contains("$AJ20"),
            "never references API Inventory Cost: {inv}"
        );

        let fresh = formula_cell(&bytes, &sheet, "AM20");
        assert!(
            fresh.contains("$S20"),
            "fresh cost uses Excel Shortage: {fresh}"
        );
        assert!(
            fresh.contains("$AL20"),
            "fresh cost uses its own price cell: {fresh}"
        );
        assert!(
            !fresh.contains("$AN20"),
            "never references API Fresh Cost: {fresh}"
        );
    }

    // Mutating the API Requirement Cost cell's literal value in the
    // generated bytes leaves the Excel Requirement Cost *formula* (the
    // thing that must stay independent) byte-for-byte unchanged.
    #[test]
    fn tamper_api_cell_leaves_excel_formula_unchanged() {
        let build = test_build("Tamper");
        let operations = vec![with_facility(
            op(0, None, 1, 1),
            Some("0"),
            "0",
            "0",
            "0",
            "0",
            "0",
        )];
        let boundaries = vec![with_price(
            boundary(
                0,
                0,
                None,
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                100,
                100,
                60,
                40,
            ),
            "5",
        )];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(34, Some("3"))],
            &[(34, "0")],
        );
        let bytes = generate(&model);
        let sheet = op_sheet_names(&bytes)[0].clone();
        let before = formula_cell(&bytes, &sheet, "AZ20");
        assert!(!before.is_empty());

        // Locate the API Requirement Cost cell (BA20, a literal 380) in
        // the raw sheet XML and tamper with its value in place, whatever
        // style attributes it happens to carry.
        let xml_map = sheet_xml_map(&bytes);
        let sheet_xml = xml_map.get(&sheet).unwrap();
        let needle = "<c r=\"BA20\"";
        let cell_start = sheet_xml.find(needle).expect("BA20 cell exists");
        let cell_end = sheet_xml[cell_start..]
            .find("</c>")
            .expect("BA20 cell closes")
            + cell_start
            + "</c>".len();
        let original_cell = &sheet_xml[cell_start..cell_end];
        assert!(
            original_cell.contains("<v>380</v>"),
            "API Requirement Cost is the literal 380 we expect: {original_cell}"
        );
        let tampered_cell = original_cell.replace("<v>380</v>", "<v>999999</v>");
        let tampered_xml = sheet_xml.replacen(original_cell, &tampered_cell, 1);
        assert_ne!(
            &tampered_xml, sheet_xml,
            "the replacement actually matched something"
        );

        // Re-read the (conceptually) tampered sheet's Excel formula text --
        // it is defined purely by cell references, so it is identical
        // regardless of what BA20 holds.
        let after = tampered_xml
            .split("<c r=\"AZ20\"")
            .nth(1)
            .and_then(|s| s.split("</c>").next())
            .and_then(|c| c.split("<f>").nth(1))
            .and_then(|s| s.split("</f>").next())
            .unwrap_or_default();
        assert_eq!(
            before, after,
            "Excel formula text is unaffected by the API value"
        );
    }

    // The canonical surplus fixture -- child total 1_000_000, produced
    // 1_000, parent consumes 500 -> consumed 500_000, retained 500_000,
    // root excludes the retained half.
    #[test]
    fn build_child_surplus_conserves_exactly_and_root_excludes_retained() {
        let build = test_build("Surplus Child");
        let operations = vec![
            with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
            with_facility(
                op(1, Some(0), 1, 1_000),
                Some("0"),
                "0",
                "0",
                "0",
                "0",
                "200000",
            ),
        ];
        let boundaries = vec![
            boundary(
                0,
                0,
                None,
                90_001,
                MaterialBoundaryResolution::Build,
                FulfillmentScope::Missing,
                500,
                500,
                0,
                500,
            ),
            with_price(
                boundary(
                    1,
                    1,
                    Some(0),
                    34,
                    MaterialBoundaryResolution::Buy,
                    FulfillmentScope::Missing,
                    800_000,
                    800_000,
                    0,
                    800_000,
                ),
                "1",
            ),
        ];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(34, None), basis(90_001, None)],
            &[(34, "0"), (90_001, "0")],
        );
        let bytes = generate(&model);
        let names = op_sheet_names(&bytes);
        let (root_sheet, child_sheet) = (names[0].clone(), names[1].clone());
        let mut wb = open_wb(&bytes);

        let child_material = op_table_row(&mut wb, &child_sheet, "34");
        let _ = &child_material; // sanity: child sheet has the Tritanium row

        let parent_row = op_table_row(&mut wb, &root_sheet, "90001");
        assert_eq!(
            cell_f64(&parent_row["API Child Consumed Cost"]),
            Some(500_000.0)
        );
        assert_eq!(
            cell_f64(&parent_row["API Retained Surplus Basis"]),
            Some(500_000.0)
        );
        assert_eq!(
            cell_f64(&parent_row["API Requirement Cost"]),
            Some(500_000.0)
        );

        // The Excel consumed-cost formula is the canonical proportional
        // form -- child total * consumed / produced -- never
        // `child_unit_cost * consumed`.
        let consumed_formula = formula_cell(&bytes, &root_sheet, "AT20");
        assert!(
            consumed_formula.contains('*') && consumed_formula.contains('/'),
            "proportional formula: {consumed_formula}"
        );
        assert!(
            !consumed_formula.contains("$AS20"),
            "never uses the display-only Excel Child Unit Cost cell: {consumed_formula}"
        );

        // Retained surplus basis is an exact subtraction of the (already
        // rounded) child total and consumed-cost cells.
        let retained_formula = formula_cell(&bytes, &root_sheet, "AW20");
        assert!(retained_formula.contains("$AQ20") && retained_formula.contains("$AT20"));
        assert!(
            !retained_formula.contains("ROUND"),
            "exact subtraction, never re-rounded"
        );
    }

    // Partial intermediate inventory (required 1_426, reused 143 @ 4,
    // shortage 1_283 consumed proportionally from a 20_000 child total) --
    // requirement cost is inventory + consumed, never `required *
    // child_unit_cost`.
    #[test]
    fn partial_intermediate_inventory_uses_inventory_plus_consumed_child_cost() {
        let build = test_build("Partial Inventory");
        let operations = vec![
            with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
            with_facility(
                op(1, Some(0), 1, 10_000),
                Some("0"),
                "0",
                "0",
                "0",
                "0",
                "0",
            ),
        ];
        let boundaries = vec![
            boundary(
                0,
                0,
                None,
                90_001,
                MaterialBoundaryResolution::Build,
                FulfillmentScope::Missing,
                1_426,
                1_426,
                143,
                1_283,
            ),
            with_price(
                boundary(
                    1,
                    1,
                    Some(0),
                    34,
                    MaterialBoundaryResolution::Buy,
                    FulfillmentScope::Missing,
                    10_000,
                    10_000,
                    0,
                    10_000,
                ),
                "2",
            ),
        ];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(34, None), basis(90_001, Some("4"))],
            &[(34, "0"), (90_001, "0")],
        );
        let bytes = generate(&model);
        let root_sheet = op_sheet_names(&bytes)[0].clone();
        let mut wb = open_wb(&bytes);
        let row = op_table_row(&mut wb, &root_sheet, "90001");

        assert_eq!(cell_f64(&row["API Inventory Cost"]), Some(572.0)); // 143*4
        assert_eq!(cell_f64(&row["API Child Consumed Cost"]), Some(2_566.0)); // 20000*1283/10000
        assert_eq!(cell_f64(&row["API Requirement Cost"]), Some(3_138.0)); // 572+2566, NOT 1426*2

        let req_formula = formula_cell(&bytes, &root_sheet, "AZ20");
        assert!(req_formula.contains("$AI20") && req_formula.contains("$AT20"));
    }

    // A fully-covered Build intermediate (child pruned, shortage 0) --
    // requirement cost is inventory basis alone; no child evidence is
    // fabricated.
    #[test]
    fn fully_covered_build_has_no_fabricated_child_cost() {
        let build = test_build("Fully Covered");
        let operations = vec![with_facility(
            op(0, None, 1, 1),
            Some("0"),
            "0",
            "0",
            "0",
            "0",
            "0",
        )];
        let boundaries = vec![boundary(
            0,
            0,
            None,
            90_001,
            MaterialBoundaryResolution::Build,
            FulfillmentScope::Missing,
            500,
            500,
            500,
            0,
        )];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(90_001, Some("7"))],
            &[(90_001, "0")],
        );
        let bytes = generate(&model);
        let root_sheet = op_sheet_names(&bytes)[0].clone();
        assert_eq!(op_sheet_names(&bytes).len(), 1, "no child sheet is created");
        let mut wb = open_wb(&bytes);
        let row = op_table_row(&mut wb, &root_sheet, "90001");

        assert_eq!(cell_f64(&row["API Inventory Cost"]), Some(3_500.0)); // 500*7
        assert_eq!(cell_f64(&row["API Requirement Cost"]), Some(3_500.0));
        assert!(
            matches!(row["Child Total Production Cost"], Data::Empty),
            "no child total is fabricated: {:?}",
            row["Child Total Production Cost"]
        );
        assert!(matches!(row["API Child Consumed Cost"], Data::Empty));
    }

    // A Reaction boundary flows through the identical proportional
    // consumed-cost formula as a Build boundary.
    #[test]
    fn reaction_child_flows_through_identically_to_build() {
        let build = test_build("Reaction Child");
        let mut child = with_facility(
            op(1, Some(0), 2, 100),
            Some("0"),
            "0",
            "0",
            "0",
            "0",
            "10000",
        );
        child.activity = MaterialActivity::Reaction;
        let operations = vec![
            with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
            child,
        ];
        let boundaries = vec![
            boundary(
                0,
                0,
                None,
                90_001,
                MaterialBoundaryResolution::Reaction,
                FulfillmentScope::Missing,
                150,
                150,
                0,
                150,
            ),
            // The reaction child's own recipe input (Pyerite), fully
            // covered so the child's own material cost is 0 -- its
            // 10_000 total is fixed-supplemental installation alone.
            // Without a boundary here `child_op_index` cannot resolve
            // and the projection would (correctly) report incomplete.
            boundary(
                1,
                1,
                Some(0),
                35,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                50,
                50,
                50,
                0,
            ),
        ];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(90_001, None), basis(35, Some("0"))],
            &[(90_001, "0"), (35, "0")],
        );
        let bytes = generate(&model);
        let root_sheet = op_sheet_names(&bytes)[0].clone();
        let mut wb = open_wb(&bytes);
        let row = op_table_row(&mut wb, &root_sheet, "90001");
        assert_eq!(cell_text(&row["Resolution"]), "reaction");
        // child total = 10_000 (fixed supplemental only); produced 200
        // (2 runs * 100); consumed 150/200 of 10_000 = 7_500.
        assert_eq!(cell_f64(&row["API Child Consumed Cost"]), Some(7_500.0));
        assert_eq!(cell_f64(&row["API Retained Surplus Basis"]), Some(2_500.0));

        let consumed_formula = formula_cell(&bytes, &root_sheet, "AT20");
        assert!(consumed_formula.contains("$C20=\"buy\""));
    }

    // With no facility selected, installation is INCOMPLETE
    // (blank), never zero-substituted, and Cost Status must be able to
    // read that off a literal cell, not a re-derivation.
    #[test]
    fn no_facility_installation_is_incomplete_never_zero() {
        let build = test_build("No Facility");
        let operations = vec![op(0, None, 1, 1)]; // no with_facility(): facility_id stays None
        let boundaries = vec![with_price(
            boundary(
                0,
                0,
                None,
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                100,
                100,
                0,
                100,
            ),
            "5",
        )];
        let model = model_from(&build, operations, boundaries, vec![basis(34, None)], &[]);
        let bytes = generate(&model);
        let root_sheet = op_sheet_names(&bytes)[0].clone();

        // Own Installation Total is written as the literal text
        // "INCOMPLETE" (no facility -> no formula at all), never 0.
        let xml_map = sheet_xml_map(&bytes);
        let sheet_xml = xml_map.get(&root_sheet).unwrap();
        assert!(
            sheet_xml.contains("INCOMPLETE"),
            "installation is marked incomplete, not silently zeroed: {sheet_xml}"
        );

        // Material cost is still fully computable (Buy boundary, priced,
        // no inventory) -- incompleteness is scoped to installation only.
        let mut wb = open_wb(&bytes);
        let row = op_table_row(&mut wb, &root_sheet, "34");
        assert_eq!(cell_f64(&row["API Fresh Cost"]), Some(500.0));
        assert_eq!(cell_f64(&row["API Requirement Cost"]), Some(500.0));
    }

    // A grandchild's retained surplus must not leak into the child's
    // consumed contribution or the root cost.
    #[test]
    fn nested_grandchild_surplus_does_not_leak_upward() {
        let build = test_build("Nested Surplus");
        let operations = vec![
            with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"), // root
            with_facility(op(1, Some(0), 1, 100), Some("0"), "0", "0", "0", "0", "0"), // child
            with_facility(
                op(2, Some(1), 1, 1_000),
                Some("0"),
                "0",
                "0",
                "0",
                "0",
                "100000",
            ), // grandchild: fixed cost only, produces surplus
        ];
        let boundaries = vec![
            // root needs 60 of 90_001 (child's product) -> child sized to
            // produce 100 (1 run), consuming 60, retaining 40 worth.
            boundary(
                0,
                0,
                None,
                90_001,
                MaterialBoundaryResolution::Build,
                FulfillmentScope::Missing,
                60,
                60,
                0,
                60,
            ),
            // child needs 500 of 90_002 (grandchild's product) -> 1 run of
            // 1_000, consuming 500, retaining 500 worth (basis 100_000/2).
            boundary(
                1,
                1,
                Some(0),
                90_002,
                MaterialBoundaryResolution::Build,
                FulfillmentScope::Missing,
                500,
                500,
                0,
                500,
            ),
            // The grandchild's own recipe input, fully covered at a
            // zero basis so its own material cost is 0 -- its 100_000
            // total is fixed-supplemental installation alone. Without
            // this, `child_op_index` cannot resolve for boundary 1.
            boundary(
                2,
                2,
                Some(1),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                100,
                100,
                100,
                0,
            ),
        ];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![
                basis(90_001, None),
                basis(90_002, None),
                basis(34, Some("0")),
            ],
            &[(90_001, "0"), (90_002, "0"), (34, "0")],
        );
        let bytes = generate(&model);
        let names = op_sheet_names(&bytes);
        let (root_sheet, child_sheet) = (names[0].clone(), names[1].clone());
        let mut wb = open_wb(&bytes);

        // Child's own total production cost is 100_000 (material 0 +
        // install 100_000) -- consumed by root proportionally, 60/100.
        let child_row = op_table_row(&mut wb, &child_sheet, "90002");
        assert_eq!(
            cell_f64(&child_row["API Child Consumed Cost"]),
            Some(50_000.0)
        ); // 100000*500/1000
        assert_eq!(
            cell_f64(&child_row["API Retained Surplus Basis"]),
            Some(50_000.0)
        );

        let root_row = op_table_row(&mut wb, &root_sheet, "90001");
        // Root consumes 60/100 of the CHILD's total_production_cost
        // (material 50_000 + install 0 = 50_000), i.e. 30_000 -- the
        // grandchild's own retained 50_000 never enters this number.
        assert_eq!(
            cell_f64(&root_row["API Child Consumed Cost"]),
            Some(30_000.0)
        );
        assert_eq!(
            cell_f64(&root_row["API Retained Surplus Basis"]),
            Some(20_000.0)
        );
    }

    // =====================================================================
    // Money-rounding proofs
    // =====================================================================

    /// Mirrors `money_round_half_even_ratio`'s exact-integer algorithm in
    /// Rust (not Excel) arithmetic, so the ALGORITHM itself can be proven
    /// correct independent of any Excel formula-evaluation engine (none
    /// is available in this workspace/CI). Both
    /// this function and the real Excel formula perform the identical
    /// steps; only the language differs.
    fn half_even_ratio_mirror(total_scaled: i128, num: i128, den: i128) -> i128 {
        let prod = total_scaled * num;
        let q = prod / den; // floor division; every operand here is nonnegative
        let r = prod - q * den;
        let twice_r = 2 * r;
        if twice_r < den {
            q
        } else if twice_r > den {
            q + 1
        } else if q % 2 == 0 {
            q
        } else {
            q + 1
        }
    }

    #[test]
    fn half_even_ratio_mirror_matches_round_dp_on_midpoints_and_organic_values() {
        // (total, consumed, produced, pinned expectation -- only for the
        // deliberately constructed exact ties; organic cases are proven
        // by self-consistency against `round_dp(4)` instead of a
        // hand-derived literal).
        let cases: &[(&str, i128, i128, Option<&str>)] = &[
            ("0.0001", 1, 2, Some("0.0000")), // 0.00005 tie, digit 0 (even) -> stays
            ("0.0003", 1, 2, Some("0.0002")), // 0.00015 tie, digit 1 (odd) -> up to even 2
            ("0.0005", 1, 2, Some("0.0002")), // 0.00025 tie, digit 2 (even) -> stays
            ("0.0007", 1, 2, Some("0.0004")), // 0.00035 tie, digit 3 (odd) -> up to even 4
            ("85445.6789", 1_283, 10_000, None),
            ("123456789.1234", 7, 13, None),
            ("1.0000", 1, 3, None),
        ];
        for (total, num, den, expected) in cases {
            let total_dec: Decimal = total.parse().unwrap();
            let direct = (total_dec * Decimal::from(*num) / Decimal::from(*den)).round_dp(4);
            if let Some(expected) = expected {
                assert_eq!(
                    direct.to_string(),
                    *expected,
                    "pinned round_dp(4) expectation for {total}*{num}/{den}"
                );
            }
            let total_scaled: i128 = (total_dec * Decimal::from(10_000))
                .round_dp(0)
                .to_i128()
                .unwrap();
            let mirrored = half_even_ratio_mirror(total_scaled, *num, *den);
            let mirrored_money = Decimal::from(mirrored) / Decimal::from(10_000);
            assert_eq!(
                mirrored_money.round_dp(4),
                direct,
                "exact-integer mirror algorithm for {total}*{num}/{den}"
            );
        }
    }

    // The critical midpoint regression, threaded through the *full*
    // `project_build_cost` + workbook path (not just the pure mirror
    // above), for both tie-break directions.
    #[test]
    fn consumed_child_cost_exact_midpoints_round_half_to_even_end_to_end() {
        for (fixed_supplemental, expected_consumed, expected_retained, label) in [
            ("0.0001", "0.0000", "0.0001", "even retained digit stays"),
            (
                "0.0003",
                "0.0002",
                "0.0001",
                "odd retained digit rounds up to even",
            ),
        ] {
            let build = test_build(&format!("Midpoint {label}"));
            let operations = vec![
                with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
                with_facility(
                    op(1, Some(0), 1, 2),
                    Some("0"),
                    "0",
                    "0",
                    "0",
                    "0",
                    fixed_supplemental,
                ),
            ];
            let boundaries = vec![
                boundary(
                    0,
                    0,
                    None,
                    90_001,
                    MaterialBoundaryResolution::Build,
                    FulfillmentScope::Missing,
                    1,
                    1,
                    0,
                    1,
                ),
                // op1's own (fully-covered, zero-basis) boundary -- needed
                // purely so `child_op_by_parent_boundary` can resolve the
                // link from b0 to op1 (an operation with zero boundaries
                // of its own can never be found as anyone's child). Keeps op1's material cost at 0 so
                // its total is exactly the fixed supplemental below.
                boundary(
                    1,
                    1,
                    Some(0),
                    99,
                    MaterialBoundaryResolution::Buy,
                    FulfillmentScope::Missing,
                    1,
                    1,
                    1,
                    0,
                ),
            ];
            let model = model_from(
                &build,
                operations,
                boundaries,
                vec![basis(90_001, None), basis(99, Some("0"))],
                &[(90_001, "0"), (99, "0")],
            );

            let child = op_cost(&model.cost, 1);
            assert_eq!(
                child.total_production_cost,
                Some(money(fixed_supplemental)),
                "{label}: child total"
            );
            let b0 = boundary_cost(&model.cost, 0);
            assert_eq!(
                b0.child_consumed_cost,
                Some(money(expected_consumed)),
                "{label}: consumed"
            );
            assert_eq!(
                b0.child_surplus_retained_basis,
                Some(money(expected_retained)),
                "{label}: retained"
            );
            assert_eq!(
                money(expected_consumed).0 + money(expected_retained).0,
                money(fixed_supplemental).0,
                "{label}: exact conservation"
            );

            // Structural: the OP sheet's Excel Child Consumed Cost
            // formula for this boundary uses the exact-integer
            // half-to-even reconstruction, never plain ROUND.
            let bytes = generate(&model);
            let root_sheet = op_sheet_names(&bytes)[0].clone();
            let formula = formula_cell(&bytes, &root_sheet, "AT20");
            assert!(
                formula.contains("MOD(") && formula.contains("INT("),
                "{label}: half-even reconstruction present: {formula}"
            );
            // `ROUND(x,0)` legitimately appears (recovering the exact
            // scaled-total integer from its `f64` storage); the naive
            // 4dp money-rounding call it replaced must not.
            assert!(
                !formula.contains(",4)"),
                "{label}: no naive ROUND(x,4) money-rounding call for the critical midpoint boundary: {formula}"
            );
        }
    }

    // An installation-component midpoint (facility tax / SCC), the
    // general floating-point half-to-even reconstruction rather than the
    // exact-integer one (adjusted prices/percentages are of
    // unconstrained precision).
    #[test]
    fn installation_component_exact_midpoints_round_half_to_even() {
        let build = test_build("Installation Midpoint");
        // EIV = adjusted_price(34)=1 * base_qpr=1 * runs=1 = 1 exactly.
        // facility_tax_percent="0.005" -> 1*0.005/100 = 0.00005 tie,
        // digit 0 (even) -> stays 0.0000.
        // scc_surcharge_percent="0.015" -> 1*0.015/100 = 0.00015 tie,
        // digit 1 (odd) -> rounds up to even 0.0002.
        let operations = vec![with_facility(
            op(0, None, 1, 1),
            Some("0"),
            "0",
            "0.005",
            "0.015",
            "0",
            "0",
        )];
        let boundaries = vec![with_price(
            boundary(
                0,
                0,
                None,
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                1,
                1,
                0,
                1,
            ),
            "1",
        )];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(34, None)],
            &[(34, "1")],
        );

        let root = op_cost(&model.cost, 0);
        assert_eq!(root.own_installation.eiv, Some(money("1.0000")), "EIV");
        assert_eq!(
            root.own_installation.facility_tax,
            Some(money("0.0000")),
            "facility tax midpoint, even digit stays"
        );
        assert_eq!(
            root.own_installation.scc_surcharge,
            Some(money("0.0002")),
            "SCC surcharge midpoint, odd digit rounds up to even"
        );

        let bytes = generate(&model);
        let root_sheet = op_sheet_names(&bytes)[0].clone();
        let install_row = installation_section_row(1);
        let facility_tax_formula = formula_cell(
            &bytes,
            &root_sheet,
            &format!("B{}", install_row + INST_ROW_FACILITY_TAX + 1),
        );
        // The half-even reconstruction legitimately contains `ROUND(...,9)`
        // (the FP-noise-collapsing step, not a domain-rounding one) --
        // what must be absent is the naive `ROUND(expr,4)` money-rounding
        // call this formula replaced.
        assert!(
            facility_tax_formula.contains("MOD(") && facility_tax_formula.contains("INT("),
            "facility tax uses the half-to-even reconstruction: {facility_tax_formula}"
        );
        assert!(
            !facility_tax_formula.contains(",4)"),
            "no naive ROUND(x,4) money-rounding call remains: {facility_tax_formula}"
        );
    }

    // Proves the workbook follows Rust's "sum unrounded terms, round
    // once" EIV formula rather than the incorrect "round each line, then
    // sum" alternative -- constructed so the two approaches diverge.
    // Two terms of 0.00005 each: sum-then-round(0.0001) = 0.0001 exactly
    // (no rounding needed); round-then-sum rounds each 0.00005 term to
    // 0.0000 (even, stays) first, giving 0.0000 + 0.0000 = 0.0000.
    #[test]
    fn eiv_sums_unrounded_terms_before_rounding_once_not_the_other_way_round() {
        let build = test_build("EIV Sum Then Round");
        let operations = vec![with_facility(
            op(0, None, 1, 1),
            Some("0"),
            "0",
            "0",
            "0",
            "0",
            "0",
        )];
        let boundaries = vec![
            with_price(
                boundary(
                    0,
                    0,
                    None,
                    34,
                    MaterialBoundaryResolution::Buy,
                    FulfillmentScope::Missing,
                    1,
                    1,
                    0,
                    1,
                ),
                "1",
            ),
            with_price(
                boundary(
                    1,
                    0,
                    None,
                    35,
                    MaterialBoundaryResolution::Buy,
                    FulfillmentScope::Missing,
                    1,
                    1,
                    0,
                    1,
                ),
                "1",
            ),
        ];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(34, None), basis(35, None)],
            &[(34, "0.00005"), (35, "0.00005")],
        );

        let root = op_cost(&model.cost, 0);
        assert_eq!(
            root.own_installation.eiv,
            Some(money("0.0001")),
            "sum-then-round: 0.00005+0.00005=0.0001 exactly, never rounded to 0.0000"
        );

        // Structural: the per-boundary EIV Term cells carry the raw
        // unrounded product; only the op-level EIV sum cell rounds.
        let bytes = generate(&model);
        let root_sheet = op_sheet_names(&bytes)[0].clone();
        let term0 = formula_cell(&bytes, &root_sheet, "BD20");
        let term1 = formula_cell(&bytes, &root_sheet, "BD21");
        for (label, term) in [("term0", &term0), ("term1", &term1)] {
            assert!(
                !term.contains("ROUND(") && !term.contains("INT("),
                "{label} is unrounded: {term}"
            );
        }
        let eiv_row = installation_section_row(2);
        let eiv_formula = formula_cell(
            &bytes,
            &root_sheet,
            &format!("B{}", eiv_row + INST_ROW_EIV + 1),
        );
        assert!(
            eiv_formula.contains("SUM("),
            "EIV sums the terms in one shot: {eiv_formula}"
        );
    }

    // =====================================================================
    // Additional cost-topology fixtures
    // =====================================================================

    // Full scope Build -- inventory is ignored even though it
    // physically exists (planned_use == 0 by scope, not by
    // unavailability), the child is still produced and sized from the
    // *full* requirement, and discrete output leaves a real surplus.
    #[test]
    fn full_build_ignores_inventory_and_sizes_child_from_full_requirement() {
        let build = test_build("Full Build");
        let operations = vec![
            with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
            with_facility(op(1, Some(0), 4, 300), Some("0"), "0", "0", "0", "0", "500"),
        ];
        let boundaries = vec![
            boundary(
                0,
                0,
                None,
                90_001,
                MaterialBoundaryResolution::Build,
                FulfillmentScope::Full,
                1_000,
                1_000,
                0, // Full: ignored even though inventory exists (basis below)
                1_000,
            ),
            boundary(
                1,
                1,
                Some(0),
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                100,
                400,
                400,
                0,
            ),
        ];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(90_001, Some("999")), basis(34, Some("5"))],
            &[(90_001, "0"), (34, "0")],
        );

        let b0 = boundary_cost(&model.cost, 0);
        assert_eq!(
            b0.inventory_cost,
            Some(Money::zero()),
            "Full scope: inventory cost is 0 despite a real basis existing for 90_001"
        );
        let child = op_cost(&model.cost, 1);
        assert_eq!(
            child.produced_quantity, 1_200,
            "child is still produced: 4 runs * 300"
        );
        assert_eq!(
            child.material_component_cost,
            Some(money("2000")),
            "child material: 400 * 5 inventory basis"
        );
        assert_eq!(
            child.total_production_cost,
            Some(money("2500")),
            "child total: 2000 material + 500 fixed installation"
        );
        let expected_consumed =
            (money("2500").0 * Decimal::from(1_000u64) / Decimal::from(1_200u64)).round_dp(4);
        assert_eq!(
            b0.child_consumed_cost,
            Some(Money(expected_consumed)),
            "parent consumed child cost follows the canonical proportional formula"
        );
        assert_eq!(
            b0.child_surplus_retained_basis,
            Some(Money(money("2500").0 - expected_consumed)),
            "discrete surplus (1200 produced - 1000 consumed) retains a real basis"
        );
        assert_eq!(
            b0.requirement_cost, b0.child_consumed_cost,
            "Full scope requirement cost is the consumed child cost alone (inventory is 0)"
        );

        // API == Excel: the literal cells the workbook wrote are exactly
        // the `BuildCostProjection` values above (by construction), and
        // the Excel Requirement Cost formula still routes through the
        // ordinary Build branch (inventory + consumed), fabricating
        // nothing extra for Full scope.
        let bytes = generate(&model);
        let root_sheet = op_sheet_names(&bytes)[0].clone();
        let mut wb = open_wb(&bytes);
        let row = op_table_row(&mut wb, &root_sheet, "90001");
        assert_eq!(cell_text(&row["Scope"]), "Full");
        assert_eq!(
            cell_f64(&row["API Requirement Cost"]),
            b0.requirement_cost.map(|m| m.0.to_f64().unwrap())
        );
        let req_formula = formula_cell(&bytes, &root_sheet, "AZ20");
        assert!(req_formula.contains("$AI20") && req_formula.contains("$AT20"));
    }

    // Two different facilities (root A, child B) -- each
    // operation's own installation must use only its own primitives,
    // and the parent's material cost must contain only the *consumed
    // share* of the child's total, never the child's raw total.
    #[test]
    fn different_facilities_no_installation_leakage() {
        let build = test_build("Different Facilities");
        let operations = vec![
            with_facility(op(0, None, 1, 1), Some("0.1"), "0", "2", "0", "0", "10"),
            with_facility(
                op(1, Some(0), 1, 10),
                Some("0.2"),
                "50",
                "0",
                "5",
                "0",
                "20",
            ),
        ];
        let boundaries = vec![
            boundary(
                0,
                0,
                None,
                90_001,
                MaterialBoundaryResolution::Build,
                FulfillmentScope::Missing,
                10,
                10,
                0,
                5,
            ),
            with_price(
                boundary(
                    1,
                    1,
                    Some(0),
                    35,
                    MaterialBoundaryResolution::Buy,
                    FulfillmentScope::Missing,
                    4,
                    4,
                    0,
                    4,
                ),
                "1",
            ),
        ];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(90_001, None), basis(35, None)],
            &[(90_001, "3"), (35, "2")],
        );

        let root = op_cost(&model.cost, 0);
        let child = op_cost(&model.cost, 1);

        // Root's installation is derived from A's primitives alone.
        assert_eq!(
            root.own_installation.eiv,
            Some(money("30")),
            "root EIV: 3*10*1"
        );
        assert_eq!(
            root.own_installation.facility_tax,
            Some(money("0.6")),
            "root facility tax: 30*2%"
        );
        assert_eq!(
            root.own_installation.scc_surcharge,
            Some(money("0")),
            "root has no SCC surcharge configured"
        );
        assert_eq!(
            root.own_installation.total,
            Some(money("13.6")),
            "root: 10 fixed + 3 system index + 0.6 tax"
        );

        // Child's installation is derived from B's primitives alone --
        // none of A's percentages (2% tax, 0% SCC) leak in.
        assert_eq!(
            child.own_installation.eiv,
            Some(money("8")),
            "child EIV: 2*4*1"
        );
        assert_eq!(
            child.own_installation.facility_tax,
            Some(money("0")),
            "child has no facility tax configured"
        );
        assert_eq!(
            child.own_installation.scc_surcharge,
            Some(money("0.4")),
            "child SCC: 8*5%"
        );
        assert_eq!(
            child.own_installation.total,
            Some(money("21.2")),
            "child: 20 fixed + 0.8 system index + 0.4 SCC"
        );

        // Parent's material cost contains only the *consumed share* of
        // the child's total (25.2), never the child's raw total (25.2
        // happens to also be the child's total here since the child has
        // no surplus of its own to retain beyond what's consumed --
        // the boundary below is the one that actually proves the split).
        let child_material = money("4"); // 4 * 1 fresh
        assert_eq!(child.material_component_cost, Some(child_material));
        assert_eq!(child.total_production_cost, Some(money("25.2")));
        let b0 = boundary_cost(&model.cost, 0);
        let expected_consumed =
            (money("25.2").0 * Decimal::from(5u64) / Decimal::from(10u64)).round_dp(4);
        assert_eq!(b0.child_consumed_cost, Some(Money(expected_consumed)));
        assert_eq!(expected_consumed, money("12.6").0);
        assert_eq!(
            root.material_component_cost,
            Some(money("12.6")),
            "root material is the consumed share, not the child's full 25.2 total"
        );
        assert_eq!(root.total_production_cost, Some(money("26.2")));

        // Structural: each op sheet's own primitive percentage cells
        // show its own facility's numbers, never the other's.
        let bytes = generate(&model);
        let names = op_sheet_names(&bytes);
        let (root_sheet, child_sheet) = (names[0].clone(), names[1].clone());
        let root_install = installation_section_row(1);
        let child_install = installation_section_row(1);
        let mut wb = open_wb(&bytes);
        let root_tax_pct = wb
            .worksheet_range(&root_sheet)
            .unwrap()
            .get_value((root_install + INST_ROW_FACILITY_TAX_PCT, 1))
            .and_then(cell_f64)
            .unwrap();
        assert_eq!(
            root_tax_pct, 2.0,
            "root sheet carries A's own facility tax %"
        );
        let child_tax_pct = wb
            .worksheet_range(&child_sheet)
            .unwrap()
            .get_value((child_install + INST_ROW_FACILITY_TAX_PCT, 1))
            .and_then(cell_f64)
            .unwrap();
        assert_eq!(
            child_tax_pct, 0.0,
            "child sheet carries B's own facility tax %, not A's 2%"
        );
    }

    // The same raw type demanded at two different boundaries with
    // insufficient combined inventory -- the quantity-side Allocation
    // Ledger allocates it globally once; cost formulas must reference
    // that same allocation, never perform a second, cost-specific one.
    #[test]
    fn shared_inventory_cost_uses_the_ledgers_allocation_not_a_second_one() {
        let build = test_build("Shared Inventory");
        let operations = vec![
            with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
            with_facility(op(1, Some(0), 1, 1), Some("0"), "0", "0", "0", "0", "0"),
        ];
        // Physical stock of type 34 is 100 (unit basis 2); root takes 60
        // first (DFS pre-order), leaving 40 for the descendant, which
        // makes up the rest (40 more required) with a fresh purchase.
        let boundaries = vec![
            with_price(
                boundary(
                    0,
                    0,
                    None,
                    34,
                    MaterialBoundaryResolution::Buy,
                    FulfillmentScope::Missing,
                    60,
                    60,
                    60,
                    0,
                ),
                "3",
            ),
            boundary(
                1,
                0,
                None,
                90_001,
                MaterialBoundaryResolution::Build,
                FulfillmentScope::Missing,
                1,
                1,
                0,
                1,
            ),
            with_price(
                boundary(
                    2,
                    1,
                    Some(1),
                    34,
                    MaterialBoundaryResolution::Buy,
                    FulfillmentScope::Missing,
                    80,
                    80,
                    40,
                    40,
                ),
                "3",
            ),
        ];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(34, Some("2")), basis(90_001, None)],
            &[(34, "0"), (90_001, "0")],
        );

        let b0 = boundary_cost(&model.cost, 0);
        let b2 = boundary_cost(&model.cost, 2);
        // Total planned inventory use across both boundaries never
        // exceeds the physical stock -- if `build_cost.rs` performed a
        // second, cost-specific allocation instead of trusting the
        // ledger's `api_planned_use`, nothing here would catch a
        // double-count (there is no stock ceiling inside this module at
        // all) -- this fixture's whole point is that the *inputs*
        // already reflect a single global allocation, and the outputs
        // below are a faithful, non-duplicating function of them.
        assert!(b0.inventory_quantity + b2.inventory_quantity <= 100);
        assert_eq!(b0.inventory_quantity, 60);
        assert_eq!(b2.inventory_quantity, 40);
        // Both boundaries used the *same* basis-by-type entry (unit 2),
        // never a second, boundary-specific lookup.
        assert_eq!(b0.inventory_unit_basis, Some(dec("2")));
        assert_eq!(b2.inventory_unit_basis, Some(dec("2")));
        assert_eq!(b0.inventory_cost, Some(money("120")), "60 * 2");
        assert_eq!(b2.inventory_cost, Some(money("80")), "40 * 2");
        assert_eq!(b2.fresh_cost, Some(money("120")), "40 * 3 fresh");
        assert_eq!(
            b2.requirement_cost,
            Some(money("200")),
            "80 inventory + 120 fresh"
        );

        let child = op_cost(&model.cost, 1);
        assert_eq!(child.material_component_cost, Some(money("200")));
        assert_eq!(child.total_production_cost, Some(money("200")));
        let b1 = boundary_cost(&model.cost, 1);
        assert_eq!(
            b1.child_consumed_cost,
            Some(money("200")),
            "exact-output child (1 produced, 1 consumed): whole total consumed"
        );
        let root = op_cost(&model.cost, 0);
        assert_eq!(
            root.material_component_cost,
            Some(money("320")),
            "120 (b0) + 200 (b1)"
        );
        assert_eq!(root.total_production_cost, Some(money("320")));

        // API == Excel for both requirements and the root total.
        let bytes = generate(&model);
        let root_sheet = op_sheet_names(&bytes)[0].clone();
        let mut wb = open_wb(&bytes);
        let row0 = op_table_row(&mut wb, &root_sheet, "34");
        assert_eq!(cell_f64(&row0["API Requirement Cost"]), Some(120.0));
        let root_install = installation_section_row(2);
        let root_total_api = wb
            .worksheet_range(&root_sheet)
            .unwrap()
            .get_value((
                root_install + INST_ROW_TOTAL_PRODUCTION_COST,
                INST_COL_API.into(),
            ))
            .and_then(cell_f64)
            .unwrap();
        assert_eq!(root_total_api, 320.0);
    }

    // =====================================================================
    // The realistic multi-op integration guard
    // =====================================================================

    /// Topology: root (Manufacturing, facility A) with three boundaries --
    /// a partial-inventory Buy (type 34), a Build branch (type 90_001 ->
    /// op1, Manufacturing, facility B), and a Reaction sibling branch
    /// (type 90_003 -> op2, Reaction, facility C). op1 has its own Build
    /// boundary (type 35 -> op3, a Manufacturing grandchild, facility D)
    /// which re-demands type 34 (the *same* raw type the root buys
    /// directly) as a fresh-only Buy. Every producing boundary sizes to
    /// a discrete, non-exact multiple of its child's `output_per_run`,
    /// so every one retains a real, nonzero surplus basis.
    ///
    /// Numbers were hand-derived and cross-checked against this exact
    /// construction before being pinned below;
    /// the two proportional child-consumption divisions are NOT tied,
    /// so their expected values are computed here via the same
    /// `round_dp(4)` formula the production code uses, rather than a
    /// second hand-derived literal, to avoid compounding a hand-math
    /// error into a false-positive pinned test.
    type RealisticFixture = (
        Build,
        Vec<VerificationOperationInput>,
        Vec<VerificationBoundaryInput>,
        Vec<InventoryBasisEntry>,
        Vec<(i64, &'static str)>,
    );

    fn realistic_fixture() -> RealisticFixture {
        let build = test_build("Realistic Multi-Op");
        let operations = vec![
            with_facility(
                op(0, None, 1, 1),
                Some("0.05"),
                "10",
                "1",
                "0.5",
                "0",
                "100",
            ),
            with_facility(op(1, Some(0), 8, 7), Some("0.02"), "0", "2", "0", "1", "50"),
            {
                let mut op2 =
                    with_facility(op(2, Some(0), 4, 4), Some("0.01"), "0", "0", "1", "0", "20");
                op2.activity = MaterialActivity::Reaction;
                op2
            },
            with_facility(
                op(3, Some(1), 15, 17),
                Some("0.03"),
                "5",
                "0.5",
                "0",
                "0.2",
                "200",
            ),
        ];
        let boundaries = vec![
            with_price(
                boundary(
                    0,
                    0,
                    None,
                    34,
                    MaterialBoundaryResolution::Buy,
                    FulfillmentScope::Missing,
                    1_000,
                    1_000,
                    400,
                    600,
                ),
                "3",
            ),
            spawn(
                boundary(
                    1,
                    0,
                    None,
                    90_001,
                    MaterialBoundaryResolution::Build,
                    FulfillmentScope::Missing,
                    50,
                    50,
                    0,
                    50,
                ),
                7,
                8,
                56,
                6,
            ),
            spawn(
                boundary(
                    2,
                    0,
                    None,
                    90_003,
                    MaterialBoundaryResolution::Reaction,
                    FulfillmentScope::Missing,
                    20,
                    20,
                    5,
                    15,
                ),
                4,
                4,
                16,
                1,
            ),
            spawn(
                boundary(
                    3,
                    1,
                    Some(1),
                    35,
                    MaterialBoundaryResolution::Build,
                    FulfillmentScope::Missing,
                    30,
                    240,
                    0,
                    240,
                ),
                17,
                15,
                255,
                15,
            ),
            boundary(
                4,
                2,
                Some(2),
                36,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                25,
                100,
                100,
                0,
            ),
            with_price(
                boundary(
                    5,
                    3,
                    Some(3),
                    34,
                    MaterialBoundaryResolution::Buy,
                    FulfillmentScope::Missing,
                    20,
                    300,
                    0,
                    300,
                ),
                "4",
            ),
        ];
        let inventory_basis = vec![
            basis(34, Some("2")),
            basis(90_003, Some("10")),
            basis(36, Some("1.5")),
        ];
        let adjusted_prices: Vec<(i64, &'static str)> = vec![
            (34, "2"),
            (90_001, "100"),
            (90_003, "50"),
            (35, "5"),
            (36, "3"),
        ];
        (
            build,
            operations,
            boundaries,
            inventory_basis,
            adjusted_prices,
        )
    }

    /// The full material-side conservation matrix (mirrors
    /// `crates/iskworks-core/src/build_cost/tests.rs::assert_conservation`,
    /// which is private to that crate and so can't be reused directly).
    fn assert_conservation(cost: &BuildCostProjection) {
        for boundary in &cost.boundaries {
            if !boundary.complete {
                continue;
            }
            match boundary.kind {
                BoundaryCostKind::Buy => {
                    assert_eq!(
                        boundary.inventory_cost.unwrap().0 + boundary.fresh_cost.unwrap().0,
                        boundary.requirement_cost.unwrap().0,
                        "Buy boundary {} decomposition",
                        boundary.traversal_index
                    );
                }
                BoundaryCostKind::Build | BoundaryCostKind::Reaction => {
                    assert_eq!(
                        boundary.inventory_cost.unwrap().0
                            + boundary.child_consumed_cost.unwrap().0,
                        boundary.requirement_cost.unwrap().0,
                        "Build/Reaction boundary {} decomposition",
                        boundary.traversal_index
                    );
                    if let (Some(total), Some(consumed), Some(surplus)) = (
                        boundary.child_total_production_cost,
                        boundary.child_consumed_cost,
                        boundary.child_surplus_retained_basis,
                    ) {
                        assert_eq!(
                            consumed.0 + surplus.0,
                            total.0,
                            "exact child conservation at boundary {}",
                            boundary.traversal_index
                        );
                    }
                }
                BoundaryCostKind::FullyCovered => {
                    assert_eq!(
                        boundary.inventory_cost, boundary.requirement_cost,
                        "fully-covered boundary {}",
                        boundary.traversal_index
                    );
                }
                BoundaryCostKind::Unresolved => {}
            }
        }
        for operation in &cost.operations {
            if !operation.complete {
                continue;
            }
            let sum_requirements: Decimal = cost
                .boundaries
                .iter()
                .filter(|b| b.op_index == operation.op_index)
                .map(|b| b.requirement_cost.unwrap().0)
                .sum();
            assert_eq!(
                Some(Money(sum_requirements)),
                operation.material_component_cost,
                "op {} sum(requirement_cost) == material_component_cost",
                operation.op_index
            );
            assert_eq!(
                Some(Money(
                    operation.material_component_cost.unwrap().0
                        + operation.own_installation.total.unwrap().0
                )),
                operation.total_production_cost,
                "op {} total = material + installation",
                operation.op_index
            );
        }
        let root_total = op_cost(cost, 0).total_production_cost;
        assert_eq!(
            cost.root.planning_total_production_cost, root_total,
            "root == root operation total"
        );
        let naive_sum: Decimal = cost
            .operations
            .iter()
            .map(|o| o.total_production_cost.unwrap().0)
            .sum();
        assert_ne!(
            Some(naive_sum),
            root_total.map(|m| m.0),
            "summing every operation's total double-counts descendants"
        );
    }

    /// `fresh_outlay + inventory_basis_consumed == root_total +
    /// retained_surplus_basis`, exactly. A general structural identity,
    /// not a fixture-specific literal -- proven directly on whatever the
    /// realistic fixture's own numbers turn out to be.
    fn assert_whole_plan_reconciliation(cost: &BuildCostProjection) {
        let lhs = cost.root.total_fresh_outlay.0 + cost.root.total_inventory_basis_consumed.0;
        let rhs = cost.root.planning_total_production_cost.unwrap().0
            + cost.root.total_surplus_retained_basis.0;
        assert_eq!(lhs, rhs, "whole-plan reconciliation");
    }

    #[test]
    fn realistic_multi_op_fixture_composes_every_cost_semantic() {
        let (build, operations, boundaries, inventory_basis, adjusted_prices) = realistic_fixture();

        // Quantity self-consistency check (the honest scope of a
        // "Quantity status" proof without a live Excel formula engine):
        // every boundary's `api_required` is
        // exactly what `MAX(runs, ROUNDUP(base*runs,0))` -- the Excel
        // Required formula, with every ME/facility factor at its
        // identity value in this fixture -- would independently
        // recompute from primitives, using each boundary's *owning
        // operation's* real `node_runs`.
        let node_runs_by_op: HashMap<u32, u64> = operations
            .iter()
            .map(|o| (o.op_index, o.node_runs))
            .collect();
        for b in &boundaries {
            let runs = node_runs_by_op[&b.op_index];
            let expected_required = (b.base_quantity_per_run * runs).max(runs);
            assert_eq!(
                b.api_required, expected_required,
                "boundary {} quantity self-consistency",
                b.traversal_index
            );
        }

        let model = model_from(
            &build,
            operations,
            boundaries,
            inventory_basis,
            &adjusted_prices,
        );
        let cost = &model.cost;
        assert!(
            cost.complete,
            "every boundary/operation is cost-complete: {:?}",
            cost.warnings
        );
        assert_conservation(cost);
        assert_whole_plan_reconciliation(cost);

        // -- pinned, hand-verified numbers for the exact (non-tied) parts --
        let root = op_cost(cost, 0);
        assert_eq!(root.own_installation.eiv, Some(money("8000")));
        assert_eq!(root.own_installation.total, Some(money("580")));
        let op1 = op_cost(cost, 1);
        assert_eq!(op1.own_installation.eiv, Some(money("1200")));
        assert_eq!(op1.own_installation.total, Some(money("110")));
        let op2 = op_cost(cost, 2);
        assert_eq!(op2.own_installation.eiv, Some(money("300")));
        assert_eq!(op2.own_installation.total, Some(money("26")));
        let op3 = op_cost(cost, 3);
        assert_eq!(op3.own_installation.eiv, Some(money("600")));
        assert_eq!(op3.own_installation.total, Some(money("221.3")));
        assert_eq!(
            op3.material_component_cost,
            Some(money("1200")),
            "300 fresh * 4"
        );
        assert_eq!(op3.total_production_cost, Some(money("1421.3")));

        let b0 = boundary_cost(cost, 0);
        assert_eq!(
            b0.requirement_cost,
            Some(money("2600")),
            "800 inventory + 1800 fresh"
        );
        let b4 = boundary_cost(cost, 4);
        assert_eq!(
            b4.requirement_cost,
            Some(money("150")),
            "fully covered, inventory only"
        );
        let b2_reaction = boundary_cost(cost, 2);
        assert_eq!(
            b2_reaction.child_consumed_cost,
            Some(money("165")),
            "176 * 15/16 divides exactly"
        );
        assert_eq!(b2_reaction.child_surplus_retained_basis, Some(money("11")));

        // -- proportional divisions: formula-derived expectation, not a
        // second hand-derived literal --
        let b3 = boundary_cost(cost, 3);
        let expected_b3_consumed = (op3.total_production_cost.unwrap().0 * Decimal::from(240u64)
            / Decimal::from(255u64))
        .round_dp(4);
        assert_eq!(b3.child_consumed_cost, Some(Money(expected_b3_consumed)));
        assert_eq!(
            b3.child_surplus_retained_basis,
            Some(Money(
                op3.total_production_cost.unwrap().0 - expected_b3_consumed
            ))
        );

        let b1 = boundary_cost(cost, 1);
        let expected_b1_consumed = (op1.total_production_cost.unwrap().0 * Decimal::from(50u64)
            / Decimal::from(56u64))
        .round_dp(4);
        assert_eq!(b1.child_consumed_cost, Some(Money(expected_b1_consumed)));
        assert_eq!(
            b1.child_surplus_retained_basis,
            Some(Money(
                op1.total_production_cost.unwrap().0 - expected_b1_consumed
            ))
        );

        // Nested surplus does not leak. The grandchild's (op3's)
        // own retained basis lives *only* on b3; it is not a separate
        // line anywhere else, and op1's material cost is strictly the
        // *consumed share* of op3's total, never op3's raw total.
        assert_ne!(
            op1.material_component_cost.unwrap().0,
            op3.total_production_cost.unwrap().0,
            "op1 material must be the consumed share of op3, not op3's raw total"
        );
        let total_retained_basis: Decimal = [b1, b2_reaction, b3]
            .iter()
            .map(|b| b.child_surplus_retained_basis.unwrap().0)
            .sum();
        assert_eq!(
            cost.root.total_surplus_retained_basis.0, total_retained_basis,
            "each boundary's retained basis is counted exactly once at the root"
        );

        let root_total = root.total_production_cost.unwrap();
        assert_eq!(
            root.material_component_cost,
            Some(Money(
                b0.requirement_cost.unwrap().0
                    + b1.requirement_cost.unwrap().0
                    + b2_reaction.requirement_cost.unwrap().0
            )),
        );
        assert_eq!(
            root_total.0,
            root.material_component_cost.unwrap().0 + money("580").0
        );

        // -- reopen the generated workbook and inspect structure --
        let bytes = generate(&model);
        let mut wb = open_wb(&bytes);
        let names: Vec<String> = wb.sheet_names().iter().map(|s| s.to_string()).collect();
        assert!(names.contains(&"Summary".to_string()));
        assert!(names.contains(&"Types".to_string()));
        assert!(names.contains(&"Operations".to_string()));
        assert!(names.contains(&"Engineering - Requirements".to_string()));
        assert_eq!(op_sheet_names(&bytes).len(), 4, "one sheet per operation");

        let summary_row = table_row_by_label(&mut wb, "Summary", "Workbook Version");
        assert_eq!(cell_text(&summary_row), "v4");

        let ops_headers = headers_of(&mut wb, "Operations");
        for h in [
            "Excel Material Cost",
            "API Material Cost",
            "Excel Installation",
            "API Installation",
            "Excel Total Cost",
            "API Total Cost",
            "Cost Check",
        ] {
            assert!(ops_headers.iter().any(|x| x == h), "Operations column {h}");
        }
        let req_headers = headers_of(&mut wb, "Engineering - Requirements");
        for h in [
            "API Inventory Cost",
            "API Fresh Cost",
            "API Child Consumed Cost",
            "API Requirement Cost",
            "API Retained Basis",
            "Cost Evidence",
        ] {
            assert!(
                req_headers.iter().any(|x| x == h),
                "Engineering - Requirements column {h}"
            );
        }
        // Literal text cells are shared-string references in the raw
        // sheet XML (resolved through `xl/sharedStrings.xml`, which
        // `sheet_xml_map` doesn't read) -- use calamine, which resolves
        // them, rather than a raw-XML text search.
        let summary_range = wb.worksheet_range("Summary").unwrap();
        let summary_cells: Vec<String> = summary_range
            .rows()
            .flat_map(|row| row.iter().map(cell_text))
            .collect();
        for section in ["ROOT COST", "COST VERIFICATION", "MONEY ROUNDING"] {
            assert!(
                summary_cells.iter().any(|c| c == section),
                "Summary contains the {section} section header"
            );
        }

        // Formula independence, spot-checked on this realistic
        // fixture's own cells -- never the corresponding API cell.
        let root_sheet = op_sheet_names(&bytes)[0].clone();
        let req_formula = formula_cell(&bytes, &root_sheet, "AZ20"); // b0, first row
        assert!(
            !req_formula.contains("$BA20"),
            "Excel Requirement Cost never reads its own API cell"
        );
        let install_row0 = installation_section_row(3);
        let material_formula = formula_cell(
            &bytes,
            &root_sheet,
            &format!("B{}", install_row0 + INST_ROW_MATERIAL_COST + 1),
        );
        assert!(
            !material_formula.contains(&format!("${}", col_letter(u32::from(INST_COL_API)))),
            "Excel Material Cost never references the API column: {material_formula}"
        );
        let total_formula = formula_cell(
            &bytes,
            &root_sheet,
            &format!("B{}", install_row0 + INST_ROW_TOTAL_PRODUCTION_COST + 1),
        );
        assert!(
            total_formula.contains(&format!("B{}", install_row0 + INST_ROW_MATERIAL_COST + 1))
                && total_formula.contains(&format!(
                    "B{}",
                    install_row0 + INST_ROW_OWN_INSTALL_TOTAL + 1
                )),
            "Excel Total = Excel Material + Excel Installation: {total_formula}"
        );
    }

    /// Finds the row on a Summary-shaped (label in col A, value in col B)
    /// sheet whose label matches `label`, returning its value cell.
    fn table_row_by_label(wb: &mut Xlsx<Cursor<Vec<u8>>>, sheet: &str, label: &str) -> Data {
        let range = wb.worksheet_range(sheet).unwrap();
        for row in range.rows() {
            if row.first().map(cell_text).as_deref() == Some(label) {
                return row.get(1).cloned().unwrap_or(Data::Empty);
            }
        }
        panic!("no row on {sheet} with label {label}");
    }

    // =====================================================================
    // Tamper extension, explicit incomplete/Reaction assertions, the
    // display-format-is-not-rounding audit
    // =====================================================================

    // Tamper the *child's* API Total Production Cost cell (not the
    // simple all-Buy fixture's own requirement cost) -- the more
    // consequential case, since the parent's consumed-cost formula
    // cross-references the child sheet. Expected: the child's own Excel
    // Total Production Cost formula is untouched, the parent's
    // Excel-side consumed-cost formula still points at that (unchanged)
    // Excel cell rather than the tampered API one, and the child's own
    // Total-Production-Cost Delta formula would read the tampered value
    // (a real MISMATCH) purely because it's the one thing that reads
    // that cell at all.
    #[test]
    fn tamper_api_child_total_does_not_change_excel_child_total() {
        let build = test_build("Tamper Child Total");
        let operations = vec![
            with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
            with_facility(
                op(1, Some(0), 1, 1_000),
                Some("0"),
                "0",
                "0",
                "0",
                "0",
                "200000",
            ),
        ];
        let boundaries = vec![
            boundary(
                0,
                0,
                None,
                90_001,
                MaterialBoundaryResolution::Build,
                FulfillmentScope::Missing,
                500,
                500,
                0,
                500,
            ),
            with_price(
                boundary(
                    1,
                    1,
                    Some(0),
                    34,
                    MaterialBoundaryResolution::Buy,
                    FulfillmentScope::Missing,
                    800_000,
                    800_000,
                    0,
                    800_000,
                ),
                "1",
            ),
        ];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(34, None), basis(90_001, None)],
            &[(34, "0"), (90_001, "0")],
        );
        let bytes = generate(&model);
        let names = op_sheet_names(&bytes);
        let (root_sheet, child_sheet) = (names[0].clone(), names[1].clone());

        let child_install_row = installation_section_row(1);
        let total_row_xr = child_install_row + INST_ROW_TOTAL_PRODUCTION_COST + 1;
        let excel_total_cell = format!("B{total_row_xr}");
        let api_total_cell = format!("C{total_row_xr}");
        let excel_formula_before = formula_cell(&bytes, &child_sheet, &excel_total_cell);
        assert!(!excel_formula_before.is_empty());

        // The parent's own Child Total Production Cost cell (AQ20) is
        // the direct cross-sheet reference to the child's *Excel* Total
        // Production Cost cell; the parent's Child Consumed Cost
        // formula (AT20) then reads AQ20, one level removed -- confirm
        // both links before tampering, so the point of the test is
        // meaningful.
        let child_total_ref_formula = formula_cell(&bytes, &root_sheet, "AQ20");
        assert!(
            child_total_ref_formula.contains(&format!(
                "'{}'!$B${total_row_xr}",
                child_sheet.replace('\'', "''")
            )),
            "parent's AQ20 references the child's Excel total cell directly: {child_total_ref_formula}"
        );
        let consumed_formula_before = formula_cell(&bytes, &root_sheet, "AT20");
        assert!(
            consumed_formula_before.contains("$AQ20"),
            "parent's consumed-cost formula reads AQ20 (one level removed from the child sheet): {consumed_formula_before}"
        );

        // Tamper the child's *API* Total Production Cost literal --
        // never a formula primitive -- in the raw XML.
        let xml_map = sheet_xml_map(&bytes);
        let child_xml = xml_map.get(&child_sheet).unwrap();
        let needle = format!("<c r=\"{api_total_cell}\"");
        let cell_start = child_xml.find(&needle).expect("API total cell exists");
        let cell_end = child_xml[cell_start..].find("</c>").unwrap() + cell_start + "</c>".len();
        let original_cell = &child_xml[cell_start..cell_end];
        assert!(
            original_cell.contains("<v>1000000</v>"),
            "API child total is the literal 1_000_000 we expect: {original_cell}"
        );
        let tampered_cell = original_cell.replace("<v>1000000</v>", "<v>42</v>");
        let tampered_child_xml = child_xml.replacen(original_cell, &tampered_cell, 1);
        assert_ne!(&tampered_child_xml, child_xml);

        // The child's own Excel Total Production Cost formula, re-read
        // from the (conceptually) tampered sheet, is byte-identical --
        // it is a sum of the child's own Excel Material/Installation
        // cells, never a read of its own API neighbour.
        let excel_formula_after = tampered_child_xml
            .split(&format!("<c r=\"{excel_total_cell}\""))
            .nth(1)
            .and_then(|s| s.split("</c>").next())
            .and_then(|c| c.split("<f>").nth(1))
            .and_then(|s| s.split("</f>").next())
            .unwrap_or_default();
        assert_eq!(excel_formula_before, excel_formula_after);

        // The child's own Delta formula for that row is the one and
        // only formula that reads the tampered API cell -- it would
        // report a real (nonzero) MISMATCH once evaluated, exactly
        // because nothing else in the dependency graph does.
        let delta_cell = format!("D{total_row_xr}");
        let delta_formula = formula_cell(&bytes, &child_sheet, &delta_cell);
        assert!(
            delta_formula.contains(&api_total_cell),
            "the Delta formula is the sole reader of the tampered API cell: {delta_formula}"
        );
        assert!(
            !consumed_formula_before.contains(&api_total_cell),
            "the parent's consumed-cost formula never reads that API cell either"
        );
    }

    // No-facility installation is INCOMPLETE, never a false OK
    // (API 0 vs Excel 0) or a false MISMATCH -- explicit checks on both
    // the Rust evidence and the exact cell the workbook writes.
    #[test]
    fn no_facility_cost_is_explicitly_incomplete_not_zero_or_mismatch() {
        let build = test_build("Explicit Incomplete");
        let operations = vec![op(0, None, 1, 1)]; // no with_facility(): facility_id stays None
        let boundaries = vec![with_price(
            boundary(
                0,
                0,
                None,
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                100,
                100,
                0,
                100,
            ),
            "5",
        )];
        let model = model_from(&build, operations, boundaries, vec![basis(34, None)], &[]);

        let root = op_cost(&model.cost, 0);
        assert!(
            !root.own_installation.complete,
            "API installation is incomplete"
        );
        assert_eq!(root.own_installation.total, None, "never fabricated as 0");
        assert!(!root.complete, "operation as a whole is incomplete");
        assert_eq!(root.total_production_cost, None);
        // Material side is independently complete -- incompleteness is
        // scoped to installation, not a global "everything is 0".
        assert_eq!(root.material_component_cost, Some(money("500")));

        let bytes = generate(&model);
        let root_sheet = op_sheet_names(&bytes)[0].clone();
        let install_row = installation_section_row(1);
        let total_xr = install_row + INST_ROW_OWN_INSTALL_TOTAL + 1;
        let mut wb = open_wb(&bytes);
        let range = wb.worksheet_range(&root_sheet).unwrap();
        let excel_total_cell = range
            .get_value((total_xr - 1, INST_COL_EXCEL.into()))
            .cloned()
            .unwrap();
        assert_eq!(
            cell_text(&excel_total_cell),
            "INCOMPLETE",
            "the literal cell text is exactly INCOMPLETE, never 0 or blank"
        );
        let api_total_cell = range
            .get_value((total_xr - 1, INST_COL_API.into()))
            .cloned()
            .unwrap_or(Data::Empty);
        assert!(
            matches!(api_total_cell, Data::Empty),
            "the API cell is blank (never a fabricated 0): {api_total_cell:?}"
        );
    }

    // Reaction cost -- no manufacturing ME formula applies (the ME
    // Factor column already forces 1 for a reaction at the quantity
    // layer; here we confirm the *cost* side independently reproduces
    // the reaction child's own installation and achieves exact total
    // parity), and total production cost parity end to end.
    #[test]
    fn reaction_own_installation_is_independently_reproduced_with_total_parity() {
        let build = test_build("Reaction Installation");
        let mut reaction_child = with_facility(
            op(1, Some(0), 2, 100),
            Some("0.04"),
            "0",
            "3",
            "0",
            "0",
            "10000",
        );
        reaction_child.activity = MaterialActivity::Reaction;
        let operations = vec![
            with_facility(op(0, None, 1, 1), Some("0"), "0", "0", "0", "0", "0"),
            reaction_child,
        ];
        let boundaries = vec![
            boundary(
                0,
                0,
                None,
                90_001,
                MaterialBoundaryResolution::Reaction,
                FulfillmentScope::Missing,
                150,
                150,
                0,
                150,
            ),
            with_price(
                boundary(
                    1,
                    1,
                    Some(0),
                    35,
                    MaterialBoundaryResolution::Buy,
                    FulfillmentScope::Missing,
                    50,
                    50,
                    50,
                    0,
                ),
                "1",
            ),
        ];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(90_001, None), basis(35, Some("0"))],
            &[(90_001, "0"), (35, "10")],
        );

        let child = op_cost(&model.cost, 1);
        assert_eq!(child.activity, MaterialActivity::Reaction);
        // EIV = adjusted_price(35)=10 * base_qpr=50 * runs=2 = 1000, no
        // ME/facility material reduction applied to it (unconstrained by
        // this reaction's `blueprint_me` == 0, which is always the case
        // for a Reaction -- there is no manufacturing ME formula here).
        assert_eq!(child.own_installation.eiv, Some(money("1000")));
        let unmodified = money("1000").0 * dec("0.04");
        assert_eq!(
            child.own_installation.unmodified_system_index_cost,
            Some(Money(unmodified.round_dp(4)))
        );
        assert_eq!(
            child.own_installation.facility_tax,
            Some(money("30")),
            "1000 * 3%"
        );
        assert_eq!(
            child.own_installation.total,
            Some(Money(
                unmodified.round_dp(4) + money("30").0 + money("10000").0
            ))
        );
        assert_eq!(
            child.material_component_cost,
            Some(money("0")),
            "50 units fully covered at 0 basis"
        );
        let expected_child_total =
            child.material_component_cost.unwrap().0 + child.own_installation.total.unwrap().0;
        assert_eq!(
            child.total_production_cost,
            Some(Money(expected_child_total))
        );

        let b0 = boundary_cost(&model.cost, 0);
        let expected_consumed =
            (expected_child_total * Decimal::from(150u64) / Decimal::from(200u64)).round_dp(4);
        assert_eq!(b0.child_consumed_cost, Some(Money(expected_consumed)));
        assert_eq!(
            b0.child_surplus_retained_basis,
            Some(Money(expected_child_total - expected_consumed))
        );
        // Total parity end to end: root's total IS the (only)
        // requirement's consumed cost, since inventory is 0.
        let root = op_cost(&model.cost, 0);
        assert_eq!(root.total_production_cost, b0.requirement_cost);
    }

    // The canonical Money boundaries never rely on the `#,##0.0000`
    // *display* format to look rounded -- every one of them either (a)
    // is a formula whose text contains an explicit domain-rounding
    // function (`ROUND`/`INT`/`MOD`), or (b) is a pure pass-through
    // reference to a cell that is (a), never a bare, unrounded
    // arithmetic expression relying on presentation alone.
    #[test]
    fn money_cells_perform_explicit_domain_rounding_never_display_formatting_alone() {
        let build = test_build("Display Format Audit");
        let operations = vec![with_facility(
            op(0, None, 1, 1),
            Some("0.05"),
            "10",
            "1",
            "0.5",
            "0.25",
            "100",
        )];
        let boundaries = vec![with_price(
            boundary(
                0,
                0,
                None,
                34,
                MaterialBoundaryResolution::Buy,
                FulfillmentScope::Missing,
                100,
                100,
                40,
                60,
            ),
            "5",
        )];
        let model = model_from(
            &build,
            operations,
            boundaries,
            vec![basis(34, Some("3"))],
            &[(34, "2")],
        );
        let bytes = generate(&model);
        let root_sheet = op_sheet_names(&bytes)[0].clone();

        // Per-boundary formula cells that produce a Money value from a
        // fresh multiplication -- each must round explicitly.
        for (label, cell) in [
            ("Excel Inventory Cost", "AI20"),
            ("Excel Fresh Cost", "AM20"),
        ] {
            let formula = formula_cell(&bytes, &root_sheet, cell);
            assert!(
                formula.contains("ROUND(") || formula.contains("INT("),
                "{label} performs explicit domain rounding, not display formatting: {formula}"
            );
        }
        // Excel Requirement Cost is case (b): a pure pass-through *sum*
        // of cells that are already case (a) -- it must reference them,
        // never re-derive from raw primitives without rounding.
        let requirement_formula = formula_cell(&bytes, &root_sheet, "AZ20");
        assert!(
            requirement_formula.contains("$AI20") && requirement_formula.contains("$AM20"),
            "Excel Requirement Cost only combines already-rounded cells: {requirement_formula}"
        );
        // Installation-section formula cells.
        let install_row = installation_section_row(1);
        for (label, offset) in [
            ("EIV", INST_ROW_EIV),
            (
                "Unmodified System Index Cost",
                INST_ROW_UNMOD_SYSTEM_INDEX_COST,
            ),
            ("System Index Cost", INST_ROW_SYSTEM_INDEX_COST),
            ("Facility Tax", INST_ROW_FACILITY_TAX),
            ("SCC Surcharge", INST_ROW_SCC_SURCHARGE),
            ("Alliance Surcharge", INST_ROW_ALLIANCE_SURCHARGE),
            ("Material / Component Cost", INST_ROW_MATERIAL_COST),
        ] {
            let cell = format!("B{}", install_row + offset + 1);
            let formula = formula_cell(&bytes, &root_sheet, &cell);
            assert!(
                formula.contains("ROUND(") || formula.contains("INT("),
                "{label} performs explicit domain rounding, not display formatting: {formula}"
            );
        }
        // The number format itself is presentation-only (Excel's
        // `#,##0.0000` display pattern never alters the underlying
        // stored/computed value, only how it is drawn) -- `rust_xlsxwriter::Format`
        // exposes no public getter to re-read it back for a runtime
        // assertion, so this is enforced by construction instead: every
        // Money cell above goes through `money4_format()` (a single
        // `"#,##0.0000"` definition, never a per-cell one) for display,
        // and -- as just asserted -- through an explicit `ROUND`/`INT`
        // rounding function in its *formula text* for the actual value.
    }

    // Writes one deterministic, realistic workbook to disk for
    // manual opening in Microsoft Excel -- not part of CI (no assertions
    // beyond "it wrote bytes"), and not run by default.
    #[test]
    #[ignore = "manual artifact generation for opening in real Excel -- run explicitly with --ignored"]
    fn generate_realistic_workbook_artifact_for_manual_excel_verification() {
        let (build, operations, boundaries, inventory_basis, adjusted_prices) = realistic_fixture();
        let model = model_from(
            &build,
            operations,
            boundaries,
            inventory_basis,
            &adjusted_prices,
        );
        let bytes = generate(&model);
        let path = std::env::temp_dir().join("isk-works-verification-workbook-v4.1-realistic.xlsx");
        std::fs::write(&path, &bytes).expect("write manual verification artifact");
        eprintln!("wrote {} bytes to {}", bytes.len(), path.display());
    }
}
