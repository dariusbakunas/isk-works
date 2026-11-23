//! The reference fixture is a real job: Tristan Blueprint to Nemesis
//! Blueprint with every skill at 5, read from the industry window on
//! 2026-11-23, once with no decryptor and once with a Process Decryptor.

use std::str::FromStr;
use std::time::Instant;

use super::*;

fn dec(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}

fn isk(value: &str) -> Money {
    Money(dec(value))
}

const ALL_FIVE: InventionSkills = InventionSkills {
    encryption: 5,
    science: [5, 5],
};

fn process() -> DecryptorEffect {
    DecryptorEffect {
        probability_multiplier: dec("1.1"),
        me_modifier: 3,
        te_modifier: 6,
        run_modifier: 0,
    }
}

fn augmentation() -> DecryptorEffect {
    DecryptorEffect {
        probability_multiplier: dec("0.6"),
        me_modifier: -2,
        te_modifier: 2,
        run_modifier: 9,
    }
}

#[test]
fn chance_matches_the_industry_window() {
    let none = invention_chance(dec("0.30"), ALL_FIVE, &DecryptorEffect::NONE);
    assert_eq!(none, dec("0.4375"));
    // The game truncates: 43.75% reads 43.7%.
    assert_eq!(chance_percent_truncated(none), dec("43.7"));

    let with_process = invention_chance(dec("0.30"), ALL_FIVE, &process());
    assert_eq!(with_process, dec("0.48125"));
    assert_eq!(chance_percent_truncated(with_process), dec("48.1"));
}

#[test]
fn chance_uses_each_skill_level_and_is_capped_at_one() {
    let untrained = InventionSkills {
        encryption: 0,
        science: [0, 0],
    };
    assert_eq!(
        invention_chance(dec("0.34"), untrained, &DecryptorEffect::NONE),
        dec("0.34")
    );
    let fours = InventionSkills {
        encryption: 4,
        science: [4, 4],
    };
    // 0.34 × (1 + 4/40 + 8/30)
    assert_eq!(
        chance_percent_truncated(invention_chance(dec("0.34"), fours, &DecryptorEffect::NONE)),
        dec("46.4")
    );
    let sure = DecryptorEffect {
        probability_multiplier: dec("5"),
        ..DecryptorEffect::NONE
    };
    assert_eq!(invention_chance(dec("0.34"), ALL_FIVE, &sure), Decimal::ONE);
    // A level above 5 counts as 5.
    let over = InventionSkills {
        encryption: 9,
        science: [9, 9],
    };
    assert_eq!(
        invention_chance(dec("0.30"), over, &DecryptorEffect::NONE),
        dec("0.4375")
    );
}

#[test]
fn skill_factor_is_reported_alone() {
    assert_eq!(skill_chance_factor(ALL_FIVE).round_dp(4), dec("1.4583"));
    let fours = InventionSkills {
        encryption: 4,
        science: [4, 4],
    };
    assert_eq!(skill_chance_factor(fours).round_dp(3), dec("1.367"));
}

#[test]
fn invented_copy_matches_the_industry_window() {
    assert_eq!(
        invented_copy(1, &DecryptorEffect::NONE),
        InventedCopy {
            material_efficiency: 2,
            time_efficiency: 4,
            runs: 1,
        }
    );
    assert_eq!(
        invented_copy(1, &process()),
        InventedCopy {
            material_efficiency: 5,
            time_efficiency: 10,
            runs: 1,
        }
    );
}

#[test]
fn augmentation_floors_me_at_zero_and_adds_runs() {
    assert_eq!(
        invented_copy(1, &augmentation()),
        InventedCopy {
            material_efficiency: 0,
            time_efficiency: 6,
            runs: 10,
        }
    );
    assert_eq!(invented_copy(10, &augmentation()).runs, 19);
    // A negative run modifier never leaves a copy without a run.
    let fewer = DecryptorEffect {
        run_modifier: -5,
        ..DecryptorEffect::NONE
    };
    assert_eq!(invented_copy(1, &fewer).runs, 1);
}

fn fixture_rates(index: Option<&str>) -> ScienceJobRates {
    ScienceJobRates {
        system_cost_index: index.map(dec),
        facility_tax_percent: dec("1"),
        scc_surcharge_percent: dec("4"),
        alliance_surcharge_percent: Decimal::ZERO,
    }
}

/// The window showed EIV 12,687,150; job cost base 253,743; system cost
/// index 7.60% = 19,289; facility tax 1% = 2,537; SCC 4% = 10,150. The
/// structure's rig and role bonus (not modelled) took 5,294 off the index
/// line, so the total here is 31,976 where the game charged 26,682.
#[test]
fn job_fee_lines_match_the_industry_window() {
    let fee = science_job_fee(Some(isk("12687150")), 1, fixture_rates(Some("0.076018")));
    let whole = |value: Option<Money>| value.unwrap().0.round();

    assert_eq!(fee.estimated_item_value, Some(isk("12687150")));
    assert_eq!(whole(fee.job_cost_base), dec("253743"));
    assert_eq!(whole(fee.system_index_cost), dec("19289"));
    assert_eq!(whole(fee.facility_tax), dec("2537"));
    assert_eq!(whole(fee.scc_surcharge), dec("10150"));
    assert_eq!(fee.alliance_surcharge, Some(Money(Decimal::ZERO)));
    assert_eq!(whole(fee.total), dec("31976"));
}

