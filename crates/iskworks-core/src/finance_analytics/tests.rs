use super::*;
use chrono::TimeZone;

fn d(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}
fn m(value: &str) -> Money {
    Money::parse(value).unwrap()
}
fn cat(name: &str, total: &str, previous: Option<&str>) -> CategoryTotal {
    CategoryTotal {
        category: name.into(),
        total: m(total),
        previous: previous.map(m),
    }
}

#[test]
fn previous_window_is_equal_length_and_adjacent() {
    assert_eq!(
        previous_window(d("2026-09-01"), d("2026-09-30")),
        (d("2026-08-02"), d("2026-08-31"))
    );
    assert_eq!(
        previous_window(d("2026-09-29"), d("2026-09-29")),
        (d("2026-09-28"), d("2026-09-28"))
    );
}

#[test]
fn buckets_start_on_monday_and_first_of_month() {
    // 2026-09-29 is a Tuesday.
    assert_eq!(
        bucket_start(d("2026-09-29"), Granularity::Week),
        d("2026-09-28")
    );
    assert_eq!(
        bucket_start(d("2026-09-28"), Granularity::Week),
        d("2026-09-28")
    );
    assert_eq!(
        bucket_start(d("2026-09-29"), Granularity::Month),
        d("2026-09-01")
    );
    assert_eq!(
        bucket_start(d("2026-09-29"), Granularity::Day),
        d("2026-09-29")
    );
    assert_eq!(
        next_bucket(d("2026-12-01"), Granularity::Month),
        d("2027-01-01")
    );
    assert_eq!(
        next_bucket(d("2026-09-28"), Granularity::Week),
        d("2026-10-05")
    );
}

#[test]
fn cash_flow_zero_fills_and_accumulates() {
    let mut sums = BTreeMap::new();
    sums.insert(d("2026-09-07"), (m("100"), m("40")));
    sums.insert(d("2026-09-21"), (m("10"), m("50")));
    let buckets =
        build_cash_flow(d("2026-09-01"), d("2026-09-27"), Granularity::Week, &sums).unwrap();
    let starts: Vec<_> = buckets.iter().map(|b| b.start).collect();
    assert_eq!(
        starts,
        vec![
            d("2026-08-31"),
            d("2026-09-07"),
            d("2026-09-14"),
            d("2026-09-21")
        ]
    );
    assert_eq!(buckets[0].net, m("0"));
    assert_eq!(buckets[1].net, m("60"));
    assert_eq!(buckets[2].cumulative_net, m("60"));
    assert_eq!(buckets[3].net.0.to_string(), "-40.0000");
    assert_eq!(buckets[3].cumulative_net, m("20"));
}

#[test]
fn delta_handles_growth_decline_zero_baseline_and_no_comparison() {
    assert_eq!(delta(m("10"), None), None);
    let up = delta(m("150"), Some(m("100"))).unwrap();
    assert!((up.percent.unwrap() - 50.0).abs() < 1e-9);
    let down = delta(m("50"), Some(m("100"))).unwrap();
    assert!((down.percent.unwrap() + 50.0).abs() < 1e-9);
    let new = delta(m("5"), Some(m("0"))).unwrap();
    assert!(new.is_new && new.percent.is_none());
    let none = delta(m("0"), Some(m("0"))).unwrap();
    assert!(!none.is_new && none.percent.is_none());
}

#[test]
fn negative_baseline_uses_absolute_denominator() {
    let d = delta(m("0"), Some(Money(rust_decimal::Decimal::new(-1000000, 4)))).unwrap();
    assert!((d.percent.unwrap() - 100.0).abs() < 1e-9);
}

