//! `AppState`'s admin dependencies: the [`AdminDeps`] group, its
//! `with_*` builders and accessors.

use super::AppState;
use crate::error::ApiError;
use crate::{AuthApplicationError, AuthService};
use std::sync::Arc;

/// Login and administration: EVE SSO auth, admin rules, invites, users and the maintenance notice.
#[derive(Clone, Default)]
pub(crate) struct AdminDeps {
    pub(crate) auth_service: Option<Arc<AuthService>>,
    pub(crate) admin_config: iskworks_core::AdminConfig,
    pub(super) invite_admin_repository: Option<Arc<dyn iskworks_core::InviteAdminRepository>>,
    pub(super) admin_users_repository: Option<Arc<dyn iskworks_core::AdminUsersRepository>>,
    pub(super) maintenance_notice_repository:
        Option<Arc<dyn iskworks_core::MaintenanceNoticeRepository>>,
    pub(crate) invite_cipher: Option<iskworks_esi::SecretCipher>,
}

impl AppState {
    #[must_use]
    pub fn with_auth(mut self, service: Option<AuthService>) -> Self {
        self.admin.auth_service = service.map(Arc::new);
        self
    }

    #[must_use]
    pub fn with_admin_config(mut self, config: iskworks_core::AdminConfig) -> Self {
        self.admin.admin_config = config;
        self
    }

    #[must_use]
    pub fn with_invite_admin_repository(
        mut self,
        repository: Arc<dyn iskworks_core::InviteAdminRepository>,
    ) -> Self {
        self.admin.invite_admin_repository = Some(repository);
        self
    }

    /// Enables storing and revealing invite codes (encrypted at rest).
    /// Without it, newly created invites cannot be revealed later.
    #[must_use]
    pub fn with_invite_cipher(mut self, cipher: iskworks_esi::SecretCipher) -> Self {
        self.admin.invite_cipher = Some(cipher);
        self
    }

    #[must_use]
    pub fn with_admin_users_repository(
        mut self,
        repository: Arc<dyn iskworks_core::AdminUsersRepository>,
    ) -> Self {
        self.admin.admin_users_repository = Some(repository);
        self
    }

    #[must_use]
    pub fn with_maintenance_notice_repository(
        mut self,
        repository: Arc<dyn iskworks_core::MaintenanceNoticeRepository>,
    ) -> Self {
        self.admin.maintenance_notice_repository = Some(repository);
        self
    }

    /// `None` when not wired in, which the public endpoint treats as no notice.
    pub(crate) fn maintenance_notice_repository(
        &self,
    ) -> Option<Arc<dyn iskworks_core::MaintenanceNoticeRepository>> {
        self.admin.maintenance_notice_repository.clone()
    }

    pub(crate) fn admin_users_repository(
        &self,
    ) -> Result<Arc<dyn iskworks_core::AdminUsersRepository>, ApiError> {
        self.admin.admin_users_repository.clone().ok_or_else(|| {
            ApiError::Auth(AuthApplicationError::Configuration(
                "User administration is not available.".to_string(),
            ))
        })
    }

    pub(crate) fn invite_admin_repository(
        &self,
    ) -> Result<Arc<dyn iskworks_core::InviteAdminRepository>, ApiError> {
        self.admin.invite_admin_repository.clone().ok_or_else(|| {
            ApiError::Auth(AuthApplicationError::Configuration(
                "Invite administration is not available.".to_string(),
            ))
        })
    }

    pub(crate) fn auth_service(&self) -> Result<Arc<AuthService>, ApiError> {
        self.admin.auth_service.clone().ok_or_else(|| {
            ApiError::Auth(AuthApplicationError::Configuration(
                "EVE SSO login is not configured. Set EVE_SSO_CLIENT_ID.".to_string(),
            ))
        })
    }
}