#[test]
fn job_fee_scales_with_runs() {
    let one = science_job_fee(Some(isk("1000000")), 1, fixture_rates(Some("0.05")));
    let ten = science_job_fee(Some(isk("1000000")), 10, fixture_rates(Some("0.05")));
    // base 20,000 × (5% + 1% + 4%)
    assert_eq!(one.total, Some(isk("2000")));
    assert_eq!(ten.total, Some(isk("20000")));
}

#[test]
fn a_job_fee_without_its_eiv_or_index_is_unknown_not_zero() {
    let no_index = science_job_fee(Some(isk("1000000")), 1, fixture_rates(None));
    assert_eq!(no_index.total, None);
    assert_eq!(no_index.system_index_cost, None);
    // The lines that can be worked out still are.
    assert_eq!(no_index.facility_tax, Some(isk("200")));

    let no_eiv = science_job_fee(None, 1, fixture_rates(Some("0.05")));
    assert_eq!(no_eiv.total, None);
    assert_eq!(no_eiv.job_cost_base, None);
}

/// The window showed 06:21:39 with a 15% role bonus and a 50.4% rig on top
/// of Advanced Industry 5; without those two the same job takes 15:05:15.
#[test]
fn invention_time_applies_advanced_industry_only() {
    assert_eq!(invention_duration_seconds(63_900, 0), 63_900);
    assert_eq!(invention_duration_seconds(63_900, 5), 54_315);
    assert_eq!(invention_duration_seconds(63_900, 9), 54_315);
}

#[test]
fn attempts_cover_the_expected_and_the_confident_case() {
    let plan = attempts_for(10, dec("0.465"), dec("0.9")).unwrap();
    assert_eq!(plan.expected, 22);
    assert_eq!(plan.confident, 28);

    assert_eq!(
        attempts_for(7, Decimal::ONE, dec("0.9")),
        Some(AttemptPlan {
            expected: 7,
            confident: 7,
        })
    );
    assert_eq!(
        attempts_for(0, dec("0.4"), dec("0.9")),
        Some(AttemptPlan {
            expected: 0,
            confident: 0,
        })
    );
    assert_eq!(attempts_for(3, Decimal::ZERO, dec("0.9")), None);
}

#[test]
fn one_copy_at_even_odds_needs_four_attempts_to_be_ninety_percent_sure() {
    // 1 − 0.5ⁿ ≥ 0.9 first holds at n = 4.
    let plan = attempts_for(1, dec("0.5"), dec("0.9")).unwrap();
    assert_eq!(plan.expected, 2);
    assert_eq!(plan.confident, 4);
}

#[test]
fn the_confident_figure_is_never_below_the_expected_one() {
    for (copies, chance) in [(1, "0.14"), (3, "0.3"), (25, "0.883"), (200, "0.4375")] {
        let plan = attempts_for(copies, dec(chance), dec("0.9")).unwrap();
        assert!(plan.confident >= plan.expected, "{copies} at {chance}");
    }
}

#[test]
fn a_large_target_is_planned_quickly() {
    let started = Instant::now();
    let plan = attempts_for(1_000, dec("0.14"), dec("0.9")).unwrap();
    assert_eq!(plan.expected, 7_143);
    assert!(plan.confident > plan.expected && plan.confident < 8_000);
    assert!(started.elapsed().as_millis() < 250);
}

fn datacore(type_id: i64, price: Option<&str>) -> QuoteMaterial {
    QuoteMaterial {
        type_id,
        type_name: format!("Datacore {type_id}"),
        quantity: 2,
        unit_price: price.map(isk),
    }
}

fn decryptor(type_id: i64, effect: DecryptorEffect, price: Option<&str>) -> QuoteDecryptor {
    QuoteDecryptor {
        type_id,
        name: format!("Decryptor {type_id}"),
        effect,
        unit_price: price.map(isk),
    }
}

/// A 10-run copy at 50% with fours-free arithmetic: datacores 2 × 100 +
/// 2 × 100, fee 90, copy 10, so an attempt is 500 and a copy 1,000.
fn inputs() -> QuoteInputs {
    QuoteInputs {
        base_chance: dec("0.5"),
        base_runs: 10,
        skills: InventionSkills {
            encryption: 0,
            science: [0, 0],
        },
        materials: vec![datacore(1, Some("100")), datacore(2, Some("100"))],
        decryptors: Vec::new(),
        job_fee: Some(isk("90")),
        copy_cost: Some(isk("10")),
        manufacturing_cost_by_copy: (0..=10).map(|me| ((me, 10), Some(isk("1000")))).collect(),
        output_per_run: 1,
        target_runs: 100,
        confidence: dec("0.9"),
    }
}

