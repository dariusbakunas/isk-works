//! What the player entered for planets, kept per workspace and reused by
//! every scenario: resource densities and per-planet settings.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use crate::{InventoryError, WorkspaceId};

/// One stored resource reading on one planet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDensity {
    pub planet_id: i64,
    pub resource_type_id: i64,
    pub density_pct: Option<Decimal>,
    pub head_value: Option<Decimal>,
    pub updated_at: DateTime<Utc>,
}

/// Sets or clears one resource reading. Both values empty clears it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DensityWrite {
    pub planet_id: i64,
    pub resource_type_id: i64,
    pub density_pct: Option<Decimal>,
    pub head_value: Option<Decimal>,
}

impl DensityWrite {
    #[must_use]
    pub fn clears(&self) -> bool {
        self.density_pct.is_none() && self.head_value.is_none()
    }

    /// Density is 0–100 %, a head value is positive.
    pub fn validate(&self) -> Result<(), InventoryError> {
        if self
            .density_pct
            .is_some_and(|pct| pct < Decimal::ZERO || pct > Decimal::ONE_HUNDRED)
        {
            return Err(InventoryError::Validation(
                "densityPct must be between 0 and 100".to_string(),
            ));
        }
        if self.head_value.is_some_and(|value| value <= Decimal::ZERO) {
            return Err(InventoryError::Validation(
                "headValue must be positive".to_string(),
            ));
        }
        Ok(())
    }
}

/// One planet's stored settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPlanetSettings {
    pub planet_id: i64,
    /// Overrides the scenario's default customs rate (0.1 = 10 %).
    pub customs_rate: Option<Decimal>,
    /// Other players extract here.
    pub contested: bool,
    pub updated_at: DateTime<Utc>,
}

/// Sets or clears one planet's settings. No override and not contested
/// clears them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanetSettingsWrite {
    pub planet_id: i64,
    pub customs_rate: Option<Decimal>,
    pub contested: bool,
}

impl PlanetSettingsWrite {
    #[must_use]
    pub fn clears(&self) -> bool {
        self.customs_rate.is_none() && !self.contested
    }

    /// A customs rate is 0–1.
    pub fn validate(&self) -> Result<(), InventoryError> {
        if self
            .customs_rate
            .is_some_and(|rate| rate < Decimal::ZERO || rate > Decimal::ONE)
        {
            return Err(InventoryError::Validation(
                "customsRate must be between 0 and 1".to_string(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
pub trait PiPlannerInputsRepository: Send + Sync {
    /// Readings for these planets, in no particular order.
    async fn densities(
        &self,
        workspace_id: WorkspaceId,
        planet_ids: &[i64],
    ) -> Result<Vec<StoredDensity>, InventoryError>;
    /// Upserts every write, or deletes the reading when the write clears it.
    async fn save_densities(
        &self,
        workspace_id: WorkspaceId,
        writes: &[DensityWrite],
    ) -> Result<(), InventoryError>;
    async fn planet_settings(
        &self,
        workspace_id: WorkspaceId,
        planet_ids: &[i64],
    ) -> Result<Vec<StoredPlanetSettings>, InventoryError>;
    /// Upserts every write, or deletes the row when the write clears it.
    async fn save_planet_settings(
        &self,
        workspace_id: WorkspaceId,
        writes: &[PlanetSettingsWrite],
    ) -> Result<(), InventoryError>;
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn dec(value: &str) -> Decimal {
        Decimal::from_str(value).unwrap()
    }

    fn density(density_pct: Option<Decimal>, head_value: Option<Decimal>) -> DensityWrite {
        DensityWrite {
            planet_id: 1,
            resource_type_id: 2,
            density_pct,
            head_value,
        }
    }

    #[test]
    fn density_write_bounds() {
        assert!(density(Some(dec("0")), None).validate().is_ok());
        assert!(density(Some(dec("100")), Some(dec("146.1")))
            .validate()
            .is_ok());
        assert!(density(Some(dec("100.1")), None).validate().is_err());
        assert!(density(Some(dec("-1")), None).validate().is_err());
        assert!(density(None, Some(dec("0"))).validate().is_err());
        assert!(density(None, None).clears());
        assert!(!density(None, Some(dec("1"))).clears());
    }

    #[test]
    fn planet_settings_write_bounds() {
        let write = |customs_rate, contested| PlanetSettingsWrite {
            planet_id: 1,
            customs_rate,
            contested,
        };
        assert!(write(Some(dec("0.1")), false).validate().is_ok());
        assert!(write(Some(dec("1.5")), false).validate().is_err());
        assert!(write(None, false).clears());
        assert!(!write(None, true).clears());
        assert!(!write(Some(dec("0")), false).clears());
    }
}