#[test]
fn fold_keeps_top_n_and_merges_other() {
    let rows = vec![
        cat("Ships", "500", Some("400")),
        cat("Modules", "300", Some("300")),
        cat("Drones", "50", Some("10")),
        cat("Skills", "20", None),
        cat("Other", "5", Some("5")),
    ];
    let folded = fold_top_categories(rows, 2).unwrap();
    let names: Vec<_> = folded.iter().map(|r| r.category.as_str()).collect();
    assert_eq!(names, vec!["Ships", "Modules", "Other"]);
    assert_eq!(folded[2].total, m("75"));
    assert_eq!(folded[2].previous, Some(m("15")));
}

#[test]
fn fold_without_overflow_adds_no_other() {
    let folded = fold_top_categories(vec![cat("Ships", "5", None)], 8).unwrap();
    assert_eq!(folded.len(), 1);
}

fn snap(
    id: ConnectedCharacterId,
    day: u32,
    balance: &str,
) -> (ConnectedCharacterId, DateTime<Utc>, Money) {
    (
        id,
        Utc.with_ymd_and_hms(2026, 9, day, 12, 0, 0).unwrap(),
        m(balance),
    )
}

#[test]
fn wallet_series_starts_when_every_character_has_a_balance() {
    let a = ConnectedCharacterId::new();
    let b = ConnectedCharacterId::new();
    let series = wallet_balance_series(&[
        snap(a, 1, "100"),
        snap(a, 3, "120"),
        snap(b, 5, "50"),
        snap(a, 7, "130"),
        snap(a, 7, "140"),
    ])
    .unwrap();
    assert_eq!(
        series,
        vec![(d("2026-09-05"), m("170")), (d("2026-09-07"), m("190"))]
    );
    assert_eq!(
        balance_at_or_before(&series, d("2026-09-06")),
        Some(m("170"))
    );
    assert_eq!(balance_at_or_before(&series, d("2026-09-04")), None);
}

#[test]
fn heatmap_fills_zero_days_ending_on_the_last_day() {
    let mut nets = BTreeMap::new();
    nets.insert(d("2026-09-29"), m("7"));
    let cells = fill_heatmap(d("2026-09-29"), 3, &nets);
    assert_eq!(cells.len(), 3);
    assert_eq!(cells[0].date, d("2026-09-27"));
    assert_eq!(cells[0].net, m("0"));
    assert_eq!(cells[2].net, m("7"));
}

fn loc(name: &str, expenses: &str) -> LocationTotal {
    LocationTotal {
        location_id: 1,
        location_name: name.into(),
        region_name: None,
        income: m("0"),
        expenses: m(expenses),
        net: m("0"),
        transaction_count: 1,
    }
}
fn chr(name: &str, income: &str) -> CharacterTotal {
    CharacterTotal {
        connection_id: ConnectedCharacterId::new(),
        character_name: name.into(),
        income: m(income),
        expenses: m("0"),
        net: m("0"),
    }
}

#[test]
fn insights_fire_only_above_thresholds() {
    let spending = vec![
        cat("Mutaplasmids", "156", Some("529")),
        cat("Ships", "412", Some("400")),
    ];
    let income = vec![cat("Ships", "2180", Some("2100"))];
    let locations = vec![loc("Jita", "600"), loc("Amarr", "300")];
    let characters = vec![chr("Corvin", "480"), chr("Valka", "520")];
    let insights = derive_insights(&InsightInputs {
        spending: &spending,
        income: &income,
        by_location: &locations,
        by_character: &characters,
        fees: None,
        income_total: Money::zero(),
    })
    .unwrap();
    let kinds: Vec<_> = insights.iter().map(|i| i.kind).collect();
    assert_eq!(
        kinds,
        vec![
            InsightKind::CategoryChange,
            InsightKind::LocationConcentration,
            InsightKind::CharacterConcentration
        ]
    );
    assert_eq!(insights[0].subject, "Mutaplasmids");
    assert!(insights[0].change_percent.unwrap() < -70.0);
    assert_eq!(insights[1].subject, "Jita");
    assert!((insights[1].share_percent.unwrap() - 66.666).abs() < 0.01);
    assert_eq!(insights[2].subject, "Valka");
}

