use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone, Default)]
struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

impl CapturedLogs {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

thread_local! {
    static THREAD_LOGS: std::cell::RefCell<Option<Arc<Mutex<Vec<u8>>>>> =
        const { std::cell::RefCell::new(None) };
}

/// Appends to the buffer the current thread is capturing into, if any.
struct ThreadLogWriter;

impl Write for ThreadLogWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        THREAD_LOGS.with(|logs| {
            if let Some(logs) = &*logs.borrow() {
                logs.lock().unwrap().extend_from_slice(buffer);
            }
        });
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Captures this test thread's `iskworks_esi` log output (debug and up). Every test
/// shares one global subscriber: per-test `set_default` subscribers
/// raced over which subscriber a log callsite was enabled for, so a
/// test running alongside others sometimes captured nothing.
fn capture_logs() -> CapturedLogs {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            // Only this crate: reqwest's own debug logs carry full URLs.
            .with_env_filter("iskworks_esi=debug")
            .with_writer(|| ThreadLogWriter)
            .try_init();
    });
    let logs = CapturedLogs::default();
    THREAD_LOGS.with(|current| *current.borrow_mut() = Some(Arc::clone(&logs.0)));
    logs
}

#[test]
fn identity_claims_without_scope_deserialize_with_no_scopes() {
    let claims: Claims = serde_json::from_value(serde_json::json!({
        "sub": "CHARACTER:EVE:2112625428",
        "name": "Scope Free Pilot",
        "aud": ["client-id", "EVE Online"]
    }))
    .unwrap();

    assert!(claims.scp.into_set().is_empty());
}

#[test]
fn wallet_price_is_parsed_from_json_number_lexically() {
    let raw = serde_json::json!({
        "transaction_id": 11, "client_id": 22, "location_id": 33, "type_id": 34,
        "quantity": 100000, "unit_price": 4.2500, "is_buy": true, "is_personal": true,
        "journal_ref_id": 44, "date": "2026-07-25T12:00:00Z"
    });
    let value = parse_wallet(raw).unwrap();
    assert_eq!(value.unit_price, Decimal::from_str("4.25").unwrap());
    assert_eq!(
        value.unit_price * Decimal::from(100000),
        Decimal::from(425000)
    );
}

#[test]
fn wallet_journal_entries_keep_signed_exact_amounts_and_optional_fields() {
    let fee = parse_wallet_journal(serde_json::json!({
        "id": 1, "date": "2026-09-28T10:00:00Z", "ref_type": "brokers_fee",
        "amount": -12345.6700, "balance": 25240000000.1200,
        "first_party_id": 90000001, "second_party_id": 1000125,
        "context_id": 555, "context_id_type": "market_transaction_id",
        "description": "Broker fee", "tax": 0.5
    }))
    .unwrap();
    assert_eq!(fee.ref_type, "brokers_fee");
    assert_eq!(fee.amount, Decimal::from_str("-12345.67").unwrap());
    assert_eq!(
        fee.balance,
        Some(Decimal::from_str("25240000000.12").unwrap())
    );
    assert_eq!(
        fee.context_id_type.as_deref(),
        Some("market_transaction_id")
    );
    assert_eq!(fee.tax, Some(Decimal::from_str("0.5").unwrap()));

    let sparse = parse_wallet_journal(serde_json::json!({
        "id": 2, "date": "2026-09-28T10:00:00Z", "ref_type": "player_donation", "amount": 100
    }))
    .unwrap();
    assert_eq!(sparse.balance, None);
    assert_eq!(sparse.first_party_id, None);
    assert_eq!(sparse.description, None);
}

#[test]
fn wallet_journal_rejects_missing_or_overprecise_fields() {
    let base =
        serde_json::json!({"id": 3, "date": "2026-09-28T10:00:00Z", "ref_type": "x", "amount": 1});
    assert!(parse_wallet_journal(base.clone()).is_ok());
    for key in ["id", "date", "ref_type", "amount"] {
        let mut broken = base.clone();
        broken.as_object_mut().unwrap().remove(key);
        assert_eq!(
            parse_wallet_journal(broken),
            Err(EsiError::InvalidResponse),
            "{key}"
        );
    }
    let mut precise = base;
    precise["amount"] = serde_json::json!(1.23456);
    assert_eq!(
        parse_wallet_journal(precise),
        Err(EsiError::InvalidResponse)
    );
}

#[test]
fn wallet_balance_is_parsed_from_json_number_lexically() {
    let balance = parse_wallet_balance(serde_json::json!(25240000000.1200)).unwrap();
    assert_eq!(
        balance.balance,
        Decimal::from_str("25240000000.12").unwrap()
    );
    assert_eq!(
        parse_wallet_balance(serde_json::json!(-1)),
        Err(EsiError::InvalidResponse)
    );
}

#[test]
fn manufacturing_and_reaction_cost_indices_are_selected_from_industry_activities() {
    let raw = serde_json::json!({
        "solar_system_id": 30000772,
        "cost_indices": [
            { "activity": "copying", "cost_index": 0.0123 },
            { "activity": "manufacturing", "cost_index": 0.0979 },
            { "activity": "reaction", "cost_index": 0.0231 }
        ]
    });

    let value = parse_industry_system(raw).unwrap();
    assert_eq!(value.solar_system_id, 30_000_772);
    assert_eq!(value.manufacturing, Decimal::from_str("0.0979").unwrap());
    assert_eq!(value.reaction, Decimal::from_str("0.0231").unwrap());
}

#[test]
fn industry_system_without_a_reaction_cost_index_is_an_invalid_response() {
    let raw = serde_json::json!({
        "solar_system_id": 30000772,
        "cost_indices": [
            { "activity": "manufacturing", "cost_index": 0.0979 }
        ]
    });

    assert!(matches!(
        parse_industry_system(raw),
        Err(EsiError::InvalidResponse)
    ));
}

#[test]
fn market_adjusted_price_is_parsed_without_using_average_price() {
    let raw = serde_json::json!({
        "type_id": 34,
        "adjusted_price": 3.689123456,
        "average_price": 4.25
    });

    let value = parse_adjusted_price(raw).unwrap();
    assert_eq!(value.type_id, 34);
    assert_eq!(
        value.adjusted_price,
        Decimal::from_str("3.689123456").unwrap()
    );
}

#[test]
fn market_record_without_adjusted_price_is_not_an_eiv_observation() {
    let raw = serde_json::json!({
        "type_id": 35,
        "average_price": 12.5
    });

    assert_eq!(parse_adjusted_price(raw), Err(EsiError::InvalidResponse));
}

