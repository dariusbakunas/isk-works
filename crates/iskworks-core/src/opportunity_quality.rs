use serde::Serialize;

use iskworks_sde::CandidateProductClassification;

use crate::{OpportunityWarning, OpportunityWarningKind};

const SPECIAL_EDITION_SHIPS_MARKET_GROUP_ID: i64 = 1_612;
const THIN_OUTPUT_VISIBLE_RUN_EQUIVALENTS: u64 = 20;
const THIN_OUTPUT_CONSUMED_PERCENT: u64 = 10;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpportunityEligibilityStatus {
    Eligible,
    EligibleWithWarnings,
    ExcludedFromDefaultRanking,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityExclusionReason {
    pub code: String,
    pub message: String,
    pub market_group_id: i64,
    pub market_group_name: String,
}

impl OpportunityExclusionReason {
    pub(crate) fn sde_special_edition(market_group_id: i64, name: &str) -> Self {
        Self {
            code: "sdeSpecialEdition".into(),
            message: format!(
                "The authoritative SDE classifies this product under {name}; it is retained for diagnostics but excluded from ordinary opportunity rankings."
            ),
            market_group_id,
            market_group_name: name.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityEligibility {
    pub status: OpportunityEligibilityStatus,
    pub exclusion_reasons: Vec<OpportunityExclusionReason>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpportunityEvidenceQuality {
    Strong,
    Qualified,
    Weak,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityQuality {
    pub evidence_quality: OpportunityEvidenceQuality,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityExcludedCost {
    pub code: String,
    pub message: String,
}

pub(crate) fn classify_recipe_eligibility(
    classification: &CandidateProductClassification,
) -> Vec<OpportunityExclusionReason> {
    classification
        .market_group_ancestry
        .iter()
        .find(|group| group.market_group_id == SPECIAL_EDITION_SHIPS_MARKET_GROUP_ID)
        .map(|group| {
            vec![OpportunityExclusionReason::sde_special_edition(
                group.market_group_id,
                &group.name,
            )]
        })
        .unwrap_or_default()
}

pub(crate) fn derive_evidence_quality(
    warnings: &[OpportunityWarning],
) -> OpportunityEvidenceQuality {
    if warnings.iter().any(|warning| {
        matches!(
            warning.kind,
            OpportunityWarningKind::MissingOutputPrice
                | OpportunityWarningKind::InsufficientMarketDepth
                | OpportunityWarningKind::ThinOutputBook
                | OpportunityWarningKind::IncompleteEivBasis
        )
    }) {
        OpportunityEvidenceQuality::Weak
    } else if warnings.iter().any(|warning| {
        matches!(
            warning.kind,
            OpportunityWarningKind::StaleMarketEvidence | OpportunityWarningKind::ThinInputBook
        )
    }) {
        OpportunityEvidenceQuality::Qualified
    } else {
        OpportunityEvidenceQuality::Strong
    }
}

pub(crate) fn opportunity_excluded_costs() -> Vec<OpportunityExcludedCost> {
    [
        (
            "brokerFees",
            "Broker fees are excluded from gross profitability.",
        ),
        (
            "salesTax",
            "Sales tax is excluded from gross profitability.",
        ),
        (
            "haulingAndLogistics",
            "Hauling and logistics are excluded from gross profitability.",
        ),
        (
            "characterSkills",
            "Character skill effects are excluded from this comparison.",
        ),
        (
            "blueprintAcquisitionAndOpportunityCost",
            "Blueprint acquisition and opportunity cost are excluded from gross profitability.",
        ),
        (
            "saleTimeAndLiquidity",
            "Sale time and liquidity are not modeled as costs.",
        ),
    ]
    .into_iter()
    .map(|(code, message)| OpportunityExcludedCost {
        code: code.into(),
        message: message.into(),
    })
    .collect()
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpportunityThinBookReason {
    BestLevelAtOrBelowOutputQuantity,
    OutputAtOrAboveTenPercentOfVisibleVolume,
    VisibleVolumeBelowTwentyRunEquivalents,
}

#[must_use]
pub(crate) fn is_thin_output_book(
    output_quantity: u64,
    best_level_quantity: Option<u64>,
    total_visible_quantity: u64,
) -> Vec<OpportunityThinBookReason> {
    let mut reasons = Vec::new();
    if best_level_quantity.is_some_and(|quantity| quantity <= output_quantity) {
        reasons.push(OpportunityThinBookReason::BestLevelAtOrBelowOutputQuantity);
    }
    if total_visible_quantity > 0
        && output_quantity.saturating_mul(100)
            >= total_visible_quantity.saturating_mul(THIN_OUTPUT_CONSUMED_PERCENT)
    {
        reasons.push(OpportunityThinBookReason::OutputAtOrAboveTenPercentOfVisibleVolume);
    }
    if total_visible_quantity < output_quantity.saturating_mul(THIN_OUTPUT_VISIBLE_RUN_EQUIVALENTS)
    {
        reasons.push(OpportunityThinBookReason::VisibleVolumeBelowTwentyRunEquivalents);
    }
    reasons
}

#[cfg(test)]
mod tests {
    use iskworks_sde::{CandidateMarketGroup, CandidateProductClassification};

    use super::*;
    fn classification(ancestry: Vec<(i64, &str)>) -> CandidateProductClassification {
        CandidateProductClassification {
            category_id: Some(6),
            category_name: Some("Ship".into()),
            group_id: Some(25),
            group_name: Some("Frigate".into()),
            meta_group_id: Some(1),
            meta_group_name: Some("Tech I".into()),
            market_group_id: ancestry.last().map(|(id, _)| *id),
            market_group_name: ancestry.last().map(|(_, name)| (*name).to_string()),
            market_group_ancestry: ancestry
                .into_iter()
                .map(|(market_group_id, name)| CandidateMarketGroup {
                    market_group_id,
                    name: name.into(),
                })
                .collect(),
        }
    }

    fn warning(kind: OpportunityWarningKind) -> OpportunityWarning {
        OpportunityWarning {
            kind,
            message: "fixture".into(),
            type_ids: Vec::new(),
            details: None,
        }
    }

    #[test]
    fn special_edition_ancestry_excludes_candidate_without_name_matching() {
        let special = classification(vec![
            (4, "Ships"),
            (1_612, "Special Edition Ships"),
            (1_619, "Special Edition Frigates"),
        ]);

        assert_eq!(
            classify_recipe_eligibility(&special),
            vec![OpportunityExclusionReason::sde_special_edition(
                1_612,
                "Special Edition Ships"
            )]
        );
        assert!(classify_recipe_eligibility(&classification(vec![
            (4, "Ships"),
            (1_361, "Frigates")
        ]))
        .is_empty());
    }

    #[test]
    fn quality_is_derived_from_warning_semantics_not_warning_count() {
        assert_eq!(
            derive_evidence_quality(&[]),
            OpportunityEvidenceQuality::Strong
        );
        assert_eq!(
            derive_evidence_quality(&[warning(OpportunityWarningKind::StaleMarketEvidence)]),
            OpportunityEvidenceQuality::Qualified
        );
        assert_eq!(
            derive_evidence_quality(&[warning(OpportunityWarningKind::ThinOutputBook)]),
            OpportunityEvidenceQuality::Weak
        );
        assert_eq!(
            derive_evidence_quality(&[warning(OpportunityWarningKind::IncompleteEivBasis)]),
            OpportunityEvidenceQuality::Weak
        );
    }

    #[test]
    fn excluded_costs_are_stable_structured_diagnostics() {
        assert_eq!(
            opportunity_excluded_costs()
                .into_iter()
                .map(|cost| cost.code)
                .collect::<Vec<_>>(),
            vec![
                "brokerFees",
                "salesTax",
                "haulingAndLogistics",
                "characterSkills",
                "blueprintAcquisitionAndOpportunityCost",
                "saleTimeAndLiquidity",
            ]
        );
    }

    #[test]
    fn thin_output_book_triggers_at_exact_documented_boundaries() {
        // Best-level boundary, isolated via a deep, wide book that can't trip the
        // other two rules.
        assert_eq!(
            is_thin_output_book(2, Some(2), 1_000),
            vec![OpportunityThinBookReason::BestLevelAtOrBelowOutputQuantity]
        );
        assert!(is_thin_output_book(2, Some(3), 1_000).is_empty());

        // 10%-of-visible-volume boundary. At exactly 10 run-equivalents the
        // run-equivalents rule is necessarily also active (10 < 20), so this
        // checks the 10% rule's own on/off behavior via containment rather
        // than exact-vec equality.
        assert!(is_thin_output_book(10, None, 100)
            .contains(&OpportunityThinBookReason::OutputAtOrAboveTenPercentOfVisibleVolume));
        assert!(!is_thin_output_book(9, None, 100)
            .contains(&OpportunityThinBookReason::OutputAtOrAboveTenPercentOfVisibleVolume));

        // Run-equivalents boundary, isolated via a ratio well under 10%.
        assert_eq!(
            is_thin_output_book(1, None, 19),
            vec![OpportunityThinBookReason::VisibleVolumeBelowTwentyRunEquivalents]
        );
        assert!(is_thin_output_book(1, None, 20).is_empty());
    }
}
