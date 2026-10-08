use super::*;

pub(super) fn parse_industry_system(raw: Value) -> Result<IndustrySystemCostIndex, EsiError> {
    let solar_system_id = required_i64(&raw, "solar_system_id")?;
    let indices = raw.get("cost_indices").and_then(Value::as_array);
    let cost_index_for = |activity: &str| -> Option<Decimal> {
        indices?
            .iter()
            .find(|index| index.get("activity").and_then(Value::as_str) == Some(activity))
            .and_then(|index| index.get("cost_index"))
            .map(Value::to_string)
            .and_then(|value| Decimal::from_str(&value).ok())
    };
    let manufacturing = cost_index_for("manufacturing").ok_or(EsiError::InvalidResponse)?;
    let reaction = cost_index_for("reaction").ok_or(EsiError::InvalidResponse)?;
    Ok(IndustrySystemCostIndex {
        solar_system_id,
        manufacturing,
        reaction,
    })
}

pub(super) fn parse_adjusted_price(raw: Value) -> Result<AdjustedPrice, EsiError> {
    let adjusted_price = raw
        .get("adjusted_price")
        .map(Value::to_string)
        .and_then(|value| Decimal::from_str(&value).ok())
        .ok_or(EsiError::InvalidResponse)?;
    Ok(AdjustedPrice {
        type_id: required_i64(&raw, "type_id")?,
        adjusted_price,
    })
}

pub(super) fn parse_market_order(raw: Value) -> Result<MarketOrderObservation, EsiError> {
    let system_id = required_i64(&raw, "system_id")?;
    parse_market_order_with_system_id(raw, system_id)
}

/// Structure-market order JSON (`GET /markets/structures/{structure_id}/`)
/// has no per-order `system_id` field -- the structure's system is implied,
/// not repeated -- unlike region orders. `solar_system_id` here is not a
/// guess: it's the exact value ESI itself returned when the structure was
/// resolved (`GET /universe/structures/{id}/`, captured into
/// `market_location_names.solar_system_id`), passed in by the caller
/// rather than read from this response.
pub(super) fn parse_structure_market_order(
    raw: Value,
    solar_system_id: i64,
) -> Result<MarketOrderObservation, EsiError> {
    parse_market_order_with_system_id(raw, solar_system_id)
}

pub(super) fn parse_market_order_with_system_id(
    raw: Value,
    system_id: i64,
) -> Result<MarketOrderObservation, EsiError> {
    let price = raw
        .get("price")
        .map(Value::to_string)
        .and_then(|value| Decimal::from_str(&value).ok())
        .ok_or(EsiError::InvalidResponse)?;
    let issued_at = required_str(&raw, "issued")?
        .parse()
        .map_err(|_| EsiError::InvalidResponse)?;
    Ok(MarketOrderObservation {
        order_id: required_i64(&raw, "order_id")?,
        type_id: required_i64(&raw, "type_id")?,
        location_id: required_i64(&raw, "location_id")?,
        system_id,
        is_buy_order: raw
            .get("is_buy_order")
            .and_then(Value::as_bool)
            .ok_or(EsiError::InvalidResponse)?,
        price,
        volume_remain: required_u64(&raw, "volume_remain")?,
        volume_total: required_u64(&raw, "volume_total")?,
        min_volume: required_u64(&raw, "min_volume")?,
        order_range: required_str(&raw, "range")?,
        issued_at,
        duration_days: required_u32(&raw, "duration")?,
    })
}

pub(super) fn metadata(response: &reqwest::Response) -> EsiResponseMetadata {
    let text = |name: header::HeaderName| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let number = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
    };
    EsiResponseMetadata {
        etag: text(header::ETAG),
        expires: text(header::EXPIRES),
        last_modified: text(header::LAST_MODIFIED),
        pages: number("x-pages"),
        error_limit_remain: number("x-esi-error-limit-remain"),
        error_limit_reset: number("x-esi-error-limit-reset"),
    }
}

pub(super) fn required_i64(value: &Value, key: &'static str) -> Result<i64, EsiError> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or(EsiError::InvalidResponse)
}

pub(super) fn required_u64(value: &Value, key: &'static str) -> Result<u64, EsiError> {
    required_i64(value, key)?
        .try_into()
        .map_err(|_| EsiError::InvalidResponse)
}