#[test]
fn quiet_periods_produce_no_insights() {
    let spending = vec![cat("Ships", "100", Some("95"))];
    let locations = vec![loc("Jita", "40"), loc("Amarr", "35"), loc("Rens", "25")];
    let characters = vec![chr("A", "34"), chr("B", "33"), chr("C", "33")];
    let insights = derive_insights(&InsightInputs {
        spending: &spending,
        income: &[],
        by_location: &locations,
        by_character: &characters,
        fees: None,
        income_total: Money::zero(),
    })
    .unwrap();
    assert!(insights.is_empty());
}

#[test]
fn a_single_location_or_character_is_not_concentration() {
    assert!(location_concentration(&[loc("Jita", "100")])
        .unwrap()
        .is_none());
    assert!(character_concentration(&[chr("A", "100")])
        .unwrap()
        .is_none());
}

#[test]
fn category_change_ignores_tiny_categories() {
    let spending = vec![
        cat("Ships", "10000", Some("10000")),
        cat("Drones", "50", Some("10")),
    ];
    let insights = derive_insights(&InsightInputs {
        spending: &spending,
        income: &[],
        by_location: &[],
        by_character: &[],
        fees: None,
        income_total: Money::zero(),
    })
    .unwrap();
    assert!(insights.is_empty());
}

fn fee(kind: &str, current: bool, bucket: Option<&str>, total: &str) -> FeeRow {
    FeeRow {
        ref_type: kind.to_string(),
        in_current: current,
        bucket: bucket.map(d),
        total: m(total),
    }
}

#[test]
fn fees_sum_by_type_and_zero_fill_buckets() {
    let rows = vec![
        fee("brokers_fee", true, Some("2026-09-07"), "30"),
        fee("transaction_tax", true, Some("2026-09-07"), "50"),
        fee("market_provider_tax", true, Some("2026-09-21"), "5"),
        fee("brokers_fee", false, None, "40"),
        fee("transaction_tax", false, None, "60"),
    ];
    let starts = [
        d("2026-08-31"),
        d("2026-09-07"),
        d("2026-09-14"),
        d("2026-09-21"),
    ];
    let kpi = build_fees(
        &rows,
        &starts,
        Some((d("2026-08-01"), d("2026-08-30"))),
        Some(d("2026-07-01")),
    )
    .unwrap()
    .unwrap();
    assert_eq!(kpi.value, m("85"));
    assert_eq!(kpi.brokers_fee, m("30"));
    assert_eq!(kpi.transaction_tax, m("50"));
    assert_eq!(kpi.market_provider_tax, m("5"));
    assert_eq!(kpi.sparkline, vec![m("0"), m("80"), m("0"), m("5")]);
    let delta = kpi.delta.unwrap();
    assert_eq!(delta.previous, m("100"));
    assert!((delta.percent.unwrap() + 15.0).abs() < 1e-9);
    assert_eq!(kpi.available_from, Some(d("2026-07-01")));
}

#[test]
fn no_journal_data_means_no_fee_kpi_rather_than_a_zero() {
    assert_eq!(
        build_fees(&[], &[d("2026-09-01")], None, None).unwrap(),
        None
    );
}

#[test]
fn journal_covering_the_range_with_no_fees_is_a_real_zero() {
    let kpi = build_fees(&[], &[d("2026-09-01")], None, Some(d("2026-08-01")))
        .unwrap()
        .unwrap();
    assert_eq!(kpi.value, m("0"));
    assert!(kpi.delta.is_none());
}

#[test]
fn the_delta_is_withheld_when_the_journal_does_not_reach_the_previous_window() {
    let rows = vec![
        fee("brokers_fee", true, Some("2026-09-07"), "30"),
        fee("brokers_fee", false, None, "10"),
    ];
    // The journal only starts mid-way through the previous window, so its
    // total is a partial figure and comparing against it would mislead.
    let kpi = build_fees(
        &rows,
        &[d("2026-09-07")],
        Some((d("2026-08-01"), d("2026-08-30"))),
        Some(d("2026-08-15")),
    )
    .unwrap()
    .unwrap();
    assert!(kpi.delta.is_none());
    assert_eq!(kpi.value, m("30"));
}

