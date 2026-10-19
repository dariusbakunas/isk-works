use std::collections::BTreeMap;
use std::str::FromStr;

use rust_decimal::Decimal;
use uuid::Uuid;

use super::*;
use crate::facility::tests_common::{
    fixture_profile, group_scoped_rig, product_in, unrestricted_rig,
};
use crate::facility::RigApplicability;

fn filter(id: i64, role: FacilityRole, categories: &[i64], groups: &[i64]) -> TargetFilter {
    TargetFilter {
        filter_id: id,
        role,
        name: format!("Filter {id}"),
        category_ids: categories.iter().copied().collect(),
        group_ids: groups.iter().copied().collect(),
    }
}

fn sde_filters() -> Vec<TargetFilter> {
    use FacilityRole::*;
    vec![
        filter(3, Manufacturing, &[6, 32], &[]),
        filter(5, Manufacturing, &[], &[25, 31, 420]),
        filter(7, Manufacturing, &[], &[26, 4902]),
        filter(8, Manufacturing, &[32], &[358, 4902]),
        filter(16, Reaction, &[], &[974]),
    ]
}

fn facility(n: u128) -> FacilityProfileId {
    FacilityProfileId(Uuid::from_u128(n))
}

fn default_for(role: FacilityRole, target: Option<i64>, n: u128) -> FacilityDefault {
    FacilityDefault {
        role,
        target_filter_id: target,
        facility_profile_id: facility(n),
        rig_covered_at_assignment: true,
    }
}

fn ids(targets: Vec<&TargetFilter>) -> Vec<i64> {
    targets.into_iter().map(|f| f.filter_id).collect()
}

#[test]
fn a_group_match_is_more_specific_than_a_category_match() {
    let filters = sde_filters();
    let frigate = product_in(Some(6), Some(25));
    assert_eq!(
        ids(matching_targets(
            FacilityRole::Manufacturing,
            frigate,
            &filters
        )),
        vec![5, 3]
    );
}

#[test]
fn among_category_matches_fewer_categories_win() {
    let filters = sde_filters();
    let subsystem = product_in(Some(32), Some(954));
    assert_eq!(
        ids(matching_targets(
            FacilityRole::Manufacturing,
            subsystem,
            &filters
        )),
        vec![8, 3]
    );
}

#[test]
fn equally_specific_group_matches_fall_back_to_the_lower_filter_id() {
    let filters = sde_filters();
    let expedition_command = product_in(Some(6), Some(4902));
    assert_eq!(
        ids(matching_targets(
            FacilityRole::Manufacturing,
            expedition_command,
            &filters
        )),
        vec![7, 8, 3]
    );
}

#[test]
fn matching_ignores_the_other_roles_filters() {
    let filters = sde_filters();
    let hybrid = product_in(Some(4), Some(974));
    assert!(matching_targets(FacilityRole::Manufacturing, hybrid, &filters).is_empty());
    assert_eq!(
        ids(matching_targets(FacilityRole::Reaction, hybrid, &filters)),
        vec![16]
    );
}

#[test]
fn resolve_skips_targets_without_a_default_and_then_uses_the_fallback() {
    use FacilityRole::*;
    let filters = sde_filters();
    let frigate = product_in(Some(6), Some(25));
    let defaults = vec![
        default_for(Manufacturing, Some(3), 1),
        default_for(Manufacturing, None, 2),
    ];
    assert_eq!(
        resolve_default(Manufacturing, frigate, &filters, &defaults),
        Some(ResolvedDefault {
            facility_profile_id: facility(1),
            source: DefaultSource::Target(3)
        })
    );
    let module = product_in(Some(7), Some(53));
    assert_eq!(
        resolve_default(Manufacturing, module, &filters, &defaults),
        Some(ResolvedDefault {
            facility_profile_id: facility(2),
            source: DefaultSource::Fallback
        })
    );
}

#[test]
fn resolve_prefers_the_most_specific_default() {
    use FacilityRole::*;
    let filters = sde_filters();
    let defaults = vec![
        default_for(Manufacturing, Some(3), 1),
        default_for(Manufacturing, Some(5), 2),
    ];
    let frigate = product_in(Some(6), Some(25));
    assert_eq!(
        resolve_default(Manufacturing, frigate, &filters, &defaults).map(|r| r.facility_profile_id),
        Some(facility(2))
    );
}

