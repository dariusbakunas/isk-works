use super::*;

pub(super) struct FixtureEsiTransport;

#[async_trait]
impl EsiTransport for FixtureEsiTransport {
    async fn exchange_code(
        &self,
        _code: &str,
        _verifier: &PkceVerifier,
    ) -> Result<AuthenticatedToken, EsiError> {
        Err(EsiError::Configuration(
            "fixture authorization bypasses OAuth callback",
        ))
    }

    async fn refresh(&self, _refresh_token: &str) -> Result<RefreshedToken, EsiError> {
        Ok(RefreshedToken {
            access_token: "fixture-access-token".to_string(),
            rotated_refresh_token: Some("fixture-rotated-refresh-token".to_string()),
            expires_at: Utc::now() + Duration::minutes(20),
            identity: fixture_identity(),
        })
    }

    async fn assets(
        &self,
        _access_token: &str,
        _character_id: i64,
        page: u32,
        etag: Option<&str>,
    ) -> Result<EsiResponse<AssetObservation>, EsiError> {
        if etag == Some("\"fixture-assets-v1\"") {
            return Ok(EsiResponse {
                records: Vec::new(),
                not_modified: true,
                metadata: EsiResponseMetadata {
                    etag: etag.map(str::to_string),
                    pages: Some(2),
                    error_limit_remain: Some(100),
                    error_limit_reset: Some(1),
                    ..Default::default()
                },
            });
        }
        let records = match page {
            1 => vec![
                fixture_asset(91_001, 34, 125_000, false),
                fixture_asset(91_002, 35, 48_000, false),
            ],
            2 => vec![
                fixture_asset(91_003, 587, 1, true),
                fixture_asset(91_004, 681, 1, true),
            ],
            _ => return Err(EsiError::InvalidResponse),
        };
        Ok(EsiResponse {
            records,
            not_modified: false,
            metadata: EsiResponseMetadata {
                etag: Some("\"fixture-assets-v1\"".to_string()),
                pages: Some(2),
                error_limit_remain: Some(100),
                error_limit_reset: Some(1),
                ..Default::default()
            },
        })
    }

    async fn blueprints(
        &self,
        _access_token: &str,
        _character_id: i64,
        page: u32,
    ) -> Result<EsiResponse<BlueprintAssetObservation>, EsiError> {
        if page != 1 {
            return Err(EsiError::InvalidResponse);
        }
        Ok(EsiResponse {
            records: vec![BlueprintAssetObservation {
                item_id: 92_001,
                type_id: 691,
                location_id: 60_003_760,
                location_flag: "Hangar".into(),
                material_efficiency: 10,
                time_efficiency: 20,
                runs: 7,
                quantity: -2,
                raw: json!({"fixture": true}),
            }],
            not_modified: false,
            metadata: EsiResponseMetadata {
                pages: Some(1),
                ..Default::default()
            },
        })
    }

    async fn wallet_transactions(
        &self,
        _access_token: &str,
        _character_id: i64,
        from_id: Option<i64>,
        etag: Option<&str>,
    ) -> Result<EsiResponse<WalletTransactionObservation>, EsiError> {
        if from_id.is_none() && etag == Some("\"fixture-wallet-v1\"") {
            return Ok(EsiResponse {
                records: Vec::new(),
                not_modified: true,
                metadata: EsiResponseMetadata {
                    etag: etag.map(str::to_string),
                    error_limit_remain: Some(100),
                    error_limit_reset: Some(1),
                    ..Default::default()
                },
            });
        }
        let records = if from_id.is_none() {
            vec![
                fixture_wallet(8_800_001, 34, 100_000, "4.2500", true),
                fixture_wallet(8_800_002, 35, 10_000, "12.5000", true),
                fixture_wallet(8_800_003, 36, 5_000, "72.0000", false),
            ]
        } else {
            Vec::new()
        };
        Ok(EsiResponse {
            records,
            not_modified: false,
            metadata: EsiResponseMetadata {
                etag: Some("\"fixture-wallet-v1\"".to_string()),
                error_limit_remain: Some(100),
                error_limit_reset: Some(1),
                ..Default::default()
            },
        })
    }

    async fn universe_names(
        &self,
        ids: &[i64],
    ) -> Result<Vec<iskworks_esi::EveEntityName>, EsiError> {
        Ok(ids
            .iter()
            .filter_map(|&id| match id {
                98_000_001 => Some(iskworks_esi::EveEntityName {
                    id,
                    name: "Perimeter Industrial Holdings".to_string(),
                    category: "corporation".to_string(),
                }),
                30_000_142 => Some(iskworks_esi::EveEntityName {
                    id,
                    name: "Jita".to_string(),
                    category: "solar_system".to_string(),
                }),
                _ => None,
            })
            .collect())
    }