#[test]
fn parse_market_order_preserves_buy_and_sell_book_fields() {
    let buy = parse_market_order(serde_json::json!({
        "order_id": 7_386_855_683_i64,
        "type_id": 34,
        "location_id": 60_003_760,
        "system_id": 30_000_142,
        "is_buy_order": true,
        "price": 4.2500,
        "volume_remain": 100_000,
        "volume_total": 250_000,
        "min_volume": 1_000,
        "range": "region",
        "issued": "2026-07-25T12:00:00Z",
        "duration": 90
    }))
    .unwrap();
    let sell = parse_market_order(serde_json::json!({
        "order_id": 7_386_855_684_i64,
        "type_id": 34,
        "location_id": 60_003_760,
        "system_id": 30_000_142,
        "is_buy_order": false,
        "price": 4.5000,
        "volume_remain": 50_000,
        "volume_total": 50_000,
        "min_volume": 1,
        "range": "station",
        "issued": "2026-07-25T13:00:00Z",
        "duration": 30
    }))
    .unwrap();

    assert!(buy.is_buy_order);
    assert!(!sell.is_buy_order);
    assert_eq!(buy.price, Decimal::from_str("4.25").unwrap());
    assert_eq!(sell.price, Decimal::from_str("4.5").unwrap());
    assert_eq!(buy.volume_remain, 100_000);
    assert_eq!(buy.volume_total, 250_000);
    assert_eq!(buy.min_volume, 1_000);
    assert_eq!(buy.order_range, "region");
    assert_eq!(sell.order_range, "station");
    assert_eq!(
        buy.issued_at,
        DateTime::parse_from_rfc3339("2026-07-25T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    );
    assert_eq!(buy.duration_days, 90);
}

#[test]
fn parse_market_order_rejects_invalid_volumes_and_dates() {
    let invalid_volume = serde_json::json!({
        "order_id": 1,
        "type_id": 34,
        "location_id": 60_003_760,
        "system_id": 30_000_142,
        "is_buy_order": false,
        "price": 4.5,
        "volume_remain": -1,
        "volume_total": 1,
        "min_volume": 1,
        "range": "station",
        "issued": "2026-07-25T13:00:00Z",
        "duration": 30
    });
    let invalid_date = serde_json::json!({
        "order_id": 1,
        "type_id": 34,
        "location_id": 60_003_760,
        "system_id": 30_000_142,
        "is_buy_order": false,
        "price": 4.5,
        "volume_remain": 1,
        "volume_total": 1,
        "min_volume": 1,
        "range": "station",
        "issued": "not-a-date",
        "duration": 30
    });

    assert_eq!(
        parse_market_order(invalid_volume),
        Err(EsiError::InvalidResponse)
    );
    assert_eq!(
        parse_market_order(invalid_date),
        Err(EsiError::InvalidResponse)
    );
}

#[test]
fn parse_structure_market_order_fills_in_known_solar_system_id() {
    // Deliberately no "system_id" field -- structure order JSON never
    // has one, unlike region orders.
    let order = parse_structure_market_order(
        serde_json::json!({
            "order_id": 7_386_855_683_i64,
            "type_id": 34,
            "location_id": 1_050_487_654_321_i64,
            "is_buy_order": true,
            "price": 4.2500,
            "volume_remain": 100_000,
            "volume_total": 250_000,
            "min_volume": 1_000,
            "range": "region",
            "issued": "2026-07-25T12:00:00Z",
            "duration": 90
        }),
        30_000_505,
    )
    .unwrap();

    assert_eq!(order.system_id, 30_000_505);
    assert_eq!(order.location_id, 1_050_487_654_321);
    assert!(order.is_buy_order);
}

#[tokio::test]
async fn regional_market_orders_uses_public_filtered_endpoint_and_metadata() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0_u8; 4096];
        let read = socket.read(&mut request).await.unwrap();
        let request = String::from_utf8(request[..read].to_vec()).unwrap();
        assert!(request.starts_with(
            "GET /latest/markets/10000002/orders/?order_type=all&page=2&type_id=34 HTTP/1.1"
        ));
        assert!(request
            .to_ascii_lowercase()
            .contains("if-none-match: prior-etag"));
        let body = serde_json::json!([{
            "order_id": 7_386_855_683_i64,
            "type_id": 34,
            "location_id": 60_003_760,
            "system_id": 30_000_142,
            "is_buy_order": true,
            "price": 4.25,
            "volume_remain": 100_000,
            "volume_total": 250_000,
            "min_volume": 1_000,
            "range": "region",
            "issued": "2026-07-25T12:00:00Z",
            "duration": 90
        }])
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\netag: next-etag\r\nexpires: Sat, 25 Jul 2026 12:05:00 GMT\r\nx-pages: 3\r\nx-esi-error-limit-remain: 97\r\nx-esi-error-limit-reset: 42\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    let transport = HttpEsiTransport::public(format!("http://{address}"));

    let response = transport
        .regional_market_orders(10_000_002, 34, 2, Some("prior-etag"))
        .await
        .unwrap();
    server.await.unwrap();

    assert_eq!(response.records.len(), 1);
    assert_eq!(response.records[0].order_id, 7_386_855_683);
    assert_eq!(response.metadata.etag.as_deref(), Some("next-etag"));
    assert_eq!(response.metadata.pages, Some(3));
    assert_eq!(response.metadata.error_limit_remain, Some(97));
    assert_eq!(response.metadata.error_limit_reset, Some(42));
    assert_eq!(
        response.metadata.expires.as_deref(),
        Some("Sat, 25 Jul 2026 12:05:00 GMT")
    );
    assert_eq!(
        response.metadata.expires_at(),
        Some("2026-07-25T12:05:00Z".parse().unwrap())
    );
}

#[tokio::test]
async fn every_request_identifies_the_app_and_pins_the_compatibility_date() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0_u8; 4096];
        let read = socket.read(&mut request).await.unwrap();
        let request = String::from_utf8(request[..read].to_vec())
            .unwrap()
            .to_ascii_lowercase();
        let body = "[]";
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        request
    });
    let transport = HttpEsiTransport::public(format!("http://{address}"));

    transport.market_prices().await.unwrap();
    let request = server.await.unwrap();

    let user_agent = crate::client_identity::process_user_agent().to_ascii_lowercase();
    assert!(user_agent.starts_with("iskworks/"));
    assert!(
        request.contains(&format!("user-agent: {user_agent}\r\n")),
        "missing user-agent in {request}"
    );
    assert!(
        request.contains("x-compatibility-date: 2020-01-01\r\n"),
        "missing compatibility date in {request}"
    );
}

