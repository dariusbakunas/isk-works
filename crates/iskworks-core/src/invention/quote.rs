use rust_decimal::Decimal;
use serde::Serialize;

use super::{
    attempts_for, invented_copy, invention_chance, AttemptPlan, DecryptorEffect, InventedCopy,
    InventionSkills, ManufacturingCostByCopy,
};
use crate::Money;

/// A datacore (or other input) one attempt consumes.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteMaterial {
    pub type_id: i64,
    pub type_name: String,
    pub quantity: u64,
    pub unit_price: Option<Money>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteDecryptor {
    pub type_id: i64,
    pub name: String,
    #[serde(flatten)]
    pub effect: DecryptorEffect,
    pub unit_price: Option<Money>,
}

/// Everything a quote is worked out from. Prices and costs that could not
/// be found are `None`.
#[derive(Debug, Clone)]
pub struct QuoteInputs {
    pub base_chance: Decimal,
    /// Runs on the invented copy before any decryptor.
    pub base_runs: u64,
    pub skills: InventionSkills,
    pub materials: Vec<QuoteMaterial>,
    pub decryptors: Vec<QuoteDecryptor>,
    /// The invention job fee for one attempt.
    pub job_fee: Option<Money>,
    /// One run of the Tech I blueprint copy.
    pub copy_cost: Option<Money>,
    pub manufacturing_cost_by_copy: ManufacturingCostByCopy,
    /// Units one manufacturing run of the product yields.
    pub output_per_run: u64,
    /// Manufacturing runs the user wants copies for.
    pub target_runs: u64,
    /// For the shopping figure, such as 0.9.
    pub confidence: Decimal,
}

/// Why an option's costs are incomplete.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum QuoteGap {
    MaterialPrice { type_id: i64 },
    DecryptorPrice { type_id: i64 },
    JobFee,
    CopyCost,
    ManufacturingCost,
}

/// One row of the comparison: "no decryptor" or one decryptor.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventionOption {
    /// `None` for "no decryptor".
    pub decryptor: Option<QuoteDecryptor>,
    /// Rounded to six decimals.
    #[serde(with = "rust_decimal::serde::str")]
    pub chance: Decimal,
    pub copy: InventedCopy,
    /// Datacores, decryptor, job fee and one copy run.
    pub attempt_cost: Option<Money>,
    /// `attempt cost ÷ chance`: the expected cost of one successful copy.
    pub cost_per_copy: Option<Money>,
    /// Blueprint cost per manufacturing run.
    pub cost_per_run: Option<Money>,
    /// One manufacturing run at the copy's ME, as a share of a job over all
    /// the copy's runs, inputs bought at market.
    pub manufacturing_cost_per_run: Option<Money>,
    /// `(manufacturing cost per run + cost per run) ÷ output per run`.
    pub unit_cost: Option<Money>,
    /// Against "no decryptor"; `None` when either side is incomplete.
    #[serde(with = "rust_decimal::serde::str_option")]
    pub unit_cost_delta: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub unit_cost_delta_percent: Option<Decimal>,
    /// `ceil(target runs ÷ runs on the copy)`.
    pub copies_needed: u64,
    pub attempts: Option<AttemptPlan>,
    /// `attempt cost × confident attempts`.
    pub shopping_cost: Option<Money>,
    /// Lowest complete unit cost; a tie goes to fewer expected attempts.
    pub best: bool,
    pub complete: bool,
    pub gaps: Vec<QuoteGap>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventionQuote {
    /// "No decryptor" first, then the decryptors in the order given.
    pub options: Vec<InventionOption>,
}

