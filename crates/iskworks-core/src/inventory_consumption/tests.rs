use super::*;

const PHOTONIC: i64 = 33362;
const MORPHITE: i64 = 11399;
const ROBOTICS: i64 = 9848;

fn date(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, month, day).unwrap()
}

fn isk(value: &str) -> Money {
    Money::parse(value).unwrap()
}

fn day(on: NaiveDate, type_id: i64, quantity: u64, value: &str) -> ConsumptionDay {
    ConsumptionDay {
        day: on,
        type_id,
        quantity,
        value: isk(value),
    }
}

fn fact(
    type_id: i64,
    name: &str,
    category: MaterialCategory,
    first: NaiveDate,
    on_hand: u64,
) -> ConsumptionTypeFact {
    ConsumptionTypeFact {
        type_id,
        name: name.into(),
        category,
        first_consumed: first,
        on_hand,
    }
}

fn types() -> Vec<ConsumptionTypeFact> {
    vec![
        fact(
            PHOTONIC,
            "Photonic Metamaterials",
            MaterialCategory::ReactionIntermediates,
            date(9, 9),
            512,
        ),
        fact(
            MORPHITE,
            "Morphite",
            MaterialCategory::Minerals,
            date(10, 12),
            0,
        ),
        fact(
            ROBOTICS,
            "Robotics",
            MaterialCategory::Planetary,
            date(9, 22),
            100,
        ),
    ]
}

fn query(from: NaiveDate, to: NaiveDate, granularity: Granularity) -> ConsumptionQuery {
    ConsumptionQuery {
        from,
        to,
        granularity,
        category: None,
    }
}

/// Tuesday 2026-10-20 is "today": its week (from Monday 10-19) is partial.
const TODAY: (u32, u32) = (10, 20);

fn today() -> NaiveDate {
    date(TODAY.0, TODAY.1)
}

fn run(days: Vec<ConsumptionDay>, query: ConsumptionQuery) -> InventoryConsumption {
    build_consumption(
        &ConsumptionFacts {
            days,
            types: types(),
        },
        &query,
        today(),
    )
}

#[test]
fn weekly_buckets_start_monday_cover_the_range_and_flag_the_current_week() {
    let report = run(
        vec![
            day(date(10, 6), PHOTONIC, 10, "100"),
            day(date(10, 13), PHOTONIC, 20, "200"),
            day(date(10, 18), MORPHITE, 5, "50"),
        ],
        query(date(10, 1), today(), Granularity::Week),
    );
    let starts: Vec<NaiveDate> = report.buckets.iter().map(|b| b.start).collect();
    assert_eq!(
        starts,
        vec![date(9, 28), date(10, 5), date(10, 12), date(10, 19)]
    );
    let totals: Vec<Money> = report.buckets.iter().map(|b| b.total).collect();
    assert_eq!(totals, vec![isk("0"), isk("100"), isk("250"), isk("0")]);
    let partial: Vec<bool> = report.buckets.iter().map(|b| b.partial).collect();
    assert_eq!(partial, vec![false, false, false, true]);
    assert_eq!(report.buckets[2].by_category.len(), 2);
    assert_eq!(
        report.buckets[2].by_category[0].category,
        MaterialCategory::Minerals
    );
}

#[test]
fn days_outside_the_range_count_only_toward_sparklines() {
    let report = run(
        vec![
            day(date(9, 9), PHOTONIC, 7, "70"),
            day(date(10, 13), PHOTONIC, 3, "30"),
        ],
        query(date(10, 1), today(), Granularity::Week),
    );
    let photonic = &report.materials[0];
    assert_eq!(photonic.quantity, 3);
    assert_eq!(report.kpis.consumed_value, isk("30"));
    // Sparkline: 8 weeks ending with the week of 10-19 starts 2026-08-31.
    assert_eq!(photonic.weekly_quantity, vec![0, 7, 0, 0, 0, 0, 3, 0]);
}

#[test]
fn kpis_and_materials_rank_by_value() {
    let report = run(
        vec![
            day(date(10, 13), PHOTONIC, 4597, "512000000"),
            day(date(10, 14), ROBOTICS, 2844, "224000000"),
            day(date(10, 15), MORPHITE, 1240, "207000000"),
        ],
        query(date(9, 1), today(), Granularity::Week),
    );
    assert_eq!(report.kpis.consumed_value, isk("943000000"));
    assert_eq!(report.kpis.materials_used, 3);
    assert_eq!(
        report.kpis.busiest_bucket,
        Some(BusiestBucket {
            start: date(10, 12),
            value: isk("943000000")
        })
    );
    assert_eq!(
        report.kpis.top_category,
        Some(MaterialCategory::ReactionIntermediates)
    );
    let names: Vec<&str> = report.materials.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Photonic Metamaterials", "Robotics", "Morphite"]
    );
    assert!((report.materials[0].share_percent - 54.29).abs() < 0.01);
}