/// A fake ESI that answers each connection in turn with the next
/// canned response and records the request lines it saw.
async fn spawn_scripted_esi(
    responses: Vec<&'static str>,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let mut seen = Vec::new();
        for response in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 4096];
            let read = socket.read(&mut request).await.unwrap();
            let request = String::from_utf8(request[..read].to_vec()).unwrap();
            seen.push(request.lines().next().unwrap_or_default().to_string());
            socket.write_all(response.as_bytes()).await.unwrap();
        }
        seen
    });
    (format!("http://{address}"), handle)
}

const UNAVAILABLE: &str =
    "HTTP/1.1 503 Service Unavailable\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
const EMPTY_LIST: &str = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n[]";
const STATUS_OK: &str = concat!(
    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 70\r\nconnection: close\r\n\r\n",
    r#"{"players":0,"server_version":"1","start_time":"2026-10-07T11:00:40Z"}"#
);
// A cached answer from before downtime: 200, but the old server.
const STATUS_STALE: &str = concat!(
    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 70\r\nconnection: close\r\n\r\n",
    r#"{"players":0,"server_version":"1","start_time":"2026-10-06T11:04:12Z"}"#
);

fn during_downtime() -> DateTime<Utc> {
    "2026-10-07T11:01:00Z".parse().unwrap()
}

fn just_before_downtime() -> DateTime<Utc> {
    "2026-10-07T10:59:00Z".parse().unwrap()
}

#[tokio::test]
async fn nothing_is_sent_to_esi_just_before_downtime() {
    let transport = HttpEsiTransport::public("http://127.0.0.1:9".to_string())
        .with_downtime_guard(Arc::default(), just_before_downtime);

    assert_eq!(
        transport.market_prices().await,
        Err(EsiError::ServerDowntime {
            retry_after_seconds: Some(60)
        })
    );
}

#[tokio::test]
async fn downtime_checks_status_and_holds_requests_until_esi_is_back() {
    // One connection: the status check. A second request would find no
    // server and fail as `TemporaryFailure`, not `ServerDowntime`.
    let (base_url, server) = spawn_scripted_esi(vec![UNAVAILABLE]).await;
    let transport =
        HttpEsiTransport::public(base_url).with_downtime_guard(Arc::default(), during_downtime);

    assert!(matches!(
        transport.market_prices().await,
        Err(EsiError::ServerDowntime { .. })
    ));
    assert!(matches!(
        transport.market_prices().await,
        Err(EsiError::ServerDowntime { .. })
    ));
    assert_eq!(server.await.unwrap(), vec!["GET /latest/status/ HTTP/1.1"]);
}

#[tokio::test]
async fn a_healthy_status_check_ends_the_downtime_pause() {
    let (base_url, server) = spawn_scripted_esi(vec![STATUS_OK, EMPTY_LIST]).await;
    let transport =
        HttpEsiTransport::public(base_url).with_downtime_guard(Arc::default(), during_downtime);

    assert!(transport.market_prices().await.is_ok());
    assert_eq!(
        server.await.unwrap(),
        vec![
            "GET /latest/status/ HTTP/1.1",
            "GET /latest/markets/prices/ HTTP/1.1"
        ]
    );
}

#[tokio::test]
async fn a_status_answer_from_before_the_restart_keeps_the_pause() {
    // One connection: the status check. Sending the market request too would
    // find no server and fail as `TemporaryFailure`, not `ServerDowntime`.
    let (base_url, server) = spawn_scripted_esi(vec![STATUS_STALE]).await;
    let transport =
        HttpEsiTransport::public(base_url).with_downtime_guard(Arc::default(), during_downtime);

    assert!(matches!(
        transport.market_prices().await,
        Err(EsiError::ServerDowntime { .. })
    ));
    assert_eq!(server.await.unwrap(), vec!["GET /latest/status/ HTTP/1.1"]);
}

#[tokio::test]
async fn availability_reports_downtime_without_extra_esi_requests() {
    let (base_url, server) = spawn_scripted_esi(vec![UNAVAILABLE]).await;
    let transport =
        HttpEsiTransport::public(base_url).with_downtime_guard(Arc::default(), during_downtime);

    // The first check probes; the second reuses its answer.
    for _ in 0..2 {
        let availability = transport.availability().await;
        assert!(availability.downtime);
        assert!(availability.retry_after_seconds.is_some());
    }
    assert_eq!(server.await.unwrap().len(), 1);

    let outside = HttpEsiTransport::public("http://127.0.0.1:9".to_string());
    assert_eq!(outside.availability().await, EsiAvailability::default());
}

#[tokio::test]
async fn a_429_holds_further_requests_to_that_route_group_until_retry_after() {
    // One connection only: a second request reaching the network would
    // fail as `TemporaryFailure`, not `RateLimited`.
    let (base_url, server) = spawn_scripted_esi(vec![
        "HTTP/1.1 429 Too Many Requests\r\nretry-after: 45\r\nx-ratelimit-group: market-order\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
    ])
    .await;
    let transport = HttpEsiTransport::public(base_url);

    assert_eq!(
        transport
            .regional_market_orders(10_000_002, 34, 1, None)
            .await,
        Err(EsiError::RateLimited {
            retry_after_seconds: Some(45)
        })
    );
    assert!(matches!(
        transport
            .regional_market_orders(10_000_043, 35, 1, None)
            .await,
        Err(EsiError::RateLimited {
            retry_after_seconds: Some(1..=45)
        })
    ));
    assert_eq!(server.await.unwrap().len(), 1);
}

#[test]
fn retry_after_is_whatever_wait_esi_asked_for() {
    let seconds = std::time::Duration::from_secs;
    assert_eq!(
        EsiError::RateLimited {
            retry_after_seconds: Some(30)
        }
        .retry_after(),
        Some(seconds(30))
    );
    assert_eq!(
        EsiError::EsiErrorLimit {
            reset_seconds: Some(12)
        }
        .retry_after(),
        Some(seconds(12))
    );
    assert_eq!(
        EsiError::ServerDowntime {
            retry_after_seconds: Some(90)
        }
        .retry_after(),
        Some(seconds(90))
    );
    assert_eq!(EsiError::TemporaryFailure.retry_after(), None);
    assert_eq!(
        EsiError::RateLimited {
            retry_after_seconds: None
        }
        .retry_after(),
        None
    );
}

