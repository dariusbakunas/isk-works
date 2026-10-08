//! The pure EVE-client market-export parser: header validation, per-row
//! parsing, batch-size validation, filename sanitisation, and the
//! observation-timestamp / checksum helpers it relies on. Tolerance and
//! error behaviour are byte-for-byte identical to the pre-split module.

use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};

use crate::Money;

use super::errors::MarketError;
use super::types::{
    MarketImportTimestampSource, MarketOrderSide, MarketUpload, ParsedMarketExport,
    ParsedMarketOrder, MAX_MARKET_BATCH_BYTES, MAX_MARKET_FILENAME_BYTES,
    MAX_MARKET_FILES_PER_BATCH, MAX_MARKET_FILE_BYTES, MAX_MARKET_ROWS_PER_FILE,
};

const REQUIRED_COLUMNS: [&str; 14] = [
    "price",
    "volRemaining",
    "typeID",
    "range",
    "orderID",
    "volEntered",
    "minVolume",
    "bid",
    "issueDate",
    "duration",
    "stationID",
    "regionID",
    "solarSystemID",
    "jumps",
];

pub fn validate_market_upload_batch(files: &[MarketUpload]) -> Result<(), MarketError> {
    if files.is_empty() {
        return Err(MarketError::InvalidUpload(
            "Select at least one .txt or .csv market export.".to_string(),
        ));
    }
    if files.len() > MAX_MARKET_FILES_PER_BATCH {
        return Err(MarketError::InvalidUpload(format!(
            "A batch may contain at most {MAX_MARKET_FILES_PER_BATCH} files."
        )));
    }
    let total_bytes = files.iter().try_fold(0_usize, |total, file| {
        if file.content.len() > MAX_MARKET_FILE_BYTES {
            return Err(MarketError::TooLarge);
        }
        total
            .checked_add(file.content.len())
            .ok_or(MarketError::TooLarge)
    })?;
    if total_bytes > MAX_MARKET_BATCH_BYTES {
        return Err(MarketError::InvalidUpload(
            "The total upload exceeds the batch byte limit.".to_string(),
        ));
    }
    Ok(())
}

