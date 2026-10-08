use super::*;
use iskworks_core::{AdjustedPriceRepository, IndustryRepository};
use iskworks_esi::{BlueprintAssetObservation, SecretCipher};
use serde_json::json;
use sqlx::PgPool;

use crate::industry::PgIndustryRepository;

mod asset_snapshots;
mod blueprints;
mod character_sources;
mod planetary_preferences;
mod public_refresh;
mod wallet_names;

// ---------------------------------------------------------------------------
// Workspace and connection fixtures
// ---------------------------------------------------------------------------

pub(super) async fn fixture_connection(
    pool: &PgPool,
    workspace_id: WorkspaceId,
    owner_id: OwnerId,
    character_name: &str,
) -> ConnectedCharacter {
    let id = ConnectedCharacterId::new();
    let now = crate::db_now();
    sqlx::query(
        r#"INSERT INTO eve_connections (
                 id, workspace_id, owner_id, eve_character_id, character_name, status,
                 granted_scopes, connected_at, updated_at, revision
               ) VALUES ($1,$2,$3,$4,$5,'connected',ARRAY[]::text[],$6,$6,1)"#,
    )
    .bind(id.0)
    .bind(workspace_id.0)
    .bind(owner_id.0)
    .bind(rand_character_id())
    .bind(character_name)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    ConnectedCharacter {
        id,
        workspace_id,
        owner_id,
        eve_character_id: 0,
        character_name: character_name.to_string(),
        status: ConnectionStatus::Connected,
        granted_scopes: Vec::new(),
        access_token_expires_at: None,
        last_refreshed_at: None,
        last_error_code: None,
        last_error_message: None,
        connected_at: now,
        updated_at: now,
        disconnected_at: None,
        revision: 1,
    }
}

pub(super) async fn fixture_workspace(pool: &PgPool) -> (WorkspaceId, OwnerId) {
    let workspace_id = WorkspaceId(Uuid::new_v4());
    let owner_id = OwnerId(Uuid::new_v4());
    let now = crate::db_now();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
            "INSERT INTO workspaces (id, display_name, owner_id, created_at, updated_at) VALUES ($1, 'Blueprint Sync Test', $2, $3, $3)",
        )
        .bind(workspace_id.0)
        .bind(owner_id.0)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
            "INSERT INTO owners (id, workspace_id, owner_kind, display_name, hidden, created_at, updated_at) VALUES ($1, $2, 'manual', 'Blueprint Sync Test', false, $3, $3)",
        )
        .bind(owner_id.0)
        .bind(workspace_id.0)
        .bind(now)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (workspace_id, owner_id)
}

// ---------------------------------------------------------------------------
// Character ids
// ---------------------------------------------------------------------------

fn rand_character_id() -> i64 {
    (Uuid::new_v4().as_u128() % 1_000_000_000) as i64 + 1
}
