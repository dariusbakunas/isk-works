//! Extractor yield estimates. Every figure here comes from in-game readings
//! (2026-10-31, see the PI Planner spec's calibration tables).
//!
//! The model: units/hour per extractor head = head value × the duration
//! factor of the program length. A head value is the number the game shows
//! next to each head in the extraction program window; without one, it is
//! estimated from the scan tooltip's density %.

/// `(program hours, units per hour per point of head value)`, measured with
/// one head of value 146.1 on Base Metals, Q-3HS5 IX. Longer programs
/// average less per hour.
const DURATION_FACTORS: [(f64, f64); 10] = [
    (1.0, 35.9),
    (2.0, 35.1),
    (5.0, 36.6),
    (14.0, 32.9),
    (47.0, 22.0),
    (63.0, 19.2),
    (106.0, 14.9),
    (184.0, 10.8),
    (232.0, 9.4),
    (336.0, 7.4),
];

/// Head value per density % point. CALIBRATING: two uncontested readings gave
/// ~5, a planet with another player's extractors nearby gave 2.5, so this
/// takes the cautious end.
pub const HEAD_VALUE_PER_DENSITY_PCT: f64 = 2.4;

/// Where a planet sits, for its default density.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum SecurityBand {
    High,
    Low,
    Null,
    Wormhole,
}

impl SecurityBand {
    /// The same split the SDE reads use: wormhole space first, then ≥ 0.45
    /// high, > 0 low, the rest null.
    #[must_use]
    pub fn classify(security: Option<f64>, wormhole: bool) -> Self {
        match security {
            _ if wormhole => Self::Wormhole,
            Some(security) if security >= 0.45 => Self::High,
            Some(security) if security > 0.0 => Self::Low,
            _ => Self::Null,
        }
    }

    /// The density % assumed for a resource nobody has scanned. UNCALIBRATED
    /// placeholders, kept pessimistic.
    #[must_use]
    pub const fn default_density_pct(self) -> f64 {
        match self {
            Self::High => 15.0,
            Self::Low => 20.0,
            Self::Null | Self::Wormhole => 25.0,
        }
    }
}

/// What the player entered for one resource on one planet.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DensityEntry {
    /// The scan tooltip's "Resource Density" %, 0–100.
    pub density_pct: Option<f64>,
    /// The average head value with heads on the hotspot, from a draft
    /// program. Overrides the density.
    pub head_value: Option<f64>,
}

/// Where a yield estimate came from, most trustworthy first.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum YieldSource {
    HeadValue,
    Scanned,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YieldEstimate {
    pub units_per_hour_per_head: f64,
    pub head_value: f64,
    pub source: YieldSource,
}

/// Units per hour per point of head value for a program of `hours`,
/// interpolated between the measurements and clamped to their range.
#[must_use]
pub fn duration_factor(hours: f64) -> f64 {
    let (first_hours, first) = DURATION_FACTORS[0];
    if hours <= first_hours {
        return first;
    }
    for pair in DURATION_FACTORS.windows(2) {
        let [(low_hours, low), (high_hours, high)] = [pair[0], pair[1]];
        if hours <= high_hours {
            return low + (high - low) * (hours - low_hours) / (high_hours - low_hours);
        }
    }
    DURATION_FACTORS[DURATION_FACTORS.len() - 1].1
}

/// The yield of one head on the resource's best hotspot, for extractor
/// programs that run `program_hours`.
#[must_use]
pub fn resolve_yield(
    entry: Option<&DensityEntry>,
    band: SecurityBand,
    program_hours: f64,
) -> YieldEstimate {
    let entry = entry.copied().unwrap_or_default();
    let (head_value, source) = match (entry.head_value, entry.density_pct) {
        (Some(value), _) if value > 0.0 => (value, YieldSource::HeadValue),
        (_, Some(pct)) => (
            pct.clamp(0.0, 100.0) * HEAD_VALUE_PER_DENSITY_PCT,
            YieldSource::Scanned,
        ),
        _ => (
            band.default_density_pct() * HEAD_VALUE_PER_DENSITY_PCT,
            YieldSource::Default,
        ),
    };
    YieldEstimate {
        units_per_hour_per_head: head_value * duration_factor(program_hours),
        head_value,
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64, tolerance: f64) -> bool {
        (actual - expected).abs() <= expected * tolerance
    }

    #[test]
    fn duration_factor_interpolates_and_clamps() {
        assert_eq!(duration_factor(1.0), 35.9);
        assert_eq!(duration_factor(336.0), 7.4);
        assert_eq!(duration_factor(0.25), 35.9);
        assert_eq!(duration_factor(1_000.0), 7.4);
        assert!(close(duration_factor(55.0), 20.6, 0.001));
        assert!(duration_factor(72.0) > duration_factor(168.0));
    }

    #[test]
    fn reproduces_the_in_game_readings() {
        let head = |head_value: f64, hours: f64| {
            resolve_yield(
                Some(&DensityEntry {
                    density_pct: None,
                    head_value: Some(head_value),
                }),
                SecurityBand::Null,
                hours,
            )
            .units_per_hour_per_head
        };
        // One Base Metals head, 47 h program: 3 219/h.
        assert!(close(head(146.1, 47.0), 3_219.0, 0.1));
        // Seven Noble Gas heads summing 2 295.3, 49.5 h program: 52 660/h.
        assert!(close(head(2_295.3, 49.5), 52_660.0, 0.1));
        // One Aqueous Liquids head, 49.5 h program: 4 381/h.
        assert!(close(head(206.3, 49.5), 4_381.0, 0.1));
    }

    #[test]
    fn a_head_value_beats_a_density_which_beats_the_band_default() {
        let both = DensityEntry {
            density_pct: Some(80.0),
            head_value: Some(150.0),
        };
        assert_eq!(
            resolve_yield(Some(&both), SecurityBand::High, 24.0).source,
            YieldSource::HeadValue
        );
        let scanned = DensityEntry {
            density_pct: Some(50.0),
            head_value: None,
        };
        let estimate = resolve_yield(Some(&scanned), SecurityBand::High, 24.0);
        assert_eq!(estimate.source, YieldSource::Scanned);
        assert_eq!(estimate.head_value, 50.0 * HEAD_VALUE_PER_DENSITY_PCT);
        let blank = resolve_yield(None, SecurityBand::High, 24.0);
        assert_eq!(blank.source, YieldSource::Default);
        assert_eq!(blank.head_value, 15.0 * HEAD_VALUE_PER_DENSITY_PCT);
    }

    #[test]
    fn bands_follow_security() {
        assert_eq!(SecurityBand::classify(Some(0.5), false), SecurityBand::High);
        assert_eq!(SecurityBand::classify(Some(0.3), false), SecurityBand::Low);
        assert_eq!(
            SecurityBand::classify(Some(-0.58), false),
            SecurityBand::Null
        );
        assert_eq!(
            SecurityBand::classify(Some(-1.0), true),
            SecurityBand::Wormhole
        );
        assert!(
            SecurityBand::Null.default_density_pct() > SecurityBand::High.default_density_pct()
        );
    }
}