#[test]
fn only_the_real_esi_host_waits_out_tranquility_downtime() {
    assert!(downtime_guard_for("https://esi.evetech.net").is_some());
    assert!(downtime_guard_for("http://127.0.0.1:8080").is_none());
    assert!(downtime_guard_for("not a url").is_none());
}

#[tokio::test]
async fn structure_market_orders_uses_bearer_auth_and_paginates() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0_u8; 4096];
        let read = socket.read(&mut request).await.unwrap();
        let request = String::from_utf8(request[..read].to_vec()).unwrap();
        assert!(
            request.starts_with("GET /latest/markets/structures/1050487654321/?page=2 HTTP/1.1")
        );
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer structure-market-token"));
        // No type_id filter -- the structure endpoint returns every type
        // in the structure, unlike the region endpoint.
        let body = serde_json::json!([{
            "order_id": 7_386_855_683_i64,
            "type_id": 34,
            "location_id": 1_050_487_654_321_i64,
            "is_buy_order": false,
            "price": 4.2500,
            "volume_remain": 100_000,
            "volume_total": 250_000,
            "min_volume": 1_000,
            "range": "station",
            "issued": "2026-07-25T12:00:00Z",
            "duration": 90
        }])
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nx-pages: 1\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    let transport = HttpEsiTransport::public(format!("http://{address}"));

    let response = transport
        .structure_market_orders(
            "structure-market-token",
            1_050_487_654_321,
            30_000_505,
            2,
            None,
        )
        .await
        .unwrap();
    server.await.unwrap();

    assert_eq!(response.records.len(), 1);
    assert_eq!(response.records[0].order_id, 7_386_855_683);
    assert_eq!(response.records[0].system_id, 30_000_505);
    assert_eq!(response.metadata.pages, Some(1));
}

#[tokio::test]
async fn universe_names_posts_ids_and_accepts_partial_results() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0_u8; 4096];
        let read = socket.read(&mut request).await.unwrap();
        let request = String::from_utf8(request[..read].to_vec()).unwrap();
        assert!(request.starts_with("POST /latest/universe/names/ HTTP/1.1"));
        assert!(request.contains("[2119000222,9999999999]"));
        let body = serde_json::json!([{
            "id": 2_119_000_222_i64,
            "name": "Caldari Navy",
            "category": "corporation"
        }])
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    let transport = HttpEsiTransport::public(format!("http://{address}"));

    let names = transport
        .universe_names(&[2_119_000_222, 9_999_999_999])
        .await
        .unwrap();
    server.await.unwrap();

    assert_eq!(
        names,
        vec![EveEntityName {
            id: 2_119_000_222,
            name: "Caldari Navy".into(),
            category: "corporation".into(),
        }]
    );
}

#[tokio::test]
async fn market_http_failure_logs_safe_request_context() {
    const SENSITIVE_BODY: &str = "sensitive upstream response";
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0_u8; 4096];
        let _ = socket.read(&mut request).await.unwrap();
        let response = format!(
            "HTTP/1.1 503 Service Unavailable\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nx-esi-error-limit-remain: 12\r\nx-esi-error-limit-reset: 41\r\nconnection: close\r\n\r\n{body}",
            SENSITIVE_BODY.len(),
            body = SENSITIVE_BODY
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    let logs = capture_logs();
    let transport = HttpEsiTransport::public(format!("http://{address}"));

    let result = transport
        .regional_market_orders(10_000_002, 34, 2, None)
        .await;
    server.await.unwrap();

    assert_eq!(result, Err(EsiError::TemporaryFailure));
    let output = logs.text();
    assert!(output.contains("esi market request failed"), "{output}");
    assert!(output.contains("region_id=10000002"), "{output}");
    assert!(output.contains("type_id=34"), "{output}");
    assert!(output.contains("page=2"), "{output}");
    assert!(output.contains("status=503"), "{output}");
    assert!(output.contains("category=\"http_server\""), "{output}");
    assert!(!output.contains(SENSITIVE_BODY), "{output}");
}

#[tokio::test]
async fn closed_market_connection_logs_normalized_request_cause() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0_u8; 4096];
        let _ = socket.read(&mut request).await.unwrap();
        drop(socket);
    });
    let logs = capture_logs();
    let transport = HttpEsiTransport::public(format!("http://{address}"));

    let result = transport
        .regional_market_orders(10_000_002, 621, 1, None)
        .await;
    server.await.unwrap();

    assert_eq!(result, Err(EsiError::TemporaryFailure));
    let output = logs.text();
    assert!(output.contains("esi market request failed"), "{output}");
    assert!(output.contains("type_id=621"), "{output}");
    assert!(
        output.contains("cause=\"connection_closed\"")
            || output.contains("cause=\"connection_reset\"")
            || output.contains("cause=\"incomplete_message\""),
        "{output}"
    );
    assert!(output.contains("is_request=true"), "{output}");
    assert!(!output.contains(&format!("http://{address}")), "{output}");
}

#[tokio::test]
async fn market_request_builder_failure_is_permanent() {
    let transport = HttpEsiTransport::public(String::new());

    let result = transport
        .regional_market_orders(10_000_002, 587, 1, None)
        .await;

    assert_eq!(result, Err(EsiError::PermanentFailure));
}

#[tokio::test]
async fn successful_market_request_is_logged_at_debug() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0_u8; 4096];
        let _ = socket.read(&mut request).await.unwrap();
        let body = "[]";
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nx-pages: 1\r\nx-esi-error-limit-remain: 99\r\nx-esi-error-limit-reset: 7\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    let logs = capture_logs();
    let transport = HttpEsiTransport::public(format!("http://{address}"));

    let result = transport
        .regional_market_orders(10_000_002, 34, 1, None)
        .await
        .unwrap();
    server.await.unwrap();

    assert!(result.records.is_empty());
    let output = logs.text();
    assert!(output.contains("esi market request completed"), "{output}");
    assert!(output.contains("type_id=34"), "{output}");
    assert!(output.contains("page=1"), "{output}");
    assert!(output.contains("status=200"), "{output}");
    assert!(output.contains("page_count=1"), "{output}");
}

#[test]
fn token_debug_is_redacted() {
    let token = RefreshedToken {
        access_token: "access-secret".into(),
        rotated_refresh_token: Some("refresh-secret".into()),
        expires_at: Utc::now(),
        identity: Identity {
            character_id: 1,
            character_name: "Pilot".into(),
            scopes: BTreeSet::new(),
        },
    };
    let debug = format!("{token:?}");
    assert!(!debug.contains("access-secret"));
    assert!(!debug.contains("refresh-secret"));
}

