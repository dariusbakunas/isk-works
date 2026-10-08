use super::*;
use chrono::{Duration, Utc};
use iskworks_esi::{
    AssetObservation, AuthenticatedToken, EsiResponse, Identity, IndustrySystemCostIndex,
    RefreshedToken, StructureInformation, WalletTransactionObservation,
};
use sqlx::PgPool;
use std::collections::BTreeSet;

/// Only `exchange_code` is exercised by the login flow — the rest of
/// `EsiTransport` is ESI resource access, which login never touches.
struct FakeLoginTransport {
    character_id: i64,
    character_name: String,
}

#[async_trait::async_trait]
impl EsiTransport for FakeLoginTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        Ok(AuthenticatedToken {
            access_token: "fake-access".to_string(),
            refresh_token: "fake-refresh".to_string(),
            expires_at: Utc::now() + Duration::hours(1),
            owner_hash: Some(format!("owner-of-{}", self.character_id)),
            identity: Identity {
                character_id: self.character_id,
                character_name: self.character_name.clone(),
                scopes: BTreeSet::new(),
            },
        })
    }

    async fn refresh(&self, _refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        unimplemented!("login never refreshes a token")
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        _page: u32,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        unimplemented!("login never reads ESI resources")
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        _from_id: Option<i64>,
        _etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        unimplemented!("login never reads ESI resources")
    }

    async fn structure(
        &self,
        _access_token: &str,
        _structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        unimplemented!("login never reads ESI resources")
    }

    async fn industry_systems(&self) -> Result<EsiResponse<IndustrySystemCostIndex>, EsiError> {
        unimplemented!("login never reads ESI resources")
    }
}

fn service_with_identity(pool: PgPool, character_id: i64, character_name: &str) -> AuthService {
    AuthService::new_for_tests(
        Arc::new(PgUserRepository::new(pool.clone())),
        Arc::new(PgSessionRepository::new(pool.clone())),
        Arc::new(PgInviteRepository::new(pool)),
        Arc::new(FakeLoginTransport {
            character_id,
            character_name: character_name.to_string(),
        }),
    )
}

/// Same, but with invite-only mode enforced and a shared `PgInviteRepository`
/// handle so the test can also create/redeem invite rows.
fn service_with_invite_mode(
    pool: PgPool,
    character_id: i64,
    character_name: &str,
) -> (AuthService, Arc<PgInviteRepository>) {
    let invites = Arc::new(PgInviteRepository::new(pool.clone()));
    let service = AuthService::new_for_tests(
        Arc::new(PgUserRepository::new(pool.clone())),
        Arc::new(PgSessionRepository::new(pool)),
        invites.clone(),
        Arc::new(FakeLoginTransport {
            character_id,
            character_name: character_name.to_string(),
        }),
    )
    .with_invite_required(true);
    (service, invites)
}