pub fn parse_eve_client_market_export(
    upload: &MarketUpload,
    imported_at: DateTime<Utc>,
) -> Result<Vec<ParsedMarketExport>, MarketError> {
    if upload.content.is_empty() {
        return Err(MarketError::Empty);
    }
    if upload.content.len() > MAX_MARKET_FILE_BYTES {
        return Err(MarketError::TooLarge);
    }
    let safe_filename = sanitize_market_filename(&upload.filename)?;
    let extension = Path::new(&safe_filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("txt") && !extension.eq_ignore_ascii_case("csv") {
        return Err(MarketError::InvalidUpload(
            "Only .txt and .csv market exports are supported.".to_string(),
        ));
    }
    let bytes = upload
        .content
        .strip_prefix(&[0xEF, 0xBB, 0xBF])
        .unwrap_or(&upload.content);
    let text = std::str::from_utf8(bytes)
        .map_err(|_| MarketError::InvalidUpload("Market exports must be UTF-8.".to_string()))?;
    if text.trim().is_empty() {
        return Err(MarketError::Empty);
    }

    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|error| MarketError::InvalidHeader(error.to_string()))?
        .clone();
    let indexes = validate_headers(&headers)?;
    let mut orders_by_identity: BTreeMap<(i64, i64, i64, i64), Vec<ParsedMarketOrder>> =
        BTreeMap::new();
    let mut duplicate_counts: BTreeMap<(i64, i64, i64, i64), u64> = BTreeMap::new();
    let mut order_checksums = BTreeMap::new();

    for (position, record) in reader.records().enumerate() {
        if position >= MAX_MARKET_ROWS_PER_FILE {
            return Err(MarketError::TooLarge);
        }
        let row = u32::try_from(position + 2).map_err(|_| MarketError::TooLarge)?;
        let record = record.map_err(|error| MarketError::InvalidRow {
            row,
            column: "row".to_string(),
            message: error.to_string(),
        })?;
        if record.iter().all(|value| value.trim().is_empty()) {
            continue;
        }
        let order = parse_order(&record, &indexes, row)?;
        let identity = (
            order.type_id,
            order.location_id,
            order.solar_system_id,
            order.region_id,
        );
        match order_checksums.get(&order.order_id) {
            Some(checksum) if checksum != &order.normalized_row_checksum => {
                return Err(MarketError::ConflictingOrder(order.order_id));
            }
            Some(_) => {
                *duplicate_counts.entry(identity).or_default() += 1;
            }
            None => {
                order_checksums.insert(order.order_id, order.normalized_row_checksum.clone());
                orders_by_identity.entry(identity).or_default().push(order);
            }
        }
    }
    if orders_by_identity.is_empty() {
        return Err(MarketError::Empty);
    }
    let (observed_at, timestamp_source) =
        observation_timestamp(&safe_filename, upload.user_observed_at, imported_at);
    let physical_checksum = hash_bytes(&upload.content);
    let group_count = orders_by_identity.len();
    let mut exports = Vec::with_capacity(group_count);
    for (identity, orders) in orders_by_identity {
        let normalized_checksum = hash_text(
            &orders
                .iter()
                .map(|order| order.normalized_row_checksum.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let duplicate_order_count = duplicate_counts.get(&identity).copied().unwrap_or(0);
        let mut warnings = Vec::new();
        if timestamp_source == MarketImportTimestampSource::ImportTime {
            warnings.push(
                "The export timestamp was not available; server import time is used.".to_string(),
            );
        }
        if group_count > 1 {
            warnings.push(format!(
                "The source file contains {group_count} item/location order books; this entry represents location {}.",
                identity.1
            ));
        }
        if duplicate_order_count > 0 {
            warnings.push(format!(
                "{duplicate_order_count} exact duplicate order rows were ignored."
            ));
        }
        let file_checksum = if group_count == 1 {
            physical_checksum.clone()
        } else {
            hash_text(&format!(
                "{physical_checksum}|{}|{}|{}|{}",
                identity.0, identity.1, identity.2, identity.3
            ))
        };
        exports.push(ParsedMarketExport {
            original_filename: upload.filename.clone(),
            safe_filename: safe_filename.clone(),
            file_checksum,
            normalized_checksum,
            file_size_bytes: upload.content.len() as u64,
            observed_at,
            timestamp_source,
            type_id: identity.0,
            location_id: identity.1,
            solar_system_id: identity.2,
            region_id: identity.3,
            orders,
            duplicate_order_count,
            warnings,
        });
    }
    Ok(exports)
}

fn validate_headers(
    headers: &csv::StringRecord,
) -> Result<BTreeMap<&'static str, usize>, MarketError> {
    let mut indexes = BTreeMap::new();
    for required in REQUIRED_COLUMNS {
        let matches: Vec<_> = headers
            .iter()
            .enumerate()
            .filter(|(_, header)| header.trim() == required)
            .map(|(index, _)| index)
            .collect();
        match matches.as_slice() {
            [] => {
                return Err(MarketError::InvalidHeader(format!(
                    "required column {required} is missing"
                )))
            }
            [index] => {
                indexes.insert(required, *index);
            }
            _ => {
                return Err(MarketError::InvalidHeader(format!(
                    "required column {required} appears more than once"
                )))
            }
        }
    }
    Ok(indexes)
}

fn parse_order(
    record: &csv::StringRecord,
    indexes: &BTreeMap<&'static str, usize>,
    row: u32,
) -> Result<ParsedMarketOrder, MarketError> {
    let field = |name: &'static str| -> Result<&str, MarketError> {
        record
            .get(indexes[name])
            .map(str::trim)
            .ok_or_else(|| MarketError::InvalidRow {
                row,
                column: name.to_string(),
                message: "value is missing".to_string(),
            })
    };
    let price = Money::parse(field("price")?).map_err(|_| {
        invalid(
            row,
            "price",
            "expected a positive decimal with at most four fractional digits",
        )
    })?;
    if price.0 <= Decimal::ZERO {
        return Err(invalid(row, "price", "price must be greater than zero"));
    }
    let remaining_volume = parse_whole_quantity(field("volRemaining")?, row, "volRemaining", true)?;
    let entered_volume = parse_whole_quantity(field("volEntered")?, row, "volEntered", true)?;
    let minimum_volume = parse_whole_quantity(field("minVolume")?, row, "minVolume", false)?;
    let type_id = parse_positive_i64(field("typeID")?, row, "typeID")?;
    let order_id = parse_positive_i64(field("orderID")?, row, "orderID")?;
    let location_id = parse_positive_i64(field("stationID")?, row, "stationID")?;
    let region_id = parse_positive_i64(field("regionID")?, row, "regionID")?;
    let solar_system_id = parse_positive_i64(field("solarSystemID")?, row, "solarSystemID")?;
    let order_range = parse_i32(field("range")?, row, "range")?;
    let duration_days = parse_i32(field("duration")?, row, "duration")?;
    if duration_days <= 0 {
        return Err(invalid(row, "duration", "duration must be positive"));
    }
    let jumps = parse_i32(field("jumps")?, row, "jumps")?;
    if jumps < 0 {
        return Err(invalid(row, "jumps", "jumps cannot be negative"));
    }
    let side = match field("bid")? {
        "True" | "true" => MarketOrderSide::Buy,
        "False" | "false" => MarketOrderSide::Sell,
        _ => return Err(invalid(row, "bid", "expected True or False")),
    };
    let issued_at = NaiveDateTime::parse_from_str(field("issueDate")?, "%Y-%m-%d %H:%M:%S%.f")
        .map(|value| Utc.from_utc_datetime(&value))
        .map_err(|_| invalid(row, "issueDate", "expected YYYY-MM-DD HH:MM:SS.sss"))?;
    let canonical = format!(
        "{order_id}|{type_id}|{:?}|{}|{remaining_volume}|{entered_volume}|{minimum_volume}|{order_range}|{}|{duration_days}|{location_id}|{region_id}|{solar_system_id}|{jumps}",
        side,
        price.0,
        issued_at.to_rfc3339()
    );
    Ok(ParsedMarketOrder {
        order_id,
        type_id,
        side,
        price,
        remaining_volume,
        entered_volume,
        minimum_volume,
        order_range,
        issued_at,
        duration_days,
        location_id,
        solar_system_id,
        region_id,
        jumps,
        normalized_row_checksum: hash_text(&canonical),
        source_row_number: row,
    })
}

