#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("resources/app-icon.ico");
    res.set_icon_with_id("resources/app-icon.ico", "app-icon");
    if let Err(e) = res.compile() {
        eprintln!("Warning: Failed to compile windows resource: {}", e);
    }
}

#[cfg(not(windows))]
fn main() {}