#[test]
fn a_quote_works_the_costs_down_to_the_unit() {
    let quote = quote(&inputs());

    assert_eq!(quote.options.len(), 1);
    let none = &quote.options[0];
    assert!(none.decryptor.is_none());
    assert_eq!(none.chance, dec("0.5"));
    assert_eq!(none.attempt_cost, Some(isk("500")));
    assert_eq!(none.cost_per_copy, Some(isk("1000")));
    assert_eq!(none.cost_per_run, Some(isk("100")));
    assert_eq!(none.manufacturing_cost_per_run, Some(isk("1000")));
    assert_eq!(none.unit_cost, Some(isk("1100")));
    assert_eq!(none.unit_cost_delta, Some(Decimal::ZERO));
    assert_eq!(none.copies_needed, 10);
    let attempts = none.attempts.unwrap();
    assert_eq!(attempts.expected, 20);
    assert_eq!(
        none.shopping_cost,
        Some(Money(dec("500") * Decimal::from(attempts.confident)))
    );
    assert!(none.complete && none.best && none.gaps.is_empty());
}

#[test]
fn unit_cost_divides_by_the_units_a_run_yields() {
    let mut inputs = inputs();
    inputs.output_per_run = 100;

    assert_eq!(quote(&inputs).options[0].unit_cost, Some(isk("11")));
}

/// The cheapest copy is not the cheapest product: the second decryptor's
/// copy costs more per run but its ME saves more than that on materials.
#[test]
fn options_rank_by_unit_cost_not_by_cost_per_copy() {
    let mut inputs = inputs();
    let better_me = DecryptorEffect {
        me_modifier: 3,
        ..DecryptorEffect::NONE
    };
    inputs.decryptors = vec![decryptor(10, better_me, Some("200"))];
    inputs
        .manufacturing_cost_by_copy
        .insert((5, 10), Some(isk("900")));

    let quote = quote(&inputs);

    let (none, me5) = (&quote.options[0], &quote.options[1]);
    assert!(me5.cost_per_copy > none.cost_per_copy);
    assert_eq!(me5.cost_per_run, Some(isk("140")));
    assert_eq!(me5.unit_cost, Some(isk("1040")));
    assert_eq!(me5.unit_cost_delta, Some(dec("-60")));
    assert_eq!(me5.unit_cost_delta_percent, Some(dec("-5.5")));
    assert!(me5.best && !none.best);
}

#[test]
fn a_tie_on_unit_cost_goes_to_fewer_attempts() {
    let mut inputs = inputs();
    // Twice the chance at twice the attempt cost: the same cost per copy.
    let surer = DecryptorEffect {
        probability_multiplier: dec("2"),
        ..DecryptorEffect::NONE
    };
    inputs.decryptors = vec![decryptor(10, surer, Some("500"))];

    let quote = quote(&inputs);

    assert_eq!(quote.options[0].unit_cost, quote.options[1].unit_cost);
    assert!(quote.options[1].best && !quote.options[0].best);
}

#[test]
fn a_decryptor_without_a_price_is_incomplete_and_never_best() {
    let mut inputs = inputs();
    inputs.decryptors = vec![decryptor(10, process(), None)];

    let quote = quote(&inputs);

    let (none, unpriced) = (&quote.options[0], &quote.options[1]);
    assert!(none.complete && none.best);
    assert!(!unpriced.complete && !unpriced.best);
    assert_eq!(
        unpriced.gaps,
        vec![QuoteGap::DecryptorPrice { type_id: 10 }]
    );
    assert_eq!(unpriced.attempt_cost, None);
    assert_eq!(unpriced.unit_cost, None);
    assert_eq!(unpriced.unit_cost_delta, None);
    // The chance, the copy and the attempts need no price.
    assert_eq!(unpriced.copy.material_efficiency, 5);
    assert!(unpriced.attempts.is_some());
}

#[test]
fn a_datacore_without_a_price_leaves_every_option_incomplete() {
    let mut inputs = inputs();
    inputs.materials[1].unit_price = None;
    inputs.decryptors = vec![decryptor(10, process(), Some("200"))];

    let quote = quote(&inputs);

    assert!(quote.options.iter().all(|option| !option.complete));
    assert!(quote.options.iter().all(|option| !option.best));
    assert_eq!(
        quote.options[0].gaps,
        vec![QuoteGap::MaterialPrice { type_id: 2 }]
    );
}

#[test]
fn missing_fees_and_manufacturing_costs_are_named() {
    let mut inputs = inputs();
    inputs.job_fee = None;
    inputs.copy_cost = None;
    inputs.manufacturing_cost_by_copy.insert((2, 10), None);

    let none = &quote(&inputs).options[0];

    assert_eq!(
        none.gaps,
        vec![
            QuoteGap::JobFee,
            QuoteGap::CopyCost,
            QuoteGap::ManufacturingCost
        ]
    );
    assert!(!none.complete);
}