fn parse_whole_quantity(
    value: &str,
    row: u32,
    column: &'static str,
    allow_zero: bool,
) -> Result<u64, MarketError> {
    let decimal = Decimal::from_str(value)
        .map_err(|_| invalid(row, column, "expected a whole-number quantity"))?;
    if decimal.is_sign_negative() || !decimal.fract().is_zero() {
        return Err(invalid(
            row,
            column,
            "quantity must be a non-negative whole number",
        ));
    }
    let quantity = decimal
        .to_u64()
        .ok_or_else(|| invalid(row, column, "quantity is too large"))?;
    if !allow_zero && quantity == 0 {
        return Err(invalid(row, column, "quantity must be positive"));
    }
    Ok(quantity)
}

fn parse_positive_i64(value: &str, row: u32, column: &'static str) -> Result<i64, MarketError> {
    let parsed = value
        .parse::<i64>()
        .map_err(|_| invalid(row, column, "expected a positive integer ID"))?;
    if parsed <= 0 {
        return Err(invalid(row, column, "ID must be positive"));
    }
    Ok(parsed)
}

fn parse_i32(value: &str, row: u32, column: &'static str) -> Result<i32, MarketError> {
    value
        .parse::<i32>()
        .map_err(|_| invalid(row, column, "expected an integer"))
}

fn invalid(row: u32, column: &'static str, message: &'static str) -> MarketError {
    MarketError::InvalidRow {
        row,
        column: column.to_string(),
        message: message.to_string(),
    }
}

pub fn sanitize_market_filename(filename: &str) -> Result<String, MarketError> {
    let filename = filename.trim();
    if filename.is_empty()
        || filename.len() > MAX_MARKET_FILENAME_BYTES
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
        || filename.chars().any(char::is_control)
    {
        return Err(MarketError::InvalidUpload(
            "The upload filename is invalid.".to_string(),
        ));
    }
    Ok(filename.to_string())
}

fn observation_timestamp(
    filename: &str,
    user_supplied: Option<DateTime<Utc>>,
    imported_at: DateTime<Utc>,
) -> (DateTime<Utc>, MarketImportTimestampSource) {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(filename);
    const EXPORT_TIMESTAMP_LENGTH: usize = 17;
    if stem.len() >= EXPORT_TIMESTAMP_LENGTH {
        for start in (0..=stem.len() - EXPORT_TIMESTAMP_LENGTH).rev() {
            let candidate = &stem[start..start + EXPORT_TIMESTAMP_LENGTH];
            if let Ok(value) = NaiveDateTime::parse_from_str(candidate, "%Y.%m.%d %H%M%S") {
                return (
                    Utc.from_utc_datetime(&value),
                    MarketImportTimestampSource::Filename,
                );
            }
        }
    }
    if let Some(value) = user_supplied {
        return (value, MarketImportTimestampSource::UserSupplied);
    }
    (imported_at, MarketImportTimestampSource::ImportTime)
}

