//! App-wide administration — who counts as an admin and the operations the
//! admin section exposes. Admin identity is an EVE character id (matched
//! against `users.eve_character_id`), configured by the operator through the
//! environment; there is deliberately no database role to escalate.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::auth::{AuthError, AuthenticatedUser, UserId};
use crate::invite::InviteId;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AdminConfigError {
    #[error("invalid admin character id `{0}`: expected a positive integer")]
    InvalidCharacterId(String),
}

/// The set of EVE character ids allowed into the admin section. Empty means
/// nobody is an admin.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdminConfig {
    character_ids: HashSet<i64>,
}

impl AdminConfig {
    #[must_use]
    pub fn new(character_ids: impl IntoIterator<Item = i64>) -> Self {
        Self {
            character_ids: character_ids.into_iter().collect(),
        }
    }

    /// Parse a comma-separated list of character ids. Blank entries are
    /// skipped; anything else that is not a positive integer is an error so
    /// a typo can never silently turn into "no admins" or a wrong admin.
    pub fn parse(raw: &str) -> Result<Self, AdminConfigError> {
        let mut character_ids = HashSet::new();
        for part in raw
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            let id = part
                .parse::<i64>()
                .ok()
                .filter(|id| *id > 0)
                .ok_or_else(|| AdminConfigError::InvalidCharacterId(part.to_string()))?;
            character_ids.insert(id);
        }
        Ok(Self { character_ids })
    }

    #[must_use]
    pub fn is_admin(&self, user: &AuthenticatedUser) -> bool {
        self.is_admin_character(user.eve_character_id)
    }

    #[must_use]
    pub fn is_admin_character(&self, eve_character_id: i64) -> bool {
        self.character_ids.contains(&eve_character_id)
    }
}

/// One `invite_codes` row, minus the hash — everything an operator needs and
/// nothing sensitive. The raw code is never stored, so it is never listed.
#[derive(Debug, Clone)]
pub struct InviteSummary {
    pub id: InviteId,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub disabled_at: Option<DateTime<Utc>>,
    pub max_uses: i32,
    pub use_count: i32,
    pub note: Option<String>,
    /// Whether an encrypted copy of the code is stored, so an admin can
    /// reveal it again.
    pub revealable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteStatus {
    Active,
    Disabled,
    Expired,
    Exhausted,
}

impl InviteStatus {
    /// The label the admin API and CLI show for this status.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            InviteStatus::Active => "active",
            InviteStatus::Disabled => "disabled",
            InviteStatus::Expired => "expired",
            InviteStatus::Exhausted => "exhausted",
        }
    }
}

/// Largest `max_uses` a new invite may have.
pub const MAX_INVITE_USES: i32 = 1000;
/// Longest note (in characters, after trimming) a new invite may carry.
pub const MAX_INVITE_NOTE_CHARS: usize = 200;

/// Which field of a new invite is invalid, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InviteFieldError {
    pub field: &'static str,
    pub message: &'static str,
}

/// Checks a new invite's settings -- the same rules for the admin API and
/// the operator CLI -- and returns its note trimmed, with a blank note as
/// `None`.
pub fn validate_new_invite(
    max_uses: i32,
    expires_at: Option<DateTime<Utc>>,
    note: Option<String>,
    now: DateTime<Utc>,
) -> Result<Option<String>, InviteFieldError> {
    if !(1..=MAX_INVITE_USES).contains(&max_uses) {
        return Err(InviteFieldError {
            field: "maxUses",
            message: "Max uses must be between 1 and 1000.",
        });
    }
    if expires_at.is_some_and(|at| at <= now) {
        return Err(InviteFieldError {
            field: "expiresAt",
            message: "Expiry must be in the future.",
        });
    }
    let note = note
        .map(|note| note.trim().to_string())
        .filter(|note| !note.is_empty());
    if note
        .as_ref()
        .is_some_and(|note| note.chars().count() > MAX_INVITE_NOTE_CHARS)
    {
        return Err(InviteFieldError {
            field: "note",
            message: "Note must be 200 characters or fewer.",
        });
    }
    Ok(note)
}

impl InviteSummary {
    /// Mirrors the redeemability predicate in
    /// `PgInviteRepository::find_redeemable_invite_id`; precedence is
    /// disabled, then expired, then exhausted.
    #[must_use]
    pub fn status(&self, now: DateTime<Utc>) -> InviteStatus {
        if self.disabled_at.is_some() {
            InviteStatus::Disabled
        } else if self.expires_at.is_some_and(|at| at <= now) {
            InviteStatus::Expired
        } else if self.use_count >= self.max_uses {
            InviteStatus::Exhausted
        } else {
            InviteStatus::Active
        }
    }
}