fn query_param(url: &str, name: &str) -> String {
    let query = url
        .split('?')
        .nth(1)
        .expect("authorization url has a query string");
    query
        .split('&')
        .find_map(|pair| {
            let (key, value) = pair.split_once('=').expect("query pair has a value");
            (key == name).then(|| value.to_string())
        })
        .unwrap_or_else(|| panic!("missing query param {name}"))
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn begin_login_requests_no_esi_scopes(pool: PgPool) {
    let service = service_with_identity(pool, 1, "Test Character");
    let url = service.begin_login(None).await.unwrap().authorization_url;
    assert_eq!(query_param(&url, "scope"), "");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn complete_login_issues_a_session_that_resolves_back_to_the_character(pool: PgPool) {
    let service = service_with_identity(pool, 1, "Test Character");
    let url = service.begin_login(None).await.unwrap().authorization_url;
    let state = query_param(&url, "state");

    let pending = service.try_consume_login(&state).await.unwrap().unwrap();
    let token = service.finish_login(pending, "fake-code").await.unwrap();
    let resolved = service.resolve_session(&token).await.unwrap().unwrap();
    assert_eq!(resolved.eve_character_name, "Test Character");
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn an_unknown_state_does_not_match_the_login_flow(pool: PgPool) {
    let service = service_with_identity(pool, 1, "Test Character");
    let pending = service.try_consume_login("not-a-real-state").await.unwrap();
    assert!(pending.is_none());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn a_state_cannot_be_consumed_twice(pool: PgPool) {
    let service = service_with_identity(pool, 1, "Test Character");
    let url = service.begin_login(None).await.unwrap().authorization_url;
    let state = query_param(&url, "state");

    assert!(service.try_consume_login(&state).await.unwrap().is_some());
    assert!(service.try_consume_login(&state).await.unwrap().is_none());
}

#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn each_users_own_workspace_is_isolated_through_the_router(pool: PgPool) {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let auth_service = AuthService::new_for_tests(
        Arc::new(PgUserRepository::new(pool.clone())),
        Arc::new(PgSessionRepository::new(pool.clone())),
        Arc::new(PgInviteRepository::new(pool.clone())),
        Arc::new(MultiIdentityFakeTransport),
    );
    let workspace_repository = iskworks_storage::PgWorkspaceRepository::new(pool);
    let state = crate::AppState::new(Arc::new(workspace_repository)).with_auth(Some(auth_service));
    let app = crate::build_router(state);

    let no_cookie_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/workspace")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_cookie_response.status(), StatusCode::UNAUTHORIZED);

    let cookie_a = login_and_extract_cookie(&app, "1:CharacterA").await;
    let cookie_b = login_and_extract_cookie(&app, "2:CharacterB").await;
    assert_ne!(cookie_a, cookie_b);

    let workspace_a = fetch_workspace(&app, &cookie_a).await;
    let workspace_b = fetch_workspace(&app, &cookie_b).await;

    assert_eq!(workspace_a["workspace"]["name"], "CharacterA");
    assert_eq!(workspace_b["workspace"]["name"], "CharacterB");
    assert_ne!(
        workspace_a["workspace"]["id"],
        workspace_b["workspace"]["id"]
    );
}

/// Regression: with auth configured, an anonymous
/// caller cannot create/claim the singleton workspace on a fresh DB,
/// and the SDE/reference surface is not freely callable either. A real
/// first login still onboards normally.
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
#[sqlx::test(migrations = "../../migrations")]
async fn anonymous_callers_cannot_create_a_workspace_or_hit_reference_routes(pool: PgPool) {
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use tower::ServiceExt;

    let auth_service = AuthService::new_for_tests(
        Arc::new(PgUserRepository::new(pool.clone())),
        Arc::new(PgSessionRepository::new(pool.clone())),
        Arc::new(PgInviteRepository::new(pool.clone())),
        Arc::new(MultiIdentityFakeTransport),
    );
    let workspace_repository = iskworks_storage::PgWorkspaceRepository::new(pool.clone());
    let state = crate::AppState::new(Arc::new(workspace_repository)).with_auth(Some(auth_service));
    let app = crate::build_router(state);

    // Anonymous workspace creation is refused, and nothing is written.
    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/workspace")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"attacker"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::UNAUTHORIZED);
    let workspace_count: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        workspace_count, 0,
        "no workspace row may be created anonymously"
    );

    // A reference/search route is also behind the session gate.
    let reference = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/blueprints/search?q=rifter&limit=999999999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reference.status(), StatusCode::UNAUTHORIZED);

    // The OAuth callback stays anonymous (no session exists yet).
    let callback = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/eve/oauth/callback?state=not-a-real-state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(
        callback.status(),
        StatusCode::UNAUTHORIZED,
        "the OAuth callback must not be gated by require_session_middleware"
    );

    // A genuine first login still onboards.
    let cookie = login_and_extract_cookie(&app, "7:FirstPilot").await;
    let workspace = fetch_workspace(&app, &cookie).await;
    assert_eq!(workspace["workspace"]["name"], "FirstPilot");
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, 1);
}

async fn login_and_extract_cookie(app: &axum::Router, code: &str) -> String {
    crate::routes::esi::tests::login(app, code).await
}