#[must_use]
pub fn quote(inputs: &QuoteInputs) -> InventionQuote {
    let mut options = std::iter::once(None)
        .chain(inputs.decryptors.iter().map(Some))
        .map(|decryptor| option(inputs, decryptor))
        .collect::<Vec<_>>();

    let baseline = options[0].unit_cost.filter(|_| options[0].complete);
    for option in &mut options {
        let delta = baseline
            .zip(option.unit_cost.filter(|_| option.complete))
            .map(|(none, this)| this.0 - none.0);
        option.unit_cost_delta = delta;
        option.unit_cost_delta_percent = delta.zip(baseline).and_then(|(delta, none)| {
            (!none.0.is_zero()).then(|| (delta / none.0 * Decimal::ONE_HUNDRED).round_dp(1))
        });
    }
    let best = options
        .iter()
        .enumerate()
        .filter(|(_, option)| option.complete)
        .filter_map(|(index, option)| {
            let attempts = option.attempts.map_or(u64::MAX, |plan| plan.expected);
            Some(((option.unit_cost?, attempts, index), index))
        })
        .min()
        .map(|(_, index)| index);
    if let Some(best) = best {
        options[best].best = true;
    }
    InventionQuote { options }
}

fn option(inputs: &QuoteInputs, decryptor: Option<&QuoteDecryptor>) -> InventionOption {
    let effect = decryptor.map_or(DecryptorEffect::NONE, |decryptor| decryptor.effect);
    let chance = invention_chance(inputs.base_chance, &inputs.skills, &effect);
    let copy = invented_copy(inputs.base_runs, &effect);
    let mut gaps = Vec::new();

    let mut attempt_cost = Some(Decimal::ZERO);
    let mut add = |value: Option<Decimal>, gap: QuoteGap| match value {
        Some(value) => attempt_cost = attempt_cost.and_then(|sum| sum.checked_add(value)),
        None => {
            attempt_cost = None;
            gaps.push(gap);
        }
    };
    for material in &inputs.materials {
        add(
            material
                .unit_price
                .and_then(|price| price.0.checked_mul(Decimal::from(material.quantity))),
            QuoteGap::MaterialPrice {
                type_id: material.type_id,
            },
        );
    }
    if let Some(decryptor) = decryptor {
        add(
            decryptor.unit_price.map(|price| price.0),
            QuoteGap::DecryptorPrice {
                type_id: decryptor.type_id,
            },
        );
    }
    add(inputs.job_fee.map(|fee| fee.0), QuoteGap::JobFee);
    add(inputs.copy_cost.map(|cost| cost.0), QuoteGap::CopyCost);

    let cost_per_copy = attempt_cost
        .filter(|_| chance > Decimal::ZERO)
        .and_then(|cost| cost.checked_div(chance));
    let cost_per_run = cost_per_copy.and_then(|cost| cost.checked_div(Decimal::from(copy.runs)));
    let manufacturing_cost_per_run = inputs
        .manufacturing_cost_by_copy
        .get(&(copy.material_efficiency, copy.runs))
        .copied()
        .flatten();
    if manufacturing_cost_per_run.is_none() {
        gaps.push(QuoteGap::ManufacturingCost);
    }
    let unit_cost = cost_per_run
        .zip(manufacturing_cost_per_run)
        .filter(|_| inputs.output_per_run > 0)
        .and_then(|(blueprint, manufacturing)| {
            manufacturing
                .0
                .checked_add(blueprint)?
                .checked_div(Decimal::from(inputs.output_per_run))
        });

    let copies_needed = inputs.target_runs.div_ceil(copy.runs);
    let attempts = attempts_for(copies_needed, chance, inputs.confidence);
    let shopping_cost = attempt_cost
        .zip(attempts)
        .and_then(|(cost, plan)| cost.checked_mul(Decimal::from(plan.confident)));
    let money = |value: Option<Decimal>| value.map(|value| Money(value.round_dp(4)));
    InventionOption {
        decryptor: decryptor.cloned(),
        chance: display_chance(chance),
        copy,
        attempt_cost: money(attempt_cost),
        cost_per_copy: money(cost_per_copy),
        cost_per_run: money(cost_per_run),
        manufacturing_cost_per_run,
        unit_cost: money(unit_cost),
        unit_cost_delta: None,
        unit_cost_delta_percent: None,
        copies_needed,
        attempts,
        shopping_cost: money(shopping_cost),
        best: false,
        complete: gaps.is_empty() && unit_cost.is_some(),
        gaps,
    }
}

/// A chance for display: six decimals, without trailing zeros. Costs and
/// attempts are worked out from the exact value.
fn display_chance(chance: Decimal) -> Decimal {
    chance.round_dp(6).normalize()
}
