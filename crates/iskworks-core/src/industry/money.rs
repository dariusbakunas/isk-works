use super::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(#[serde(with = "rust_decimal::serde::str")] pub Decimal);

impl Money {
    pub fn parse(value: &str) -> Result<Self, IndustryError> {
        let mut value = Decimal::from_str(value.trim()).map_err(|_| IndustryError::InvalidMoney)?;
        if value.is_sign_negative() || value.normalize().scale() > 4 {
            return Err(IndustryError::InvalidMoney);
        }
        value.rescale(4);
        Ok(Self(value))
    }

    #[must_use]
    pub fn zero() -> Self {
        let mut value = Decimal::ZERO;
        value.rescale(4);
        Self(value)
    }

    pub fn checked_mul_quantity(self, quantity: u64) -> Result<Self, IndustryError> {
        let quantity = Decimal::from_i128_with_scale(i128::from(quantity), 0);
        self.0
            .checked_mul(quantity)
            .map(Self)
            .ok_or(IndustryError::MoneyOverflow)
    }

    pub fn checked_add(self, other: Self) -> Result<Self, IndustryError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(IndustryError::MoneyOverflow)
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, IndustryError> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or(IndustryError::MoneyOverflow)
    }

    pub fn checked_div_quantity(self, quantity: u64) -> Result<Self, IndustryError> {
        if quantity == 0 {
            return Err(IndustryError::MoneyOverflow);
        }
        let mut result = self
            .0
            .checked_div(Decimal::from(quantity))
            .ok_or(IndustryError::MoneyOverflow)?;
        result.rescale(4);
        Ok(Self(result))
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BuildId(pub Uuid);

impl BuildId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for BuildId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PriceSourceId(pub Uuid);

impl PriceSourceId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PriceSourceId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PriceSnapshotId(pub Uuid);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BuildPlanId(pub Uuid);

/// The blueprint/facility/duration/cost a Manufacturing/Reaction ticket
/// actually assumed, frozen at ticket-creation time from the
/// `BuildPlanRevision` already computed for its linked build by
/// `calculate_build_snapshot_with_coverage` -- not a second calculation, just retaining
/// more of one that already ran. `Build` is mutable and has no historical
/// versioning beyond a revision counter, so this is the only durable
/// record of what a ticket assumed once the underlying Build is edited.
/// `None` for Acquisition tickets (nothing to manufacture) and for
/// Reaction tickets' `blueprint`/`installation_cost` fields specifically
/// (reaction formulas have no blueprint concept in this codebase -- see
/// `crate::blueprint`).
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecutionSnapshot {
    pub runs: u64,
    pub blueprint: Option<crate::BlueprintSnapshot>,
    pub facility: Option<IndustryFacilityProfile>,
    pub duration_seconds: Option<u64>,
    pub installation_cost: Option<InstallationCostBreakdown>,
    pub material_value: Option<Money>,
}
