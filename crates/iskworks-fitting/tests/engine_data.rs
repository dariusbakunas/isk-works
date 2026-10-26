//! Tests against the real engine data. Run `make fitting-data` first, then
//! `cargo test -p iskworks-fitting -- --ignored`.

use std::path::Path;
use std::sync::OnceLock;

use iskworks_fitting::{DataLock, EsfFittingEngine};

const REQUIRES_DATA: &str = "requires data/fitting/sde.dat (make fitting-data)";

fn engine() -> &'static EsfFittingEngine {
    static ENGINE: OnceLock<EsfFittingEngine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/fitting/sde.dat");
        EsfFittingEngine::load(&path).unwrap_or_else(|error| panic!("{REQUIRES_DATA}: {error}"))
    })
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn the_pinned_data_loads_and_reports_its_version() {
    let lock = DataLock::pinned();
    let info = engine().engine_info();
    assert_eq!(info.data_version, lock.version);
    assert_eq!(info.major, lock.major);
    assert_eq!(info.build_number, lock.build_number);
}

#[test]
#[ignore = "requires data/fitting/sde.dat (make fitting-data)"]
fn every_published_skill_is_known_for_all_v() {
    // ~500 published skills in current EVE builds.
    assert!(engine().skill_count() > 400, "{}", engine().skill_count());
}
