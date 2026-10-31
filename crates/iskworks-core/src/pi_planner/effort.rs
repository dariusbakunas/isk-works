//! How much tending a plan takes: extractor resets and haul runs, and the
//! storage each colony needs to buffer goods between them.
//!
//! Many players move output up to the planet's customs office or skyhook at
//! every login and only haul from it now and then. The launchpad then holds a
//! login's worth of goods and the skyhook a haul's worth.

/// m³ a launchpad holds.
pub const LAUNCHPAD_M3: f64 = 10_000.0;
/// m³ an extra storage facility holds.
pub const STORAGE_M3: f64 = 12_000.0;
/// m³ a customs office or skyhook holds (observed in game, 2026-10-31).
pub const SKYHOOK_M3: f64 = 35_000.0;
/// The reset and haul intervals the planner offers, in days.
pub const INTERVAL_DAYS: [u32; 6] = [1, 2, 3, 4, 7, 14];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffortSettings {
    /// How often extractor programs are restarted.
    pub reset_days: u32,
    /// How often goods are hauled between planets and to market.
    pub haul_days: u32,
    /// Output goes up to the skyhook at every login.
    pub uplift_at_login: bool,
}

impl EffortSettings {
    /// How often a character with extraction colonies logs in anyway: at
    /// every reset, or every haul when that comes sooner.
    #[must_use]
    pub fn login_days(&self, extracts: bool) -> u32 {
        if extracts {
            self.reset_days.min(self.haul_days)
        } else {
            self.haul_days
        }
    }
}

/// What a colony holds between visits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Buffers {
    /// On the launchpad and storage facilities.
    pub on_planet_m3: f64,
    /// At the customs office or skyhook.
    pub skyhook_m3: f64,
}

/// Buffers for a colony moving `m3_per_hour` (output plus inputs).
#[must_use]
pub fn buffers(m3_per_hour: f64, login_days: u32, settings: EffortSettings) -> Buffers {
    let over = |days: u32| m3_per_hour * 24.0 * f64::from(days);
    if settings.uplift_at_login {
        Buffers {
            on_planet_m3: over(login_days.min(settings.haul_days)),
            skyhook_m3: over(settings.haul_days),
        }
    } else {
        Buffers {
            on_planet_m3: over(settings.haul_days),
            skyhook_m3: 0.0,
        }
    }
}

/// Storage facilities needed beyond the colony's launchpad.
#[must_use]
pub fn storage_needed(on_planet_m3: f64) -> u32 {
    if on_planet_m3 <= LAUNCHPAD_M3 {
        0
    } else {
        ((on_planet_m3 - LAUNCHPAD_M3) / STORAGE_M3).ceil() as u32
    }
}

impl Buffers {
    /// The skyhook can't be extended, so overflowing it means hauling more
    /// often.
    #[must_use]
    pub fn skyhook_fits(&self) -> bool {
        self.skyhook_m3 <= SKYHOOK_M3
    }
}

/// One colony, for effort counting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColonyVisit {
    pub character: usize,
    pub extracts: bool,
    /// Output plus inputs moved through the customs office.
    pub m3_per_hour: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Effort {
    pub logins_per_week_by_character: Vec<f64>,
    pub haul_runs_per_week: f64,
    /// Customs offices visited on each run.
    pub stops_per_run: u32,
    pub m3_per_run: f64,
}

fn per_week(days: u32) -> f64 {
    7.0 / f64::from(days.max(1))
}

fn lcm(a: u32, b: u32) -> u32 {
    let gcd = |mut a: u32, mut b: u32| {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    };
    a / gcd(a, b).max(1) * b
}

