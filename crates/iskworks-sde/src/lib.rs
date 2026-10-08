//! Minimal CCP SDE parsing and import orchestration for ISK Works.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;
use zip::ZipArchive;

mod import;
mod read;

#[cfg(test)]
mod tests;

pub use import::*;
pub use read::*;
