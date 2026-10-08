use super::*;
use async_trait::async_trait;
use iskworks_sde::{ManufacturingRecipe, ReactionFormulaRecipe, SdeError};

/// A synthetic multi-level chain, kept independent of real SDE data
/// (unlike the reaction-parsing tests, nothing about a *specific* real
/// EVE recipe's numbers matters here -- only the shape of the
/// dependency graph does):
///
/// Root (blueprint 9000, product 9001) needs Component A (9101, built
/// by blueprint 9102) and Component B (9201, built by blueprint 9202).
/// Both A and B's recipes need shared Material M (9301) -- the netting
/// case. Root also directly needs M's sibling Material N (9302), which
/// is *also* needed two levels deeper via Component C (9401, built by
/// blueprint 9402, itself needed by A) -- the different-depths case.
struct FixtureRepository;

#[async_trait]
impl SdeReadRepository for FixtureRepository {
    async fn active_sde(&self) -> Result<Option<iskworks_sde::ActiveSde>, SdeError> {
        Ok(None)
    }

    async fn search_manufacturing_blueprints(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<iskworks_sde::BlueprintSearchResult>, SdeError> {
        Ok(Vec::new())
    }

    async fn search_types(
        &self,
        _query: &str,
        _limit: u32,
    ) -> Result<Vec<iskworks_sde::TypeSearchResult>, SdeError> {
        Ok(Vec::new())
    }

    async fn manufacturing_recipe(
        &self,
        blueprint_type_id: i64,
    ) -> Result<Option<ManufacturingRecipe>, SdeError> {
        let recipe = match blueprint_type_id {
            9_000 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Root Blueprint".to_string(),
                duration_seconds: Some(100),
                materials: vec![
                    line(9_101, "Component A", 2),
                    line(9_201, "Component B", 3),
                    line(9_302, "Material N", 5),
                ],
                products: vec![line(9_001, "Root Product", 1)],
            },
            9_102 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Component A Blueprint".to_string(),
                duration_seconds: Some(50),
                materials: vec![line(9_301, "Material M", 4), line(9_401, "Component C", 1)],
                products: vec![line(9_101, "Component A", 1)],
            },
            9_202 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Component B Blueprint".to_string(),
                duration_seconds: Some(50),
                materials: vec![line(9_301, "Material M", 6)],
                products: vec![line(9_201, "Component B", 1)],
            },
            9_402 => ManufacturingRecipe {
                blueprint_type_id,
                blueprint_name: "Component C Blueprint".to_string(),
                duration_seconds: Some(20),
                materials: vec![line(9_302, "Material N", 2)],
                products: vec![line(9_401, "Component C", 1)],
            },
            _ => return Ok(None),
        };
        Ok(Some(recipe))
    }

    async fn reaction_formula(
        &self,
        reaction_formula_type_id: i64,
    ) -> Result<Option<ReactionFormulaRecipe>, SdeError> {
        // Alternate reaction-formula route to Component B (9_201),
        // instead of manufacturing blueprint 9_202 -- same product,
        // same Material M(9_301) demand per run -- used only to prove
        // ME/TE never applies to a reaction-resolved component, by
        // comparison against the manufacturing route's own numbers.
        let recipe = match reaction_formula_type_id {
            9_500 => ReactionFormulaRecipe {
                reaction_formula_type_id,
                reaction_formula_name: "Component B Reaction Formula".to_string(),
                duration_seconds: Some(50),
                materials: vec![line(9_301, "Material M", 6)],
                products: vec![line(9_201, "Component B", 1)],
            },
            // A family of Component B reaction formulas with different
            // canonical output-per-run values, for the linked-run-sizing
            // invariant (`runs = ceil(required / output_per_run)`).
            9_601 => ReactionFormulaRecipe {
                reaction_formula_type_id,
                reaction_formula_name: "Component B Bulk Reaction Formula".to_string(),
                duration_seconds: Some(3_600),
                materials: vec![line(9_301, "Material M", 100)],
                products: vec![line(9_201, "Component B", 10_000)],
            },
            9_602 => ReactionFormulaRecipe {
                reaction_formula_type_id,
                reaction_formula_name: "Component B Small Reaction Formula".to_string(),
                duration_seconds: Some(1_200),
                materials: vec![line(9_301, "Material M", 5)],
                products: vec![line(9_201, "Component B", 200)],
            },
            9_603 => ReactionFormulaRecipe {
                reaction_formula_type_id,
                reaction_formula_name: "Component B Medium Reaction Formula".to_string(),
                duration_seconds: Some(2_400),
                materials: vec![line(9_301, "Material M", 40)],
                products: vec![line(9_201, "Component B", 3_000)],
            },
            9_604 => ReactionFormulaRecipe {
                reaction_formula_type_id,
                reaction_formula_name: "Component B Unit Reaction Formula".to_string(),
                duration_seconds: Some(600),
                materials: vec![line(9_301, "Material M", 6)],
                products: vec![line(9_201, "Component B", 1)],
            },
            _ => return Ok(None),
        };
        Ok(Some(recipe))
    }
}

