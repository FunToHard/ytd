use github_auto_updater::{AssetInfo, AssetMatcher};

fn mock_asset(name: &str) -> AssetInfo {
    AssetInfo {
        id: 1,
        name: name.to_string(),
        size: 1024,
        browser_download_url: format!("https://github.com/mock/{}", name),
        content_type: None,
    }
}

#[test]
fn test_find_installer_preferred() {
    let assets = vec![
        mock_asset("ytd-daemon-windows-x86_64.zip"),
        mock_asset("YTD-Setup.exe"),
        mock_asset("ytd-extension.zip"),
        mock_asset("SHA256SUMS.txt"),
    ];

    let target = AssetMatcher::find_target_asset(&assets, true);
    assert!(target.is_some());
    assert_eq!(target.unwrap().name, "YTD-Setup.exe");
}

#[test]
fn test_find_portable_preferred() {
    let assets = vec![
        mock_asset("ytd-daemon-windows-x86_64.zip"),
        mock_asset("YTD-Setup.exe"),
        mock_asset("ytd-extension.zip"),
        mock_asset("SHA256SUMS.txt"),
    ];

    let target = AssetMatcher::find_target_asset(&assets, false);
    assert!(target.is_some());
    assert_eq!(target.unwrap().name, "ytd-daemon-windows-x86_64.zip");
}

#[test]
fn test_find_checksum_asset() {
    let assets = vec![
        mock_asset("ytd-daemon-windows-x86_64.zip"),
        mock_asset("YTD-Setup.exe"),
        mock_asset("SHA256SUMS.txt"),
    ];

    let checksum = AssetMatcher::find_checksum_asset(&assets);
    assert!(checksum.is_some());
    assert_eq!(checksum.unwrap().name, "SHA256SUMS.txt");
}
