use github_auto_updater::ChecksumVerifier;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_parse_checksum_map() {
    let content = r#"
# Release Checksums
b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9  YTD-Setup.exe
a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0123456789abcdef0 *ytd-daemon-windows-x86_64.zip
invalid_hash  file.txt
"#;

    let map = ChecksumVerifier::parse_checksum_map(content);
    assert_eq!(map.len(), 2);
    assert_eq!(
        map.get("YTD-Setup.exe").unwrap(),
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
    assert_eq!(
        map.get("ytd-daemon-windows-x86_64.zip").unwrap(),
        "a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0123456789abcdef0"
    );
}

#[test]
fn test_file_checksum_verification() {
    let mut temp = NamedTempFile::new().unwrap();
    write!(temp, "hello world").unwrap();
    temp.flush().unwrap();

    // SHA256 of "hello world" is:
    // b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
    let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

    let matches = ChecksumVerifier::verify_file(temp.path(), expected).unwrap();
    assert!(matches);

    let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let fails = ChecksumVerifier::verify_file(temp.path(), wrong_hash).unwrap();
    assert!(!fails);
}
