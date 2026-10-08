use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderValue, Response, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{Duration, NaiveDate, Utc};
use iskworks_core::{
    FinanceAnalytics, FinanceAnalyticsQuery, FinanceError, Granularity, Money, TopItem,
};
use serde::Deserialize;

use super::finance::{parse_date, parse_direction, parse_ids, parse_transaction_types};
use crate::csv_cell::safe_cell;
use crate::{workspace_context, ApiError, AppState};

const DEFAULT_RANGE_DAYS: i64 = 30;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/finance/analytics", get(get_analytics))
        .route("/api/finance/analytics/export", get(export_analytics))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnalyticsParams {
    connection_ids: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    direction: Option<String>,
    transaction_types: Option<String>,
    granularity: Option<String>,
    compare_previous: Option<bool>,
    category: Option<String>,
    exclude_inventory_buys: Option<bool>,
    section: Option<String>,
}

impl AnalyticsParams {
    fn domain(&self, today: NaiveDate) -> Result<FinanceAnalyticsQuery, FinanceError> {
        let date_to = parse_date(self.date_to.as_deref(), "dateTo")?.unwrap_or(today);
        let date_from = parse_date(self.date_from.as_deref(), "dateFrom")?
            .unwrap_or(date_to - Duration::days(DEFAULT_RANGE_DAYS - 1));
        let (include_income, include_expenses) = FinanceAnalyticsQuery::sides(
            parse_direction(self.direction.as_deref())?,
            &parse_transaction_types(self.transaction_types.as_deref())?,
        );
        FinanceAnalyticsQuery {
            connection_ids: parse_ids(self.connection_ids.as_deref())?,
            date_from,
            date_to,
            include_income,
            include_expenses,
            granularity: parse_granularity(self.granularity.as_deref())?,
            compare_previous: self.compare_previous.unwrap_or(true),
            exclude_inventory_buys: self.exclude_inventory_buys.unwrap_or(false),
            category: self.category.clone(),
        }
        .validate()
    }
}

fn parse_granularity(value: Option<&str>) -> Result<Granularity, FinanceError> {
    match value.unwrap_or("week") {
        "day" => Ok(Granularity::Day),
        "week" => Ok(Granularity::Week),
        "month" => Ok(Granularity::Month),
        value => Err(FinanceError::Validation(format!(
            "unsupported granularity: {value}"
        ))),
    }
}

async fn get_analytics(
    State(state): State<AppState>,
    Query(params): Query<AnalyticsParams>,
) -> Result<Json<FinanceAnalytics>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let query = params.domain(Utc::now().date_naive())?;
    Ok(Json(
        state
            .finance_analytics_repository()?
            .analytics(workspace_id, query)
            .await?,
    ))
}

async fn export_analytics(
    State(state): State<AppState>,
    Query(params): Query<AnalyticsParams>,
) -> Result<Response<Body>, ApiError> {
    let (workspace_id, _) = workspace_context(&state).await?;
    let query = params.domain(Utc::now().date_naive())?;
    let section = params.section.as_deref().unwrap_or_default();
    if !SECTIONS.contains(&section) {
        return Err(FinanceError::Validation(format!(
            "section must be one of: {}",
            SECTIONS.join(", ")
        ))
        .into());
    }
    let analytics = state
        .finance_analytics_repository()?
        .analytics(workspace_id, query)
        .await?;
    let csv = analytics_csv(&analytics, section)?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&format!(
                "attachment; filename=isk-works-analytics-{section}.csv"
            ))
            .expect("section names are header-safe"),
        )
        .body(Body::from(csv))
        .expect("static Finance response headers are valid"))
}

const SECTIONS: &[&str] = &[
    "kpis",
    "cashFlow",
    "spending",
    "income",
    "flow",
    "characters",
    "locations",
    "topExpenses",
    "topEarners",
    "heatmap",
];

fn persistence(error: impl ToString) -> FinanceError {
    FinanceError::Persistence(error.to_string())
}

fn decimal(value: Money) -> String {
    value.0.to_string()
}

fn optional(value: Option<Money>) -> String {
    value.map(decimal).unwrap_or_default()
}