    async fn structure(
        &self,
        _access_token: &str,
        structure_id: i64,
    ) -> Result<StructureInformation, EsiError> {
        if structure_id == 1_050_474_463_169 {
            Ok(StructureInformation {
                structure_id,
                name: "C-J6MT - GEZ - T2 Ship, Comps, Structures".to_string(),
                owner_id: 98_000_001,
                solar_system_id: 30_000_772,
                type_id: Some(35_832),
            })
        } else {
            Err(EsiError::AccessDenied)
        }
    }

    async fn industry_systems(
        &self,
    ) -> Result<EsiResponse<iskworks_esi::IndustrySystemCostIndex>, EsiError> {
        Ok(EsiResponse {
            records: vec![iskworks_esi::IndustrySystemCostIndex {
                solar_system_id: 30_000_772,
                manufacturing: "0.0979".parse().expect("fixture cost index"),
                reaction: "0.0450".parse().expect("fixture cost index"),
            }],
            not_modified: false,
            metadata: EsiResponseMetadata {
                expires: Some((Utc::now() + Duration::hours(1)).to_rfc2822()),
                ..Default::default()
            },
        })
    }

    async fn market_prices(&self) -> Result<EsiResponse<iskworks_esi::AdjustedPrice>, EsiError> {
        Ok(EsiResponse {
            records: vec![
                iskworks_esi::AdjustedPrice {
                    type_id: 34,
                    adjusted_price: "3.689123456".parse().expect("fixture adjusted price"),
                },
                iskworks_esi::AdjustedPrice {
                    type_id: 35,
                    adjusted_price: "11.25".parse().expect("fixture adjusted price"),
                },
                iskworks_esi::AdjustedPrice {
                    type_id: 36,
                    adjusted_price: "68.5".parse().expect("fixture adjusted price"),
                },
                iskworks_esi::AdjustedPrice {
                    type_id: 37,
                    adjusted_price: "125.75".parse().expect("fixture adjusted price"),
                },
            ],
            not_modified: false,
            metadata: EsiResponseMetadata {
                expires: Some((Utc::now() + Duration::hours(1)).to_rfc2822()),
                ..Default::default()
            },
        })
    }

    async fn character_public_info(
        &self,
        character_id: i64,
    ) -> Result<EsiResponse<CharacterPublicInfo>, EsiError> {
        Ok(EsiResponse {
            records: vec![CharacterPublicInfo {
                character_id,
                name: "Fixture Industrialist".to_string(),
                corporation_id: 98_000_001,
                security_status: Some("5.00000".parse().expect("fixture security status")),
            }],
            not_modified: false,
            metadata: EsiResponseMetadata {
                expires: Some((Utc::now() + Duration::hours(6)).to_rfc2822()),
                ..Default::default()
            },
        })
    }

