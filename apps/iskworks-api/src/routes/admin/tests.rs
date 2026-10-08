use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use iskworks_core::{
    AdminConfig, AuthError, AuthenticatedUser, InviteAdminRepository, InviteId, InviteSummary,
    NewInvite, UserId, WorkspaceId, WorkspaceRepository, WorkspaceState,
};
use tower::ServiceExt;

use crate::{admin_router, AppState, CURRENT_USER};
use iskworks_core::{AppError, NewWorkspace};

struct NoWorkspaces;

#[async_trait]
impl WorkspaceRepository for NoWorkspaces {
    async fn get_workspace_state(&self) -> Result<WorkspaceState, AppError> {
        Err(AppError::Persistence("unused".to_string()))
    }
    async fn get_workspace_state_by_id(
        &self,
        _workspace_id: WorkspaceId,
    ) -> Result<WorkspaceState, AppError> {
        Err(AppError::Persistence("unused".to_string()))
    }
    async fn create_workspace(
        &self,
        _new_workspace: NewWorkspace,
    ) -> Result<WorkspaceState, AppError> {
        Err(AppError::Persistence("unused".to_string()))
    }
}

#[derive(Default)]
struct MemoryInvites {
    rows: Mutex<Vec<(InviteSummary, String, Option<String>)>>,
}

#[async_trait]
impl InviteAdminRepository for MemoryInvites {
    async fn create_invite(&self, new_invite: NewInvite) -> Result<InviteId, AuthError> {
        let id = InviteId::new();
        self.rows.lock().unwrap().push((
            InviteSummary {
                id,
                created_at: chrono::Utc::now(),
                expires_at: new_invite.expires_at,
                disabled_at: None,
                max_uses: new_invite.max_uses,
                use_count: 0,
                note: new_invite.note,
                revealable: new_invite.code_ciphertext.is_some(),
            },
            new_invite.code_hash,
            new_invite.code_ciphertext,
        ));
        Ok(id)
    }
    async fn list_invites(&self) -> Result<Vec<InviteSummary>, AuthError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .map(|(summary, _, _)| summary.clone())
            .collect())
    }
    async fn disable_invite(&self, id: InviteId) -> Result<bool, AuthError> {
        let mut rows = self.rows.lock().unwrap();
        match rows.iter_mut().find(|(summary, _, _)| summary.id == id) {
            Some((summary, _, _)) => {
                summary.disabled_at.get_or_insert_with(chrono::Utc::now);
                Ok(true)
            }
            None => Ok(false),
        }
    }
    async fn delete_invite(&self, id: InviteId) -> Result<bool, AuthError> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|(summary, _, _)| summary.id != id);
        Ok(rows.len() < before)
    }
    async fn invite_code_ciphertext(&self, id: InviteId) -> Result<Option<String>, AuthError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|(summary, _, _)| summary.id == id)
            .and_then(|(_, _, ciphertext)| ciphertext.clone()))
    }
}

const ADMIN_CHARACTER: i64 = 91_000_001;

fn user(character_id: i64) -> AuthenticatedUser {
    AuthenticatedUser {
        user_id: UserId::new(),
        workspace_id: WorkspaceId::new(),
        eve_character_id: character_id,
        eve_character_name: "Pilot".to_string(),
    }
}

const SECOND_ADMIN_CHARACTER: i64 = 91_000_002;

fn summary(id: UserId, character_id: i64, name: &str) -> iskworks_core::AdminUserSummary {
    iskworks_core::AdminUserSummary {
        user_id: id,
        eve_character_id: character_id,
        eve_character_name: name.to_string(),
        created_at: chrono::Utc::now(),
        last_login_at: chrono::Utc::now(),
        character_count: 3,
        characters_needing_attention: 1,
        active_session_count: 2,
        disabled_at: None,
    }
}

/// Seeded with a regular user and the two configured admins.
struct MemoryUsers {
    regular: UserId,
    first_admin: UserId,
    second_admin: UserId,
    rows: Mutex<Vec<iskworks_core::AdminUserSummary>>,
    orphans: Mutex<i64>,
}

impl Default for MemoryUsers {
    fn default() -> Self {
        let (regular, first_admin, second_admin) = (UserId::new(), UserId::new(), UserId::new());
        Self {
            regular,
            first_admin,
            second_admin,
            rows: Mutex::new(vec![
                summary(regular, 90_000_042, "Listed Pilot"),
                summary(first_admin, ADMIN_CHARACTER, "First Admin"),
                summary(second_admin, SECOND_ADMIN_CHARACTER, "Second Admin"),
            ]),
            orphans: Mutex::new(2),
        }
    }
}