pub(super) fn required_u32(value: &Value, key: &'static str) -> Result<u32, EsiError> {
    required_i64(value, key)?
        .try_into()
        .map_err(|_| EsiError::InvalidResponse)
}

pub(super) fn required_str(value: &Value, key: &'static str) -> Result<String, EsiError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or(EsiError::InvalidResponse)
}

pub(super) fn parse_asset(raw: Value) -> Result<AssetObservation, EsiError> {
    Ok(AssetObservation {
        item_id: required_i64(&raw, "item_id")?,
        type_id: required_i64(&raw, "type_id")?,
        quantity: required_i64(&raw, "quantity")?,
        location_id: required_i64(&raw, "location_id")?,
        location_type: required_str(&raw, "location_type")?,
        location_flag: required_str(&raw, "location_flag")?,
        is_singleton: raw
            .get("is_singleton")
            .and_then(Value::as_bool)
            .ok_or(EsiError::InvalidResponse)?,
        is_blueprint_copy: raw.get("is_blueprint_copy").and_then(Value::as_bool),
        raw,
    })
}

pub(super) fn parse_blueprint(raw: Value) -> Result<BlueprintAssetObservation, EsiError> {
    Ok(BlueprintAssetObservation {
        item_id: required_i64(&raw, "item_id")?,
        type_id: required_i64(&raw, "type_id")?,
        location_id: required_i64(&raw, "location_id")?,
        location_flag: required_str(&raw, "location_flag")?,
        material_efficiency: required_i64(&raw, "material_efficiency")?
            .try_into()
            .map_err(|_| EsiError::InvalidResponse)?,
        time_efficiency: required_i64(&raw, "time_efficiency")?
            .try_into()
            .map_err(|_| EsiError::InvalidResponse)?,
        runs: required_i64(&raw, "runs")?,
        quantity: required_i64(&raw, "quantity")?,
        raw,
    })
}

pub(super) fn parse_wallet(raw: Value) -> Result<WalletTransactionObservation, EsiError> {
    let price_text = raw
        .get("unit_price")
        .ok_or(EsiError::InvalidResponse)?
        .to_string();
    let date = required_str(&raw, "date")?
        .parse()
        .map_err(|_| EsiError::InvalidResponse)?;
    Ok(WalletTransactionObservation {
        transaction_id: required_i64(&raw, "transaction_id")?,
        client_id: required_i64(&raw, "client_id")?,
        location_id: required_i64(&raw, "location_id")?,
        type_id: required_i64(&raw, "type_id")?,
        quantity: required_i64(&raw, "quantity")?,
        unit_price: Decimal::from_str(&price_text).map_err(|_| EsiError::InvalidResponse)?,
        is_buy: raw
            .get("is_buy")
            .and_then(Value::as_bool)
            .ok_or(EsiError::InvalidResponse)?,
        is_personal: raw
            .get("is_personal")
            .and_then(Value::as_bool)
            .ok_or(EsiError::InvalidResponse)?,
        journal_ref_id: required_i64(&raw, "journal_ref_id")?,
        transacted_at: date,
        raw,
    })
}

pub(super) fn parse_wallet_journal(raw: Value) -> Result<WalletJournalObservation, EsiError> {
    let amount = optional_decimal(&raw, "amount").ok_or(EsiError::InvalidResponse)?;
    if amount.normalize().scale() > 4 {
        return Err(EsiError::InvalidResponse);
    }
    Ok(WalletJournalObservation {
        ref_id: required_i64(&raw, "id")?,
        date: required_date(&raw, "date")?,
        ref_type: required_str(&raw, "ref_type")?,
        amount,
        balance: optional_decimal(&raw, "balance"),
        first_party_id: optional_i64(&raw, "first_party_id"),
        second_party_id: optional_i64(&raw, "second_party_id"),
        context_id: optional_i64(&raw, "context_id"),
        context_id_type: raw
            .get("context_id_type")
            .and_then(Value::as_str)
            .map(str::to_string),
        description: raw
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        reason: raw
            .get("reason")
            .and_then(Value::as_str)
            .map(str::to_string),
        tax: optional_decimal(&raw, "tax"),
        tax_receiver_id: optional_i64(&raw, "tax_receiver_id"),
        raw,
    })
}