/// Logins, haul runs and haul size for these colonies. A reset and a haul on
/// the same day count as one login.
#[must_use]
pub fn effort(colonies: &[ColonyVisit], characters: usize, settings: EffortSettings) -> Effort {
    let reset = settings.reset_days.max(1);
    let haul = settings.haul_days.max(1);
    let logins_per_week_by_character = (0..characters)
        .map(|character| {
            let own = colonies
                .iter()
                .filter(|colony| colony.character == character);
            match (own.clone().any(|colony| colony.extracts), own.count() > 0) {
                (true, _) => per_week(reset) + per_week(haul) - per_week(lcm(reset, haul)),
                (false, true) => per_week(haul),
                (false, false) => 0.0,
            }
        })
        .collect();
    let m3_per_hour = colonies
        .iter()
        .map(|colony| colony.m3_per_hour)
        .sum::<f64>();
    Effort {
        logins_per_week_by_character,
        haul_runs_per_week: if colonies.is_empty() {
            0.0
        } else {
            per_week(haul)
        },
        stops_per_run: colonies.len() as u32,
        m3_per_run: m3_per_hour * 24.0 * f64::from(haul),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two P1 at 160/h each, 0.19 m³ a unit.
    const TWO_P1: f64 = 2.0 * 160.0 * 0.19;

    fn settings(reset_days: u32, haul_days: u32, uplift_at_login: bool) -> EffortSettings {
        EffortSettings {
            reset_days,
            haul_days,
            uplift_at_login,
        }
    }

    #[test]
    fn uplifting_at_each_login_keeps_the_launchpad_small() {
        let weekly = settings(3, 7, true);
        let held = buffers(TWO_P1, weekly.login_days(true), weekly);
        assert!((held.on_planet_m3 - 4_377.6).abs() < 1e-6);
        assert!((held.skyhook_m3 - 10_214.4).abs() < 1e-6);
        assert_eq!(storage_needed(held.on_planet_m3), 0);
        assert!(held.skyhook_fits());
    }

    #[test]
    fn hauling_straight_from_the_launchpad_needs_storage() {
        let weekly = settings(3, 7, false);
        let held = buffers(TWO_P1, weekly.login_days(true), weekly);
        assert!((held.on_planet_m3 - 10_214.4).abs() < 1e-6);
        assert_eq!(storage_needed(held.on_planet_m3), 1);
        assert_eq!(storage_needed(LAUNCHPAD_M3 + 2.0 * STORAGE_M3 + 1.0), 3);
    }

    #[test]
    fn a_factory_overflows_the_skyhook_on_a_fortnightly_haul() {
        // Four P1 inputs at 160/h plus the P3 output.
        let factory = 4.0 * 160.0 * 0.19 + 6.0 * 3.0;
        assert!(!buffers(factory, 14, settings(3, 14, true)).skyhook_fits());
        assert!(buffers(factory, 3, settings(3, 3, true)).skyhook_fits());
    }

    #[test]
    fn resets_and_hauls_on_the_same_day_are_one_login() {
        let colonies = [
            ColonyVisit {
                character: 0,
                extracts: true,
                m3_per_hour: 30.4,
            },
            ColonyVisit {
                character: 1,
                extracts: false,
                m3_per_hour: 10.0,
            },
        ];
        let same_day = effort(&colonies, 3, settings(3, 3, true));
        assert!((same_day.logins_per_week_by_character[0] - 7.0 / 3.0).abs() < 1e-9);
        assert!((same_day.logins_per_week_by_character[1] - 7.0 / 3.0).abs() < 1e-9);
        assert_eq!(same_day.logins_per_week_by_character[2], 0.0);
        assert_eq!(same_day.stops_per_run, 2);
        assert!((same_day.m3_per_run - 40.4 * 72.0).abs() < 1e-6);

        // Resets every 3 days and hauls every 7 meet once every 21 days.
        let weekly = effort(&colonies, 2, settings(3, 7, true));
        assert!((weekly.logins_per_week_by_character[0] - 3.0).abs() < 1e-9);
        assert_eq!(weekly.logins_per_week_by_character[1], 1.0);
        assert_eq!(weekly.haul_runs_per_week, 1.0);
    }
}
