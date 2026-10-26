use std::path::PathBuf;

use super::*;

fn scratch_file(name: &str, bytes: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("iskworks-fitting-{}-{name}", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path
}

fn lock(major: i32, build_number: i32) -> DataLock {
    DataLock {
        version: format!("{major}.{build_number}.0"),
        major,
        build_number,
    }
}

#[test]
fn the_engine_is_shareable_across_requests() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<EsfFittingEngine>();
}

#[test]
fn a_missing_data_file_is_a_read_error() {
    let error = EsfFittingEngine::load(Path::new("/nonexistent/iskworks/sde.dat"))
        .err()
        .unwrap();
    assert!(matches!(error, FittingLoadError::Read(_)), "{error:?}");
    assert!(error.to_string().contains("make fitting-data"), "{error}");
}

#[test]
fn a_file_that_is_not_engine_data_is_refused() {
    let path = scratch_file("garbage.dat", b"definitely not flatbuffers");
    let error = EsfFittingEngine::load(&path).err().unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(error, FittingLoadError::Invalid(_)), "{error:?}");
}

#[test]
fn data_from_the_locked_major_and_build_is_accepted() {
    assert_eq!(check_version(&lock(13, 3_650_286), 13, 3_650_286), Ok(()));
}

#[test]
fn data_from_another_major_or_build_is_stale() {
    for (major, build) in [(13, 3_600_000), (12, 3_650_286), (14, 3_650_286)] {
        assert_eq!(
            check_version(&lock(13, 3_650_286), major, build),
            Err(FittingLoadError::StaleData {
                expected: "13.3650286".to_string(),
                found: format!("{major}.{build}"),
            })
        );
    }
}
