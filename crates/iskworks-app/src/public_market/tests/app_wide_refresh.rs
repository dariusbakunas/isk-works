use super::*;

// --- App-wide public market refresh (`public_market_coverage`) ---

const DOMAIN_REGION_ID: i64 = 10_000_043;

fn public_work(region_id: i64, type_id: i64) -> iskworks_core::PublicMarketCoverageWork {
    iskworks_core::PublicMarketCoverageWork {
        region_id,
        item: coverage(MarketCoverageRegistration {
            type_id,
            type_name: "Tritanium".to_string(),
        }),
    }
}

#[tokio::test]
async fn refresh_public_fetches_each_region_once_and_keeps_the_whole_regional_book() {
    let repository = Arc::new(RecordingRepository::default());
    let mut forge = response(1, 101, JITA_LOCATION_ID);
    forge
        .records
        .push(response(1, 103, 60_003_761).records.remove(0));
    let domain = response(1, 201, 60_008_494);
    let transport = PageTransport::new(vec![Ok(forge), Ok(domain)]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    service
        .refresh_public(
            vec![
                public_work(JITA_REGION_ID, 34),
                public_work(DOMAIN_REGION_ID, 34),
            ],
            no_cancel(),
        )
        .await;

    assert_eq!(
        transport.calls.load(Ordering::SeqCst),
        2,
        "one call per (region, type)"
    );
    let mut regions = transport.requested_regions.lock().unwrap().clone();
    regions.sort_unstable();
    assert_eq!(regions, vec![JITA_REGION_ID, DOMAIN_REGION_ID]);
    let mut completed = repository.public_completed.lock().unwrap().clone();
    completed.sort_by_key(|batch| batch.region_id);
    assert_eq!(completed.len(), 2);
    let forge_book = &completed[0];
    assert_eq!(forge_book.region_id, JITA_REGION_ID);
    assert_eq!(
        forge_book
            .orders
            .iter()
            .map(|order| order.location_id)
            .collect::<Vec<_>>(),
        vec![JITA_LOCATION_ID, 60_003_761],
        "the whole regional book is stored; stations filter on read"
    );
    assert!(
        repository.completed.lock().unwrap().is_empty(),
        "nothing is written per workspace"
    );
    assert!(repository.public_failures.lock().unwrap().is_empty());
}

#[tokio::test]
async fn refresh_public_revalidates_an_unchanged_book() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Ok(not_modified_page(Some(1)))]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());
    let mut work = public_work(JITA_REGION_ID, 34);
    work.item.prior_etag = Some("etag-1".to_string());
    work.item.order_count = 3;

    service.refresh_public(vec![work], no_cancel()).await;

    assert_eq!(
        transport.requested_etags(),
        vec![Some("etag-1".to_string())]
    );
    assert_eq!(
        *repository.public_revalidations.lock().unwrap(),
        vec![(JITA_REGION_ID, 34)]
    );
    assert!(repository.public_completed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn refresh_public_records_a_failed_fetch() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(vec![Err(EsiError::PermanentFailure)]);
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    service
        .refresh_public(vec![public_work(JITA_REGION_ID, 34)], no_cancel())
        .await;

    let failures = repository.public_failures.lock().unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!((failures[0].0, failures[0].1), (JITA_REGION_ID, 34));
}

#[tokio::test]
async fn refresh_public_with_no_work_makes_no_esi_calls() {
    let repository = Arc::new(RecordingRepository::default());
    let transport = PageTransport::new(Vec::new());
    let service = PublicMarketService::new(repository.clone(), transport.clone());

    service.refresh_public(Vec::new(), no_cancel()).await;

    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
}
