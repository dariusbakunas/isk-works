//! Ranking: the stable `sort_opportunity_candidates` order and the six
//! `OpportunityRankings` lists (sell-side / liquidation x profit / margin /
//! profit-per-hour), each with `ExcludedFromDefaultRanking` candidates
//! filtered out.

use std::str::FromStr;

use rust_decimal::Decimal;
use serde::Serialize;

use crate::OpportunityEligibilityStatus;

use super::candidate_projection::OpportunityCandidate;
use super::evidence::OpportunityCompleteness;
use super::valuation::OpportunityValuation;

pub fn sort_opportunity_candidates(candidates: &mut [OpportunityCandidate]) {
    candidates.sort_by(|left, right| {
        let left_complete = left.completeness == OpportunityCompleteness::Complete;
        let right_complete = right.completeness == OpportunityCompleteness::Complete;
        right_complete
            .cmp(&left_complete)
            .then_with(|| {
                let left_profit = left
                    .metrics
                    .estimated_gross_profit_per_manufacturing_hour
                    .as_deref()
                    .and_then(|value| Decimal::from_str(value).ok());
                let right_profit = right
                    .metrics
                    .estimated_gross_profit_per_manufacturing_hour
                    .as_deref()
                    .and_then(|value| Decimal::from_str(value).ok());
                right_profit.cmp(&left_profit)
            })
            .then_with(|| left.product_name.cmp(&right.product_name))
            .then_with(|| left.product_type_id.cmp(&right.product_type_id))
    });
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityRankingEntry {
    pub product_type_id: i64,
    /// The candidate's recipe type id -- a blueprint or a reaction formula,
    /// whichever produced it. Secondary identity metadata; the frontend
    /// matches ranking entries to candidates by `product_type_id` alone.
    pub recipe_type_id: i64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityRankings {
    pub sell_side_gross_profit: Vec<OpportunityRankingEntry>,
    pub immediate_liquidation_gross_profit: Vec<OpportunityRankingEntry>,
    pub sell_side_gross_margin: Vec<OpportunityRankingEntry>,
    pub immediate_liquidation_gross_margin: Vec<OpportunityRankingEntry>,
    pub sell_side_gross_profit_per_manufacturing_hour: Vec<OpportunityRankingEntry>,
    pub immediate_liquidation_gross_profit_per_manufacturing_hour: Vec<OpportunityRankingEntry>,
}

fn ranking_metric_gross_profit(valuation: &OpportunityValuation) -> Option<Decimal> {
    valuation.gross_profit.map(|money| money.0)
}

fn ranking_metric_gross_margin(valuation: &OpportunityValuation) -> Option<Decimal> {
    valuation
        .gross_margin_percent
        .as_deref()
        .and_then(|value| Decimal::from_str(value).ok())
}

fn ranking_metric_gross_profit_per_hour(valuation: &OpportunityValuation) -> Option<Decimal> {
    valuation
        .gross_profit_per_manufacturing_hour
        .as_deref()
        .and_then(|value| Decimal::from_str(value).ok())
}

fn project_ranking(
    candidates: &[OpportunityCandidate],
    valuation: fn(&OpportunityCandidate) -> &OpportunityValuation,
    metric: impl Fn(&OpportunityValuation) -> Option<Decimal>,
) -> Vec<OpportunityRankingEntry> {
    let mut eligible: Vec<&OpportunityCandidate> = candidates
        .iter()
        .filter(|candidate| {
            candidate.eligibility.status != OpportunityEligibilityStatus::ExcludedFromDefaultRanking
        })
        .collect();
    eligible.sort_by(|left, right| {
        let left_valuation = valuation(left);
        let right_valuation = valuation(right);
        let left_complete = left_valuation.completeness == OpportunityCompleteness::Complete;
        let right_complete = right_valuation.completeness == OpportunityCompleteness::Complete;
        right_complete
            .cmp(&left_complete)
            .then_with(|| metric(right_valuation).cmp(&metric(left_valuation)))
            .then_with(|| left.product_name.cmp(&right.product_name))
            .then_with(|| left.product_type_id.cmp(&right.product_type_id))
    });
    eligible
        .into_iter()
        .map(|candidate| OpportunityRankingEntry {
            product_type_id: candidate.product_type_id,
            recipe_type_id: candidate.recipe.type_id(),
        })
        .collect()
}

fn sell_side_valuation(candidate: &OpportunityCandidate) -> &OpportunityValuation {
    &candidate.valuations.sell_side
}

fn immediate_liquidation_valuation(candidate: &OpportunityCandidate) -> &OpportunityValuation {
    &candidate.valuations.immediate_liquidation
}

pub(super) fn project_rankings(candidates: &[OpportunityCandidate]) -> OpportunityRankings {
    let sell_side = sell_side_valuation;
    let immediate_liquidation = immediate_liquidation_valuation;
    OpportunityRankings {
        sell_side_gross_profit: project_ranking(candidates, sell_side, ranking_metric_gross_profit),
        immediate_liquidation_gross_profit: project_ranking(
            candidates,
            immediate_liquidation,
            ranking_metric_gross_profit,
        ),
        sell_side_gross_margin: project_ranking(candidates, sell_side, ranking_metric_gross_margin),
        immediate_liquidation_gross_margin: project_ranking(
            candidates,
            immediate_liquidation,
            ranking_metric_gross_margin,
        ),
        sell_side_gross_profit_per_manufacturing_hour: project_ranking(
            candidates,
            sell_side,
            ranking_metric_gross_profit_per_hour,
        ),
        immediate_liquidation_gross_profit_per_manufacturing_hour: project_ranking(
            candidates,
            immediate_liquidation,
            ranking_metric_gross_profit_per_hour,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::opportunity::*;

    #[test]
    fn ranking_places_complete_profit_per_hour_first_and_uses_stable_ties() {
        let mut candidates = vec![
            OpportunityCandidate::ranking_fixture(30, "Incomplete", false, None),
            OpportunityCandidate::ranking_fixture(20, "Beta", true, Some("50")),
            OpportunityCandidate::ranking_fixture(10, "Alpha", true, Some("50")),
            OpportunityCandidate::ranking_fixture(40, "Gamma", true, Some("100")),
        ];

        sort_opportunity_candidates(&mut candidates);

        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.product_type_id)
                .collect::<Vec<_>>(),
            vec![40, 10, 20, 30]
        );
    }

    #[test]
    fn excluded_candidate_is_retained_but_not_ranked() {
        let mut metamorphosis =
            OpportunityCandidate::ranking_fixture(79_214, "Metamorphosis", true, Some("500"));
        metamorphosis.eligibility = crate::OpportunityEligibility {
            status: OpportunityEligibilityStatus::ExcludedFromDefaultRanking,
            exclusion_reasons: vec![crate::OpportunityExclusionReason::sde_special_edition(
                1_612,
                "Special Edition Ships",
            )],
        };
        let alpha = OpportunityCandidate::ranking_fixture(10, "Rifter", true, Some("50"));
        let beta = OpportunityCandidate::ranking_fixture(20, "Merlin", true, Some("30"));

        let candidates = vec![metamorphosis, alpha, beta];
        let rankings = project_rankings(&candidates);

        assert_eq!(candidates.len(), 3);
        assert!(!rankings
            .sell_side_gross_profit_per_manufacturing_hour
            .iter()
            .any(|entry| entry.product_type_id == 79_214));
        assert_eq!(
            rankings
                .sell_side_gross_profit_per_manufacturing_hour
                .iter()
                .map(|entry| entry.product_type_id)
                .collect::<Vec<_>>(),
            vec![10, 20]
        );
    }

    #[test]
    fn valuation_metric_changes_battleship_order() {
        let mut high_sell = OpportunityCandidate::ranking_fixture(100, "Megathron", true, None);
        high_sell.valuations.sell_side = OpportunityValuation {
            revenue: Some(money("500000000")),
            gross_profit: Some(money("400000000")),
            gross_margin_percent: Some("80".to_string()),
            gross_profit_per_manufacturing_hour: Some("400".to_string()),
            completeness: OpportunityCompleteness::Complete,
        };
        high_sell.valuations.immediate_liquidation = OpportunityValuation {
            revenue: Some(money("200000000")),
            gross_profit: Some(money("100000000")),
            gross_margin_percent: Some("20".to_string()),
            gross_profit_per_manufacturing_hour: Some("100".to_string()),
            completeness: OpportunityCompleteness::Complete,
        };

        let mut high_liquidation =
            OpportunityCandidate::ranking_fixture(200, "Apocalypse", true, None);
        high_liquidation.valuations.sell_side = OpportunityValuation {
            revenue: Some(money("300000000")),
            gross_profit: Some(money("200000000")),
            gross_margin_percent: Some("40".to_string()),
            gross_profit_per_manufacturing_hour: Some("200".to_string()),
            completeness: OpportunityCompleteness::Complete,
        };
        high_liquidation.valuations.immediate_liquidation = OpportunityValuation {
            revenue: Some(money("450000000")),
            gross_profit: Some(money("350000000")),
            gross_margin_percent: Some("70".to_string()),
            gross_profit_per_manufacturing_hour: Some("350".to_string()),
            completeness: OpportunityCompleteness::Complete,
        };

        let candidates = vec![high_sell, high_liquidation];
        let rankings = project_rankings(&candidates);

        assert_eq!(
            rankings.sell_side_gross_profit_per_manufacturing_hour[0].product_type_id,
            100
        );
        assert_eq!(
            rankings.immediate_liquidation_gross_profit_per_manufacturing_hour[0].product_type_id,
            200
        );
        assert_ne!(
            rankings.sell_side_gross_profit_per_manufacturing_hour,
            rankings.immediate_liquidation_gross_profit_per_manufacturing_hour
        );
    }
}
