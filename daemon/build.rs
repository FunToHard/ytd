#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("resources/app-icon.ico");
    res.set_icon_with_id("resources/app-icon.ico", "app-icon");
    res.set("FileDescription", "YTD");
    res.set("ProductName", "YTD");
    res.set("OriginalFilename", "ytd-daemon.exe");
    res.set("CompanyName", "YTD Project");
    res.set("LegalCopyright", "Copyright (c) 2026 FunToHard");
    if let Err(e) = res.compile() {
        eprintln!("Warning: Failed to compile windows resource: {}", e);
    }
}

#[cfg(not(windows))]
fn main() {}