/// Fields for creating an invite. `code_hash` is `hash_invite_code_from_raw`
/// of a freshly generated code — the plaintext is shown once and discarded.
#[derive(Debug, Clone)]
pub struct NewInvite {
    pub code_hash: String,
    pub max_uses: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
    /// Encrypted envelope of the plaintext code, when reveal is enabled.
    pub code_ciphertext: Option<String>,
}

#[async_trait]
pub trait InviteAdminRepository: Send + Sync {
    async fn create_invite(&self, new_invite: NewInvite) -> Result<InviteId, AuthError>;
    /// Newest first. Never includes `code_hash`.
    async fn list_invites(&self) -> Result<Vec<InviteSummary>, AuthError>;
    /// Idempotent. Returns `false` if no such invite exists.
    async fn disable_invite(&self, id: InviteId) -> Result<bool, AuthError>;
    /// Permanently remove an invite (any status). Returns `false` if no such
    /// invite exists. Users who already registered with it are unaffected.
    async fn delete_invite(&self, id: InviteId) -> Result<bool, AuthError>;
    /// The stored encrypted code, if this invite has one.
    async fn invite_code_ciphertext(&self, id: InviteId) -> Result<Option<String>, AuthError>;
}

#[async_trait]
impl<T> InviteAdminRepository for Arc<T>
where
    T: InviteAdminRepository + ?Sized,
{
    async fn create_invite(&self, new_invite: NewInvite) -> Result<InviteId, AuthError> {
        (**self).create_invite(new_invite).await
    }

    async fn list_invites(&self) -> Result<Vec<InviteSummary>, AuthError> {
        (**self).list_invites().await
    }

    async fn disable_invite(&self, id: InviteId) -> Result<bool, AuthError> {
        (**self).disable_invite(id).await
    }

    async fn delete_invite(&self, id: InviteId) -> Result<bool, AuthError> {
        (**self).delete_invite(id).await
    }

    async fn invite_code_ciphertext(&self, id: InviteId) -> Result<Option<String>, AuthError> {
        (**self).invite_code_ciphertext(id).await
    }
}