#[test]
fn an_unknown_product_resolves_to_the_fallback_only() {
    use FacilityRole::*;
    let filters = sde_filters();
    let defaults = vec![default_for(Manufacturing, Some(3), 1)];
    assert_eq!(
        resolve_default(Manufacturing, product_in(None, None), &filters, &defaults),
        None
    );
    let defaults = vec![
        default_for(Manufacturing, Some(3), 1),
        default_for(Manufacturing, None, 2),
    ];
    assert_eq!(
        resolve_default(Manufacturing, product_in(None, None), &filters, &defaults)
            .map(|r| r.source),
        Some(DefaultSource::Fallback)
    );
}

#[test]
fn resolve_never_crosses_roles() {
    let filters = sde_filters();
    let defaults = vec![default_for(FacilityRole::Reaction, None, 9)];
    assert_eq!(
        resolve_default(
            FacilityRole::Manufacturing,
            product_in(Some(6), Some(25)),
            &filters,
            &defaults
        ),
        None
    );
}

#[test]
fn probes_pair_each_group_with_its_category_and_list_bare_categories() {
    let groups = BTreeMap::from([(25, 6), (31, 6)]);
    let probes = target_probes(
        &filter(1, FacilityRole::Manufacturing, &[32], &[25, 31]),
        &groups,
    );
    assert_eq!(
        probes,
        vec![
            product_in(Some(6), Some(25)),
            product_in(Some(6), Some(31)),
            product_in(Some(32), None)
        ]
    );
}

fn category_scoped_rig(slot: u8, material: &str, time: &str, categories: &[i64]) -> FacilityRig {
    let filter = RigTargetFilter::Restricted {
        category_ids: categories.iter().copied().collect(),
        group_ids: Default::default(),
    };
    FacilityRig {
        applicability: RigApplicability {
            material: filter.clone(),
            time: filter,
        },
        ..unrestricted_rig(slot, 100 + i64::from(slot), "Category rig", material, time)
    }
}

fn dec(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}

fn small_t1_probes() -> Vec<ProductClassification> {
    let groups = BTreeMap::from([(25, 6), (31, 6), (420, 6)]);
    target_probes(
        &filter(5, FacilityRole::Manufacturing, &[], &[25, 31, 420]),
        &groups,
    )
}

fn ships_probes() -> Vec<ProductClassification> {
    target_probes(
        &filter(3, FacilityRole::Manufacturing, &[6, 32], &[]),
        &BTreeMap::new(),
    )
}

#[test]
fn a_category_rig_covers_a_group_target_inside_its_category() {
    let mut profile = fixture_profile("1", "15");
    profile.rigs = vec![category_scoped_rig(1, "2", "20", &[6, 32])];
    let bonus = facility_target_bonus(&profile, Some(&small_t1_probes())).unwrap();
    assert!(bonus.covered);
    // 1 - 0.99 * 0.98 = 2.98%; 1 - 0.85 * 0.80 = 32%
    assert_eq!(bonus.material_bonus_percent, dec("2.98"));
    assert_eq!(bonus.time_bonus_percent, dec("32"));
}

#[test]
fn a_rig_covering_only_part_of_a_target_is_left_out() {
    let mut profile = fixture_profile("1", "15");
    profile.rigs = vec![group_scoped_rig(
        1,
        200,
        "Small rig",
        "2",
        "20",
        [25, 31, 420],
    )];
    let bonus = facility_target_bonus(&profile, Some(&ships_probes())).unwrap();
    assert!(!bonus.covered);
    assert_eq!(bonus.material_bonus_percent, dec("1"));
    assert_eq!(bonus.time_bonus_percent, dec("15"));
}

#[test]
fn the_fallback_bonus_counts_only_unrestricted_rigs() {
    let mut profile = fixture_profile("1", "15");
    profile.rigs = vec![
        group_scoped_rig(1, 200, "Small rig", "2", "20", [25]),
        unrestricted_rig(2, 201, "Generic rig", "1", "0"),
    ];
    let bonus = facility_target_bonus(&profile, None).unwrap();
    assert!(!bonus.covered);
    assert_eq!(bonus.material_bonus_percent, dec("1.99"));
    assert_eq!(bonus.time_bonus_percent, dec("15"));
}

#[test]
fn reaction_bonuses_have_no_structure_base() {
    let mut profile = fixture_profile("1", "15");
    profile.role = FacilityRole::Reaction;
    profile.rigs = vec![group_scoped_rig(1, 300, "Hybrid rig", "2.4", "24", [974])];
    let probes = target_probes(
        &filter(16, FacilityRole::Reaction, &[], &[974]),
        &BTreeMap::from([(974, 4)]),
    );
    let bonus = facility_target_bonus(&profile, Some(&probes)).unwrap();
    assert!(bonus.covered);
    assert_eq!(bonus.material_bonus_percent, dec("2.4"));
    assert_eq!(bonus.time_bonus_percent, dec("24"));
}

