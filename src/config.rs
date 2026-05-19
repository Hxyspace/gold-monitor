use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

static CONFIG: Mutex<Option<serde_json::Value>> = Mutex::new(None);

fn config_file() -> PathBuf {
    let mut p = std::env::current_exe().ok()
        .and_then(|e| e.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    p.push("config.json");
    p
}

pub fn load() {
    let cfg = fs::read_to_string(config_file()).ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({"x": 100, "y": 100, "visible": true}));
    *CONFIG.lock().unwrap() = Some(cfg);
}

pub fn get_bool(key: &str, default: bool) -> bool {
    CONFIG.lock().unwrap().as_ref()
        .and_then(|v| v[key].as_bool())
        .unwrap_or(default)
}

pub fn get_i32(key: &str, default: i32) -> i32 {
    CONFIG.lock().unwrap().as_ref()
        .and_then(|v| v[key].as_i64())
        .map(|v| v as i32)
        .unwrap_or(default)
}

pub fn set(key: &str, val: serde_json::Value) {
    if let Ok(mut cfg) = CONFIG.lock() {
        if let Some(obj) = cfg.as_mut().and_then(|v| v.as_object_mut()) {
            obj.insert(key.to_string(), val);
        }
    }
    save();
}

fn save() {
    if let Ok(cfg) = CONFIG.lock() {
        if let Some(v) = cfg.as_ref() {
            let _ = fs::write(config_file(), v.to_string());
        }
    }
}
