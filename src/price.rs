use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use crate::{taskbar, overlay};

pub static PRICES: Mutex<(f64, f64, f64)> = Mutex::new((0.0, 0.0, 0.0));

fn parse_price(line: &str) -> Option<f64> {
    let s = line.find('"')? + 1;
    let e = line.rfind('"')?;
    if s >= e { return None; }
    line[s..e].split(',').nth(3)?.parse().ok()
}

fn fetch_prices() -> (f64, f64, f64) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    let url = format!(
        "https://api.jijinhao.com/sQuoteCenter/realTime.htm?codes=JO_92233,JO_71,JO_350022&_={}",
        ts
    );

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

pub fn price_loop() {
    loop {
        let (xau, au9999, paxg) = fetch_prices();
        *PRICES.lock().unwrap() = (xau, au9999, paxg);
        taskbar::update_prices(xau, au9999, paxg);
        overlay::update();
        thread::sleep(Duration::from_secs(10));
    }
}
