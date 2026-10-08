use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use iskworks_api::{build_router, AppState};
use tower::ServiceExt;

mod support;
use support::workspace::configured_workspace;

#[tokio::test]
async fn finance_query_rejects_unsupported_sort_before_storage_access() {
    let app = build_router(AppState::new(Arc::new(configured_workspace(
        "Finance Test",
    ))));
    let response = app
        .oneshot(
            Request::get("/api/finance/transactions?sort=profit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn finance_query_rejects_unknown_category_before_storage_access() {
    let app = build_router(AppState::new(Arc::new(configured_workspace(
        "Finance Test",
    ))));
    let response = app
        .oneshot(
            Request::get("/api/finance/transactions?category=Nonsense")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

mod analytics {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::NaiveDate;
    use iskworks_core::{
        AnalyticsKpis, AnalyticsRange, ExcludedTrades, FinanceAnalytics, FinanceAnalyticsQuery,
        FinanceAnalyticsRepository, FinanceError, Granularity, KpiValue, MarginKpi, Money,
        WalletKpi, WorkspaceId,
    };

    use super::*;

    #[derive(Default)]
    struct FakeAnalytics {
        seen: Mutex<Vec<FinanceAnalyticsQuery>>,
    }

    #[async_trait]
    impl FinanceAnalyticsRepository for FakeAnalytics {
        async fn analytics(
            &self,
            _workspace_id: WorkspaceId,
            query: FinanceAnalyticsQuery,
        ) -> Result<FinanceAnalytics, FinanceError> {
            self.seen.lock().unwrap().push(query.clone());
            let kpi = || KpiValue {
                value: Money::zero(),
                delta: None,
                sparkline: vec![],
            };
            Ok(FinanceAnalytics {
                range: AnalyticsRange {
                    date_from: query.date_from,
                    date_to: query.date_to,
                    previous_date_from: None,
                    previous_date_to: None,
                    granularity: query.granularity,
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
            })
        }
    }

    fn app(fake: Arc<FakeAnalytics>) -> axum::Router {
        build_router(
            AppState::new(Arc::new(configured_workspace("Finance Test")))
                .with_finance_analytics_repository(fake),
        )
    }

    async fn get(app: axum::Router, uri: &str) -> (StatusCode, Vec<u8>) {
        let response = app
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, bytes.to_vec())
    }

    #[tokio::test]
    async fn analytics_passes_filters_through_and_serializes_camel_case() {
        let fake = Arc::new(FakeAnalytics::default());
        let (status, bytes) = get(
            app(fake.clone()),
            "/api/finance/analytics?dateFrom=2026-09-01&dateTo=2026-09-30&direction=expense\
             &granularity=month&comparePrevious=false&category=Ships&excludeInventoryBuys=true",
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["range"]["granularity"], "month");
        assert_eq!(body["kpis"]["income"]["value"], "0.0000");
        assert!(body["excludedIntraAccount"]["transactionCount"].is_number());
        let seen = fake.seen.lock().unwrap();
        let query = &seen[0];
        assert_eq!(
            query.date_from,
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()
        );
        assert!(!query.include_income && query.include_expenses);
        assert_eq!(query.granularity, Granularity::Month);
        assert!(!query.compare_previous);
        assert_eq!(query.category.as_deref(), Some("Ships"));
        assert!(query.exclude_inventory_buys);
    }

    #[tokio::test]
    async fn analytics_rejects_invalid_params_before_storage_access() {
        let fake = Arc::new(FakeAnalytics::default());
        for uri in [
            "/api/finance/analytics?granularity=hour",
            "/api/finance/analytics?category=Nonsense",
            "/api/finance/analytics?dateFrom=2026-10-02&dateTo=2026-10-01",
            "/api/finance/analytics?dateFrom=2019-01-01&dateTo=2026-10-01",
            "/api/finance/analytics/export?section=nope",
        ] {
            let (status, _) = get(app(fake.clone()), uri).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        }
        assert!(fake.seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn analytics_export_returns_csv_for_a_section() {
        let fake = Arc::new(FakeAnalytics::default());
        let (status, bytes) = get(
            app(fake),
            "/api/finance/analytics/export?section=heatmap&dateFrom=2026-09-01&dateTo=2026-09-02",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(String::from_utf8(bytes).unwrap(), "Date,Net\n");
    }

    #[tokio::test]
    async fn analytics_without_a_repository_is_a_server_error_not_a_panic() {
        let app = build_router(AppState::new(Arc::new(configured_workspace(
            "Finance Test",
        ))));
        let (status, _) = get(app, "/api/finance/analytics").await;
        assert!(status.is_server_error());
    }
}
