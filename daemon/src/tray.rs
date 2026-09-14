use crate::config::SharedConfig;
use tokio::sync::broadcast;
use tracing::{error, info};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIconBuilder,
};

pub fn run_tray(
    config: SharedConfig,
    tx_exit: broadcast::Sender<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let icon_bytes = include_bytes!("../resources/icon_32x32.rgba");
    let icon = Icon::from_rgba(icon_bytes.to_vec(), 32, 32)?;

    let menu = Menu::new();

    let item_status = MenuItem::new("YTD Daemon: Online", false, None);

    let dep_status = crate::deps::check_dependencies();
    let item_deps = if !dep_status.all_ready {
        MenuItem::new("⚠️ Install Dependencies (yt-dlp & ffmpeg)...", true, None)
    } else {
        MenuItem::new("✓ Dependencies: Ready", false, None)
    };
    let item_install_ext = MenuItem::new("Install Browser Extension...", true, None);

    let item_music = MenuItem::new("Open Music Folder", true, None);
    let item_video = MenuItem::new("Open Video Folder", true, None);
    let item_change_video = MenuItem::new("Change Video Download Folder...", true, None);

    let initial_startup = {
        let cfg = config.read().unwrap();
        cfg.auto_start
    };
    let item_startup = CheckMenuItem::new("Run at Startup", true, initial_startup, None);

    let item_quit = MenuItem::new("Quit YTD Daemon", true, None);

    menu.append(&item_status)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&item_deps)?;
    menu.append(&item_install_ext)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&item_music)?;
    menu.append(&item_video)?;
    menu.append(&item_change_video)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&item_startup)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&item_quit)?;

    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("YTD Link Downloader Daemon")
        .with_icon(icon)
        .build()?;

    info!("System tray icon initialized successfully");

    let menu_channel = MenuEvent::receiver();

    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, PostQuitMessage, TranslateMessage, MSG,
        };

        let mut msg: MSG = std::mem::zeroed();

        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);

            while let Ok(event) = menu_channel.try_recv() {
                if event.id == item_deps.id() {
                    let dep_status = crate::deps::check_dependencies();
                    if !dep_status.all_ready {
                        let res = rfd::MessageDialog::new()
                            .set_title("YTD - Dependency Setup")
                            .set_description(
                                "YTD needs yt-dlp and ffmpeg to download and convert videos.\n\nWould you like YTD to automatically download and configure them into %APPDATA%\\ytd\\bin?"
                            )
                            .set_buttons(rfd::MessageButtons::YesNo)
                            .show();

                        if res == rfd::MessageDialogResult::Yes {
                            crate::notifier::notify_download_started("yt-dlp & ffmpeg setup started...", false);
                            std::thread::spawn(|| {
                                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
                                if let Ok(rt) = rt {
                                    rt.block_on(async {
                                        if let Err(e) = crate::deps::install_dependencies().await {
                                            crate::notifier::notify_download_failed("Dependency Setup", &e);
                                        } else {
                                            crate::notifier::notify_download_completed(
                                                "yt-dlp & ffmpeg ready",
                                                &crate::deps::get_bin_dir().to_string_lossy(),
                                                false,
                                            );
                                        }
                                    });
                                }
                            });
                        }
                    }
                } else if event.id == item_install_ext.id() {
                    crate::deps::open_extension_helper();
                } else if event.id == item_music.id() {
                    let path = {
                        let cfg = config.read().unwrap();
                        cfg.audio_download_dir.clone()
                    };
                    if let Err(e) = open::that(&path) {
                        error!("Failed to open music folder {:?}: {}", path, e);
                    }
                } else if event.id == item_video.id() {
                    let path = {
                        let cfg = config.read().unwrap();
                        cfg.video_download_dir.clone()
                    };
                    if let Err(e) = open::that(&path) {
                        error!("Failed to open video folder {:?}: {}", path, e);
                    }
                } else if event.id == item_change_video.id() {
                    let cfg_for_picker = config.clone();
                    std::thread::spawn(move || {
                        let initial = {
                            let cfg = cfg_for_picker.read().unwrap();
                            cfg.video_download_dir.clone()
                        };
                        if let Some(folder) = rfd::FileDialog::new()
                            .set_title("Select Video Download Directory")
                            .set_directory(&initial)
                            .pick_folder()
                        {
                            info!("Updated video download directory to: {:?}", folder);
                            let mut cfg = cfg_for_picker.write().unwrap();
                            cfg.update_video_dir(folder);
                        }
                    });
                } else if event.id == item_startup.id() {
                    let is_checked = item_startup.is_checked();
                    info!("Startup setting toggled to: {}", is_checked);
                    let mut cfg = config.write().unwrap();
                    cfg.update_auto_start(is_checked);
                } else if event.id == item_quit.id() {
                    info!("Quit requested from system tray context menu");
                    let _ = tx_exit.send(());
                    PostQuitMessage(0);
                }
            }
        }
    }

    Ok(())
}