fn named(
    name: &str,
    n: u128,
    material: &str,
    time: &str,
    tax: &str,
    rigs: Vec<FacilityRig>,
) -> IndustryFacilityProfile {
    let mut profile = fixture_profile(material, time);
    profile.id = facility(n);
    profile.name = name.into();
    profile.facility_tax_percent = dec(tax);
    profile.rigs = rigs;
    profile
}

fn small_rig(material: &str, time: &str) -> FacilityRig {
    group_scoped_rig(1, 200, "Small rig", material, time, [25, 31, 420])
}

#[test]
fn suggestion_needs_a_covering_rig_and_ranks_material_then_time_then_tax_then_name() {
    let probes = small_t1_probes();
    let unrigged = named("Unrigged", 1, "5", "50", "0", vec![]);
    let assert_best = |profiles: Vec<IndustryFacilityProfile>, expected: u128| {
        let suggestion =
            suggest_facility(&profiles, FacilityRole::Manufacturing, Some(&probes)).unwrap();
        assert_eq!(suggestion.map(|(p, _)| p.id), Some(facility(expected)));
    };
    assert_best(
        vec![
            unrigged.clone(),
            named("A", 2, "1", "15", "1", vec![small_rig("2", "20")]),
        ],
        2,
    );
    assert_best(
        vec![
            named("A", 2, "1", "15", "1", vec![small_rig("2", "20")]),
            named("B", 3, "1", "15", "1", vec![small_rig("2.4", "20")]),
        ],
        3,
    );
    assert_best(
        vec![
            named("A", 2, "1", "15", "1", vec![small_rig("2", "20")]),
            named("B", 3, "1", "15", "1", vec![small_rig("2", "24")]),
        ],
        3,
    );
    assert_best(
        vec![
            named("A", 2, "1", "15", "1", vec![small_rig("2", "20")]),
            named("B", 3, "1", "15", "0.5", vec![small_rig("2", "20")]),
        ],
        3,
    );
    assert_best(
        vec![
            named("B", 3, "1", "15", "1", vec![small_rig("2", "20")]),
            named("A", 2, "1", "15", "1", vec![small_rig("2", "20")]),
        ],
        2,
    );
    let only_unrigged = [unrigged];
    let none =
        suggest_facility(&only_unrigged, FacilityRole::Manufacturing, Some(&probes)).unwrap();
    assert!(none.is_none());
}

#[test]
fn suggestion_skips_archived_and_other_role_profiles() {
    let probes = small_t1_probes();
    let mut archived = named("Archived", 1, "1", "15", "0", vec![small_rig("9", "9")]);
    archived.archived_at = Some(chrono::Utc::now());
    let mut reaction = named("Reaction", 2, "1", "15", "0", vec![small_rig("9", "9")]);
    reaction.role = FacilityRole::Reaction;
    let profiles = vec![
        archived,
        reaction,
        named("Live", 3, "1", "15", "1", vec![small_rig("2", "20")]),
    ];
    let suggestion =
        suggest_facility(&profiles, FacilityRole::Manufacturing, Some(&probes)).unwrap();
    assert_eq!(suggestion.map(|(p, _)| p.id), Some(facility(3)));
}

#[test]
fn fallback_suggestion_ranks_base_bonuses_without_needing_coverage() {
    let profiles = vec![
        named("Raitaru", 1, "1", "15", "1", vec![]),
        named("Sotiyo", 2, "1", "30", "1", vec![]),
    ];
    let suggestion = suggest_facility(&profiles, FacilityRole::Manufacturing, None).unwrap();
    assert_eq!(suggestion.map(|(p, _)| p.id), Some(facility(2)));
}

#[test]
fn status_warns_only_when_coverage_present_at_assignment_is_lost() {
    let mut default = default_for(FacilityRole::Manufacturing, Some(5), 1);
    assert_eq!(
        default_status(Some(&default), true, false),
        DefaultStatus::Set
    );
    assert_eq!(
        default_status(Some(&default), false, false),
        DefaultStatus::CoverageLost
    );
    default.rig_covered_at_assignment = false;
    assert_eq!(
        default_status(Some(&default), false, false),
        DefaultStatus::Set
    );
    let fallback = default_for(FacilityRole::Manufacturing, None, 1);
    assert_eq!(
        default_status(Some(&fallback), false, false),
        DefaultStatus::Set
    );
    assert_eq!(default_status(None, false, true), DefaultStatus::Fallback);
    assert_eq!(default_status(None, false, false), DefaultStatus::None);
}