fn line(type_id: i64, name: &str, quantity: i64) -> RecipeLine {
    RecipeLine {
        type_id,
        type_name: name.to_string(),
        quantity,
    }
}

fn manufacturing(blueprint_type_id: i64) -> RecipeSelection {
    RecipeSelection::Manufacturing { blueprint_type_id }
}

fn reaction(reaction_formula_type_id: i64) -> RecipeSelection {
    RecipeSelection::Reaction {
        reaction_formula_type_id,
    }
}

fn resolution(type_id: i64, blueprint_type_id: i64) -> ComponentResolution {
    ComponentResolution {
        type_id,
        recipe: manufacturing(blueprint_type_id),
        facility_override: None,
        blueprint_selection: None,
    }
}

#[test]
fn component_resolution_with_facility_override_round_trips_through_json() {
    let resolution = ComponentResolution {
        type_id: 9_101,
        recipe: manufacturing(9_102),
        facility_override: Some(ComponentFacilityOverride {
            facility_profile_id: crate::FacilityProfileId::new(),
        }),
        blueprint_selection: None,
    };

    let value = serde_json::to_value(resolution.clone()).expect("serializes");
    let round_tripped: ComponentResolution = serde_json::from_value(value).expect("deserializes");

    assert_eq!(round_tripped, resolution);
}

/// A component override persisted by an older client still carries an
/// `expectedProfileRevision` key. It must deserialize (the field is
/// simply ignored) so no Build-row migration is required.
#[test]
fn legacy_component_override_json_with_expected_profile_revision_still_loads() {
    let profile_id = crate::FacilityProfileId::new();
    let legacy = serde_json::json!({
        "typeId": 9_101,
        "recipe": { "mode": "manufacturing", "blueprintTypeId": 9_102 },
        "facilityOverride": {
            "facilityProfileId": profile_id,
            "expectedProfileRevision": 7
        },
        "blueprintSelection": null
    });
    let parsed: ComponentResolution =
        serde_json::from_value(legacy).expect("legacy override JSON deserializes");
    assert_eq!(
        parsed.facility_override,
        Some(ComponentFacilityOverride {
            facility_profile_id: profile_id,
        })
    );
}

fn find(expansion: &ComponentExpansion, type_id: i64) -> &ResolvedComponent {
    expansion
        .components
        .iter()
        .find(|component| component.type_id == type_id)
        .unwrap_or_else(|| panic!("component {type_id} not present in expansion"))
}

