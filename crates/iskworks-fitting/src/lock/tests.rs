use super::*;

#[test]
fn a_lock_reads_version_and_integrity_and_skips_comments() {
    let lock =
        DataLock::parse("# comment\n\nversion=13.3650286.0\nintegrity=sha512-abc==\n").unwrap();
    assert_eq!(lock.version, "13.3650286.0");
    assert_eq!(lock.major, 13);
    assert_eq!(lock.build_number, 3_650_286);
}

#[test]
fn a_lock_without_a_version_is_refused() {
    assert_eq!(
        DataLock::parse("integrity=sha512-abc==\n"),
        Err(LockError::MissingVersion)
    );
}

#[test]
fn a_version_that_is_not_major_build_patch_is_refused() {
    for version in ["13", "13.x.0", "13.3650286", "a.b.c"] {
        assert_eq!(
            DataLock::parse(&format!("version={version}\n")),
            Err(LockError::InvalidVersion(version.to_string())),
            "{version}"
        );
    }
}

#[test]
fn the_repository_lock_parses() {
    let lock = DataLock::pinned();
    assert!(lock.major >= esf_data::MIN_SDE_VERSION);
    assert!(lock.build_number > 0);
}
