use super::*;

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]
async fn wallet_client_names_stop_being_unresolved_after_caching(pool: PgPool) {
    let (workspace_id, owner_id) = fixture_workspace(&pool).await;
    let connection = fixture_connection(&pool, workspace_id, owner_id, "Wallet Pilot").await;
    let repository = PgEsiRepository::new(pool);
    let run = repository
        .start_sync(&connection, EsiSyncKind::WalletTransactions)
        .await
        .unwrap();
    repository
        .complete_wallet(
            &run,
            &[WalletTransactionObservation {
                transaction_id: 88_000_001,
                client_id: 2_119_000_222,
                location_id: 60_003_760,
                type_id: 34,
                quantity: 10,
                unit_price: Decimal::new(425, 2),
                is_buy: true,
                is_personal: true,
                journal_ref_id: 77,
                transacted_at: crate::db_now(),
                raw: serde_json::json!({"fixture": false}),
            }],
            None,
        )
        .await
        .unwrap();

    assert_eq!(
        repository
            .unresolved_wallet_client_ids(workspace_id)
            .await
            .unwrap(),
        vec![2_119_000_222]
    );
    // An id ESI couldn't name is skipped for a week, then asked about again.
    repository
        .record_entity_name_misses(&[2_119_000_222])
        .await
        .unwrap();
    assert!(repository
        .unresolved_wallet_client_ids(workspace_id)
        .await
        .unwrap()
        .is_empty());
    sqlx::query("UPDATE eve_entity_name_misses SET checked_at = now() - interval '8 days'")
        .execute(&repository.pool)
        .await
        .unwrap();
    assert_eq!(
        repository
            .unresolved_wallet_client_ids(workspace_id)
            .await
            .unwrap(),
        vec![2_119_000_222]
    );
    repository
        .cache_entity_names(&[EveEntityName {
            id: 2_119_000_222,
            name: "Caldari Navy".into(),
            category: "corporation".into(),
        }])
        .await
        .unwrap();
    assert!(repository
        .unresolved_wallet_client_ids(workspace_id)
        .await
        .unwrap()
        .is_empty());
}
