//! The one rule for a material line's inventory coverage.
//!
//! Both coverage reports use it: the saved Build's (`ProductionRepository::coverage`,
//! computed in storage from loaded rows) and the Create Build preview's
//! ([`super::project_create_coverage`]). Storage only loads quantities; every
//! derived figure, the coverage state, the projected-cost condition and the
//! report warnings come from here.

use super::{ProductionError, QuantityCoverageState};
use crate::Money;

/// The loaded quantities of one material line.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CoverageQuantities {
    /// Units ISK Works accounts for (the inventory balance).
    pub owned: u64,
    /// Units reserved for this Build. Builds hold no reservations of their
    /// own today (Epics do), so callers pass `0`.
    pub reserved_here: u64,
    /// Units reserved by anything else (every active reservation).
    pub reserved_elsewhere: u64,
    pub required: u64,
}

/// Everything derived from [`CoverageQuantities`].
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CoverageFigures {
    /// Owned stock nobody has reserved.
    pub unreserved: u64,
    /// What this Build can use: its own reservation plus unreserved stock.
    pub available: u64,
    pub covered: u64,
    pub missing: u64,
    /// How much more this Build could reserve toward its requirement.
    pub reservable_additional: u64,
    pub state: QuantityCoverageState,
    /// Whether the stock available to this Build covers the requirement, so
    /// its historical cost can stand in for the whole line. Free stock only:
    /// units reserved elsewhere are not this Build's to cost.
    pub cost_resolvable: bool,
}

#[must_use]
pub fn coverage_figures(quantities: CoverageQuantities) -> CoverageFigures {
    let CoverageQuantities {
        owned,
        reserved_here,
        reserved_elsewhere,
        required,
    } = quantities;
    let unreserved = owned.saturating_sub(reserved_here.saturating_add(reserved_elsewhere));
    let available = reserved_here.saturating_add(unreserved);
    let covered = required.min(available);
    CoverageFigures {
        unreserved,
        available,
        covered,
        missing: required.saturating_sub(covered),
        reservable_additional: required.saturating_sub(reserved_here).min(unreserved),
        state: quantity_coverage_state(owned, reserved_here, covered, required),
        cost_resolvable: available >= required,
    }
}

/// The coverage state of one line.
///
/// A reservation for this Build decides first (an over-reserved line is
/// `Reserved`), then an empty balance, then how much of the requirement the
/// available stock covers.
#[must_use]
pub fn quantity_coverage_state(
    owned: u64,
    reserved_here: u64,
    covered: u64,
    required: u64,
) -> QuantityCoverageState {
    if reserved_here > 0 && reserved_here >= required {
        QuantityCoverageState::Reserved
    } else if reserved_here > 0 {
        QuantityCoverageState::ReservedPartially
    } else if owned == 0 {
        QuantityCoverageState::NoInventory
    } else if covered >= required {
        QuantityCoverageState::Covered
    } else if covered > 0 {
        QuantityCoverageState::PartiallyCovered
    } else {
        QuantityCoverageState::Missing
    }
}

/// The historical cost of the whole requirement, when the available stock
/// covers it and its average cost is known.
pub fn projected_historical_cost(
    figures: &CoverageFigures,
    average_unit_cost: Option<Money>,
    required: u64,
) -> Result<Option<Money>, ProductionError> {
    if !figures.cost_resolvable {
        return Ok(None);
    }
    average_unit_cost
        .map(|cost| cost.checked_mul_quantity(required))
        .transpose()
        .map_err(|_| ProductionError::ArithmeticOverflow)
}

/// The per-line warning for a gap between ISK Works' accounted quantity and
/// what EVE last reported. `difference` is EVE minus ISK Works.
#[must_use]
pub fn esi_reconciliation_warning(difference: i64) -> Option<String> {
    match difference {
        0 => None,
        delta if delta > 0 => Some(format!(
            "EVE currently reports {delta} more units than ISK Works inventory. These units are not reservable until they are recorded through an Inventory Event."
        )),
        delta => Some(format!(
            "ISK Works accounts for {} more units than EVE currently reports. Review the discrepancy before relying on the physical quantity.",
            delta.unsigned_abs()
        )),
    }
}