fn hash_bytes(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn hash_text(value: &str) -> String {
    hash_bytes(value.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::MarketImportPreviewFile;

    const SAMPLE: &str = "price,volRemaining,typeID,range,orderID,volEntered,minVolume,bid,issueDate,duration,stationID,regionID,solarSystemID,jumps,\n\
3.97,226641769.0,34,32767,7386855683,500000000,1,False,2026-07-26 18:43:04.000,90,1049588174021,10000009,30000772,0,\n\
3.98,32698916.0,34,32767,7381912781,110000000,1,False,2026-07-26 15:38:53.000,90,1049588174021,10000009,30000772,0,\n\
3.81,120036013.0,34,-1,7385903626,253569381,1,True,2026-07-26 19:01:03.000,90,1049588174021,10000009,30000772,0,\n";

    fn upload(content: &str) -> MarketUpload {
        MarketUpload {
            filename: "Insmother-Tritanium-2026.07.26 192639.txt".to_string(),
            content: content.as_bytes().to_vec(),
            user_observed_at: None,
        }
    }

    #[test]
    fn parses_realistic_export_exactly() {
        let mut exports =
            parse_eve_client_market_export(&upload(SAMPLE), Utc::now()).expect("valid export");
        assert_eq!(exports.len(), 1);
        let parsed = exports.pop().unwrap();
        assert_eq!(parsed.type_id, 34);
        assert_eq!(parsed.location_id, 1_049_588_174_021);
        assert_eq!(parsed.orders.len(), 3);
        assert_eq!(parsed.orders[0].price, Money::parse("3.9700").unwrap());
        assert_eq!(parsed.orders[0].remaining_volume, 226_641_769);
        assert_eq!(
            parsed.timestamp_source,
            MarketImportTimestampSource::Filename
        );
        assert_eq!(
            parsed.observed_at,
            Utc.with_ymd_and_hms(2026, 7, 26, 19, 26, 39).unwrap()
        );
        let preview = MarketImportPreviewFile::from_parsed(&parsed, "Tritanium".to_string());
        assert_eq!(preview.lowest_sell, Some(Money::parse("3.9700").unwrap()));
        assert_eq!(preview.highest_buy, Some(Money::parse("3.8100").unwrap()));
    }

    #[test]
    fn accepts_bom_crlf_and_csv_extension() {
        let content = format!("\u{feff}{}", SAMPLE.replace('\n', "\r\n"));
        let mut upload = upload(&content);
        upload.filename = "Tritanium.csv".to_string();
        let mut exports = parse_eve_client_market_export(
            &upload,
            Utc.with_ymd_and_hms(2026, 7, 26, 20, 0, 0).unwrap(),
        )
        .unwrap();
        let parsed = exports.pop().unwrap();
        assert_eq!(parsed.orders.len(), 3);
        assert_eq!(
            parsed.timestamp_source,
            MarketImportTimestampSource::ImportTime
        );
    }

    #[test]
    fn rejects_unsafe_filename_and_fractional_volume() {
        let mut unsafe_upload = upload(SAMPLE);
        unsafe_upload.filename = "../market.txt".to_string();
        assert!(matches!(
            parse_eve_client_market_export(&unsafe_upload, Utc::now()),
            Err(MarketError::InvalidUpload(_))
        ));

        let fractional = SAMPLE.replace("226641769.0", "226641769.5");
        assert!(matches!(
            parse_eve_client_market_export(&upload(&fractional), Utc::now()),
            Err(MarketError::InvalidRow {
                column,
                ..
            }) if column == "volRemaining"
        ));
    }

    #[test]
    fn rejects_missing_columns_and_conflicting_order() {
        let missing = SAMPLE.replace("price,", "unitPrice,");
        assert!(matches!(
            parse_eve_client_market_export(&upload(&missing), Utc::now()),
            Err(MarketError::InvalidHeader(_))
        ));
        let conflict = format!(
            "{SAMPLE}3.99,1.0,34,32767,7386855683,1,1,False,2026-07-26 18:43:04.000,90,1049588174021,10000009,30000772,0,\n"
        );
        assert!(matches!(
            parse_eve_client_market_export(&upload(&conflict), Utc::now()),
            Err(MarketError::ConflictingOrder(7_386_855_683))
        ));
    }

    #[test]
    fn splits_mixed_location_files_into_deterministic_order_books() {
        let mixed = SAMPLE.replacen("1049588174021", "60003760", 1);
        let exports =
            parse_eve_client_market_export(&upload(&mixed), Utc::now()).expect("mixed locations");

        assert_eq!(exports.len(), 2);
        assert_eq!(
            exports
                .iter()
                .map(|export| export.location_id)
                .collect::<Vec<_>>(),
            vec![60_003_760, 1_049_588_174_021]
        );
        assert_eq!(
            exports
                .iter()
                .map(|export| export.orders.len())
                .sum::<usize>(),
            3
        );
        assert_ne!(exports[0].file_checksum, exports[1].file_checksum);
        assert!(exports.iter().all(|export| export
            .warnings
            .iter()
            .any(|warning| warning.contains("2 item/location order books"))));
    }
}
