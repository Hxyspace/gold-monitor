use std::thread;
use std::time::Duration;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::fs;
use std::path::PathBuf;
use tauri::{Emitter, Manager};
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

static CLICKTHROUGH: AtomicBool = AtomicBool::new(false);

// --- 配置持久化 ---

static CONFIG: Mutex<Option<serde_json::Value>> = Mutex::new(None);

fn config_file() -> PathBuf {
    let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("gold-monitor");
    let _ = fs::create_dir_all(&p);
    p.push("config.json");
    p
}

fn load_config() -> serde_json::Value {
    fs::read_to_string(config_file()).ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({"x": 100, "y": 100, "visible": true, "clickthrough": false}))
}

fn save_config() {
    if let Ok(cfg) = CONFIG.lock() {
        if let Some(v) = cfg.as_ref() {
            let _ = fs::write(config_file(), v.to_string());
        }
    }
}

fn update_config(key: &str, val: serde_json::Value) {
    if let Ok(mut cfg) = CONFIG.lock() {
        if let Some(obj) = cfg.as_mut().and_then(|v| v.as_object_mut()) {
            obj.insert(key.to_string(), val);
        }
    }
    save_config();
}

// --- 价格获取 ---

fn parse_price(line: &str) -> Option<f64> {
    let s = line.find('"')? + 1;
    let e = line.rfind('"')?;
    if s >= e { return None; }
    line[s..e].split(',').nth(3)?.parse().ok()
}

fn fetch_prices() -> (f64, f64, f64) {
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    let url = format!("https://api.jijinhao.com/sQuoteCenter/realTime.htm?codes=JO_92233,JO_71,JO_350022&_={}", ts);

    let Ok(client) = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .build() else { return (0.0, 0.0, 0.0) };

    let text = client.get(&url)
        .header("Referer", "https://quote.cngold.org/")
        .send().ok()
        .and_then(|r| r.text().ok())
        .unwrap_or_default();

    let mut xau = 0.0;
    let mut au9999 = 0.0;
    let mut paxg = 0.0;
    for line in text.lines() {
        if line.contains("JO_92233") { xau = parse_price(line).unwrap_or(0.0); }
        else if line.contains("JO_71") { au9999 = parse_price(line).unwrap_or(0.0); }
        else if line.contains("JO_350022") { paxg = parse_price(line).unwrap_or(0.0); }
    }
    (xau, au9999, paxg)
}

fn price_loop(handle: tauri::AppHandle) {
    loop {
        let (xau, au9999, paxg) = fetch_prices();
        let _ = handle.emit("price-update", serde_json::json!({"xau": xau, "au9999": au9999, "paxg": paxg}));
        taskbar::update_prices(xau, au9999, paxg);
        thread::sleep(Duration::from_secs(10));
    }
}

// --- 入口 ---

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let cfg = load_config();
            *CONFIG.lock().unwrap() = Some(cfg.clone());

            let visible = cfg["visible"].as_bool().unwrap_or(true);
            let clickthrough = cfg["clickthrough"].as_bool().unwrap_or(false);
            CLICKTHROUGH.store(clickthrough, Ordering::Relaxed);

            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_min_size(Some(tauri::Size::Logical(tauri::LogicalSize { width: 50.0, height: 50.0 })));
                let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize { width: 72.0, height: 56.0 }));
                if let (Some(x), Some(y)) = (cfg["x"].as_i64(), cfg["y"].as_i64()) {
                    let _ = win.set_position(tauri::Position::Logical(tauri::LogicalPosition { x: x as f64, y: y as f64 }));
                }
                if visible { let _ = win.show(); }
                if clickthrough { let _ = win.set_ignore_cursor_events(true); }
                win.on_window_event(|event| {
                    if let tauri::WindowEvent::Moved(pos) = event {
                        update_config("x", serde_json::json!(pos.x));
                        update_config("y", serde_json::json!(pos.y));
                    }
                });
            }

            let handle = app.handle().clone();
            thread::spawn(move || price_loop(handle));

            // 托盘菜单
            let show_item = CheckMenuItem::with_id(app, "show", "显示窗口", true, visible, None::<&str>)?;
            let click_item = CheckMenuItem::with_id(app, "clickthrough", "鼠标穿透", true, clickthrough, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &click_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("Gold Monitor")
                .on_menu_event(move |app, event| {
                    match event.id.as_ref() {
                        "show" => {
                            if let Some(win) = app.get_webview_window("main") {
                                let visible = win.is_visible().unwrap_or(false);
                                if visible {
                                    let _ = win.hide();
                                    update_config("visible", serde_json::json!(false));
                                } else {
                                    let _ = win.show();
                                    update_config("visible", serde_json::json!(true));
                                }
                            }
                        }
                        "clickthrough" => {
                            if let Some(win) = app.get_webview_window("main") {
                                let new_val = !CLICKTHROUGH.load(Ordering::Relaxed);
                                CLICKTHROUGH.store(new_val, Ordering::Relaxed);
                                let _ = win.set_ignore_cursor_events(new_val);
                                update_config("clickthrough", serde_json::json!(new_val));
                            }
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    }
                })
                .menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        if let Some(win) = tray.app_handle().get_webview_window("main") {
                            if win.is_visible().unwrap_or(false) {
                                let _ = win.hide();
                                update_config("visible", serde_json::json!(false));
                            } else {
                                let _ = win.show();
                                let _ = win.set_focus();
                                update_config("visible", serde_json::json!(true));
                            }
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