#[async_trait]
impl iskworks_core::AdminUsersRepository for MemoryUsers {
    async fn list_users(&self) -> Result<Vec<iskworks_core::AdminUserSummary>, AuthError> {
        Ok(self.rows.lock().unwrap().clone())
    }
    async fn find_user(
        &self,
        id: UserId,
    ) -> Result<Option<iskworks_core::AdminUserSummary>, AuthError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|user| user.user_id == id)
            .cloned())
    }
    async fn set_user_disabled(&self, id: UserId, disabled: bool) -> Result<bool, AuthError> {
        let mut rows = self.rows.lock().unwrap();
        match rows.iter_mut().find(|user| user.user_id == id) {
            Some(user) => {
                user.disabled_at = disabled.then(chrono::Utc::now);
                Ok(true)
            }
            None => Ok(false),
        }
    }
    async fn delete_user(&self, id: UserId) -> Result<bool, AuthError> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|user| user.user_id != id);
        Ok(rows.len() < before)
    }
    async fn count_orphaned_workspaces(&self) -> Result<i64, AuthError> {
        Ok(*self.orphans.lock().unwrap())
    }
    async fn erase_orphaned_workspaces(&self) -> Result<i64, AuthError> {
        Ok(std::mem::take(&mut *self.orphans.lock().unwrap()))
    }
}

fn base_state(invites: Arc<MemoryInvites>) -> AppState {
    AppState::new(Arc::new(NoWorkspaces))
        .with_admin_config(AdminConfig::new([ADMIN_CHARACTER, SECOND_ADMIN_CHARACTER]))
        .with_invite_admin_repository(invites)
        .with_admin_users_repository(Arc::new(MemoryUsers::default()))
}

fn users_app(users: Arc<MemoryUsers>) -> axum::Router {
    let state = base_state(Arc::default()).with_admin_users_repository(users);
    admin_router(state.clone()).with_state(state)
}

/// The signed-in caller: the first configured admin's own account.
fn caller(users: &MemoryUsers) -> Option<AuthenticatedUser> {
    Some(AuthenticatedUser {
        user_id: users.first_admin,
        workspace_id: WorkspaceId::new(),
        eve_character_id: ADMIN_CHARACTER,
        eve_character_name: "First Admin".to_string(),
    })
}

fn app(invites: Arc<MemoryInvites>) -> axum::Router {
    let cipher = iskworks_esi::SecretCipher::for_tests();
    let state = base_state(invites).with_invite_cipher(cipher);
    admin_router(state.clone()).with_state(state)
}

fn app_without_cipher(invites: Arc<MemoryInvites>) -> axum::Router {
    let state = base_state(invites);
    admin_router(state.clone()).with_state(state)
}

