#![windows_subsystem = "windows"]

mod taskbar;
mod tray;
mod overlay;
mod price;
mod config;

use windows::Win32::UI::WindowsAndMessaging::*;

fn main() {
    config::load();

    // Start price fetch loop
    std::thread::spawn(|| price::price_loop());

    // Create tray icon + overlay window
    tray::create_tray();
    if config::get_bool("visible", true) {
        overlay::show();
    }
    if config::get_bool("taskbar", false) {
        taskbar::toggle();
    }

    // Main message loop (tray + overlay share this thread)
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
