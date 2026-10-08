//! Request normalization and the shared error type. `normalize_evaluation`
//! validates the command against the resolved scope (blueprint ME/TE for a
//! manufacturing scope, no efficiency for a reaction scope) and fixes the
//! Phase-1 policy defaults. Pure domain policy.

use iskworks_sde::{CandidateRecipeKind, SdeError};
use serde::Deserialize;
use thiserror::Error;

use crate::{
    BlueprintError, BlueprintKind, FacilityError, FacilityProfileId, IndustryError, InventoryError,
    MarketError, MarketPriceRequest, MarketPricingPolicy,
};

use super::scope_catalog::{profitability_scope, ProfitabilityScopeId};

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateOpportunitiesCommand {
    pub scope_id: ProfitabilityScopeId,
    /// The facility is resolved by id against its *current* settings every
    /// evaluation -- an evaluation is not a frozen plan, so there is no
    /// recorded revision to send. An older client's `expectedFacilityRevision`
    /// key is ignored by serde.
    pub facility_profile_id: FacilityProfileId,
    /// `Some` for a manufacturing scope (validated 0-10/0-20 blueprint ME/TE),
    /// `None` for a reaction scope -- reaction formulas have no material/time
    /// efficiency concept in EVE at all. Sending the wrong one for the
    /// scope's activity is a caller error (`normalize_evaluation` rejects
    /// it), not silently ignored.
    pub material_efficiency: Option<u8>,
    pub time_efficiency: Option<u8>,
    pub market_scope: crate::MarketScope,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NormalizedOpportunityEvaluation {
    pub scope_id: ProfitabilityScopeId,
    pub facility_profile_id: FacilityProfileId,
    pub material_efficiency: Option<u8>,
    pub time_efficiency: Option<u8>,
    pub market_scope: crate::MarketScope,
    pub runs: u64,
    pub material_pricing_policy: MarketPricingPolicy,
    pub output_pricing_policy: MarketPricingPolicy,
}

#[derive(Debug, Error)]
pub enum OpportunityError {
    #[error(transparent)]
    Blueprint(#[from] BlueprintError),
    #[error("invalid opportunity candidate: {0}")]
    InvalidCandidate(String),
    #[error("opportunity arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("unknown opportunity scope")]
    UnknownScope,
    #[error("materialEfficiency/timeEfficiency are required for a manufacturing scope")]
    MissingManufacturingEfficiency,
    #[error("materialEfficiency/timeEfficiency do not apply to a reaction scope")]
    UnexpectedReactionEfficiency,
    #[error(transparent)]
    Market(#[from] MarketError),
    #[error(transparent)]
    Industry(#[from] IndustryError),
    #[error(transparent)]
    Facility(#[from] FacilityError),
    #[error("static data lookup failed: {0}")]
    StaticData(String),
    #[error("adjusted-price lookup failed: {0}")]
    AdjustedPrices(String),
}

impl From<SdeError> for OpportunityError {
    fn from(error: SdeError) -> Self {
        Self::StaticData(error.to_string())
    }
}

impl From<InventoryError> for OpportunityError {
    fn from(error: InventoryError) -> Self {
        Self::AdjustedPrices(error.to_string())
    }
}

/// Opportunities' own default policy pair -- deliberately independent of
/// Build's (`industry::build::default_material_pricing_policy`/
/// `default_output_pricing_policy`, which default material to
/// `HighestBuy`): scanning many candidates at once wants "what could I
/// actually acquire this quantity for" (`AcquireQuantityFromSellOrders`)
/// rather than a single-unit buy-order snapshot. Converging these two
/// pairs is deferred to a later product decision -- this keeps the scanner's existing behavior unchanged.
#[must_use]
pub fn material_price_request(
    type_id: i64,
    type_name: impl Into<String>,
    requested_quantity: u64,
) -> MarketPriceRequest {
    MarketPriceRequest {
        type_id,
        type_name: type_name.into(),
        requested_quantity,
        pricing_policy: MarketPricingPolicy::AcquireQuantityFromSellOrders,
    }
}

#[must_use]
pub fn output_price_request(
    type_id: i64,
    type_name: impl Into<String>,
    requested_quantity: u64,
) -> MarketPriceRequest {
    MarketPriceRequest {
        type_id,
        type_name: type_name.into(),
        requested_quantity,
        pricing_policy: MarketPricingPolicy::LowestSell,
    }
}

pub fn normalize_evaluation(
    command: EvaluateOpportunitiesCommand,
) -> Result<NormalizedOpportunityEvaluation, OpportunityError> {
    let scope = profitability_scope(command.scope_id).ok_or(OpportunityError::UnknownScope)?;
    match scope.recipe_kind {
        CandidateRecipeKind::Manufacturing => {
            let material_efficiency = command
                .material_efficiency
                .ok_or(OpportunityError::MissingManufacturingEfficiency)?;
            let time_efficiency = command
                .time_efficiency
                .ok_or(OpportunityError::MissingManufacturingEfficiency)?;
            crate::blueprint::validate(
                BlueprintKind::Original,
                material_efficiency,
                time_efficiency,
                None,
                1,
                true,
            )?;
        }
        CandidateRecipeKind::Reaction => {
            if command.material_efficiency.is_some() || command.time_efficiency.is_some() {
                return Err(OpportunityError::UnexpectedReactionEfficiency);
            }
        }
    }
    Ok(NormalizedOpportunityEvaluation {
        scope_id: command.scope_id,
        facility_profile_id: command.facility_profile_id,
        material_efficiency: command.material_efficiency,
        time_efficiency: command.time_efficiency,
        market_scope: command.market_scope,
        runs: 1,
        material_pricing_policy: MarketPricingPolicy::AcquireQuantityFromSellOrders,
        output_pricing_policy: MarketPricingPolicy::LowestSell,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::tests_common::*;

    #[test]
    fn command_requires_valid_me_te_and_keeps_phase_one_policies_fixed() {
        let normalized = normalize_evaluation(valid_command()).unwrap();
        assert_eq!(normalized.runs, 1);
        assert_eq!(
            normalized.material_pricing_policy,
            crate::MarketPricingPolicy::AcquireQuantityFromSellOrders
        );
        assert_eq!(
            normalized.output_pricing_policy,
            crate::MarketPricingPolicy::LowestSell
        );

        let mut invalid_me = valid_command();
        invalid_me.material_efficiency = Some(11);
        assert!(matches!(
            normalize_evaluation(invalid_me),
            Err(OpportunityError::Blueprint(
                crate::BlueprintError::InvalidMaterialEfficiency
            ))
        ));

        let mut invalid_te = valid_command();
        invalid_te.time_efficiency = Some(21);
        assert!(matches!(
            normalize_evaluation(invalid_te),
            Err(OpportunityError::Blueprint(
                crate::BlueprintError::InvalidTimeEfficiency
            ))
        ));
    }
}