#[test]
fn parse_character_public_info_reads_identity_corp_and_security_status() {
    let raw = serde_json::json!({
        "name": "Aeva Stark",
        "corporation_id": 98_000_001,
        "security_status": -0.283_45,
    });
    let info = parse_character_public_info(2_119_000_001, raw).unwrap();
    assert_eq!(info.character_id, 2_119_000_001);
    assert_eq!(info.name, "Aeva Stark");
    assert_eq!(info.corporation_id, 98_000_001);
    assert_eq!(
        info.security_status,
        Some(rust_decimal::Decimal::from_str("-0.28345").unwrap())
    );
}

#[test]
fn parse_character_public_info_rejects_a_missing_name() {
    let raw = serde_json::json!({ "corporation_id": 98_000_001 });
    assert!(parse_character_public_info(2_119_000_001, raw).is_err());
}

#[test]
fn parse_character_location_reads_solar_system_and_optional_docking_point() {
    let raw = serde_json::json!({
        "solar_system_id": 30_000_142,
        "station_id": 60_003_760,
    });
    let location = parse_character_location(raw).unwrap();
    assert_eq!(location.solar_system_id, 30_000_142);
    assert_eq!(location.station_id, Some(60_003_760));
    assert_eq!(location.structure_id, None);
}

#[test]
fn parse_character_skills_reads_total_sp_and_skill_entries() {
    let raw = serde_json::json!({
        "total_sp": 61_200_000_i64,
        "unallocated_sp": 500_000_i64,
        "skills": [
            {
                "skill_id": 3380,
                "active_skill_level": 5,
                "trained_skill_level": 5,
                "skillpoints_in_skill": 1_280_000_i64,
            }
        ],
    });
    let skills = parse_character_skills(raw).unwrap();
    assert_eq!(skills.total_sp, 61_200_000);
    assert_eq!(skills.unallocated_sp, Some(500_000));
    assert_eq!(skills.skills.len(), 1);
    assert_eq!(skills.skills[0].skill_id, 3380);
    assert_eq!(skills.skills[0].active_skill_level, 5);
}

#[test]
fn parse_character_skill_queue_entry_reads_finish_date_when_present() {
    let raw = serde_json::json!({
        "skill_id": 3327,
        "finished_level": 5,
        "queue_position": 0,
        "start_date": "2026-08-20T00:00:00Z",
        "finish_date": "2026-08-26T06:44:00Z",
    });
    let entry = parse_character_skill_queue_entry(raw).unwrap();
    assert_eq!(entry.skill_id, 3327);
    assert_eq!(entry.finished_level, 5);
    assert!(entry.finish_date.is_some());
}

#[test]
fn parse_character_skill_queue_entry_allows_an_unstarted_future_entry() {
    let raw = serde_json::json!({
        "skill_id": 3336,
        "finished_level": 5,
        "queue_position": 1,
    });
    let entry = parse_character_skill_queue_entry(raw).unwrap();
    assert!(entry.start_date.is_none());
    assert!(entry.finish_date.is_none());
}

#[test]
fn parse_character_skill_queue_entry_reads_sp_bounds_when_present() {
    let raw = serde_json::json!({
        "skill_id": 3327,
        "finished_level": 5,
        "queue_position": 0,
        "start_date": "2026-08-20T00:00:00Z",
        "finish_date": "2026-08-26T06:44:00Z",
        "training_start_sp": 45_000,
        "level_start_sp": 40_000,
        "level_end_sp": 256_000,
    });
    let entry = parse_character_skill_queue_entry(raw).unwrap();
    assert_eq!(entry.training_start_sp, Some(45_000));
    assert_eq!(entry.level_start_sp, Some(40_000));
    assert_eq!(entry.level_end_sp, Some(256_000));
}

#[test]
fn parse_character_skill_queue_entry_allows_missing_sp_bounds() {
    let raw = serde_json::json!({
        "skill_id": 3336,
        "finished_level": 5,
        "queue_position": 1,
    });
    let entry = parse_character_skill_queue_entry(raw).unwrap();
    assert_eq!(entry.training_start_sp, None);
    assert_eq!(entry.level_start_sp, None);
    assert_eq!(entry.level_end_sp, None);
}

#[test]
fn parse_character_planet_reads_the_colony_header() {
    let planet = parse_character_planet(serde_json::json!({
        "last_update": "2026-10-01T06:23:47Z",
        "num_pins": 14,
        "owner_id": 2_119_000_001_i64,
        "planet_id": 40_009_077,
        "planet_type": "barren",
        "solar_system_id": 30_000_142,
        "upgrade_level": 5
    }))
    .unwrap();
    assert_eq!(planet.planet_id, 40_009_077);
    assert_eq!(planet.planet_type, "barren");
    assert_eq!(planet.upgrade_level, 5);
    assert_eq!(
        planet.last_update,
        "2026-10-01T06:23:47Z".parse::<DateTime<Utc>>().unwrap()
    );
}

#[test]
fn parse_character_planet_detail_reads_extractors_factories_and_contents() {
    let detail = parse_character_planet_detail(&serde_json::json!({
        "links": [],
        "routes": [],
        "pins": [
            {
                "pin_id": 1, "type_id": 3_060, "latitude": 1.0, "longitude": 1.0,
                "install_time": "2026-09-28T06:00:00Z",
                "expiry_time": "2026-10-05T06:00:00Z",
                "last_cycle_start": "2026-10-01T06:00:00Z",
                "extractor_details": {
                    "cycle_time": 1_800, "head_radius": 0.01,
                    "heads": [{"head_id": 0, "latitude": 1.0, "longitude": 1.0},
                              {"head_id": 1, "latitude": 1.0, "longitude": 1.0}],
                    "product_type_id": 2_268, "qty_per_cycle": 6_000
                }
            },
            {
                "pin_id": 2, "type_id": 2_470, "latitude": 1.0, "longitude": 1.0,
                "factory_details": {"schematic_id": 65},
                "contents": [{"type_id": 2_389, "amount": 40}]
            },
            {
                "pin_id": 3, "type_id": 2_544, "latitude": 1.0, "longitude": 1.0,
                "schematic_id": null,
                "contents": [{"type_id": 9_838, "amount": 1_250}]
            }
        ]
    }))
    .unwrap();

    assert_eq!(detail.pins.len(), 3);
    assert_eq!(
        detail.pins[0].extractor,
        Some(PlanetExtractorObservation {
            product_type_id: 2_268,
            qty_per_cycle: 6_000,
            cycle_time_seconds: 1_800,
            head_count: 2,
        })
    );
    assert!(detail.pins[0].expiry_time.is_some());
    assert_eq!(detail.pins[1].schematic_id, Some(65));
    assert_eq!(detail.pins[1].extractor, None);
    assert_eq!(detail.pins[2].schematic_id, None);
    assert_eq!(
        detail.pins[2].contents,
        vec![PlanetPinContentObservation {
            type_id: 9_838,
            amount: 1_250
        }]
    );
}

