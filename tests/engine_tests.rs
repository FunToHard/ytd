use github_auto_updater::UpdateOptions;

#[test]
fn test_update_options_defaults() {
    let options = UpdateOptions::new("FunToHard", "ytd", "1.0.0");
    assert_eq!(options.owner, "FunToHard");
    assert_eq!(options.repo, "ytd");
    assert_eq!(options.current_version, "1.0.0");
    assert!(!options.allow_prerelease);
    assert!(options.prefer_installer);
}

#[test]
fn test_semver_comparisons() {
    let current = semver::Version::parse("1.0.0").unwrap();
    let higher = semver::Version::parse("1.0.1").unwrap();
    let major_higher = semver::Version::parse("2.0.0").unwrap();
    let lower = semver::Version::parse("0.9.9").unwrap();

    assert!(higher > current);
    assert!(major_higher > current);
    assert!(lower < current);
}