pub(super) fn parse_wallet_balance(raw: Value) -> Result<WalletBalanceObservation, EsiError> {
    let balance = Decimal::from_str(&raw.to_string()).map_err(|_| EsiError::InvalidResponse)?;
    if balance.is_sign_negative() || balance.normalize().scale() > 4 {
        return Err(EsiError::InvalidResponse);
    }
    Ok(WalletBalanceObservation { balance })
}

pub(super) fn parse_entity_name(raw: Value) -> Result<EveEntityName, EsiError> {
    let id = required_i64(&raw, "id")?;
    if id <= 0 {
        return Err(EsiError::InvalidResponse);
    }
    Ok(EveEntityName {
        id,
        name: required_str(&raw, "name")?,
        category: required_str(&raw, "category")?,
    })
}

pub(super) fn optional_i64(value: &Value, key: &'static str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

/// A JSON number (or numeric string) field -> `Decimal`, dropped silently
/// if absent, null or unparseable. Used for ESI money/probability fields
/// that arrive as bare JSON numbers.
pub(super) fn optional_decimal(value: &Value, key: &'static str) -> Option<Decimal> {
    let field = value.get(key)?;
    if field.is_null() {
        return None;
    }
    match field {
        Value::String(text) => Decimal::from_str(text).ok(),
        other => Decimal::from_str(&other.to_string()).ok(),
    }
}

pub(super) fn optional_date(value: &Value, key: &'static str) -> Option<DateTime<Utc>> {
    value
        .get(key)
        .and_then(Value::as_str)
        .and_then(|text| text.parse().ok())
}

pub(super) fn required_date(value: &Value, key: &'static str) -> Result<DateTime<Utc>, EsiError> {
    required_str(value, key)?
        .parse()
        .map_err(|_| EsiError::InvalidResponse)
}

pub(super) fn parse_character_public_info(
    character_id: i64,
    raw: Value,
) -> Result<CharacterPublicInfo, EsiError> {
    let security_status = raw
        .get("security_status")
        .and_then(Value::as_f64)
        .map(|value| Decimal::from_str(&format!("{value:.5}")))
        .transpose()
        .map_err(|_| EsiError::InvalidResponse)?;
    Ok(CharacterPublicInfo {
        character_id,
        name: required_str(&raw, "name")?,
        corporation_id: required_i64(&raw, "corporation_id")?,
        security_status,
    })
}

pub(super) fn parse_character_location(
    raw: Value,
) -> Result<CharacterLocationObservation, EsiError> {
    Ok(CharacterLocationObservation {
        solar_system_id: required_i64(&raw, "solar_system_id")?,
        station_id: optional_i64(&raw, "station_id"),
        structure_id: optional_i64(&raw, "structure_id"),
    })
}

pub(super) fn parse_character_skill_entry(raw: &Value) -> Result<CharacterSkillEntry, EsiError> {
    Ok(CharacterSkillEntry {
        skill_id: required_i64(raw, "skill_id")?,
        active_skill_level: required_i64(raw, "active_skill_level")?,
        trained_skill_level: required_i64(raw, "trained_skill_level")?,
        skillpoints_in_skill: required_i64(raw, "skillpoints_in_skill")?,
    })
}

pub(super) fn parse_character_skills(raw: Value) -> Result<CharacterSkillsObservation, EsiError> {
    let skills = raw
        .get("skills")
        .and_then(Value::as_array)
        .ok_or(EsiError::InvalidResponse)?
        .iter()
        .map(parse_character_skill_entry)
        .collect::<Result<_, _>>()?;
    Ok(CharacterSkillsObservation {
        total_sp: required_i64(&raw, "total_sp")?,
        unallocated_sp: optional_i64(&raw, "unallocated_sp"),
        skills,
    })
}

pub(super) fn parse_character_skill_queue_entry(
    raw: Value,
) -> Result<CharacterSkillQueueEntry, EsiError> {
    Ok(CharacterSkillQueueEntry {
        skill_id: required_i64(&raw, "skill_id")?,
        finished_level: required_i64(&raw, "finished_level")?,
        queue_position: required_i64(&raw, "queue_position")?,
        start_date: optional_date(&raw, "start_date"),
        finish_date: optional_date(&raw, "finish_date"),
        training_start_sp: optional_i64(&raw, "training_start_sp"),
        level_start_sp: optional_i64(&raw, "level_start_sp"),
        level_end_sp: optional_i64(&raw, "level_end_sp"),
    })
}

/// ESI's documented ordering contract is `queue_position`, not response
/// array order. Downstream code (the API read model, the domain training
/// derivation) may rely on the result of this function being sorted
/// ascending by `queue_position` and must not re-derive order from
/// incidental `Vec`/JSON array position.
pub(super) fn sort_skill_queue_entries(
    mut entries: Vec<CharacterSkillQueueEntry>,
) -> Vec<CharacterSkillQueueEntry> {
    entries.sort_by_key(|entry| entry.queue_position);
    entries
}

pub(super) fn parse_character_industry_job(
    raw: Value,
) -> Result<CharacterIndustryJobObservation, EsiError> {
    Ok(CharacterIndustryJobObservation {
        job_id: required_i64(&raw, "job_id")?,
        activity_id: required_i64(&raw, "activity_id")?,
        blueprint_type_id: required_i64(&raw, "blueprint_type_id")?,
        product_type_id: optional_i64(&raw, "product_type_id"),
        facility_id: required_i64(&raw, "facility_id")?,
        station_id: optional_i64(&raw, "station_id"),
        // `runs` is a required field on ESI's schema; default defensively to
        // 1 rather than reject the whole row if a malformed response omits it.
        runs: optional_i64(&raw, "runs").unwrap_or(1),
        licensed_runs: optional_i64(&raw, "licensed_runs"),
        cost: optional_decimal(&raw, "cost"),
        probability: optional_decimal(&raw, "probability"),
        duration_seconds: optional_i64(&raw, "duration"),
        status: required_str(&raw, "status")?,
        start_date: required_date(&raw, "start_date")?,
        end_date: required_date(&raw, "end_date")?,
        pause_date: optional_date(&raw, "pause_date"),
        completed_date: optional_date(&raw, "completed_date"),
        completed_character_id: optional_i64(&raw, "completed_character_id"),
        successful_runs: optional_i64(&raw, "successful_runs"),
    })
}

pub(super) fn parse_character_planet(raw: Value) -> Result<CharacterPlanetObservation, EsiError> {
    Ok(CharacterPlanetObservation {
        planet_id: required_i64(&raw, "planet_id")?,
        planet_type: required_str(&raw, "planet_type")?,
        solar_system_id: required_i64(&raw, "solar_system_id")?,
        upgrade_level: optional_i64(&raw, "upgrade_level").unwrap_or(0),
        num_pins: optional_i64(&raw, "num_pins").unwrap_or(0),
        last_update: required_date(&raw, "last_update")?,
    })
}

pub(super) fn parse_character_planet_detail(
    raw: &Value,
) -> Result<CharacterPlanetDetailObservation, EsiError> {
    let pins = raw
        .get("pins")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(parse_planet_pin)
        .collect::<Result<_, _>>()?;
    Ok(CharacterPlanetDetailObservation { pins })
}

pub(super) fn parse_planet_pin(raw: &Value) -> Result<PlanetPinObservation, EsiError> {
    let contents = raw
        .get("contents")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|item| {
            Ok(PlanetPinContentObservation {
                type_id: required_i64(item, "type_id")?,
                amount: required_i64(item, "amount")?,
            })
        })
        .collect::<Result<_, EsiError>>()?;
    let extractor = raw
        .get("extractor_details")
        .filter(|details| !details.is_null())
        .and_then(|details| {
            Some(PlanetExtractorObservation {
                product_type_id: optional_i64(details, "product_type_id")?,
                qty_per_cycle: optional_i64(details, "qty_per_cycle").unwrap_or(0),
                cycle_time_seconds: optional_i64(details, "cycle_time").unwrap_or(0),
                head_count: details
                    .get("heads")
                    .and_then(Value::as_array)
                    .map_or(0, |heads| heads.len() as i64),
            })
        });
    Ok(PlanetPinObservation {
        pin_id: required_i64(raw, "pin_id")?,
        type_id: required_i64(raw, "type_id")?,
        schematic_id: optional_i64(raw, "schematic_id").or_else(|| {
            raw.get("factory_details")
                .and_then(|details| optional_i64(details, "schematic_id"))
        }),
        contents,
        install_time: optional_date(raw, "install_time"),
        expiry_time: optional_date(raw, "expiry_time"),
        last_cycle_start: optional_date(raw, "last_cycle_start"),
        extractor,
    })
}
