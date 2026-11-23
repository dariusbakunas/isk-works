//! Invention: the chance of an attempt, the blueprint copy it yields, what
//! it costs, and how many attempts a target takes.
//!
//! Pure rules. An attempt spends datacores, an optional decryptor, a job fee
//! and one run of a Tech I blueprint copy, and succeeds with some chance.
//! Every figure that depends on a missing price or index is `None`, never
//! zero. See `docs/implementation/invention.md`.

use std::collections::BTreeMap;

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::{IndustryFacilityProfile, Money};

mod attempts;
mod quote;
#[cfg(test)]
mod tests;

pub use attempts::{attempts_for, AttemptPlan};
pub use quote::{
    quote, InventionOption, InventionQuote, QuoteDecryptor, QuoteGap, QuoteInputs, QuoteMaterial,
};

/// Material efficiency of every invented copy before a decryptor.
pub const BASE_INVENTED_ME: i32 = 2;
/// Time efficiency of every invented copy before a decryptor.
pub const BASE_INVENTED_TE: i32 = 4;
/// A science job's cost base is this share of the output's EIV.
pub const SCIENCE_JOB_COST_BASE_RATE: Decimal = Decimal::from_parts(2, 0, 0, false, 2);

/// The three skill levels the chance depends on: the encryption skill and
/// the science skill of each datacore. Other skills a job lists (Capital
/// Ship Construction) are requirements only.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventionSkills {
    pub encryption: u8,
    pub science: [u8; 2],
}

/// What a decryptor changes. [`DecryptorEffect::NONE`] is "no decryptor".
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecryptorEffect {
    #[serde(with = "rust_decimal::serde::str")]
    pub probability_multiplier: Decimal,
    pub me_modifier: i32,
    pub te_modifier: i32,
    pub run_modifier: i32,
}

impl DecryptorEffect {
    pub const NONE: Self = Self {
        probability_multiplier: Decimal::ONE,
        me_modifier: 0,
        te_modifier: 0,
        run_modifier: 0,
    };
}

/// The blueprint copy a successful attempt yields.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventedCopy {
    pub material_efficiency: u8,
    pub time_efficiency: u8,
    pub runs: u64,
}

/// `base × (1 + encryption / 40 + (science₁ + science₂) / 30) × decryptor`,
/// capped at 1. Levels above 5 count as 5.
#[must_use]
pub fn invention_chance(
    base_chance: Decimal,
    skills: InventionSkills,
    decryptor: &DecryptorEffect,
) -> Decimal {
    (base_chance * skill_chance_factor(skills) * decryptor.probability_multiplier)
        .clamp(Decimal::ZERO, Decimal::ONE)
}

/// The skill part of [`invention_chance`] alone, for display ("×1.367").
#[must_use]
pub fn skill_chance_factor(skills: InventionSkills) -> Decimal {
    let level = |value: u8| Decimal::from(value.min(5));
    Decimal::ONE
        + level(skills.encryption) / Decimal::from(40)
        + (level(skills.science[0]) + level(skills.science[1])) / Decimal::from(30)
}

/// A chance as the game shows it: a percentage truncated to one decimal
/// (0.4375 reads "43.7", not "43.8").
#[must_use]
pub fn chance_percent_truncated(chance: Decimal) -> Decimal {
    (chance * Decimal::ONE_HUNDRED).trunc_with_scale(1)
}

/// ME `2 + modifier`, TE `4 + modifier`, runs `base + modifier`. The source
/// copy's own ME and TE do not carry over. ME and TE stay within what a
/// blueprint can hold, and a copy always has at least one run.
#[must_use]
pub fn invented_copy(base_runs: u64, decryptor: &DecryptorEffect) -> InventedCopy {
    let runs = i128::from(base_runs) + i128::from(decryptor.run_modifier);
    InventedCopy {
        material_efficiency: (BASE_INVENTED_ME + decryptor.me_modifier).clamp(0, 10) as u8,
        time_efficiency: (BASE_INVENTED_TE + decryptor.te_modifier).clamp(0, 20) as u8,
        runs: u64::try_from(runs.max(1)).unwrap_or(u64::MAX),
    }
}