#[test]
fn burn_rate_and_cover_need_four_weeks_of_history() {
    let report = run(
        vec![
            // Photonic: first consumed 9-9, 42 days before 10-20.
            day(date(9, 9), PHOTONIC, 210, "1"),
            day(date(10, 13), PHOTONIC, 210, "1"),
            // Morphite: first consumed 10-12, only 8 days of history.
            day(date(10, 12), MORPHITE, 100, "1"),
        ],
        query(date(9, 9), today(), Granularity::Week),
    );
    let photonic = report
        .materials
        .iter()
        .find(|m| m.type_id == PHOTONIC)
        .unwrap();
    // 420 over 42 days = 10/day: 70/week; 512 on hand = 51.2 days.
    assert!((photonic.average_per_week.unwrap() - 70.0).abs() < 1e-9);
    assert!((photonic.days_of_cover.unwrap() - 51.2).abs() < 1e-9);
    let morphite = report
        .materials
        .iter()
        .find(|m| m.type_id == MORPHITE)
        .unwrap();
    assert_eq!(morphite.average_per_week, None);
    assert_eq!(morphite.days_of_cover, None);
}

#[test]
fn burn_rate_counts_only_days_in_the_range() {
    let report = run(
        vec![day(date(10, 14), PHOTONIC, 70, "1")],
        query(date(10, 14), today(), Granularity::Day),
    );
    // 70 over the 7 range days (10-14..10-20) = 70/week.
    let photonic = &report.materials[0];
    assert!((photonic.average_per_week.unwrap() - 70.0).abs() < 1e-9);
    assert_eq!(report.buckets.len(), 7);
    assert!(report.buckets.last().unwrap().partial);
    assert!(!report.buckets[5].partial);
}

#[test]
fn category_filter_scopes_everything_but_the_category_shares() {
    let mut q = query(date(10, 1), today(), Granularity::Week);
    q.category = Some(MaterialCategory::Minerals);
    let report = run(
        vec![
            day(date(10, 13), PHOTONIC, 1, "300"),
            day(date(10, 13), MORPHITE, 1, "100"),
        ],
        q,
    );
    assert_eq!(report.kpis.consumed_value, isk("100"));
    assert_eq!(report.materials.len(), 1);
    assert_eq!(report.buckets[2].total, isk("100"));
    assert_eq!(report.categories.len(), 2);
    assert_eq!(
        report.categories[0].category,
        MaterialCategory::ReactionIntermediates
    );
    assert_eq!(report.categories[0].share_percent, 75.0);
}

#[test]
fn no_consumption_is_an_empty_report_with_empty_buckets() {
    let report = build_consumption(
        &ConsumptionFacts::default(),
        &query(date(10, 14), today(), Granularity::Week),
        today(),
    );
    assert_eq!(report.earliest_observed_at, None);
    assert_eq!(report.buckets.len(), 2);
    assert_eq!(report.kpis.busiest_bucket, None);
    assert_eq!(report.kpis.top_category, None);
    assert!(report.materials.is_empty() && report.categories.is_empty());
}

#[test]
fn earliest_observed_is_the_first_consumption_of_any_type() {
    let report = run(vec![], query(date(10, 1), today(), Granularity::Week));
    assert_eq!(report.earliest_observed_at, Some(date(9, 9)));
}

#[test]
fn fetch_window_reaches_back_for_sparklines() {
    let q = query(date(10, 14), today(), Granularity::Week);
    assert_eq!(q.fetch_from(), date(8, 31));
    let q = query(date(1, 1), today(), Granularity::Week);
    assert_eq!(q.fetch_from(), date(1, 1));
}

#[test]
fn validation_rejects_inverted_long_and_monthly_ranges() {
    assert!(query(date(10, 2), date(10, 1), Granularity::Week)
        .validate()
        .is_err());
    assert!(query(date(1, 1), date(10, 1), Granularity::Month)
        .validate()
        .is_err());
    let long_daily = query(
        date(1, 1) - Duration::days(400),
        date(1, 1),
        Granularity::Day,
    );
    assert!(long_daily.validate().is_err());
    assert!(query(date(1, 1), date(10, 1), Granularity::Week)
        .validate()
        .is_ok());
}

#[test]
fn serializes_camel_case() {
    let report = run(
        vec![day(date(10, 13), PHOTONIC, 1, "5")],
        query(date(10, 1), today(), Granularity::Week),
    );
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["granularity"], "week");
    assert_eq!(json["earliestObservedAt"], "2026-09-09");
    assert_eq!(json["kpis"]["busiestBucket"]["start"], "2026-10-12");
    assert_eq!(
        json["buckets"][2]["byCategory"][0]["category"],
        "Reaction Intermediates"
    );
    assert_eq!(
        json["materials"][0]["weeklyQuantity"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert!(json["materials"][0]["daysOfCover"].is_number());
}