#[tokio::test]
async fn buy_only_baseline_matches_the_root_plan_unchanged() {
    let service = ComponentExpansionService::new(Arc::new(FixtureRepository));

    let expansion = service
        .expand(
            manufacturing(9_000),
            1,
            &[],
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .await
        .unwrap();

    assert_eq!(expansion.components.len(), 3);
    let a = find(&expansion, 9_101);
    assert_eq!(a.total_quantity, 2);
    assert_eq!(a.resolution, ComponentResolutionOutcome::Buy);
    let b = find(&expansion, 9_201);
    assert_eq!(b.total_quantity, 3);
    let n = find(&expansion, 9_302);
    assert_eq!(n.total_quantity, 5);
}

#[tokio::test]
async fn effective_root_requirements_are_preserved_during_expansion() {
    let service = ComponentExpansionService::new(Arc::new(FixtureRepository));
    let effective_requirements = BTreeMap::from([(9_101, 1), (9_201, 2), (9_302, 4)]);

    let expansion = service
        .expand_with_root_requirements(
            manufacturing(9_000),
            1,
            &[],
            &BTreeMap::new(),
            &BTreeMap::new(),
            &effective_requirements,
        )
        .await
        .unwrap();

    assert_eq!(find(&expansion, 9_101).total_quantity, 1);
    assert_eq!(find(&expansion, 9_201).total_quantity, 2);
    assert_eq!(find(&expansion, 9_302).total_quantity, 4);
}

#[tokio::test]
async fn single_level_build_resolution_computes_runs_and_surplus() {
    let service = ComponentExpansionService::new(Arc::new(FixtureRepository));

    let expansion = service
        .expand(
            manufacturing(9_000),
            1,
            &[resolution(9_201, 9_202)], // build Component B only
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .await
        .unwrap();

    let b = find(&expansion, 9_201);
    assert_eq!(b.total_quantity, 3);
    match b.resolution {
        ComponentResolutionOutcome::Build {
            runs,
            produced_quantity,
            surplus,
            ..
        } => {
            assert_eq!(runs, 3); // 1 unit per run, demand 3 -> 3 runs
            assert_eq!(produced_quantity, 3);
            assert_eq!(surplus, 0);
        }
        ComponentResolutionOutcome::Buy => panic!("expected Component B to be build-resolved"),
    }

    // B's own material (M) must not appear as a row -- single-level
    // worksheets never cascade a build-resolved row's own materials.
    assert!(expansion.components.iter().all(|c| c.type_id != 9_301));
}

#[tokio::test]
async fn missing_scope_sizes_runs_off_the_shortage_not_the_full_requirement() {
    let service = ComponentExpansionService::new(Arc::new(FixtureRepository));

    let mut available_quantities = BTreeMap::new();
    available_quantities.insert(9_201, 2); // 2 of the 3 required already on hand

    let expansion = service
        .expand(
            manufacturing(9_000),
            1,
            &[resolution(9_201, 9_202)],
            &BTreeMap::new(),
            &available_quantities,
        )
        .await
        .unwrap();

    let b = find(&expansion, 9_201);
    assert_eq!(b.total_quantity, 3); // demand is still reported in full
    match b.resolution {
        ComponentResolutionOutcome::Build {
            runs,
            produced_quantity,
            surplus,
            ..
        } => {
            assert_eq!(runs, 1); // only the shortage (3 - 2 = 1) needs building
            assert_eq!(produced_quantity, 1);
            assert_eq!(surplus, 0); // surplus is relative to the shortage, not the full demand
        }
        ComponentResolutionOutcome::Buy => panic!("expected Component B to be build-resolved"),
    }
}

#[tokio::test]
async fn missing_scope_fully_covered_by_inventory_stays_build_sized_to_the_full_demand() {
    // An explicit Build resolution is a commitment to manufacture the
    // component. When on-hand stock already covers the whole demand
    // there is no shortage to size against, so the job falls back to
    // the full demand -- it must NOT collapse to `Buy` (which would
    // make the resolution vanish from the graph / linked build) and it
    // must NOT be a degenerate zero-run build.
    let service = ComponentExpansionService::new(Arc::new(FixtureRepository));

    let mut available_quantities = BTreeMap::new();
    available_quantities.insert(9_201, 5); // more than the required 3

    let expansion = service
        .expand(
            manufacturing(9_000),
            1,
            &[resolution(9_201, 9_202)],
            &BTreeMap::new(),
            &available_quantities,
        )
        .await
        .unwrap();

    let b = find(&expansion, 9_201);
    assert_eq!(b.total_quantity, 3);
    match b.resolution {
        ComponentResolutionOutcome::Build {
            runs,
            produced_quantity,
            surplus,
            ..
        } => {
            assert_eq!(runs, 3); // full demand (3), not the zero shortage
            assert_eq!(produced_quantity, 3);
            assert_eq!(surplus, 0);
        }
        ComponentResolutionOutcome::Buy => {
            panic!("expected Component B to stay build-resolved")
        }
    }
}

#[tokio::test]
async fn expand_never_cascades_a_build_resolved_rows_own_materials_into_new_rows() {
    // Worksheets are single-level: resolving Component A
    // to Build must not pull its own materials (M, C) in as new rows.
    // That breakdown only ever lives on A's own linked build's own
    // worksheet.
    let service = ComponentExpansionService::new(Arc::new(FixtureRepository));

    let expansion = service
        .expand(
            manufacturing(9_000),
            1,
            &[resolution(9_101, 9_102)], // build Component A only
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .await
        .unwrap();

    assert_eq!(expansion.components.len(), 3); // A, B, N -- root's own materials, unchanged
    let a = find(&expansion, 9_101);
    assert_eq!(a.total_quantity, 2);
    match a.resolution {
        ComponentResolutionOutcome::Build {
            runs,
            produced_quantity,
            surplus,
            ..
        } => {
            assert_eq!(runs, 2); // 1 unit per run, demand 2 -> 2 runs
            assert_eq!(produced_quantity, 2);
            assert_eq!(surplus, 0);
        }
        ComponentResolutionOutcome::Buy => panic!("expected Component A to be build-resolved"),
    }
    assert!(
        expansion
            .components
            .iter()
            .all(|c| c.type_id != 9_301 && c.type_id != 9_401),
        "Component A's own materials (M, C) must not appear as rows: {:?}",
        expansion
            .components
            .iter()
            .map(|c| c.type_id)
            .collect::<Vec<_>>(),
    );
}

#[tokio::test]
async fn expand_resolves_multiple_root_materials_independently_with_no_shared_cascade() {
    let service = ComponentExpansionService::new(Arc::new(FixtureRepository));

    // A and B share a material (M) two levels deep, and C (needed only
    // via A) itself shares a material (N) with root. There is no
    // multi-level cascade: resolving A and B to Build must only ever produce rows for root's own direct
    // materials (A, B, N), never M or C, regardless of what either's
    // own recipe needs.
    let expansion = service
        .expand(
            manufacturing(9_000),
            1,
            &[
                resolution(9_101, 9_102), // build Component A
                resolution(9_201, 9_202), // build Component B
            ],
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .await
        .unwrap();

    assert_eq!(expansion.components.len(), 3); // A, B, N only
    let a = find(&expansion, 9_101);
    assert_eq!(a.total_quantity, 2);
    assert!(matches!(
        a.resolution,
        ComponentResolutionOutcome::Build { runs: 2, .. }
    ));
    let b = find(&expansion, 9_201);
    assert_eq!(b.total_quantity, 3);
    assert!(matches!(
        b.resolution,
        ComponentResolutionOutcome::Build { runs: 3, .. }
    ));
    let n = find(&expansion, 9_302);
    assert_eq!(n.total_quantity, 5);
    assert_eq!(n.resolution, ComponentResolutionOutcome::Buy);
    assert!(expansion
        .components
        .iter()
        .all(|c| c.type_id != 9_301 && c.type_id != 9_401));
}

#[tokio::test]
async fn unknown_recipe_selection_is_rejected() {
    let service = ComponentExpansionService::new(Arc::new(FixtureRepository));

    let error = service
        .expand(
            manufacturing(9_000),
            1,
            &[resolution(9_201, 42)], // blueprint 42 doesn't exist in the fixture
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ComponentExpansionError::RecipeNotFound));
}

#[tokio::test]
async fn invalid_root_runs_is_rejected() {
    let service = ComponentExpansionService::new(Arc::new(FixtureRepository));

    let error = service
        .expand(
            manufacturing(9_000),
            0,
            &[],
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ComponentExpansionError::InvalidRuns));
}

#[tokio::test]
async fn expand_reduces_duration_by_te_but_blueprint_me_no_longer_applies_to_anything() {
    let service = ComponentExpansionService::new(Arc::new(FixtureRepository));

    // Root runs 5, TE20 (te_factor=0.8) on A. ME10 is also set on A to
    // prove it's simply ignored -- with materials never cascaded
    // into new rows, there's nothing left for ME to reduce.
    let expansion = service
        .expand(
            manufacturing(9_000),
            5,
            &[resolution(9_101, 9_102)],
            &BTreeMap::from([(9_101, (10u8, 20u8))]),
            &BTreeMap::new(),
        )
        .await
        .unwrap();

    let a = find(&expansion, 9_101);
    assert_eq!(a.total_quantity, 10); // root demand 2*5=10
    match a.resolution {
        ComponentResolutionOutcome::Build {
            duration_seconds, ..
        } => {
            // A's blueprint duration is 50/run * 10 runs = 500 base;
            // ceil(500*0.8)=400.
            assert_eq!(duration_seconds, Some(400));
        }
        ComponentResolutionOutcome::Buy => panic!("expected Component A to be build-resolved"),
    }

    // A's own materials (M, C) never appear as rows.
    assert!(expansion
        .components
        .iter()
        .all(|c| c.type_id != 9_301 && c.type_id != 9_401));
}

#[tokio::test]
async fn expand_ignores_the_efficiencies_map_for_a_reaction_resolution() {
    let service = ComponentExpansionService::new(Arc::new(FixtureRepository));

    // Component B resolved via reaction formula 9_500 (not manufacturing
    // blueprint 9_202) -- same product, same demand per run. An entry
    // in the efficiencies map for B's own type_id must be completely
    // ignored, since reactions have no ME/TE concept.
    let reaction_resolution = ComponentResolution {
        type_id: 9_201,
        recipe: reaction(9_500),
        facility_override: None,
        blueprint_selection: None,
    };

    let with_populated_entry = service
        .expand(
            manufacturing(9_000),
            1,
            std::slice::from_ref(&reaction_resolution),
            &BTreeMap::from([(9_201, (10u8, 20u8))]),
            &BTreeMap::new(),
        )
        .await
        .unwrap();

    let b = find(&with_populated_entry, 9_201);
    assert_eq!(b.total_quantity, 3); // root's own demand, unreduced
    match b.resolution {
        ComponentResolutionOutcome::Build {
            duration_seconds, ..
        } => assert_eq!(duration_seconds, Some(150)), // 50/run * 3 runs, unreduced
        ComponentResolutionOutcome::Buy => panic!("expected Component B to be build-resolved"),
    }
}

/// Size a reaction-resolved Component B against an exact required
/// quantity (`effective_root_requirements` overrides root demand), for a
/// formula whose canonical output-per-run is `output_per_run`, and return
/// `(runs, produced_quantity, surplus)`.
async fn size_reaction(
    service: &ComponentExpansionService,
    reaction_formula_type_id: i64,
    required: u64,
) -> (u64, u64, u64) {
    let expansion = service
        .expand_with_root_requirements(
            manufacturing(9_000),
            1,
            &[ComponentResolution {
                type_id: 9_201,
                recipe: reaction(reaction_formula_type_id),
                facility_override: None,
                blueprint_selection: None,
            }],
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::from([(9_201, required)]),
        )
        .await
        .unwrap();
    let b = find(&expansion, 9_201);
    assert_eq!(
        b.total_quantity, required,
        "row Need must equal the requirement"
    );
    match b.resolution {
        ComponentResolutionOutcome::Build {
            runs,
            produced_quantity,
            surplus,
            ..
        } => (runs, produced_quantity, surplus),
        ComponentResolutionOutcome::Buy => panic!("expected Component B to be build-resolved"),
    }
}

#[tokio::test]
async fn reaction_linked_runs_are_minimal_ceil_division_of_the_requirement() {
    let service = ComponentExpansionService::new(Arc::new(FixtureRepository));

    // The real reproduction: 372,282 Tungsten Carbide required, a
    // reaction whose canonical output-per-run is 10,000.
    //   runs      = ceil(372_282 / 10_000)      = 38
    //   producing = 38 * 10_000                 = 380_000
    //   surplus   = 380_000 - 372_282           = 7_718
    assert_eq!(
        size_reaction(&service, 9_601, 372_282).await,
        (38, 380_000, 7_718)
    );

    // The invariant, across several output-per-run sizes and requirements:
    //   runs = ceil(required / opr); producing = runs * opr;
    //   producing >= required; 0 <= surplus < opr.
    for (formula, opr) in [
        (9_601_i64, 10_000_u64),
        (9_602, 200),
        (9_603, 3_000),
        (9_604, 1),
    ] {
        for required in [1_u64, opr - 1, opr, opr + 1, 2 * opr, 7 * opr + 3, 123_457] {
            if required == 0 {
                continue;
            }
            let (runs, producing, surplus) = size_reaction(&service, formula, required).await;
            assert_eq!(runs, required.div_ceil(opr), "runs = ceil(required / opr)");
            assert_eq!(producing, runs * opr, "producing = runs * opr");
            assert!(
                producing >= required,
                "a linked production build never makes less than required"
            );
            assert_eq!(surplus, producing - required);
            assert!(
                surplus < opr,
                "unavoidable surplus is always less than one run's output"
            );
        }
    }
}

#[tokio::test]
async fn reaction_linked_run_sizing_boundaries() {
    let service = ComponentExpansionService::new(Arc::new(FixtureRepository));
    // output-per-run 10,000 (formula 9_601).
    assert_eq!(size_reaction(&service, 9_601, 1).await.0, 1);
    assert_eq!(size_reaction(&service, 9_601, 10_000).await.0, 1);
    assert_eq!(size_reaction(&service, 9_601, 10_001).await.0, 2);
    assert_eq!(size_reaction(&service, 9_601, 20_000).await.0, 2);
}
