//! Industry-rig applicability policy: which produced items a fitted rig's
//! material and time bonuses are allowed to affect, and the helpers every
//! facility preview funnels rig selection through.
//!
//! Semantics (unchanged): an `Unrestricted` filter always applies; a
//! `Restricted` filter applies only when the product's SDE category or group
//! is listed; an unknown product classification never matches a `Restricted`
//! filter; material and time applicability are evaluated independently.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::types::FacilityRig;

/// The broad EVE classification of a job's produced item, as resolved from
/// the active SDE (`invGroups` group + its `invCategories` category). This is
/// the only thing an industry rig's applicability filter is matched against
/// -- EVE's `industryTargetFilters` are pure `(categoryID, groupID)` sets.
/// `None`/`None` means "unknown" (SDE not imported, or the product absent
/// from it): a `Restricted` rig contributes nothing in that case rather than
/// risk over-applying.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct ProductClassification {
    pub category_id: Option<i64>,
    pub group_id: Option<i64>,
}

/// Which produced items a single rig bonus (material *or* time, independently)
/// is allowed to affect. Mirrors one row of EVE's `industryTargetFilters`:
/// `Unrestricted` is a modifier-source entry with no `filterID` (a structure
/// base bonus, or a truly generic rig) -- and the compatibility default for
/// facility profiles saved before rig applicability was modeled.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum RigTargetFilter {
    #[default]
    Unrestricted,
    #[serde(rename_all = "camelCase")]
    Restricted {
        #[serde(default)]
        category_ids: BTreeSet<i64>,
        #[serde(default)]
        group_ids: BTreeSet<i64>,
    },
}

/// A fitted rig's per-activity applicability -- EVE lets a rig carry a
/// different `filterID` on its material bonus than on its time bonus, so the
/// two are tracked separately rather than assuming one filter per rig.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RigApplicability {
    pub material: RigTargetFilter,
    pub time: RigTargetFilter,
}

impl From<Option<iskworks_sde::IndustryTargetFilter>> for RigTargetFilter {
    /// `None` (no `filterID` in the SDE) is unrestricted; a resolved filter
    /// becomes its category/group sets verbatim.
    fn from(value: Option<iskworks_sde::IndustryTargetFilter>) -> Self {
        match value {
            None => Self::Unrestricted,
            Some(filter) => Self::Restricted {
                category_ids: filter.category_ids.into_iter().collect(),
                group_ids: filter.group_ids.into_iter().collect(),
            },
        }
    }
}

impl From<iskworks_sde::RigApplicability> for RigApplicability {
    fn from(value: iskworks_sde::RigApplicability) -> Self {
        Self {
            material: value.material.into(),
            time: value.time.into(),
        }
    }
}

/// Does `filter` admit a job producing an item classified as `product`?
/// `Unrestricted` always does; `Restricted` requires the product's category
/// or group to be listed, and an unknown classification never matches a
/// `Restricted` filter.
fn rig_target_applies(filter: &RigTargetFilter, product: ProductClassification) -> bool {
    match filter {
        RigTargetFilter::Unrestricted => true,
        RigTargetFilter::Restricted {
            category_ids,
            group_ids,
        } => {
            product
                .category_id
                .is_some_and(|id| category_ids.contains(&id))
                || product.group_id.is_some_and(|id| group_ids.contains(&id))
        }
    }
}

/// The subset of `rigs` whose bonus (selected per-activity by `filter_of`)
/// applies to a job producing `product`. The single chokepoint every
/// facility preview funnels through so no caller can accidentally fold in
/// every installed rig.
pub(crate) fn applicable_rigs(
    rigs: &[FacilityRig],
    product: ProductClassification,
    filter_of: impl Fn(&FacilityRig) -> &RigTargetFilter,
) -> Vec<&FacilityRig> {
    rigs.iter()
        .filter(|rig| rig_target_applies(filter_of(rig), product))
        .collect()
}

/// The `DurationCalculationStep`/trace label for the rig bonuses that were
/// folded in, naming any fitted rig that was skipped as non-applicable so
/// the worksheet can explain why its bonus didn't land. Reduces to the plain
/// comma-joined rig list when every fitted rig applied.
pub(crate) fn rig_bonus_detail(applied: &[&FacilityRig], all: &[FacilityRig]) -> String {
    let applied_names = applied
        .iter()
        .map(|rig| rig.type_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let skipped = all
        .iter()
        .filter(|rig| {
            !applied
                .iter()
                .any(|kept| kept.slot_number == rig.slot_number)
        })
        .map(|rig| rig.type_name.as_str())
        .collect::<Vec<_>>();
    if skipped.is_empty() {
        applied_names
    } else if applied_names.is_empty() {
        format!("not applicable to this product: {}", skipped.join(", "))
    } else {
        format!(
            "{applied_names} (not applicable to this product: {})",
            skipped.join(", ")
        )
    }
}

/// A worksheet warning naming every fitted rig that contributes *nothing* to
/// this job -- neither its material nor its time bonus applies to the
/// product. `None` when every rig lands at least one bonus.
pub(crate) fn skipped_rig_warning(
    rigs: &[FacilityRig],
    product: ProductClassification,
) -> Option<String> {
    let skipped = rigs
        .iter()
        .filter(|rig| {
            !rig_target_applies(&rig.applicability.material, product)
                && !rig_target_applies(&rig.applicability.time, product)
        })
        .map(|rig| rig.type_name.as_str())
        .collect::<Vec<_>>();
    (!skipped.is_empty()).then(|| {
        format!(
            "These fitted rigs don't apply to this product and contribute no bonus: {}.",
            skipped.join(", ")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facility::tests_common::product_in;

    #[test]
    fn rig_target_applies_is_true_for_an_unrestricted_filter() {
        assert!(rig_target_applies(
            &RigTargetFilter::Unrestricted,
            product_in(Some(2), Some(448)),
        ));
    }

    #[test]
    fn rig_target_applies_matches_on_category_or_group_and_nothing_else() {
        let filter = RigTargetFilter::Restricted {
            category_ids: BTreeSet::from([6]),
            group_ids: BTreeSet::from([25]),
        };
        assert!(rig_target_applies(&filter, product_in(Some(6), Some(9999)))); // category hit
        assert!(rig_target_applies(&filter, product_in(Some(1), Some(25)))); // group hit
        assert!(!rig_target_applies(&filter, product_in(Some(2), Some(448)))); // neither
        assert!(!rig_target_applies(&filter, product_in(None, None))); // unknown never matches
    }

    #[test]
    fn sde_rig_applicability_converts_none_to_unrestricted_and_some_to_restricted() {
        let resolved: RigApplicability = iskworks_sde::RigApplicability {
            material: Some(iskworks_sde::IndustryTargetFilter {
                filter_id: 5,
                name: "Small T1 Ships".into(),
                category_ids: vec![],
                group_ids: vec![25, 31, 420],
            }),
            time: None,
        }
        .into();

        assert_eq!(
            resolved.material,
            RigTargetFilter::Restricted {
                category_ids: BTreeSet::new(),
                group_ids: BTreeSet::from([25, 31, 420]),
            }
        );
        assert_eq!(resolved.time, RigTargetFilter::Unrestricted);
    }
}