/// The rates a science job fee is charged at. The cost index is the
/// activity's own (invention or copying), not the manufacturing one.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ScienceJobRates {
    pub system_cost_index: Option<Decimal>,
    pub facility_tax_percent: Decimal,
    pub scc_surcharge_percent: Decimal,
    pub alliance_surcharge_percent: Decimal,
}

impl ScienceJobRates {
    /// A facility profile's tax and surcharges with the given index. The
    /// profile's manual cost index and its job cost reduction describe
    /// manufacturing and are not used.
    #[must_use]
    pub fn from_profile(
        profile: &IndustryFacilityProfile,
        system_cost_index: Option<Decimal>,
    ) -> Self {
        Self {
            system_cost_index,
            facility_tax_percent: profile.facility_tax_percent,
            scc_surcharge_percent: profile.scc_surcharge_percent,
            alliance_surcharge_percent: profile.alliance_surcharge_percent,
        }
    }
}

/// An invention or copying job fee, line by line as the industry window
/// shows it.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScienceJobFee {
    /// EIV of the blueprint's manufacturing materials, for all runs.
    pub estimated_item_value: Option<Money>,
    /// 2% of the EIV; every other line is a share of this.
    pub job_cost_base: Option<Money>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub system_cost_index: Option<Decimal>,
    pub system_index_cost: Option<Money>,
    pub facility_tax: Option<Money>,
    pub scc_surcharge: Option<Money>,
    pub alliance_surcharge: Option<Money>,
    /// `None` when the EIV or the cost index is unknown.
    pub total: Option<Money>,
}

/// `job cost base = 0.02 × EIV × runs`, then the system cost index, the
/// facility tax and the surcharges each as a share of that base. `eiv` is
/// for one run of the output blueprint (invention) or of the blueprint
/// being copied (copying, where `runs` is copies × runs per copy).
#[must_use]
pub fn science_job_fee(eiv: Option<Money>, runs: u64, rates: ScienceJobRates) -> ScienceJobFee {
    let estimated_item_value = eiv.and_then(|value| value.0.checked_mul(Decimal::from(runs)));
    let base = estimated_item_value.and_then(|value| value.checked_mul(SCIENCE_JOB_COST_BASE_RATE));
    let share =
        |percent: Decimal| base.and_then(|base| base.checked_mul(percent / Decimal::ONE_HUNDRED));
    let system_index_cost = base
        .zip(rates.system_cost_index)
        .and_then(|(base, index)| base.checked_mul(index));
    let facility_tax = share(rates.facility_tax_percent);
    let scc_surcharge = share(rates.scc_surcharge_percent);
    let alliance_surcharge = share(rates.alliance_surcharge_percent);
    let total = [
        system_index_cost,
        facility_tax,
        scc_surcharge,
        alliance_surcharge,
    ]
    .into_iter()
    .try_fold(Decimal::ZERO, |sum, line| sum.checked_add(line?));
    let money = |value: Option<Decimal>| value.map(|value| Money(value.round_dp(4)));
    ScienceJobFee {
        estimated_item_value: money(estimated_item_value),
        job_cost_base: money(base),
        system_cost_index: rates.system_cost_index,
        system_index_cost: money(system_index_cost),
        facility_tax: money(facility_tax),
        scc_surcharge: money(scc_surcharge),
        alliance_surcharge: money(alliance_surcharge),
        total: money(total),
    }
}

/// `base time × (1 − 0.03 × Advanced Industry level)`, in whole seconds.
/// Structure and rig time bonuses are not applied.
#[must_use]
pub fn invention_duration_seconds(base_seconds: u64, advanced_industry_level: u8) -> u64 {
    let factor = Decimal::ONE
        - Decimal::from(advanced_industry_level.min(5)) * Decimal::from_parts(3, 0, 0, false, 2);
    (Decimal::from(base_seconds) * factor)
        .round()
        .to_u64()
        .unwrap_or(base_seconds)
}

/// Manufacturing cost of one run of the product, by the copy's ME. `None`
/// for an ME whose cost could not be worked out.
pub type ManufacturingCostByMe = BTreeMap<u8, Option<Money>>;