fn fee_kpi(value: &str, delta: Option<Delta>) -> FeesKpi {
    FeesKpi {
        value: m(value),
        delta,
        sparkline: vec![],
        brokers_fee: m("0"),
        transaction_tax: m("0"),
        market_provider_tax: m("0"),
        available_from: None,
    }
}

#[test]
fn fee_insight_fires_on_a_meaningful_share_of_income_only() {
    let run = |fees: Option<&FeesKpi>, income: &str| {
        derive_insights(&InsightInputs {
            spending: &[],
            income: &[],
            by_location: &[],
            by_character: &[],
            fees,
            income_total: m(income),
        })
        .unwrap()
    };
    let big = fee_kpi(
        "312",
        Some(Delta {
            previous: m("300"),
            percent: Some(4.0),
            is_new: false,
        }),
    );
    let insights = run(Some(&big), "4818");
    assert_eq!(insights.len(), 1);
    assert_eq!(insights[0].kind, InsightKind::FeeBurden);
    assert_eq!(insights[0].amount, m("312"));
    assert_eq!(insights[0].total, Some(m("4818")));
    assert!((insights[0].share_percent.unwrap() - 6.476).abs() < 0.01);
    assert_eq!(insights[0].change_percent, Some(4.0));

    // Under 1% of income is noise; no income or no fees says nothing.
    assert!(run(Some(&fee_kpi("10", None)), "4818").is_empty());
    assert!(run(Some(&fee_kpi("0", None)), "4818").is_empty());
    assert!(run(Some(&big), "0").is_empty());
    assert!(run(None, "4818").is_empty());
}

#[test]
fn sides_follow_direction_and_types() {
    use FinanceTransactionType::{MarketBuy, MarketSell};
    let both = [MarketBuy, MarketSell];
    assert_eq!(
        FinanceAnalyticsQuery::sides(FinanceDirection::All, &both),
        (true, true)
    );
    assert_eq!(
        FinanceAnalyticsQuery::sides(FinanceDirection::Income, &both),
        (true, false)
    );
    assert_eq!(
        FinanceAnalyticsQuery::sides(FinanceDirection::Expense, &both),
        (false, true)
    );
    assert_eq!(
        FinanceAnalyticsQuery::sides(FinanceDirection::All, &[MarketBuy]),
        (false, true)
    );
}

#[test]
fn query_validation_rejects_bad_ranges_and_categories() {
    let base = FinanceAnalyticsQuery {
        connection_ids: vec![],
        date_from: d("2026-09-01"),
        date_to: d("2026-09-30"),
        include_income: true,
        include_expenses: true,
        granularity: Granularity::Week,
        compare_previous: true,
        exclude_inventory_buys: false,
        category: None,
    };
    assert!(base.clone().validate().is_ok());
    assert_eq!(
        base.previous_window(),
        Some((d("2026-08-02"), d("2026-08-31")))
    );
    let inverted = FinanceAnalyticsQuery {
        date_from: d("2026-10-01"),
        ..base.clone()
    };
    assert!(inverted.validate().is_err());
    let huge = FinanceAnalyticsQuery {
        date_from: d("2020-01-01"),
        ..base.clone()
    };
    assert!(huge.validate().is_err());
    let bad = FinanceAnalyticsQuery {
        category: Some("Nope".into()),
        ..base.clone()
    };
    assert!(bad.validate().is_err());
    let off = FinanceAnalyticsQuery {
        compare_previous: false,
        exclude_inventory_buys: false,
        ..base
    };
    assert_eq!(off.previous_window(), None);
}