#[test]
fn parse_character_industry_job_reads_the_full_esi_row_shape() {
    let raw = serde_json::json!({
        "job_id": 500_123,
        "activity_id": 8,
        "blueprint_id": 1_040_000_000_001_i64,
        "blueprint_type_id": 691,
        "blueprint_location_id": 1_030_000_000_001_i64,
        "output_location_id": 1_030_000_000_002_i64,
        "installer_id": 2_119_000_001_i64,
        "product_type_id": 11_378,
        "facility_id": 1_050_474_463_169_i64,
        "station_id": 60_003_760,
        "runs": 10,
        "licensed_runs": 200,
        "cost": 1_234_567.89,
        "probability": 0.462,
        "duration": 39_600,
        "status": "active",
        "start_date": "2026-09-09T00:00:00Z",
        "end_date": "2026-09-09T11:00:00Z",
    });
    let job = parse_character_industry_job(raw).unwrap();
    assert_eq!(job.job_id, 500_123);
    assert_eq!(job.activity_id, 8);
    assert_eq!(job.product_type_id, Some(11_378));
    assert_eq!(job.station_id, Some(60_003_760));
    assert_eq!(job.runs, 10);
    assert_eq!(job.licensed_runs, Some(200));
    assert_eq!(job.cost, Some(Decimal::from_str("1234567.89").unwrap()));
    assert_eq!(job.probability, Some(Decimal::from_str("0.462").unwrap()));
    assert_eq!(job.duration_seconds, Some(39_600));
    assert!(job.pause_date.is_none());
}

#[test]
fn parse_character_industry_job_tolerates_a_minimal_active_row() {
    let raw = serde_json::json!({
        "job_id": 1,
        "activity_id": 1,
        "blueprint_type_id": 691,
        "facility_id": 60_003_760,
        "status": "active",
        "start_date": "2026-09-09T00:00:00Z",
        "end_date": "2026-09-09T01:00:00Z",
    });
    let job = parse_character_industry_job(raw).unwrap();
    assert_eq!(job.runs, 1);
    assert_eq!(job.product_type_id, None);
    assert_eq!(job.cost, None);
    assert_eq!(job.probability, None);
}

#[test]
fn sort_skill_queue_entries_normalizes_to_queue_position_ascending() {
    let make = |queue_position: i64| CharacterSkillQueueEntry {
        skill_id: queue_position,
        finished_level: 1,
        queue_position,
        start_date: None,
        finish_date: None,
        training_start_sp: None,
        level_start_sp: None,
        level_end_sp: None,
    };
    let out_of_order = vec![make(2), make(0), make(1)];
    let sorted = sort_skill_queue_entries(out_of_order);
    assert_eq!(
        sorted.iter().map(|e| e.queue_position).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn parse_character_industry_job_reads_activity_and_lifecycle_dates() {
    let raw = serde_json::json!({
        "job_id": 500_001,
        "activity_id": 1,
        "blueprint_type_id": 691,
        "facility_id": 1_050_474_463_169_i64,
        "status": "active",
        "start_date": "2026-08-20T00:00:00Z",
        "end_date": "2026-08-20T04:12:00Z",
    });
    let job = parse_character_industry_job(raw).unwrap();
    assert_eq!(job.job_id, 500_001);
    assert_eq!(job.activity_id, 1);
    assert_eq!(job.status, "active");
}

// --- explicit HTTP failure bounds ---------------------------------------
//
// The shared client (`build_client`) carries ESI_CONNECT_TIMEOUT /
// ESI_REQUEST_TIMEOUT. These prove a wedged upstream resolves as the
// retryable `TemporaryFailure` -- including a stall that only surfaces
// while the response *body* is being read -- instead of occupying the
// caller (and a market/character concurrency slot) forever, while a
// genuinely malformed complete body still maps to `InvalidResponse`.
//
// `#[tokio::test(start_paused = true)]` freezes the clock and lets the
// runtime auto-advance virtual time to the next pending timer once every
// task is parked -- so the request-timeout deadline fires deterministically
// and no test waits a real 30 seconds.

/// Accepts the connection, reads the request, then holds the socket open
/// forever without writing a byte. Returns the base URL and the server
/// task (kept alive by the caller binding it).
async fn spawn_blackhole() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = vec![0_u8; 4096];
        let _ = socket.read(&mut buffer).await;
        // Never respond; never close. The client must bail out on its
        // own request timeout, not on EOF.
        std::future::pending::<()>().await;
        drop(socket);
    });
    (format!("http://{address}"), handle)
}

#[tokio::test(start_paused = true)]
async fn hung_regional_market_request_is_bounded_as_temporary_failure() {
    let (base_url, _server) = spawn_blackhole().await;
    let transport = HttpEsiTransport::public(base_url);

    let result = transport
        .regional_market_orders(10_000_002, 34, 1, None)
        .await;

    assert!(
        matches!(result, Err(EsiError::TemporaryFailure)),
        "a never-answering upstream must time out as retryable, got {result:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn hung_public_json_helper_request_is_bounded_as_temporary_failure() {
    // character_public_info -> get_public_json
    let (base_url, _server) = spawn_blackhole().await;
    let transport = HttpEsiTransport::public(base_url);

    let result = transport.character_public_info(2_119_000_001).await;

    assert!(
        matches!(result, Err(EsiError::TemporaryFailure)),
        "get_public_json path must be bounded, got {result:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn hung_authenticated_json_helper_request_is_bounded_as_temporary_failure() {
    // character_location -> get_json (bearer auth)
    let (base_url, _server) = spawn_blackhole().await;
    let transport = HttpEsiTransport::public(base_url);

    let result = transport
        .character_location("dummy-access-token", 2_119_000_001)
        .await;

    assert!(
        matches!(result, Err(EsiError::TemporaryFailure)),
        "get_json path must be bounded, got {result:?}"
    );
}

/// Once ESI reports the error budget nearly spent, the next call fails
/// fast with `EsiErrorLimit` and never reaches the network -- the server
/// below only answers once and is gone by the second call, so reaching
/// it would surface as a connection failure instead.
#[tokio::test]
async fn a_nearly_exhausted_error_limit_pauses_further_esi_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0_u8; 4096];
        let _ = socket.read(&mut request).await.unwrap();
        let response = "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nx-esi-error-limit-remain: 5\r\nx-esi-error-limit-reset: 30\r\nconnection: close\r\n\r\n";
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    let transport = HttpEsiTransport::public(format!("http://{address}"))
        .with_error_limit_guard(Arc::new(ErrorLimitGuard::default()));

    let first = transport.industry_systems().await;
    server.await.unwrap();
    assert!(
        matches!(first, Err(EsiError::PermanentFailure)),
        "{first:?}"
    );

    let second = transport.industry_systems().await;
    assert!(
        matches!(
            second,
            Err(EsiError::EsiErrorLimit {
                reset_seconds: Some(_)
            })
        ),
        "{second:?}"
    );
}