async fn fetch_workspace(app: &axum::Router, cookie: &str) -> serde_json::Value {
    use axum::body::Body;
    use axum::http::{header, Request};
    use tower::ServiceExt;

    response_json(
        app.clone()
            .oneshot(
                Request::builder()
                    .uri("/api/workspace")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Invite-only onboarding gate.
mod invite_gate {
    use super::*;
    use iskworks_core::{generate_invite_code, hash_invite_code_from_raw};
    use iskworks_storage::{NewInvite, PgInviteRepository};

    async fn mint_invite(pool: &PgPool, max_uses: i32) -> String {
        let code = generate_invite_code();
        PgInviteRepository::new(pool.clone())
            .create_invite(NewInvite {
                code_hash: hash_invite_code_from_raw(&code),
                max_uses,
                expires_at: None,
                note: None,
                code_ciphertext: None,
            })
            .await
            .unwrap();
        code
    }

    async fn workspace_count(pool: &PgPool) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM workspaces")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn use_count(pool: &PgPool, code: &str) -> i32 {
        sqlx::query_scalar("SELECT use_count FROM invite_codes WHERE code_hash = $1")
            .bind(hash_invite_code_from_raw(code))
            .fetch_one(pool)
            .await
            .unwrap()
    }

    // --- invite mode OFF ---------------------------------------------

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn invite_mode_off_onboards_a_new_user_normally(pool: PgPool) {
        let service = service_with_identity(pool.clone(), 1, "Open Pilot");
        let url = service.begin_login(None).await.unwrap().authorization_url;
        let state = query_param(&url, "state");
        let pending = service.try_consume_login(&state).await.unwrap().unwrap();
        let token = service.finish_login(pending, "fake-code").await.unwrap();
        assert!(service.resolve_session(&token).await.unwrap().is_some());
        assert_eq!(workspace_count(&pool).await, 1);
    }

    // --- character ownership changes -------------------------------

    /// A character sold through the Character Bazaar keeps its
    /// character_id but gets a new EVE account owner (the JWT `owner`
    /// hash). The buyer must not sign into the seller's workspace --
    /// with the seller's other characters' tokens -- or inherit admin.
    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn a_character_that_changed_owner_cannot_sign_in(pool: PgPool) {
        let service = AuthService::new_for_tests(
            Arc::new(PgUserRepository::new(pool.clone())),
            Arc::new(PgSessionRepository::new(pool.clone())),
            Arc::new(PgInviteRepository::new(pool.clone())),
            Arc::new(MultiIdentityFakeTransport),
        );
        let sign_in = |code: &'static str| {
            let service = service.clone();
            async move {
                let url = service.begin_login(None).await.unwrap().authorization_url;
                let state = query_param(&url, "state");
                let pending = service.try_consume_login(&state).await.unwrap().unwrap();
                service.finish_login(pending, code).await
            }
        };

        // First sign-in records the owner; the same owner signs in again.
        assert!(sign_in("31:Traded Pilot:seller").await.is_ok());
        assert!(sign_in("31:Traded Pilot:seller").await.is_ok());

        // The character changes hands.
        assert!(matches!(
            sign_in("31:Traded Pilot:buyer").await,
            Err(AuthApplicationError::CharacterTransferred)
        ));
        assert_eq!(workspace_count(&pool).await, 1);

        // A user from before owner hashes were recorded is backfilled on
        // their next sign-in, then held to it.
        assert!(sign_in("32:Old Pilot:original").await.is_ok());
        sqlx::query("UPDATE users SET eve_owner_hash = NULL WHERE eve_character_id = 32")
            .execute(&pool)
            .await
            .unwrap();
        assert!(sign_in("32:Old Pilot:original").await.is_ok());
        assert!(matches!(
            sign_in("32:Old Pilot:someone-else").await,
            Err(AuthApplicationError::CharacterTransferred)
        ));
    }

    // --- admin-disabled users --------------------------------------

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn a_disabled_user_cannot_sign_in_until_re_enabled(pool: PgPool) {
        use iskworks_core::AdminUsersRepository;

        let service = service_with_identity(pool.clone(), 21, "Blocked Pilot");
        let sign_in = |service: &AuthService| {
            let service = service.clone();
            async move {
                let url = service.begin_login(None).await.unwrap().authorization_url;
                let state = query_param(&url, "state");
                let pending = service.try_consume_login(&state).await.unwrap().unwrap();
                service.finish_login(pending, "fake-code").await
            }
        };

        let token = sign_in(&service).await.unwrap();
        let admin = iskworks_storage::PgAdminRepository::new(pool.clone());
        let user = service.resolve_session(&token).await.unwrap().unwrap();

        assert!(admin.set_user_disabled(user.user_id, true).await.unwrap());
        assert!(
            service.resolve_session(&token).await.unwrap().is_none(),
            "existing sessions stop working"
        );
        assert!(matches!(
            sign_in(&service).await,
            Err(AuthApplicationError::AccountDisabled)
        ));

        assert!(admin.set_user_disabled(user.user_id, false).await.unwrap());
        let token = sign_in(&service).await.unwrap();
        assert!(service.resolve_session(&token).await.unwrap().is_some());
    }

    // --- invite mode ON, returning user ----------------------------

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn invite_mode_on_lets_a_returning_user_in_without_an_invite(pool: PgPool) {
        let code = mint_invite(&pool, 5).await;
        let (service, _invites) = service_with_invite_mode(pool.clone(), 7, "Returning Pilot");

        // Seed the identity as an existing user (no invite).
        PgUserRepository::new(pool.clone())
            .claim_unclaimed_workspace_or_provision(
                EveIdentity {
                    character_id: 7,
                    character_name: "Returning Pilot".to_string(),
                },
                InviteGrant::NotRequired,
            )
            .await
            .unwrap();

        // Logs in again with NO invite -> succeeds, invite untouched.
        let url = service.begin_login(None).await.unwrap().authorization_url;
        let state = query_param(&url, "state");
        let pending = service.try_consume_login(&state).await.unwrap().unwrap();
        let token = service.finish_login(pending, "fake-code").await.unwrap();
        assert!(service.resolve_session(&token).await.unwrap().is_some());
        assert_eq!(use_count(&pool, &code).await, 0);
    }

    // --- invite mode ON, new user --------------------------------

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn invite_mode_on_denies_a_new_user_without_an_invite(pool: PgPool) {
        let (service, _invites) = service_with_invite_mode(pool.clone(), 8, "Gatecrasher");
        let url = service.begin_login(None).await.unwrap().authorization_url;
        let state = query_param(&url, "state");
        let pending = service.try_consume_login(&state).await.unwrap().unwrap();

        let result = service.finish_login(pending, "fake-code").await;
        assert!(matches!(result, Err(AuthApplicationError::InviteRequired)));
        assert_eq!(workspace_count(&pool).await, 0);
    }

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn a_bad_invite_is_rejected_before_the_eve_round_trip(pool: PgPool) {
        let (service, _invites) = service_with_invite_mode(pool.clone(), 9, "Typo Pilot");
        let result = service.begin_login(Some("ISK-NOPE-NOPE-NOPE-NOPE")).await;
        assert!(matches!(result, Err(AuthApplicationError::InviteInvalid)));
        // No pending row, no workspace.
        let pending_rows: i64 =
            sqlx::query_scalar("SELECT count(*) FROM login_oauth_pending_authorizations")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(pending_rows, 0);
        assert_eq!(workspace_count(&pool).await, 0);
    }

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn a_valid_invite_onboards_one_new_user_and_is_spent_once(pool: PgPool) {
        let code = mint_invite(&pool, 1).await;
        let (service, _invites) = service_with_invite_mode(pool.clone(), 10, "Invited Pilot");

        let url = service
            .begin_login(Some(&code))
            .await
            .unwrap()
            .authorization_url;
        // The raw code is nowhere in the authorization URL.
        assert!(!url.contains(&code));
        let state = query_param(&url, "state");
        let pending = service.try_consume_login(&state).await.unwrap().unwrap();
        let token = service.finish_login(pending, "fake-code").await.unwrap();

        assert!(service.resolve_session(&token).await.unwrap().is_some());
        assert_eq!(workspace_count(&pool).await, 1);
        assert_eq!(use_count(&pool, &code).await, 1);
    }

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn an_exhausted_invite_is_refused_at_the_next_login_start(pool: PgPool) {
        let code = mint_invite(&pool, 1).await;
        let (service, _invites) = service_with_invite_mode(pool.clone(), 11, "First Pilot");

        let url = service
            .begin_login(Some(&code))
            .await
            .unwrap()
            .authorization_url;
        let state = query_param(&url, "state");
        let pending = service.try_consume_login(&state).await.unwrap().unwrap();
        service.finish_login(pending, "fake-code").await.unwrap();

        // Second attempt with the same, now-spent code.
        let result = service.begin_login(Some(&code)).await;
        assert!(matches!(result, Err(AuthApplicationError::InviteInvalid)));
        assert_eq!(use_count(&pool, &code).await, 1);
    }

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn a_replayed_callback_state_cannot_consume_the_invite_again(pool: PgPool) {
        let code = mint_invite(&pool, 5).await;
        let (service, _invites) = service_with_invite_mode(pool.clone(), 12, "Replay Pilot");

        let url = service
            .begin_login(Some(&code))
            .await
            .unwrap()
            .authorization_url;
        let state = query_param(&url, "state");
        let pending = service.try_consume_login(&state).await.unwrap().unwrap();
        service.finish_login(pending, "fake-code").await.unwrap();
        assert_eq!(use_count(&pool, &code).await, 1);

        // Replaying the same state resolves to nothing -> no second
        // finish_login, no second increment.
        assert!(service.try_consume_login(&state).await.unwrap().is_none());
        assert_eq!(use_count(&pool, &code).await, 1);
    }

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn an_invite_disabled_mid_oauth_fails_the_callback_cleanly(pool: PgPool) {
        let code = mint_invite(&pool, 5).await;
        let invites = PgInviteRepository::new(pool.clone());
        let (service, _invites) = service_with_invite_mode(pool.clone(), 13, "Slowpoke Pilot");

        let url = service
            .begin_login(Some(&code))
            .await
            .unwrap()
            .authorization_url;
        let state = query_param(&url, "state");
        let pending = service.try_consume_login(&state).await.unwrap().unwrap();

        // Operator disables the invite while the user is still at EVE.
        let id = pending
            .invite_id
            .expect("pending row carries the invite id");
        assert!(invites.disable_invite(id).await.unwrap());

        let result = service.finish_login(pending, "fake-code").await;
        assert!(matches!(result, Err(AuthApplicationError::InviteInvalid)));
        assert_eq!(workspace_count(&pool).await, 0);
        assert_eq!(use_count(&pool, &code).await, 0);
    }

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn the_session_endpoint_advertises_invite_mode(pool: PgPool) {
        use axum::body::{to_bytes, Body};
        use axum::http::Request;
        use tower::ServiceExt;

        let (service, _invites) = service_with_invite_mode(pool.clone(), 14, "Probe");
        let app = crate::build_router(
            crate::AppState::new(Arc::new(iskworks_storage::PgWorkspaceRepository::new(pool)))
                .with_auth(Some(service)),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/auth/session")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["authenticated"], false);
        assert_eq!(json["inviteRequired"], true);
    }

    #[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
    #[sqlx::test(migrations = "../../migrations")]
    async fn the_login_route_rejects_a_bad_invite_code_with_invite_invalid(pool: PgPool) {
        use axum::body::{to_bytes, Body};
        use axum::http::{header, Request, StatusCode};
        use tower::ServiceExt;

        let (service, _invites) = service_with_invite_mode(pool.clone(), 15, "Router Pilot");
        let app = crate::build_router(
            crate::AppState::new(Arc::new(iskworks_storage::PgWorkspaceRepository::new(pool)))
                .with_auth(Some(service)),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/eve/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"inviteCode":"ISK-NOPE-NOPE-NOPE-NOPE"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "invite_invalid");
    }
}
