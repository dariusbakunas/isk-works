use super::*;

#[test]
fn normalization_orders_parents_before_lower_id_children_across_insert_batches() {
    let records = vec![
        RawMarketGroup {
            key: 1,
            name: LocalizedString {
                en: Some("Child".to_string()),
            },
            parent_group_id: Some(2_000),
        },
        RawMarketGroup {
            key: 2_000,
            name: LocalizedString {
                en: Some("Parent".to_string()),
            },
            parent_group_id: None,
        },
    ];

    let normalized = normalize_market_groups(records).unwrap();

    assert_eq!(
        normalized
            .iter()
            .map(|group| group.market_group_id)
            .collect::<Vec<_>>(),
        vec![2_000, 1]
    );
}