#[tokio::test]
async fn a_420_is_an_error_limit_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0_u8; 4096];
        let _ = socket.read(&mut request).await.unwrap();
        let response = "HTTP/1.1 420 Enhance Your Calm\r\ncontent-length: 0\r\nx-esi-error-limit-remain: 0\r\nx-esi-error-limit-reset: 17\r\nconnection: close\r\n\r\n";
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    let transport = HttpEsiTransport::public(format!("http://{address}"))
        .with_error_limit_guard(Arc::new(ErrorLimitGuard::default()));

    let result = transport.industry_systems().await;
    server.await.unwrap();
    assert!(
        matches!(
            result,
            Err(EsiError::EsiErrorLimit {
                reset_seconds: Some(17)
            })
        ),
        "{result:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn hung_oauth_token_request_is_bounded_as_temporary_failure() {
    // refresh -> token_request (the OAuth/SSO client shares the same bounds)
    let (base_url, _server) = spawn_blackhole().await;
    let transport = HttpEsiTransport::new(
        "client-id".to_string(),
        "http://localhost/callback".to_string(),
        format!("{base_url}/token"),
        format!("{base_url}/jwks"),
        base_url,
        "issuer".to_string(),
    );

    let result = transport.refresh("dummy-refresh-token").await;

    assert!(
        matches!(result, Err(EsiError::TemporaryFailure)),
        "OAuth token request must be bounded, got {result:?}"
    );
}

#[tokio::test]
async fn revoke_refresh_token_posts_to_the_sso_revoke_endpoint() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0_u8; 4096];
        let read = socket.read(&mut request).await.unwrap();
        let request = String::from_utf8(request[..read].to_vec()).unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .await
            .unwrap();
        request
    });
    let base_url = format!("http://{address}");
    let transport = HttpEsiTransport::new(
        "client-id".to_string(),
        "http://localhost/callback".to_string(),
        format!("{base_url}/v2/oauth/token"),
        format!("{base_url}/jwks"),
        base_url,
        "issuer".to_string(),
    );

    transport
        .revoke_refresh_token("the-refresh-token")
        .await
        .unwrap();
    let request = server.await.unwrap();

    assert!(
        request.starts_with("POST /v2/oauth/revoke HTTP/1.1"),
        "{request}"
    );
    assert!(request.contains("token=the-refresh-token"), "{request}");
    assert!(
        request.contains("token_type_hint=refresh_token"),
        "{request}"
    );
    assert!(request.contains("client_id=client-id"), "{request}");
}

/// A revoked/expired refresh token (`invalid_grant`) means the user has to
/// reconnect; any other 400 from the token endpoint (e.g. `invalid_client`,
/// our own misconfiguration) must not be mistaken for that, or every
/// connection would be flagged for reconnection at once.
#[tokio::test]
async fn token_endpoint_invalid_grant_is_authorization_required() {
    for (error_code, expect_authorization_required) in
        [("invalid_grant", true), ("invalid_client", false)]
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 4096];
            let _ = socket.read(&mut request).await.unwrap();
            let body = format!(
                r#"{{"error":"{error_code}","error_description":"Invalid refresh token. Token missing/expired."}}"#
            );
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });
        let base_url = format!("http://{address}");
        let transport = HttpEsiTransport::new(
            "client-id".to_string(),
            "http://localhost/callback".to_string(),
            format!("{base_url}/token"),
            format!("{base_url}/jwks"),
            base_url,
            "issuer".to_string(),
        );

        let result = transport.refresh("revoked-refresh-token").await;
        server.await.unwrap();

        if expect_authorization_required {
            assert!(
                matches!(result, Err(EsiError::AuthorizationRequired)),
                "{error_code}: got {result:?}"
            );
        } else {
            assert!(
                matches!(result, Err(EsiError::PermanentFailure)),
                "{error_code}: got {result:?}"
            );
        }
    }
}

#[tokio::test(start_paused = true)]
async fn body_read_timeout_is_temporary_failure_not_invalid_response() {
    // 200 OK with headers promising more body than is ever delivered:
    // `send()` succeeds, `.json()` blocks on the missing bytes until the
    // request timeout elapses.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let _server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = vec![0_u8; 4096];
        let _ = socket.read(&mut buffer).await;
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 4096\r\n\r\n[{",
            )
            .await
            .unwrap();
        std::future::pending::<()>().await;
        drop(socket);
    });
    let transport = HttpEsiTransport::public(format!("http://{address}"));

    let result = transport
        .regional_market_orders(10_000_002, 34, 1, None)
        .await;

    assert!(
        matches!(result, Err(EsiError::TemporaryFailure)),
        "a timeout during body read must be retryable, got {result:?}"
    );
}

#[tokio::test]
async fn complete_malformed_json_body_stays_invalid_response() {
    // A fully delivered but non-JSON body: a genuine decode failure that
    // the timeout classification must not swallow.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = vec![0_u8; 4096];
        let _ = socket.read(&mut buffer).await;
        let body = "not json at all";
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    let transport = HttpEsiTransport::public(format!("http://{address}"));

    let result = transport
        .regional_market_orders(10_000_002, 34, 1, None)
        .await;
    server.await.unwrap();

    assert!(
        matches!(result, Err(EsiError::InvalidResponse)),
        "a complete malformed body must stay InvalidResponse, got {result:?}"
    );
}