    async fn character_location(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<EsiResponse<CharacterLocationObservation>, EsiError> {
        Ok(EsiResponse {
            records: vec![CharacterLocationObservation {
                solar_system_id: 30_000_142,
                station_id: Some(60_003_760),
                structure_id: None,
            }],
            not_modified: false,
            metadata: EsiResponseMetadata {
                expires: Some((Utc::now() + Duration::minutes(5)).to_rfc2822()),
                ..Default::default()
            },
        })
    }

    async fn character_skills(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<EsiResponse<CharacterSkillsObservation>, EsiError> {
        Ok(EsiResponse {
            records: vec![CharacterSkillsObservation {
                total_sp: 61_200_000,
                unallocated_sp: Some(0),
                skills: vec![CharacterSkillEntry {
                    skill_id: 3380,
                    active_skill_level: 5,
                    trained_skill_level: 5,
                    skillpoints_in_skill: 1_280_000,
                }],
            }],
            not_modified: false,
            metadata: EsiResponseMetadata {
                expires: Some((Utc::now() + Duration::hours(1)).to_rfc2822()),
                ..Default::default()
            },
        })
    }

    async fn character_skill_queue(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<EsiResponse<CharacterSkillQueueEntry>, EsiError> {
        Ok(EsiResponse {
            records: vec![CharacterSkillQueueEntry {
                skill_id: 3327,
                finished_level: 5,
                queue_position: 0,
                start_date: Some(Utc::now()),
                finish_date: Some(Utc::now() + Duration::days(6)),
                training_start_sp: Some(1_280_000),
                level_start_sp: Some(1_280_000),
                level_end_sp: Some(1_612_800),
            }],
            not_modified: false,
            metadata: EsiResponseMetadata {
                expires: Some((Utc::now() + Duration::hours(1)).to_rfc2822()),
                ..Default::default()
            },
        })
    }

    async fn character_industry_jobs(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<EsiResponse<CharacterIndustryJobObservation>, EsiError> {
        let job = |job_id,
                   activity_id,
                   blueprint_type_id,
                   product_type_id,
                   facility_id,
                   runs,
                   started_hours_ago: i64,
                   ends_in_hours: i64| {
            CharacterIndustryJobObservation {
                job_id,
                activity_id,
                blueprint_type_id,
                product_type_id: Some(product_type_id),
                facility_id,
                station_id: None,
                runs,
                licensed_runs: Some(runs),
                cost: None,
                probability: (activity_id == 8).then(|| Decimal::new(34, 2)),
                duration_seconds: Some((started_hours_ago + ends_in_hours) * 3600),
                status: "active".to_string(),
                start_date: Utc::now() - Duration::hours(started_hours_ago),
                end_date: Utc::now() + Duration::hours(ends_in_hours),
                pause_date: None,
                completed_date: None,
                completed_character_id: None,
                successful_runs: None,
            }
        };
        Ok(EsiResponse {
            records: vec![
                // Manufacturing: Ishtar hull, deep into its run.
                job(500_001, 1, 12_005, 12_005, 1_050_474_463_169, 1, 22, 4),
                // Reaction: Nanoelectrical Microprocessor batch.
                job(500_002, 9, 46_178, 60_714, 1_050_474_463_170, 400, 5, 2),
                // Research: Merlin Blueprint ME, just started.
                job(500_003, 4, 691, 691, 1_050_474_463_169, 1, 1, 11),
            ],
            not_modified: false,
            metadata: EsiResponseMetadata {
                expires: Some((Utc::now() + Duration::minutes(20)).to_rfc2822()),
                ..Default::default()
            },
        })
    }

    async fn character_planets(
        &self,
        _access_token: &str,
        _character_id: i64,
    ) -> Result<EsiResponse<CharacterPlanetObservation>, EsiError> {
        let planet = |planet_id, last_update_hours_ago| CharacterPlanetObservation {
            planet_id,
            planet_type: "barren".to_string(),
            solar_system_id: 30_000_797,
            upgrade_level: 5,
            num_pins: 6,
            last_update: Utc::now() - Duration::hours(last_update_hours_ago),
        };
        Ok(EsiResponse {
            records: vec![
                planet(FIXTURE_EXTRACTOR_PLANET, 2),
                planet(FIXTURE_EXPIRED_PLANET, 30),
                planet(FIXTURE_FACTORY_PLANET, 6),
            ],
            not_modified: false,
            metadata: EsiResponseMetadata {
                expires: Some((Utc::now() + Duration::minutes(10)).to_rfc2822()),
                ..Default::default()
            },
        })
    }

    async fn character_planet_detail(
        &self,
        _access_token: &str,
        _character_id: i64,
        planet_id: i64,
    ) -> Result<EsiResponse<CharacterPlanetDetailObservation>, EsiError> {
        // Real Barren pin/type/schematic ids: ECU 2848, Basic Industry 2473,
        // Advanced Industry 2474, Launchpad 2544, Command Center 2524.
        let pin = |pin_id, type_id, schematic_id, contents: Vec<(i64, i64)>| PlanetPinObservation {
            pin_id,
            type_id,
            schematic_id,
            contents: contents
                .into_iter()
                .map(|(type_id, amount)| PlanetPinContentObservation { type_id, amount })
                .collect(),
            install_time: None,
            expiry_time: None,
            last_cycle_start: None,
            extractor: None,
        };
        let extractor = |pin_id, product_type_id, expires_in_hours: i64| PlanetPinObservation {
            install_time: Some(Utc::now() - Duration::days(3)),
            expiry_time: Some(Utc::now() + Duration::hours(expires_in_hours)),
            last_cycle_start: Some(Utc::now() - Duration::minutes(10)),
            extractor: Some(PlanetExtractorObservation {
                product_type_id,
                qty_per_cycle: 6_000,
                cycle_time_seconds: 1_800,
                head_count: 8,
            }),
            ..pin(pin_id, 2_848, None, vec![])
        };
        let pins = match planet_id {
            // Base Metals -> Reactive Metals; extractor expires soon.
            FIXTURE_EXTRACTOR_PLANET => vec![
                pin(1, 2_524, None, vec![]),
                extractor(2, 2_267, 2),
                pin(3, 2_473, Some(126), vec![(2_267, 3_000)]),
                pin(4, 2_473, Some(126), vec![]),
                pin(5, 2_473, Some(126), vec![]),
                pin(6, 2_544, None, vec![(2_398, 30_000)]),
            ],
            // Noble Metals -> Precious Metals; extractor expired and the
            // launchpad is nearly full.
            FIXTURE_EXPIRED_PLANET => vec![
                pin(1, 2_524, None, vec![]),
                extractor(2, 2_270, -3),
                pin(3, 2_473, Some(127), vec![]),
                pin(4, 2_473, Some(127), vec![]),
                pin(5, 2_473, Some(127), vec![]),
                pin(6, 2_544, None, vec![(2_399, 51_000)]),
            ],
            // Reactive + Precious Metals -> Mechanical Parts; out of Precious.
            _ => vec![
                pin(1, 2_524, None, vec![]),
                pin(2, 2_474, Some(73), vec![(2_398, 40)]),
                pin(3, 2_474, Some(73), vec![]),
                pin(4, 2_544, None, vec![(2_398, 4_000), (3_689, 500)]),
            ],
        };
        Ok(EsiResponse {
            records: vec![CharacterPlanetDetailObservation { pins }],
            not_modified: false,
            metadata: EsiResponseMetadata {
                expires: Some((Utc::now() + Duration::minutes(10)).to_rfc2822()),
                ..Default::default()
            },
        })
    }
}

/// Real Q-3HS5 planets (III, IV, VI) so fixture-mode PI resolves names.
pub(super) const FIXTURE_EXTRACTOR_PLANET: i64 = 40_050_359;
pub(super) const FIXTURE_EXPIRED_PLANET: i64 = 40_050_361;
pub(super) const FIXTURE_FACTORY_PLANET: i64 = 40_050_367;

pub(super) fn fixture_identity() -> Identity {
    Identity {
        character_id: 2_119_000_001,
        character_name: "Fixture Industrialist".to_string(),
        scopes: BTreeSet::from([
            ASSET_SCOPE.to_string(),
            BLUEPRINT_SCOPE.to_string(),
            WALLET_SCOPE.to_string(),
            STRUCTURE_SCOPE.to_string(),
            LOCATION_SCOPE.to_string(),
            SKILLS_SCOPE.to_string(),
            SKILL_QUEUE_SCOPE.to_string(),
            INDUSTRY_JOBS_SCOPE.to_string(),
            MARKET_STRUCTURE_SCOPE.to_string(),
            PLANETS_SCOPE.to_string(),
        ]),
    }
}

pub(super) fn fixture_asset(
    item_id: i64,
    type_id: i64,
    quantity: i64,
    singleton: bool,
) -> AssetObservation {
    AssetObservation {
        item_id,
        type_id,
        quantity,
        location_id: 60_003_760,
        location_type: "station".to_string(),
        location_flag: "Hangar".to_string(),
        is_singleton: singleton,
        is_blueprint_copy: None,
        raw: json!({
            "item_id": item_id, "type_id": type_id, "quantity": quantity,
            "location_id": 60003760, "location_type": "station",
            "location_flag": "Hangar", "is_singleton": singleton
        }),
    }
}

pub(super) fn fixture_wallet(
    transaction_id: i64,
    type_id: i64,
    quantity: i64,
    unit_price: &str,
    is_buy: bool,
) -> WalletTransactionObservation {
    WalletTransactionObservation {
        transaction_id,
        client_id: 2_119_000_222,
        location_id: 60_003_760,
        type_id,
        quantity,
        unit_price: unit_price.parse::<Decimal>().expect("fixture decimal"),
        is_buy,
        is_personal: true,
        journal_ref_id: transaction_id + 100,
        transacted_at: "2026-07-24T18:15:00Z".parse().expect("fixture timestamp"),
        raw: json!({
            "transaction_id": transaction_id, "client_id": 2119000222_i64,
            "location_id": 60003760, "type_id": type_id, "quantity": quantity,
            "unit_price": unit_price, "is_buy": is_buy, "is_personal": true,
            "journal_ref_id": transaction_id + 100, "date": "2026-07-24T18:15:00Z"
        }),
    }
}
