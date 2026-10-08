//! The catalog of profitability scopes: `ProfitabilityScopeId`, the SDE
//! category/group/meta-group/market-group predicate each maps to, and the
//! lookup functions. Pure domain policy -- no I/O.

use std::collections::BTreeSet;

use iskworks_sde::{CandidateRecipeKind, ManufacturableCandidateScope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum ProfitabilityScopeId {
    #[serde(rename = "t1-frigates")]
    T1Frigates,
    #[serde(rename = "t1-destroyers")]
    T1Destroyers,
    #[serde(rename = "t1-cruisers")]
    T1Cruisers,
    #[serde(rename = "t1-battlecruisers")]
    T1Battlecruisers,
    #[serde(rename = "t1-battleships")]
    T1Battleships,
    #[serde(rename = "t1-industrial-ships")]
    T1IndustrialShips,
    #[serde(rename = "t1-rigs")]
    T1Rigs,
    #[serde(rename = "reactions")]
    Reactions,
}

impl ProfitabilityScopeId {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::T1Frigates => "t1-frigates",
            Self::T1Destroyers => "t1-destroyers",
            Self::T1Cruisers => "t1-cruisers",
            Self::T1Battlecruisers => "t1-battlecruisers",
            Self::T1Battleships => "t1-battleships",
            Self::T1IndustrialShips => "t1-industrial-ships",
            Self::T1Rigs => "t1-rigs",
            Self::Reactions => "reactions",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitabilityScopeDefinition {
    pub id: ProfitabilityScopeId,
    pub label: String,
    pub family: String,
    pub description: String,
    pub limitations: Vec<String>,
    pub recipe_kind: CandidateRecipeKind,
    #[serde(skip)]
    pub candidate_scope: ManufacturableCandidateScope,
}

/// A Tech I hull scope cleanly classified by SDE category/group + the Tech I
/// meta-group tag -- the pattern that already worked for Frigates/Battleships
/// and, verified against the real local SDE, also holds cleanly for
/// Destroyers/Cruisers/Industrial Ships (each group's Tech I members are all
/// explicitly `meta_group_id = 1` tagged, with Faction variants sharing the
/// group but a different meta-group).
fn t1_hull_scope(
    id: ProfitabilityScopeId,
    label: &str,
    group_id: i64,
) -> ProfitabilityScopeDefinition {
    t1_hull_scope_multi_group(id, label, BTreeSet::from([group_id]), BTreeSet::from([1]))
}

/// The general form behind `t1_hull_scope`: lets a scope span more than one
/// SDE group and choose whether the Tech I meta-group tag is the filtering
/// signal at all. Needed for Battlecruisers -- see the doc comment on
/// `t1_battlecruisers_scope` for why an empty `meta_group_ids` (paired with
/// `market_group_root_ids`) is the correct choice there, not a workaround.
fn t1_hull_scope_multi_group(
    id: ProfitabilityScopeId,
    label: &str,
    group_ids: BTreeSet<i64>,
    meta_group_ids: BTreeSet<i64>,
) -> ProfitabilityScopeDefinition {
    ProfitabilityScopeDefinition {
        id,
        label: label.to_string(),
        family: "Ships".to_string(),
        description: format!(
            "Published Tech I {label} with all immediate materials purchased from sell orders."
        ),
        limitations: vec![
            "Strict Tech I products only; faction, pirate, special-edition, and Tech II hulls are excluded."
                .to_string(),
            "One manufacturing run with no inventory reuse or recursive component builds."
                .to_string(),
        ],
        recipe_kind: CandidateRecipeKind::Manufacturing,
        candidate_scope: ManufacturableCandidateScope {
            recipe_kinds: BTreeSet::from([CandidateRecipeKind::Manufacturing]),
            category_ids: BTreeSet::from([6]),
            group_ids,
            meta_group_ids,
            market_group_root_ids: BTreeSet::new(),
        },
    }
}

/// Tech I Battlecruisers span two SDE groups -- Combat Battlecruiser (419,
/// whose Tech I members are cleanly `meta_group_id = 1` tagged, same as every
/// other hull scope) and Attack Battlecruiser (1201: Oracle/Naga/Talos/
/// Tornado), whose members carry **no** `meta_group_id` at all in the active
/// SDE snapshot -- verified directly against the local database, not
/// assumed. A single `meta_group_ids` filter can't apply differently per
/// group, and defaulting untagged rows to Tech I would be exactly the kind
/// of heuristic this catalog avoids elsewhere.
///
/// Real fix, not a workaround: every genuine Tech I Battlecruiser across
/// *both* groups sits under market group 469 ("Standard Battlecruisers"),
/// while every Faction/Navy/Pirate/Special-Edition variant sits under a
/// disjoint market-group branch (Navy Faction 1704, Pirate Faction 3534,
/// Special Edition 1698) -- confirmed against the real local SDE. So this
/// scope filters by `market_group_root_ids` instead of `meta_group_ids`
/// (left empty/unrestricted), reusing the exact mechanism the Rigs scope and
/// the special-edition eligibility exclusion already rely on.
const T1_BATTLECRUISER_GROUP_IDS: [i64; 2] = [419, 1201];
const STANDARD_BATTLECRUISERS_MARKET_GROUP_ID: i64 = 469;

fn t1_battlecruisers_scope() -> ProfitabilityScopeDefinition {
    let mut scope = t1_hull_scope_multi_group(
        ProfitabilityScopeId::T1Battlecruisers,
        "T1 Battlecruisers",
        BTreeSet::from(T1_BATTLECRUISER_GROUP_IDS),
        BTreeSet::new(),
    );
    scope.candidate_scope.market_group_root_ids =
        BTreeSet::from([STANDARD_BATTLECRUISERS_MARKET_GROUP_ID]);
    scope
}

/// Tech I ship rigs: not a `Ships` category product at all (rigs are
/// Category 7 "Module"), so hull-style category/group filtering doesn't
/// apply. Cleanly isolated instead by market-group ancestry -- verified
/// against the local SDE that real Tech I rig items (e.g. Small Trimark
/// Armor Pump I) trace to market group 1111 ("Rigs"), and that group 1111's
/// Tech II siblings are excluded by the ordinary `meta_group_id = 1` filter,
/// same as every hull scope.
const RIGS_MARKET_GROUP_ID: i64 = 1111;

fn t1_rigs_scope() -> ProfitabilityScopeDefinition {
    ProfitabilityScopeDefinition {
        id: ProfitabilityScopeId::T1Rigs,
        label: "T1 Rigs".to_string(),
        family: "Rigs".to_string(),
        description:
            "Published Tech I ship rigs with all immediate materials purchased from sell orders."
                .to_string(),
        limitations: vec![
            "Strict Tech I rigs only; Tech II rigs are excluded.".to_string(),
            "One manufacturing run with no inventory reuse or recursive component builds."
                .to_string(),
        ],
        recipe_kind: CandidateRecipeKind::Manufacturing,
        candidate_scope: ManufacturableCandidateScope {
            recipe_kinds: BTreeSet::from([CandidateRecipeKind::Manufacturing]),
            category_ids: BTreeSet::from([7]),
            group_ids: BTreeSet::new(),
            meta_group_ids: BTreeSet::from([1]),
            market_group_root_ids: BTreeSet::from([RIGS_MARKET_GROUP_ID]),
        },
    }
}

/// Reaction formulas have no Tech-tier concept in EVE at all (verified: none
/// of the 119 published formulas in the local SDE carry a `meta_group_id`),
/// so this scope has no meta-group filter -- `recipe_kinds` alone (Reaction
/// rows only, from the storage layer's manufacturing/reaction UNION) plus
/// Category 4 "Material" on the product is enough, matching the real data
/// exactly.
fn reactions_scope() -> ProfitabilityScopeDefinition {
    ProfitabilityScopeDefinition {
        id: ProfitabilityScopeId::Reactions,
        label: "Reactions".to_string(),
        family: "Industry".to_string(),
        description:
            "Published reaction formulas with all immediate inputs purchased from sell orders."
                .to_string(),
        limitations: vec![
            "One reaction run at the configured reaction facility.".to_string(),
            "No inventory reuse or recursive component builds.".to_string(),
        ],
        recipe_kind: CandidateRecipeKind::Reaction,
        candidate_scope: ManufacturableCandidateScope {
            recipe_kinds: BTreeSet::from([CandidateRecipeKind::Reaction]),
            category_ids: BTreeSet::from([4]),
            group_ids: BTreeSet::new(),
            meta_group_ids: BTreeSet::new(),
            market_group_root_ids: BTreeSet::new(),
        },
    }
}

#[must_use]
pub fn supported_profitability_scopes() -> Vec<ProfitabilityScopeDefinition> {
    vec![
        t1_hull_scope(ProfitabilityScopeId::T1Frigates, "T1 Frigates", 25),
        t1_hull_scope(ProfitabilityScopeId::T1Destroyers, "T1 Destroyers", 420),
        t1_hull_scope(ProfitabilityScopeId::T1Cruisers, "T1 Cruisers", 26),
        t1_battlecruisers_scope(),
        t1_hull_scope(ProfitabilityScopeId::T1Battleships, "T1 Battleships", 27),
        t1_hull_scope(
            ProfitabilityScopeId::T1IndustrialShips,
            "T1 Industrial Ships",
            28,
        ),
        t1_rigs_scope(),
        reactions_scope(),
    ]
}

#[must_use]
pub fn profitability_scope(id: ProfitabilityScopeId) -> Option<ProfitabilityScopeDefinition> {
    supported_profitability_scopes()
        .into_iter()
        .find(|scope| scope.id == id)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn catalog_maps_t1_hull_scopes_to_authoritative_sde_predicates() {
        let frigates = profitability_scope(ProfitabilityScopeId::T1Frigates).unwrap();
        assert_eq!(frigates.id.as_str(), "t1-frigates");
        assert_eq!(
            frigates.candidate_scope.recipe_kinds,
            BTreeSet::from([CandidateRecipeKind::Manufacturing])
        );
        assert_eq!(frigates.candidate_scope.category_ids, BTreeSet::from([6]));
        assert_eq!(frigates.candidate_scope.group_ids, BTreeSet::from([25]));
        assert_eq!(frigates.candidate_scope.meta_group_ids, BTreeSet::from([1]));

        let battleships = profitability_scope(ProfitabilityScopeId::T1Battleships).unwrap();
        assert_eq!(battleships.id.as_str(), "t1-battleships");
        assert_eq!(
            battleships.candidate_scope.category_ids,
            BTreeSet::from([6])
        );
        assert_eq!(battleships.candidate_scope.group_ids, BTreeSet::from([27]));
        assert_eq!(
            battleships.candidate_scope.meta_group_ids,
            BTreeSet::from([1])
        );

        // Same predicate shape for the three newly added single-group hull
        // scopes -- verified against the real local SDE that each group's
        // Tech I members are cleanly `meta_group_id = 1` tagged (Faction
        // variants share the group but a different meta-group, same pattern
        // as Frigates/Battleships above).
        for (id, group_id) in [
            (ProfitabilityScopeId::T1Destroyers, 420),
            (ProfitabilityScopeId::T1Cruisers, 26),
            (ProfitabilityScopeId::T1IndustrialShips, 28),
        ] {
            let hull_scope = profitability_scope(id).unwrap();
            assert_eq!(hull_scope.family, "Ships");
            assert_eq!(hull_scope.recipe_kind, CandidateRecipeKind::Manufacturing);
            assert_eq!(hull_scope.candidate_scope.category_ids, BTreeSet::from([6]));
            assert_eq!(
                hull_scope.candidate_scope.group_ids,
                BTreeSet::from([group_id])
            );
            assert_eq!(
                hull_scope.candidate_scope.meta_group_ids,
                BTreeSet::from([1])
            );
            assert!(hull_scope.candidate_scope.market_group_root_ids.is_empty());
        }
    }

    #[test]
    fn scope_ids_use_stable_kebab_case_json_values() {
        assert_eq!(
            serde_json::to_string(&ProfitabilityScopeId::T1Frigates).unwrap(),
            "\"t1-frigates\""
        );
        assert_eq!(
            serde_json::to_string(&ProfitabilityScopeId::T1Battlecruisers).unwrap(),
            "\"t1-battlecruisers\""
        );
        assert_eq!(
            serde_json::to_string(&ProfitabilityScopeId::T1IndustrialShips).unwrap(),
            "\"t1-industrial-ships\""
        );
        assert_eq!(
            serde_json::to_string(&ProfitabilityScopeId::T1Rigs).unwrap(),
            "\"t1-rigs\""
        );
        assert_eq!(
            serde_json::to_string(&ProfitabilityScopeId::Reactions).unwrap(),
            "\"reactions\""
        );
        assert!(serde_json::from_str::<ProfitabilityScopeId>("\"t2-frigates\"").is_err());
    }

    #[test]
    fn catalog_exposes_the_expanded_scope_set() {
        let scopes = supported_profitability_scopes();
        assert_eq!(
            scopes.iter().map(|scope| scope.id).collect::<Vec<_>>(),
            vec![
                ProfitabilityScopeId::T1Frigates,
                ProfitabilityScopeId::T1Destroyers,
                ProfitabilityScopeId::T1Cruisers,
                ProfitabilityScopeId::T1Battlecruisers,
                ProfitabilityScopeId::T1Battleships,
                ProfitabilityScopeId::T1IndustrialShips,
                ProfitabilityScopeId::T1Rigs,
                ProfitabilityScopeId::Reactions,
            ]
        );
        // Every scope IDs itself uniquely -- no duplicate ids ever slip in.
        assert_eq!(
            scopes
                .iter()
                .map(|scope| scope.id)
                .collect::<BTreeSet<_>>()
                .len(),
            scopes.len()
        );

        let ships = scopes
            .iter()
            .filter(|scope| scope.family == "Ships")
            .collect::<Vec<_>>();
        assert_eq!(ships.len(), 6);
        assert!(ships
            .iter()
            .all(|scope| scope.recipe_kind == CandidateRecipeKind::Manufacturing));
        assert!(ships
            .iter()
            .all(|scope| scope.limitations.iter().any(|item| item.contains("Tech I"))));

        let rigs = profitability_scope(ProfitabilityScopeId::T1Rigs).unwrap();
        assert_eq!(rigs.family, "Rigs");
        assert_eq!(rigs.recipe_kind, CandidateRecipeKind::Manufacturing);

        let reactions = profitability_scope(ProfitabilityScopeId::Reactions).unwrap();
        assert_eq!(reactions.family, "Industry");
        assert_eq!(reactions.recipe_kind, CandidateRecipeKind::Reaction);
        assert_eq!(
            reactions.candidate_scope.recipe_kinds,
            BTreeSet::from([CandidateRecipeKind::Reaction])
        );
    }

    #[test]
    fn t1_battlecruisers_spans_both_sde_groups_via_market_group_root_not_meta_group() {
        let scope = profitability_scope(ProfitabilityScopeId::T1Battlecruisers).unwrap();
        assert_eq!(scope.candidate_scope.group_ids, BTreeSet::from([419, 1201]));
        // Deliberately unrestricted: Attack Battlecruiser (1201) hulls carry
        // no meta_group_id at all in the real SDE -- see the doc comment on
        // `t1_battlecruisers_scope`. Purity instead comes from the market
        // group root filter below.
        assert!(scope.candidate_scope.meta_group_ids.is_empty());
        assert_eq!(
            scope.candidate_scope.market_group_root_ids,
            BTreeSet::from([STANDARD_BATTLECRUISERS_MARKET_GROUP_ID])
        );
    }

    #[test]
    fn t1_rigs_is_classified_by_category_and_market_group_root_not_ship_groups() {
        let scope = profitability_scope(ProfitabilityScopeId::T1Rigs).unwrap();
        assert_eq!(scope.candidate_scope.category_ids, BTreeSet::from([7]));
        assert!(scope.candidate_scope.group_ids.is_empty());
        assert_eq!(scope.candidate_scope.meta_group_ids, BTreeSet::from([1]));
        assert_eq!(
            scope.candidate_scope.market_group_root_ids,
            BTreeSet::from([RIGS_MARKET_GROUP_ID])
        );
    }
}
