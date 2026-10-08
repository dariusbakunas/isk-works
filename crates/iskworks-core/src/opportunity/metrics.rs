//! Profitability arithmetic: `derive_opportunity_metrics` (capital, gross
//! profit, margin %, profit/hour) and the completeness predicate.

use rust_decimal::Decimal;
use serde::Serialize;

use crate::Money;

use super::command::OpportunityError;
use super::evidence::OpportunityCompleteness;

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityMetrics {
    pub material_cost: Option<Money>,
    pub installation_cost: Option<Money>,
    pub total_estimated_manufacturing_cost: Option<Money>,
    pub estimated_output_value: Option<Money>,
    pub estimated_gross_profit: Option<Money>,
    pub gross_margin_percent: Option<String>,
    pub estimated_gross_profit_per_unit: Option<Money>,
    pub estimated_gross_profit_per_run: Option<Money>,
    pub estimated_gross_profit_per_manufacturing_hour: Option<String>,
    pub capital_required: Option<Money>,
}

pub fn derive_opportunity_metrics(
    material_cost: Option<Money>,
    installation_cost: Option<Money>,
    output_value: Option<Money>,
    output_quantity: u64,
    effective_duration_seconds: Option<u64>,
) -> Result<OpportunityMetrics, OpportunityError> {
    if output_quantity == 0 {
        return Err(OpportunityError::InvalidCandidate(
            "output quantity must be positive".to_string(),
        ));
    }
    if effective_duration_seconds == Some(0) {
        return Err(OpportunityError::InvalidCandidate(
            "effective duration must be positive".to_string(),
        ));
    }

    let capital_required = material_cost
        .zip(installation_cost)
        .map(|(material, installation)| material.checked_add(installation))
        .transpose()
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;
    let estimated_gross_profit = output_value
        .zip(capital_required)
        .map(|(revenue, capital)| revenue.checked_sub(capital))
        .transpose()
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;
    let estimated_gross_profit_per_unit = estimated_gross_profit
        .map(|profit| profit.checked_div_quantity(output_quantity))
        .transpose()
        .map_err(|_| OpportunityError::ArithmeticOverflow)?;
    let gross_margin_percent = estimated_gross_profit
        .zip(output_value)
        .map(|(profit, revenue)| checked_ratio_string(profit.0, revenue.0, Decimal::from(100)))
        .transpose()?;
    let estimated_gross_profit_per_manufacturing_hour = estimated_gross_profit
        .zip(effective_duration_seconds)
        .map(|(profit, seconds)| {
            checked_ratio_string(profit.0, Decimal::from(seconds), Decimal::from(3600))
        })
        .transpose()?;

    Ok(OpportunityMetrics {
        material_cost,
        installation_cost,
        total_estimated_manufacturing_cost: capital_required,
        estimated_output_value: output_value,
        estimated_gross_profit,
        gross_margin_percent,
        estimated_gross_profit_per_unit,
        estimated_gross_profit_per_run: estimated_gross_profit,
        estimated_gross_profit_per_manufacturing_hour,
        capital_required,
    })
}

pub(super) fn checked_ratio_string(
    numerator: Decimal,
    denominator: Decimal,
    multiplier: Decimal,
) -> Result<String, OpportunityError> {
    if denominator.is_zero() {
        return Err(OpportunityError::InvalidCandidate(
            "metric denominator must be positive".to_string(),
        ));
    }
    numerator
        .checked_mul(multiplier)
        .and_then(|value| value.checked_div(denominator))
        .map(|value| value.normalize().to_string())
        .ok_or(OpportunityError::ArithmeticOverflow)
}

pub(super) fn opportunity_completeness(
    material_cost: Option<Money>,
    installation_cost: Option<Money>,
    output_value: Option<Money>,
    product_count: usize,
) -> OpportunityCompleteness {
    if material_cost.is_some()
        && installation_cost.is_some()
        && output_value.is_some()
        && product_count == 1
    {
        OpportunityCompleteness::Complete
    } else {
        OpportunityCompleteness::Incomplete
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::opportunity::*;

    #[test]
    fn complete_metrics_use_authoritative_totals_and_effective_duration() {
        let metrics = derive_opportunity_metrics(
            Some(money("120")),
            Some(money("30")),
            Some(money("200")),
            2,
            Some(1800),
        )
        .unwrap();

        assert_eq!(metrics.capital_required, Some(money("150")));
        assert_eq!(metrics.estimated_gross_profit, Some(money("50")));
        assert_eq!(metrics.estimated_gross_profit_per_unit, Some(money("25")));
        assert_eq!(metrics.estimated_gross_profit_per_run, Some(money("50")));
        assert_eq!(
            metrics.estimated_gross_profit_per_manufacturing_hour,
            Some("100".to_string())
        );
        assert_eq!(metrics.gross_margin_percent, Some("25".to_string()));
    }

    #[test]
    fn incomplete_inputs_never_turn_into_zero_metrics() {
        let metrics =
            derive_opportunity_metrics(Some(money("120")), None, Some(money("200")), 2, Some(1800))
                .unwrap();

        assert_eq!(metrics.material_cost, Some(money("120")));
        assert_eq!(metrics.installation_cost, None);
        assert_eq!(metrics.capital_required, None);
        assert_eq!(metrics.estimated_gross_profit, None);
        assert_eq!(metrics.estimated_gross_profit_per_unit, None);
        assert_eq!(metrics.estimated_gross_profit_per_manufacturing_hour, None);
    }

    #[test]
    fn metrics_reject_zero_output_and_duration() {
        assert!(matches!(
            derive_opportunity_metrics(
                Some(money("1")),
                Some(money("1")),
                Some(money("3")),
                0,
                Some(3600),
            ),
            Err(OpportunityError::InvalidCandidate(_))
        ));
        assert!(matches!(
            derive_opportunity_metrics(
                Some(money("1")),
                Some(money("1")),
                Some(money("3")),
                1,
                Some(0),
            ),
            Err(OpportunityError::InvalidCandidate(_))
        ));
    }
}