async fn call(
    app: axum::Router,
    as_user: Option<AuthenticatedUser>,
    method: Method,
    uri: &str,
    body: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.unwrap_or("").to_string()))
        .unwrap();
    let response = CURRENT_USER
        .scope(as_user, async { app.oneshot(request).await.unwrap() })
        .await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn anonymous_and_non_admin_callers_get_403() {
    for caller in [None, Some(user(1))] {
        let (status, body) = call(
            app(Arc::default()),
            caller,
            Method::GET,
            "/api/admin/invites",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"]["code"], "forbidden");
    }
}

#[tokio::test]
async fn create_returns_the_code_once_and_list_never_does() {
    let invites = Arc::new(MemoryInvites::default());
    let admin = Some(user(ADMIN_CHARACTER));

    let (status, created) = call(
        app(invites.clone()),
        admin.clone(),
        Method::POST,
        "/api/admin/invites",
        Some(r#"{"maxUses":3,"note":"  for Bob  "}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let code = created["code"].as_str().unwrap().to_string();
    assert!(code.starts_with("ISK-"));
    assert_eq!(created["invite"]["maxUses"], 3);
    assert_eq!(created["invite"]["note"], "for Bob");
    assert_eq!(created["invite"]["status"], "active");

    // Only the hash is stored.
    let stored_hash = invites.rows.lock().unwrap()[0].1.clone();
    assert_eq!(stored_hash, iskworks_core::hash_invite_code_from_raw(&code));

    let (status, listed) = call(app(invites), admin, Method::GET, "/api/admin/invites", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert!(!listed.to_string().contains(&code));
    assert!(!listed.to_string().contains(&stored_hash));
}

#[tokio::test]
async fn create_validates_input() {
    for body in [
        r#"{"maxUses":0}"#,
        r#"{"maxUses":1001}"#,
        r#"{"expiresAt":"2001-01-01T00:00:00Z"}"#,
    ] {
        let (status, response) = call(
            app(Arc::default()),
            Some(user(ADMIN_CHARACTER)),
            Method::POST,
            "/api/admin/invites",
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(response["error"]["code"], "validation_failed");
    }
}

#[tokio::test]
async fn disable_is_idempotent_and_404s_for_unknown_ids() {
    let invites = Arc::new(MemoryInvites::default());
    let admin = Some(user(ADMIN_CHARACTER));
    let (_, created) = call(
        app(invites.clone()),
        admin.clone(),
        Method::POST,
        "/api/admin/invites",
        Some("{}"),
    )
    .await;
    let id = created["invite"]["id"].as_str().unwrap().to_string();
    let uri = format!("/api/admin/invites/{id}/disable");

    for _ in 0..2 {
        let (status, _) = call(
            app(invites.clone()),
            admin.clone(),
            Method::POST,
            &uri,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    let (_, listed) = call(
        app(invites.clone()),
        admin.clone(),
        Method::GET,
        "/api/admin/invites",
        None,
    )
    .await;
    assert_eq!(listed[0]["status"], "disabled");

    let unknown = format!("/api/admin/invites/{}/disable", uuid::Uuid::new_v4());
    let (status, _) = call(app(invites), admin, Method::POST, &unknown, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_admin_can_reveal_the_code_again_and_it_is_not_stored_in_plaintext() {
    let invites = Arc::new(MemoryInvites::default());
    let admin = Some(user(ADMIN_CHARACTER));
    let (_, created) = call(
        app(invites.clone()),
        admin.clone(),
        Method::POST,
        "/api/admin/invites",
        Some("{}"),
    )
    .await;
    let code = created["code"].as_str().unwrap().to_string();
    let id = created["invite"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["invite"]["revealable"], true);

    let stored = invites.rows.lock().unwrap()[0].2.clone().unwrap();
    assert!(!stored.contains(&code));

    let uri = format!("/api/admin/invites/{id}/code");
    let (status, revealed) = call(app(invites.clone()), admin, Method::GET, &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(revealed["code"], code);

    let (status, _) = call(app(invites), Some(user(1)), Method::GET, &uri, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn invites_without_a_stored_copy_cannot_be_revealed() {
    let invites = Arc::new(MemoryInvites::default());
    let admin = Some(user(ADMIN_CHARACTER));
    let (_, created) = call(
        app_without_cipher(invites.clone()),
        admin.clone(),
        Method::POST,
        "/api/admin/invites",
        Some("{}"),
    )
    .await;
    assert_eq!(created["invite"]["revealable"], false);
    let id = created["invite"]["id"].as_str().unwrap();
    let (status, _) = call(
        app_without_cipher(invites),
        admin,
        Method::GET,
        &format!("/api/admin/invites/{id}/code"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admins_list_users_with_flags_and_everyone_else_is_refused() {
    let users = Arc::new(MemoryUsers::default());
    let (status, body) = call(
        users_app(users.clone()),
        caller(&users),
        Method::GET,
        "/api/admin/users",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let row = |name: &str| {
        body.as_array()
            .unwrap()
            .iter()
            .find(|row| row["characterName"] == name)
            .unwrap()
            .clone()
    };
    let regular = row("Listed Pilot");
    assert_eq!(regular["characterCount"], 3);
    assert_eq!(regular["charactersNeedingAttention"], 1);
    assert_eq!(regular["activeSessions"], 2);
    assert_eq!(regular["isAdmin"], false);
    assert_eq!(regular["isCurrentUser"], false);
    assert!(regular["disabledAt"].is_null());
    assert_eq!(row("First Admin")["isAdmin"], true);
    assert_eq!(row("First Admin")["isCurrentUser"], true);
    assert_eq!(row("Second Admin")["isCurrentUser"], false);

    for who in [None, Some(user(1))] {
        let (status, _) = call(
            users_app(users.clone()),
            who,
            Method::GET,
            "/api/admin/users",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn disable_and_enable_toggle_the_user() {
    let users = Arc::new(MemoryUsers::default());
    let id = users.regular.0;
    let disabled_at = |users: &MemoryUsers| {
        users
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|user| user.user_id.0 == id)
            .unwrap()
            .disabled_at
    };

    let (status, _) = call(
        users_app(users.clone()),
        caller(&users),
        Method::POST,
        &format!("/api/admin/users/{id}/disable"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(disabled_at(&users).is_some());

    let (status, _) = call(
        users_app(users.clone()),
        caller(&users),
        Method::POST,
        &format!("/api/admin/users/{id}/enable"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(disabled_at(&users).is_none());

    let (status, _) = call(
        users_app(users.clone()),
        Some(user(1)),
        Method::POST,
        &format!("/api/admin/users/{id}/disable"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(disabled_at(&users).is_none());

    let (status, _) = call(
        users_app(users.clone()),
        caller(&users),
        Method::POST,
        &format!("/api/admin/users/{}/disable", uuid::Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn you_and_other_admins_cannot_be_disabled_or_deleted() {
    let users = Arc::new(MemoryUsers::default());
    for target in [users.first_admin, users.second_admin] {
        for (method, suffix, body) in [
            (Method::POST, "/disable", None),
            (
                Method::DELETE,
                "",
                Some(r#"{"confirmCharacterName":"First Admin"}"#),
            ),
        ] {
            let (status, response) = call(
                users_app(users.clone()),
                caller(&users),
                method,
                &format!("/api/admin/users/{}{suffix}", target.0),
                body,
            )
            .await;
            assert_eq!(status, StatusCode::CONFLICT);
            assert_eq!(response["error"]["code"], "conflict");
        }
    }
    let rows = users.rows.lock().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|user| user.disabled_at.is_none()));
}

#[tokio::test]
async fn delete_requires_the_character_name_then_removes_the_user() {
    let users = Arc::new(MemoryUsers::default());
    let uri = format!("/api/admin/users/{}", users.regular.0);

    for body in [
        r#"{"confirmCharacterName":"Wrong Name"}"#,
        r#"{"confirmCharacterName":""}"#,
    ] {
        let (status, response) = call(
            users_app(users.clone()),
            caller(&users),
            Method::DELETE,
            &uri,
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(response["error"]["code"], "validation_failed");
    }
    assert_eq!(users.rows.lock().unwrap().len(), 3);

    let (status, _) = call(
        users_app(users.clone()),
        Some(user(1)),
        Method::DELETE,
        &uri,
        Some(r#"{"confirmCharacterName":"Listed Pilot"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = call(
        users_app(users.clone()),
        caller(&users),
        Method::DELETE,
        &uri,
        Some(r#"{"confirmCharacterName":"  listed pilot "}"#),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(users.rows.lock().unwrap().len(), 2);

    let (status, _) = call(
        users_app(users.clone()),
        caller(&users),
        Method::DELETE,
        &uri,
        Some(r#"{"confirmCharacterName":"Listed Pilot"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
#[tokio::test]
async fn delete_removes_the_invite_and_404s_when_unknown_or_repeated() {
    let invites = Arc::new(MemoryInvites::default());
    let admin = Some(user(ADMIN_CHARACTER));
    let (_, created) = call(
        app(invites.clone()),
        admin.clone(),
        Method::POST,
        "/api/admin/invites",
        Some("{}"),
    )
    .await;
    let uri = format!(
        "/api/admin/invites/{}",
        created["invite"]["id"].as_str().unwrap()
    );

    let (status, _) = call(
        app(invites.clone()),
        Some(user(1)),
        Method::DELETE,
        &uri,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(invites.rows.lock().unwrap().len(), 1);

    let (status, _) = call(
        app(invites.clone()),
        admin.clone(),
        Method::DELETE,
        &uri,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(invites.rows.lock().unwrap().is_empty());

    let (status, _) = call(app(invites), admin, Method::DELETE, &uri, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn orphaned_workspaces_are_counted_and_erased_only_with_confirmation() {
    let users = Arc::new(MemoryUsers::default());

    let (status, body) = call(
        users_app(users.clone()),
        caller(&users),
        Method::GET,
        "/api/admin/orphaned-workspaces",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 2);

    for who in [None, Some(user(1))] {
        let (status, _) = call(
            users_app(users.clone()),
            who,
            Method::POST,
            "/api/admin/orphaned-workspaces/erase",
            Some(r#"{"confirmation":"erase"}"#),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    let (status, body) = call(
        users_app(users.clone()),
        caller(&users),
        Method::POST,
        "/api/admin/orphaned-workspaces/erase",
        Some(r#"{"confirmation":"nope"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(*users.orphans.lock().unwrap(), 2);

    let (status, body) = call(
        users_app(users.clone()),
        caller(&users),
        Method::POST,
        "/api/admin/orphaned-workspaces/erase",
        Some(r#"{"confirmation":" ERASE "}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["erased"], 2);
    assert_eq!(*users.orphans.lock().unwrap(), 0);
}