/// One registered user for the admin Users screen. Only what an operator
/// needs to see who is on the instance — no tokens, workspace contents, or
/// session identifiers.
#[derive(Debug, Clone)]
pub struct AdminUserSummary {
    pub user_id: UserId,
    pub eve_character_id: i64,
    pub eve_character_name: String,
    pub created_at: DateTime<Utc>,
    pub last_login_at: DateTime<Utc>,
    /// Characters currently linked to the user's workspace (not
    /// disconnected). The login character only counts if it was also linked.
    pub character_count: i64,
    /// Linked characters whose connection is not healthy (needs
    /// reconnection, missing scope, temporarily unavailable).
    pub characters_needing_attention: i64,
    pub active_session_count: i64,
    pub disabled_at: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait AdminUsersRepository: Send + Sync {
    /// Most recently active first.
    async fn list_users(&self) -> Result<Vec<AdminUserSummary>, AuthError>;
    async fn find_user(&self, id: UserId) -> Result<Option<AdminUserSummary>, AuthError>;
    /// Block (`true`) or restore (`false`) sign-in. Blocking also ends every
    /// session. Idempotent; returns `false` if no such user exists.
    async fn set_user_disabled(&self, id: UserId, disabled: bool) -> Result<bool, AuthError>;
    /// Erase the user and everything in their workspace (inventory, builds,
    /// orders, ESI data and tokens, market data...) in one all-or-nothing
    /// transaction. Irreversible. Returns `false` if no such user exists.
    async fn delete_user(&self, id: UserId) -> Result<bool, AuthError>;
    /// Workspaces left behind by users deleted before full erase existed:
    /// no user, and already claimed (the unclaimed legacy workspace is never
    /// counted).
    async fn count_orphaned_workspaces(&self) -> Result<i64, AuthError>;
    /// Erase every orphaned workspace, each in its own transaction. Returns
    /// how many were erased.
    async fn erase_orphaned_workspaces(&self) -> Result<i64, AuthError>;
}

#[async_trait]
impl<T> AdminUsersRepository for Arc<T>
where
    T: AdminUsersRepository + ?Sized,
{
    async fn list_users(&self) -> Result<Vec<AdminUserSummary>, AuthError> {
        (**self).list_users().await
    }

    async fn find_user(&self, id: UserId) -> Result<Option<AdminUserSummary>, AuthError> {
        (**self).find_user(id).await
    }

    async fn set_user_disabled(&self, id: UserId, disabled: bool) -> Result<bool, AuthError> {
        (**self).set_user_disabled(id, disabled).await
    }

    async fn delete_user(&self, id: UserId) -> Result<bool, AuthError> {
        (**self).delete_user(id).await
    }

    async fn count_orphaned_workspaces(&self) -> Result<i64, AuthError> {
        (**self).count_orphaned_workspaces().await
    }

    async fn erase_orphaned_workspaces(&self) -> Result<i64, AuthError> {
        (**self).erase_orphaned_workspaces().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::UserId;
    use crate::WorkspaceId;

    fn user(character_id: i64) -> AuthenticatedUser {
        AuthenticatedUser {
            user_id: UserId::new(),
            workspace_id: WorkspaceId::new(),
            eve_character_id: character_id,
            eve_character_name: "Pilot".to_string(),
        }
    }

    #[test]
    fn empty_and_blank_input_means_no_admins() {
        for raw in ["", "  ", " , ,"] {
            let config = AdminConfig::parse(raw).unwrap();
            assert_eq!(config, AdminConfig::default(), "{raw:?}");
            assert!(!config.is_admin(&user(1)));
        }
    }

    #[test]
    fn parses_trimmed_comma_separated_ids_and_dedupes() {
        let config = AdminConfig::parse(" 91000001, 91000002 ,91000001,").unwrap();
        assert!(config.is_admin(&user(91_000_001)));
        assert!(config.is_admin(&user(91_000_002)));
        assert!(!config.is_admin(&user(91_000_003)));
    }

    fn summary() -> InviteSummary {
        InviteSummary {
            id: InviteId::new(),
            created_at: Utc::now(),
            expires_at: None,
            disabled_at: None,
            max_uses: 2,
            use_count: 0,
            note: None,
            revealable: false,
        }
    }

    #[test]
    fn invite_status_precedence() {
        let now = Utc::now();
        let mut invite = summary();
        assert_eq!(invite.status(now), InviteStatus::Active);
        invite.use_count = 2;
        assert_eq!(invite.status(now), InviteStatus::Exhausted);
        invite.expires_at = Some(now - chrono::Duration::seconds(1));
        assert_eq!(invite.status(now), InviteStatus::Expired);
        invite.disabled_at = Some(now);
        assert_eq!(invite.status(now), InviteStatus::Disabled);
    }

    #[test]
    fn malformed_ids_are_rejected() {
        for raw in ["abc", "1,two", "-5", "0", "1.5"] {
            assert!(AdminConfig::parse(raw).is_err(), "{raw:?}");
        }
    }

    #[test]
    fn new_invite_rules_accept_the_api_bounds_and_normalize_the_note() {
        let now = Utc::now();
        assert_eq!(validate_new_invite(1, None, None, now), Ok(None));
        assert_eq!(
            validate_new_invite(
                MAX_INVITE_USES,
                Some(now + chrono::Duration::hours(1)),
                Some("  hi  ".to_string()),
                now
            ),
            Ok(Some("hi".to_string()))
        );
        assert_eq!(
            validate_new_invite(1, None, Some("   ".to_string()), now),
            Ok(None)
        );
        assert_eq!(
            validate_new_invite(1, None, Some("x".repeat(MAX_INVITE_NOTE_CHARS)), now),
            Ok(Some("x".repeat(MAX_INVITE_NOTE_CHARS)))
        );
    }

    #[test]
    fn new_invite_rules_reject_like_the_api() {
        let now = Utc::now();
        for max_uses in [0, -1, MAX_INVITE_USES + 1] {
            assert_eq!(
                validate_new_invite(max_uses, None, None, now),
                Err(InviteFieldError {
                    field: "maxUses",
                    message: "Max uses must be between 1 and 1000."
                })
            );
        }
        assert_eq!(
            validate_new_invite(1, Some(now), None, now),
            Err(InviteFieldError {
                field: "expiresAt",
                message: "Expiry must be in the future."
            })
        );
        assert_eq!(
            validate_new_invite(1, None, Some("x".repeat(MAX_INVITE_NOTE_CHARS + 1)), now),
            Err(InviteFieldError {
                field: "note",
                message: "Note must be 200 characters or fewer."
            })
        );
    }

    #[test]
    fn invite_status_labels_match_the_admin_api() {
        assert_eq!(InviteStatus::Active.as_str(), "active");
        assert_eq!(InviteStatus::Disabled.as_str(), "disabled");
        assert_eq!(InviteStatus::Expired.as_str(), "expired");
        assert_eq!(InviteStatus::Exhausted.as_str(), "exhausted");
    }
}
