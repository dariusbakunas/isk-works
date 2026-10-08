use super::*;

pub(super) fn integration_response(error: EsiApplicationError) -> (StatusCode, ErrorBody) {
    let (status, code, retryable) = match &error {
        EsiApplicationError::Protocol(iskworks_esi::EsiError::AuthorizationRequired) => {
            (StatusCode::UNAUTHORIZED, "authorization_required", false)
        }
        EsiApplicationError::Protocol(iskworks_esi::EsiError::MissingScope) => {
            (StatusCode::FORBIDDEN, "missing_scope", false)
        }
        EsiApplicationError::Protocol(iskworks_esi::EsiError::RateLimited { .. })
        | EsiApplicationError::Protocol(iskworks_esi::EsiError::EsiErrorLimit { .. }) => {
            (StatusCode::TOO_MANY_REQUESTS, "rate_limited", true)
        }
        EsiApplicationError::Protocol(iskworks_esi::EsiError::ServerDowntime { .. }) => {
            (StatusCode::SERVICE_UNAVAILABLE, "eve_downtime", true)
        }
        EsiApplicationError::Protocol(iskworks_esi::EsiError::TemporaryFailure) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "temporary_esi_failure",
            true,
        ),
        EsiApplicationError::Protocol(iskworks_esi::EsiError::InvalidResponse)
        | EsiApplicationError::Protocol(iskworks_esi::EsiError::InvalidIdentity) => {
            (StatusCode::BAD_GATEWAY, "invalid_esi_response", true)
        }
        EsiApplicationError::Persistence(InventoryError::RevisionConflict) => {
            (StatusCode::CONFLICT, "revision_conflict", false)
        }
        EsiApplicationError::Persistence(InventoryError::ItemNotFound) => {
            (StatusCode::NOT_FOUND, "integration_record_not_found", false)
        }
        EsiApplicationError::Persistence(InventoryError::Validation(_)) => {
            (StatusCode::BAD_REQUEST, "validation_failed", false)
        }
        EsiApplicationError::Configuration(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, "esi_not_configured", false)
        }
        EsiApplicationError::SyncTooSoon { .. } => {
            (StatusCode::TOO_MANY_REQUESTS, "sync_too_soon", true)
        }
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            "esi_integration_failure",
            true,
        ),
    };
    (
        status,
        ErrorBody {
            code,
            message: error.to_string(),
            fields: None,
            retryable,
            correlation_id: None,
        },
    )
}

pub(super) fn auth_response(error: AuthApplicationError) -> (StatusCode, ErrorBody) {
    let (status, code, retryable) = match &error {
        AuthApplicationError::Persistence(AuthError::InvalidLoginState) => {
            (StatusCode::BAD_REQUEST, "invalid_login_state", false)
        }
        AuthApplicationError::Protocol(_) => {
            (StatusCode::BAD_GATEWAY, "eve_sso_login_failed", true)
        }
        AuthApplicationError::Persistence(AuthError::Persistence(_)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_unavailable",
            true,
        ),
        AuthApplicationError::Configuration(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "auth_not_configured",
            false,
        ),
        // Invite-only onboarding. Two public states only:
        // `invite_required` (new identity, no invite) and
        // `invite_invalid` (anything wrong with the invite —
        // unknown, expired, disabled, or exhausted are
        // deliberately not distinguished, to avoid enumeration
        // and needless disclosure). Never leaks a hash, id, or
        // timestamp.
        AuthApplicationError::InviteRequired => (StatusCode::BAD_REQUEST, "invite_required", false),
        AuthApplicationError::InviteInvalid
        | AuthApplicationError::Persistence(AuthError::InviteRejected) => {
            (StatusCode::BAD_REQUEST, "invite_invalid", false)
        }
        AuthApplicationError::AccountDisabled => (StatusCode::FORBIDDEN, "account_disabled", false),
        AuthApplicationError::CharacterTransferred => {
            (StatusCode::FORBIDDEN, "character_transferred", false)
        }
    };
    (
        status,
        ErrorBody {
            code,
            message: error.to_string(),
            fields: None,
            retryable,
            correlation_id: None,
        },
    )
}