fn top_item_rows(items: &[TopItem]) -> Vec<Vec<String>> {
    items
        .iter()
        .map(|item| {
            vec![
                item.type_name.clone(),
                item.category.clone(),
                item.quantity.to_string(),
                decimal(item.average_unit_price),
                decimal(item.total),
                item.share_percent
                    .map(|share| format!("{share:.2}"))
                    .unwrap_or_default(),
            ]
        })
        .collect()
}

/// The chart's underlying rows, section by section. Money stays exact
/// (decimal strings) so a spreadsheet can re-aggregate it.
pub(crate) fn analytics_csv(
    analytics: &FinanceAnalytics,
    section: &str,
) -> Result<Vec<u8>, FinanceError> {
    let (header, rows): (Vec<&str>, Vec<Vec<String>>) = match section {
        "kpis" => {
            let kpis = &analytics.kpis;
            let row = |label: &str, value: Money, previous: Option<Money>| {
                vec![label.to_string(), decimal(value), optional(previous)]
            };
            let mut rows = vec![
                row(
                    "Income",
                    kpis.income.value,
                    kpis.income.delta.as_ref().map(|d| d.previous),
                ),
                row(
                    "Expenses",
                    kpis.expenses.value,
                    kpis.expenses.delta.as_ref().map(|d| d.previous),
                ),
                row(
                    "Net ISK",
                    kpis.net.value,
                    kpis.net.delta.as_ref().map(|d| d.previous),
                ),
            ];
            if let Some(fees) = &kpis.fees {
                rows.push(row(
                    "Taxes & fees",
                    fees.value,
                    fees.delta.as_ref().map(|d| d.previous),
                ));
                rows.push(row("Broker fees", fees.brokers_fee, None));
                rows.push(row("Sales tax", fees.transaction_tax, None));
                rows.push(row("Structure market fees", fees.market_provider_tax, None));
            }
            if let Some(balance) = kpis.wallet_balance.value {
                rows.push(row(
                    "Wallet balance",
                    balance,
                    kpis.wallet_balance.delta.as_ref().map(|d| d.previous),
                ));
            }
            (vec!["Metric", "Value", "Previous period"], rows)
        }
        "cashFlow" => (
            vec![
                "Period start",
                "Income",
                "Expenses",
                "Net",
                "Cumulative net",
            ],
            analytics
                .cash_flow
                .iter()
                .map(|bucket| {
                    vec![
                        bucket.start.to_string(),
                        decimal(bucket.income),
                        decimal(bucket.expenses),
                        decimal(bucket.net),
                        decimal(bucket.cumulative_net),
                    ]
                })
                .collect(),
        ),
        "spending" | "income" => {
            let list = if section == "spending" {
                &analytics.spending_by_category
            } else {
                &analytics.income_by_category
            };
            (
                vec!["Category", "Total", "Previous period"],
                list.iter()
                    .map(|row| {
                        vec![
                            row.category.clone(),
                            decimal(row.total),
                            optional(row.previous),
                        ]
                    })
                    .collect(),
            )
        }
        "flow" => {
            let mut rows: Vec<Vec<String>> = Vec::new();
            let mut income = Money::zero();
            let mut spending = Money::zero();
            for row in &analytics.income_by_category {
                income = income.checked_add(row.total).map_err(persistence)?;
                rows.push(vec![
                    "Income".into(),
                    row.category.clone(),
                    decimal(row.total),
                ]);
            }
            for row in &analytics.spending_by_category {
                spending = spending.checked_add(row.total).map_err(persistence)?;
                rows.push(vec![
                    "Spending".into(),
                    row.category.clone(),
                    decimal(row.total),
                ]);
            }
            // What balances the two sides: money kept, or money drawn from the wallet.
            if income >= spending {
                let kept = income.checked_sub(spending).map_err(persistence)?;
                rows.push(vec!["Net saved".into(), String::new(), decimal(kept)]);
            } else {
                let drawn = spending.checked_sub(income).map_err(persistence)?;
                rows.push(vec!["Drawdown".into(), String::new(), decimal(drawn)]);
            }
            (vec!["Side", "Category", "Total"], rows)
        }
        "characters" => (
            vec!["Character", "Income", "Expenses", "Net"],
            analytics
                .by_character
                .iter()
                .map(|row| {
                    vec![
                        row.character_name.clone(),
                        decimal(row.income),
                        decimal(row.expenses),
                        decimal(row.net),
                    ]
                })
                .collect(),
        ),
        "locations" => (
            vec![
                "Location",
                "Region",
                "Income",
                "Expenses",
                "Net",
                "Transactions",
            ],
            analytics
                .by_location
                .iter()
                .map(|row| {
                    vec![
                        row.location_name.clone(),
                        row.region_name.clone().unwrap_or_default(),
                        decimal(row.income),
                        decimal(row.expenses),
                        decimal(row.net),
                        row.transaction_count.to_string(),
                    ]
                })
                .collect(),
        ),
        "topExpenses" | "topEarners" => (
            vec![
                "Item",
                "Category",
                "Quantity",
                "Average unit price",
                "Total",
                "Share %",
            ],
            top_item_rows(if section == "topExpenses" {
                &analytics.top_expenses
            } else {
                &analytics.top_earners
            }),
        ),
        "heatmap" => (
            vec!["Date", "Net"],
            analytics
                .heatmap
                .iter()
                .map(|day| vec![day.date.to_string(), decimal(day.net)])
                .collect(),
        ),
        other => {
            return Err(FinanceError::Validation(format!(
                "unsupported section: {other}"
            )))
        }
    };
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(&header).map_err(persistence)?;
    for row in rows {
        writer
            .write_record(row.iter().map(|cell| safe_cell(cell).into_owned()))
            .map_err(persistence)?;
    }
    writer.into_inner().map_err(persistence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use iskworks_core::{CashFlowBucket, DayNet};

    fn d(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn params(f: impl FnOnce(&mut AnalyticsParams)) -> AnalyticsParams {
        let mut params = AnalyticsParams {
            connection_ids: None,
            date_from: None,
            date_to: None,
            direction: None,
            transaction_types: None,
            granularity: None,
            compare_previous: None,
            exclude_inventory_buys: None,
            category: None,
            section: None,
        };
        f(&mut params);
        params
    }

    #[test]
    fn defaults_to_the_last_thirty_days_weekly_with_comparison() {
        let query = params(|_| {}).domain(d("2026-09-29")).unwrap();
        assert_eq!(query.date_to, d("2026-09-29"));
        assert_eq!(query.date_from, d("2026-08-31"));
        assert_eq!(query.granularity, Granularity::Week);
        assert!(query.compare_previous);
        assert!(query.include_income && query.include_expenses);
    }

    #[test]
    fn direction_and_types_choose_the_sides() {
        let query = params(|p| p.direction = Some("income".into()))
            .domain(d("2026-09-29"))
            .unwrap();
        assert!(query.include_income && !query.include_expenses);
        let query = params(|p| p.transaction_types = Some("marketBuy".into()))
            .domain(d("2026-09-29"))
            .unwrap();
        assert!(!query.include_income && query.include_expenses);
    }

    #[test]
    fn rejects_bad_granularity_category_and_ranges() {
        let today = d("2026-09-29");
        assert!(params(|p| p.granularity = Some("hour".into()))
            .domain(today)
            .is_err());
        assert!(params(|p| p.category = Some("Nope".into()))
            .domain(today)
            .is_err());
        assert!(params(|p| p.date_from = Some("2020-01-01".into()))
            .domain(today)
            .is_err());
        assert!(params(|p| p.compare_previous = Some(false))
            .domain(today)
            .is_ok());
    }

    fn sample() -> FinanceAnalytics {
        let mut analytics = crate::routes::finance_analytics::tests_support::empty_analytics();
        analytics.cash_flow = vec![CashFlowBucket {
            start: d("2026-09-28"),
            income: Money::parse("100").unwrap(),
            expenses: Money::parse("40").unwrap(),
            net: Money::parse("60").unwrap(),
            cumulative_net: Money::parse("60").unwrap(),
        }];
        analytics.heatmap = vec![DayNet {
            date: d("2026-09-29"),
            net: Money::parse("5").unwrap(),
        }];
        analytics
    }

    #[test]
    fn csv_renders_headers_and_exact_decimals() {
        let csv = String::from_utf8(analytics_csv(&sample(), "cashFlow").unwrap()).unwrap();
        assert_eq!(
            csv,
            "Period start,Income,Expenses,Net,Cumulative net\n2026-09-28,100.0000,40.0000,60.0000,60.0000\n"
        );
        let csv = String::from_utf8(analytics_csv(&sample(), "heatmap").unwrap()).unwrap();
        assert_eq!(csv, "Date,Net\n2026-09-29,5.0000\n");
    }

    fn category(name: &str, total: &str) -> iskworks_core::CategoryTotal {
        iskworks_core::CategoryTotal {
            category: name.to_string(),
            total: Money::parse(total).unwrap(),
            previous: None,
        }
    }

    #[test]
    fn kpis_csv_includes_fees_only_when_the_journal_has_them() {
        let mut analytics = sample();
        let plain = String::from_utf8(analytics_csv(&analytics, "kpis").unwrap()).unwrap();
        assert!(!plain.contains("fees"));

        analytics.kpis.fees = Some(iskworks_core::FeesKpi {
            value: Money::parse("312").unwrap(),
            delta: None,
            sparkline: vec![],
            brokers_fee: Money::parse("100").unwrap(),
            transaction_tax: Money::parse("200").unwrap(),
            market_provider_tax: Money::parse("12").unwrap(),
            available_from: None,
        });
        let csv = String::from_utf8(analytics_csv(&analytics, "kpis").unwrap()).unwrap();
        assert!(csv.contains("Taxes & fees,312.0000,\n"), "{csv}");
        assert!(csv.contains("Broker fees,100.0000,\n"));
        assert!(csv.contains("Sales tax,200.0000,\n"));
        assert!(csv.contains("Structure market fees,12.0000,\n"));
    }

    #[test]
    fn flow_csv_lists_both_sides_and_the_balancing_row() {
        let mut analytics = sample();
        analytics.income_by_category = vec![category("Ships", "500"), category("Modules", "300")];
        analytics.spending_by_category = vec![category("Materials", "200")];
        let csv = String::from_utf8(analytics_csv(&analytics, "flow").unwrap()).unwrap();
        assert_eq!(
            csv,
            "Side,Category,Total\nIncome,Ships,500.0000\nIncome,Modules,300.0000\nSpending,Materials,200.0000\nNet saved,,600.0000\n"
        );

        // Spending more than earned shows as a drawdown, never a negative saving.
        analytics.spending_by_category = vec![category("Materials", "1000")];
        let csv = String::from_utf8(analytics_csv(&analytics, "flow").unwrap()).unwrap();
        assert!(csv.ends_with("Drawdown,,200.0000\n"), "{csv}");
    }

    #[test]
    fn csv_rejects_unknown_sections_and_covers_every_listed_one() {
        assert!(analytics_csv(&sample(), "nope").is_err());
        for section in SECTIONS {
            analytics_csv(&sample(), section).unwrap();
        }
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use iskworks_core::{
        AnalyticsKpis, AnalyticsRange, ExcludedTrades, FinanceAnalytics, Granularity, KpiValue,
        MarginKpi, Money, WalletKpi,
    };

    pub(crate) fn empty_analytics() -> FinanceAnalytics {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 29).unwrap();
        let kpi = || KpiValue {
            value: Money::zero(),
            delta: None,
            sparkline: vec![],
        };
        FinanceAnalytics {
            range: AnalyticsRange {
                date_from: day,
                date_to: day,
                previous_date_from: None,
                previous_date_to: None,
                granularity: Granularity::Week,
            },
            earliest_observed_at: None,
            available_characters: vec![],
            excluded_intra_account: ExcludedTrades {
                transaction_count: 0,
                total_isk: Money::zero(),
            },
            excluded_inventory_buys: ExcludedTrades {
                transaction_count: 0,
                total_isk: Money::zero(),
            },
            kpis: AnalyticsKpis {
                income: kpi(),
                expenses: kpi(),
                net: kpi(),
                margin: MarginKpi {
                    percent: None,
                    previous_percent: None,
                    sparkline: vec![],
                },
                wallet_balance: WalletKpi {
                    value: None,
                    delta: None,
                    sparkline: vec![],
                },
                fees: None,
                transaction_count: 0,
            },
            cash_flow: vec![],
            spending_by_category: vec![],
            income_by_category: vec![],
            by_character: vec![],
            by_location: vec![],
            top_expenses: vec![],
            top_earners: vec![],
            heatmap: vec![],
            insights: vec![],
        }
    }
}
