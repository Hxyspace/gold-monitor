use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use crate::{taskbar, overlay};

#[derive(Debug, Clone, Copy, Default)]
pub struct PriceData {
    pub xau: f64,         // 国际金价 USD/盎司
    pub au9999: f64,      // 国内金价 CNY/克
    pub paxg: f64,        // PAXG USD
    pub dxy: f64,         // 美元指数
    pub us10y: f64,       // 美国10年期国债收益率 %
    pub us10y_chg: f64,   // 较前一日变动 (bp)
}

pub static PRICES: Mutex<PriceData> = Mutex::new(PriceData {
    xau: 0.0, au9999: 0.0, paxg: 0.0, dxy: 0.0, us10y: 0.0, us10y_chg: 0.0,
});

fn parse_price(line: &str) -> Option<f64> {
    let s = line.find('"')? + 1;
    let e = line.rfind('"')?;
    if s >= e { return None; }
    line[s..e].split(',').nth(3)?.parse().ok()
}

fn fetch_gold() -> (f64, f64, f64) {
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

fn fetch_dxy() -> f64 {
    let Ok(client) = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .build() else { return 0.0 };

    let text = client.get("https://hq.sinajs.cn/list=DINIW")
        .header("Referer", "https://finance.sina.com.cn")
        .send().ok()
        .and_then(|r| r.text().ok())
        .unwrap_or_default();

    // Response: var hq_str_DINIW="15:10:23,99.3470,99.3470,99.3112,..."
    if let Some(s) = text.find('"').map(|i| i + 1) {
        if let Some(e) = text.rfind('"') {
            if s < e {
                return text[s..e].split(',')
                    .nth(1)  // current price is field index 1
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.0);
            }
        }
    }
    0.0
}

fn fetch_us10y() -> (f64, f64) {
    // Returns (current_value, change_from_previous_day_in_bp)
    // US Treasury yield only updates once per day, so we cache and only
    // refetch every hour to avoid hitting East Money rate limits.
    static CACHE: Mutex<(f64, f64, Option<Instant>)> = Mutex::new((0.0, 0.0, None));
    const REFRESH_INTERVAL: Duration = Duration::from_secs(3600);

    {
        let cache = CACHE.lock().unwrap();
        if let Some(last) = cache.2 {
            if last.elapsed() < REFRESH_INTERVAL {
                return (cache.0, cache.1);
            }
        }
    }

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    // Fetch 2 records: today + yesterday to compute daily change
    let url = format!(
        "https://datacenter.eastmoney.com/api/data/get?type=RPTA_WEB_TREASURYYIELD&sty=ALL&st=SOLAR_DATE&sr=-1&token=894050c76af8597a853f5b408b759f5d&p=1&ps=2&pageNo=1&pageNum=1&_={}",
        ts
    );

    let Ok(client) = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .timeout(Duration::from_secs(10))
        .build() else { return (0.0, 0.0) };

    let (current, change) = client.get(&url)
        .send().ok()
        .and_then(|r| r.text().ok())
        .and_then(|text| {
            let v: serde_json::Value = serde_json::from_str(&text).ok()?;
            let data = v["result"]["data"].as_array()?;
            let today = data[0]["EMG00001310"].as_f64()?;
            let prev = data.get(1)
                .and_then(|d| d["EMG00001310"].as_f64())
                .unwrap_or(today);
            // change in basis points (1 bp = 0.01%)
            Some((today, (today - prev) * 100.0))
        })
        .unwrap_or((0.0, 0.0));

    if current > 0.0 {
        let mut cache = CACHE.lock().unwrap();
        cache.0 = current;
        cache.1 = change;
        cache.2 = Some(Instant::now());
    }

    (current, change)
}

pub fn price_loop() {
    loop {
        let (xau, au9999, paxg) = fetch_gold();
        let dxy = fetch_dxy();
        let (us10y, us10y_chg) = fetch_us10y();

        let data = PriceData { xau, au9999, paxg, dxy, us10y, us10y_chg };
        *PRICES.lock().unwrap() = data;
        taskbar::update_prices(data);
        overlay::update();
        thread::sleep(Duration::from_secs(10));
    }
}