/// The report-level warnings about where coverage data comes from.
#[must_use]
pub fn coverage_source_warnings(connection_exists: bool, any_esi_observation: bool) -> Vec<String> {
    if !connection_exists {
        vec!["No EVE connection is linked to this Owner. Inventory coverage remains fully available from ISK Works accounting records.".to_string()]
    } else if !any_esi_observation {
        vec!["No complete ESI asset snapshot is available for this Owner. Coverage uses ISK Works accounting inventory only.".to_string()]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use QuantityCoverageState::*;

    fn figures(owned: u64, here: u64, elsewhere: u64, required: u64) -> CoverageFigures {
        coverage_figures(CoverageQuantities {
            owned,
            reserved_here: here,
            reserved_elsewhere: elsewhere,
            required,
        })
    }

    #[test]
    fn coverage_state_table() {
        // (owned, here, covered, required) -> state
        let cases = [
            // exact reservation
            ((100, 100, 100, 100), Reserved),
            // over-reserved still reads Reserved
            ((150, 150, 100, 100), Reserved),
            // a reservation decides before an empty balance
            ((0, 100, 100, 100), Reserved),
            ((100, 40, 100, 100), ReservedPartially),
            ((0, 40, 40, 100), ReservedPartially),
            // no reservation: empty balance first
            ((0, 0, 0, 100), NoInventory),
            ((0, 0, 0, 0), NoInventory),
            ((100, 0, 100, 100), Covered),
            ((150, 0, 100, 100), Covered),
            ((100, 0, 0, 0), Covered),
            ((100, 0, 60, 100), PartiallyCovered),
            // owned but all reserved elsewhere
            ((100, 0, 0, 100), Missing),
        ];
        for ((owned, here, covered, required), expected) in cases {
            assert_eq!(
                quantity_coverage_state(owned, here, covered, required),
                expected,
                "owned {owned}, here {here}, covered {covered}, required {required}"
            );
        }
    }

    #[test]
    fn figures_use_free_stock() {
        let line = figures(1_200, 0, 300, 1_000);
        assert_eq!(line.unreserved, 900);
        assert_eq!(line.available, 900);
        assert_eq!(line.covered, 900);
        assert_eq!(line.missing, 100);
        assert_eq!(line.reservable_additional, 900);
        assert_eq!(line.state, PartiallyCovered);
        assert!(!line.cost_resolvable);
    }

    #[test]
    fn figures_count_this_builds_reservation_as_available() {
        let line = figures(500, 200, 100, 300);
        assert_eq!(line.unreserved, 200);
        assert_eq!(line.available, 400);
        assert_eq!(line.covered, 300);
        assert_eq!(line.missing, 0);
        assert_eq!(line.reservable_additional, 100);
        assert_eq!(line.state, ReservedPartially);
        assert!(line.cost_resolvable);
    }

    #[test]
    fn reservations_beyond_the_balance_saturate() {
        let line = figures(100, 0, 250, 50);
        assert_eq!(line.unreserved, 0);
        assert_eq!(line.available, 0);
        assert_eq!(line.missing, 50);
        assert_eq!(line.state, Missing);
    }

    #[test]
    fn projected_cost_needs_free_stock_and_an_average() {
        let average = Some(Money::parse("5").unwrap());
        let covered = figures(1_200, 0, 0, 1_000);
        assert_eq!(
            projected_historical_cost(&covered, average, 1_000).unwrap(),
            Some(Money::parse("5000").unwrap())
        );
        assert_eq!(
            projected_historical_cost(&covered, None, 1_000).unwrap(),
            None
        );
        let short = figures(1_200, 0, 300, 1_000);
        assert_eq!(
            projected_historical_cost(&short, average, 1_000).unwrap(),
            None
        );
    }

    #[test]
    fn reconciliation_warning_names_the_direction() {
        assert_eq!(esi_reconciliation_warning(0), None);
        assert!(esi_reconciliation_warning(5)
            .unwrap()
            .starts_with("EVE currently reports 5 more units"));
        assert!(esi_reconciliation_warning(-7)
            .unwrap()
            .starts_with("ISK Works accounts for 7 more units"));
    }

    #[test]
    fn source_warnings() {
        assert!(coverage_source_warnings(false, false)[0].starts_with("No EVE connection"));
        assert!(coverage_source_warnings(true, false)[0].starts_with("No complete ESI asset"));
        assert!(coverage_source_warnings(true, true).is_empty());
    }
}