#[test]
fn build_client_succeeds_with_bounds() {
    // build_client `.expect`s; this is the loud-failure guard rail.
    let _client = build_client();
    assert_eq!(ESI_CONNECT_TIMEOUT, std::time::Duration::from_secs(10));
    assert_eq!(ESI_REQUEST_TIMEOUT, std::time::Duration::from_secs(30));
}

// Throwaway 2048-bit RSA key (PKCS#1 DER) that only ever signs test JWTs.
const SSO_TEST_KEY_DER: &[u8] = include_bytes!("../testdata/sso_test_rsa_key.der");
const SSO_TEST_KEY_ID: &str = "JWT-Signature-Key";

/// Serves the test key's JWKS to a single request.
async fn spawn_jwks_server() -> (String, tokio::task::JoinHandle<()>) {
    let key = jsonwebtoken::EncodingKey::from_rsa_der(SSO_TEST_KEY_DER);
    let mut jwk = jsonwebtoken::jwk::Jwk::from_encoding_key(&key, Algorithm::RS256).unwrap();
    jwk.common.key_id = Some(SSO_TEST_KEY_ID.to_string());
    let body = serde_json::to_string(&JwkSet { keys: vec![jwk] }).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0_u8; 4096];
        let _ = socket.read(&mut request).await.unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    (format!("http://{address}"), handle)
}

fn sso_transport(base_url: String) -> HttpEsiTransport {
    HttpEsiTransport::new(
        "client-id".to_string(),
        "http://localhost/callback".to_string(),
        format!("{base_url}/v2/oauth/token"),
        format!("{base_url}/jwks"),
        base_url,
        "https://login.eveonline.com".to_string(),
    )
}

fn signed_sso_token(audience: serde_json::Value) -> String {
    let mut header = jsonwebtoken::Header::new(Algorithm::RS256);
    header.kid = Some(SSO_TEST_KEY_ID.to_string());
    let claims = serde_json::json!({
        "sub": "CHARACTER:EVE:2112625428",
        "name": "Test Pilot",
        "scp": ["esi-assets.read_assets.v1", "esi-industry.read_character_jobs.v1"],
        "aud": audience,
        "iss": "https://login.eveonline.com",
        "exp": (Utc::now() + Duration::minutes(20)).timestamp(),
        "owner": "owner-hash",
    });
    jsonwebtoken::encode(
        &header,
        &claims,
        &jsonwebtoken::EncodingKey::from_rsa_der(SSO_TEST_KEY_DER),
    )
    .unwrap()
}

#[tokio::test]
async fn validate_token_accepts_a_correctly_signed_sso_access_token() {
    let (base_url, _server) = spawn_jwks_server().await;
    let token = signed_sso_token(serde_json::json!(["client-id", "EVE Online"]));

    let (identity, owner_hash) = sso_transport(base_url)
        .validate_token(&token)
        .await
        .unwrap();

    assert_eq!(identity.character_id, 2_112_625_428);
    assert_eq!(identity.character_name, "Test Pilot");
    assert!(identity.scopes.contains("esi-assets.read_assets.v1"));
    assert_eq!(owner_hash.as_deref(), Some("owner-hash"));
}

#[tokio::test]
async fn sso_signing_keys_are_fetched_once_and_reused() {
    // The fake JWKS endpoint answers a single request.
    let (base_url, server) = spawn_jwks_server().await;
    let transport = sso_transport(base_url);
    let token = signed_sso_token(serde_json::json!(["client-id", "EVE Online"]));

    transport.validate_token(&token).await.unwrap();
    server.await.unwrap();
    let (identity, _) = transport.validate_token(&token).await.unwrap();
    assert_eq!(identity.character_id, 2_112_625_428);

    // An unknown key id doesn't trigger a refetch right after a fetch.
    let mut header = decode_header(&token).unwrap();
    header.kid = Some("rotated-key".to_string());
    let foreign = jsonwebtoken::encode(
        &header,
        &serde_json::json!({"sub": "CHARACTER:EVE:1"}),
        &jsonwebtoken::EncodingKey::from_rsa_der(SSO_TEST_KEY_DER),
    )
    .unwrap();
    assert_eq!(
        transport.validate_token(&foreign).await,
        Err(EsiError::InvalidIdentity)
    );
}

#[tokio::test]
async fn validate_token_rejects_a_tampered_signature() {
    let (base_url, _server) = spawn_jwks_server().await;
    let token = signed_sso_token(serde_json::json!(["client-id", "EVE Online"]));
    let (signed, signature) = token.rsplit_once('.').unwrap();
    let flipped = if signature.starts_with('A') { 'B' } else { 'A' };
    let tampered = format!("{signed}.{flipped}{}", &signature[1..]);

    let result = sso_transport(base_url).validate_token(&tampered).await;

    assert!(
        matches!(result, Err(EsiError::InvalidIdentity)),
        "{result:?}"
    );
}

#[tokio::test]
async fn validate_token_rejects_a_token_for_another_client() {
    let (base_url, _server) = spawn_jwks_server().await;
    let token = signed_sso_token(serde_json::json!(["other-client", "EVE Online"]));

    let result = sso_transport(base_url).validate_token(&token).await;

    assert!(
        matches!(result, Err(EsiError::InvalidIdentity)),
        "{result:?}"
    );
}

fn blueprint_with(material_efficiency: i16, time_efficiency: i16) -> BlueprintAssetObservation {
    BlueprintAssetObservation {
        item_id: 1,
        type_id: 691,
        location_id: 60_003_760,
        location_flag: "Hangar".to_string(),
        material_efficiency,
        time_efficiency,
        runs: -1,
        quantity: -1,
        raw: serde_json::json!({}),
    }
}

#[test]
fn blueprint_efficiency_must_be_within_the_games_ranges() {
    assert!(blueprint_with(0, 0).efficiency_in_range());
    assert!(blueprint_with(10, 20).efficiency_in_range());
    assert!(!blueprint_with(11, 20).efficiency_in_range());
    assert!(!blueprint_with(-1, 0).efficiency_in_range());
    assert!(!blueprint_with(10, 21).efficiency_in_range());
    assert!(!blueprint_with(0, -2).efficiency_in_range());
}

#[test]
fn an_out_of_range_blueprint_still_parses_so_the_caller_can_skip_it() {
    let parsed = parse_blueprint(serde_json::json!({
        "item_id": 1, "type_id": 691, "location_id": 60_003_760, "location_flag": "Hangar",
        "material_efficiency": 11, "time_efficiency": 20, "runs": -1, "quantity": -1,
    }))
    .unwrap();
    assert!(!parsed.efficiency_in_range());
}
