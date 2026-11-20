//! Transforme les réponses brutes de la Freebox en un modèle stable pour l'interface.
//! Tolérant : chaque champ est optionnel, car les versions de l'API varient d'une box à l'autre.

use serde::Serialize;
use serde_json::Value;

pub fn st(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| match x {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}
pub fn num(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(|x| match x {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    })
}
pub fn boo(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(|x| x.as_bool())
}
pub fn arr(v: &Value) -> &[Value] {
    v.as_array().map(|a| a.as_slice()).unwrap_or(&[])
}
pub fn band_of(s: &str) -> String {
    match s.to_lowercase().as_str() {
        "2d4g" | "2.4" | "2g4" | "2.4g" | "2g" => "2.4".into(),
        "5g" | "5" | "5g1" | "5g2" => "5".into(),
        "6g" | "6" => "6".into(),
        "60g" => "60".into(),
        other => other.trim_end_matches('g').to_string(),
    }
}
/// La v2 de l'API exprime le signal en atténuation positive ; les versions récentes en dBm négatifs.
pub fn dbm(x: Option<f64>) -> Option<f64> {
    x.filter(|v| *v != 0.0 && v.abs() < 130.0).map(|v| if v > 0.0 { -v } else { v })
}
pub fn norm_mac(s: &str) -> String {
    s.trim().to_lowercase().replace('-', ":")
}

// ---------------- Connexion ----------------
#[derive(Serialize, Clone, Default, Debug)]
pub struct Wan {
    pub state: String,
    pub media: String,
    pub kind: String,
    pub down_mbps: f64,
    pub up_mbps: f64,
    pub bw_down_mbps: f64,
    pub bw_up_mbps: f64,
    pub bytes_down: f64,
    pub bytes_up: f64,
    pub ipv4: Option<String>,
}
pub fn wan(c: &Value) -> Wan {
    Wan {
        state: st(c, "state").unwrap_or_else(|| "unknown".into()),
        media: st(c, "media").unwrap_or_default(),
        kind: st(c, "type").unwrap_or_default(),
        down_mbps: num(c, "rate_down").unwrap_or(0.0) * 8.0 / 1e6,
        up_mbps: num(c, "rate_up").unwrap_or(0.0) * 8.0 / 1e6,
        bw_down_mbps: num(c, "bandwidth_down").unwrap_or(0.0) / 1e6,
        bw_up_mbps: num(c, "bandwidth_up").unwrap_or(0.0) / 1e6,
        bytes_down: num(c, "bytes_down").unwrap_or(0.0),
        bytes_up: num(c, "bytes_up").unwrap_or(0.0),
        ipv4: st(c, "ipv4"),
    }
}

// ---------------- Wi-Fi ----------------
#[derive(Serialize, Clone, Default, Debug)]
pub struct ChanUse {
    pub channel: i64,
    pub band: String,
    pub busy: Option<f64>,
    pub noise: Option<f64>,
}
#[derive(Serialize, Clone, Default, Debug)]
pub struct Ap {
    pub id: i64,
    pub name: String,
    pub band: String,
    pub state: String,
    pub channel: i64,
    pub secondary_channel: i64,
    pub width: i64,
    pub auto_channel: bool,
    pub stations: usize,
    pub busy: Option<f64>,
    pub usage: Vec<ChanUse>,
}
pub fn aps(list: &Value) -> Vec<Ap> {
    arr(list)
        .iter()
        .map(|a| {
            let cfg = a.get("config").cloned().unwrap_or(Value::Null);
            let status = a.get("status").cloned().unwrap_or(Value::Null);
            let ch = num(&status, "primary_channel").or(num(&cfg, "primary_channel")).unwrap_or(0.0) as i64;
            Ap {
                id: num(a, "id").unwrap_or(0.0) as i64,
                name: st(a, "name").unwrap_or_default(),
                band: band_of(&st(&cfg, "band").or(st(a, "band")).unwrap_or_default()),
                state: st(&status, "state").unwrap_or_else(|| "unknown".into()),
                channel: ch,
                secondary_channel: num(&status, "secondary_channel").unwrap_or(0.0) as i64,
                width: num(&status, "channel_width").or(num(&cfg, "channel_width")).unwrap_or(20.0) as i64,
                auto_channel: num(&cfg, "primary_channel").unwrap_or(0.0) == 0.0,
                ..Default::default()
            }
        })
        .collect()
}
pub fn channel_usage(list: &Value, band: &str) -> Vec<ChanUse> {
    arr(list)
        .iter()
        .map(|c| {
            let busy = num(c, "busy_percent").or_else(|| {
                let rx = num(c, "rx_busy_percent");
                let tx = num(c, "tx_percent");
                match (rx, tx) {
                    (Some(r), Some(t)) => Some(r + t),
                    (Some(r), None) => Some(r),
                    _ => None,
                }
            });
            ChanUse {
                channel: num(c, "channel").unwrap_or(0.0) as i64,
                band: st(c, "band").map(|b| band_of(&b)).unwrap_or_else(|| band.to_string()),
                busy: busy.map(|b| b.min(100.0)),
                noise: num(c, "noise_level"),
            }
        })
        .filter(|c| c.channel > 0)
        .collect()
}

#[derive(Serialize, Clone, Default, Debug)]
pub struct Neighbor {
    pub ssid: String,
    pub bssid: String,
    pub band: String,
    pub channel: i64,
    pub width: i64,
    pub signal: Option<f64>,
}
pub fn neighbors(list: &Value, band: &str) -> Vec<Neighbor> {
    arr(list)
        .iter()
        .map(|n| Neighbor {
            ssid: st(n, "ssid").unwrap_or_else(|| "(masqué)".into()),
            bssid: st(n, "bssid").map(|m| norm_mac(&m)).unwrap_or_default(),
            band: st(n, "band").map(|b| band_of(&b)).unwrap_or_else(|| band.to_string()),
            channel: num(n, "channel").unwrap_or(0.0) as i64,
            width: num(n, "channel_width").unwrap_or(20.0) as i64,
            signal: dbm(num(n, "signal")),
        })
        .filter(|n| n.channel > 0)
        .collect()
}

#[derive(Clone, Default, Debug)]
pub struct Station {
    pub mac: String,
    pub ap_id: i64,
    pub band: String,
    pub ssid: Option<String>,
    pub signal: Option<f64>,
    pub phy_down: Option<f64>,
    pub phy_up: Option<f64>,
    /// Octets envoyés par la box vers l'appareil (téléchargement de l'appareil).
    pub bytes_to_dev: Option<f64>,
    pub bytes_from_dev: Option<f64>,
    pub rate_to_dev: Option<f64>,
    pub rate_from_dev: Option<f64>,
    pub standard: Option<String>,
    pub conn_duration: Option<i64>,
    pub host_id: Option<String>,
}
fn std_from_flags(s: &Value) -> Option<String> {
    if let Some(x) = st(s, "standard").or(st(s, "wifi_standard")) {
        return Some(pretty_std(&x));
    }
    let f = s.get("flags")?;
    Some(if boo(f, "he") == Some(true) || boo(f, "eht") == Some(true) {
        if boo(f, "eht") == Some(true) { "Wi-Fi 7" } else { "Wi-Fi 6" }
    } else if boo(f, "vht") == Some(true) {
        "Wi-Fi 5"
    } else if boo(f, "ht") == Some(true) {
        "Wi-Fi 4"
    } else if boo(f, "legacy") == Some(true) {
        "Wi-Fi ancien"
    } else {
        return None;
    }
    .into())
}
pub fn pretty_std(x: &str) -> String {
    match x.to_lowercase().trim_start_matches("802.11").trim() {
        "be" | "eht" => "Wi-Fi 7".into(),
        "ax" | "he" => "Wi-Fi 6".into(),
        "ac" | "vht" => "Wi-Fi 5".into(),
        "n" | "ht" => "Wi-Fi 4".into(),
        "a" | "b" | "g" | "legacy" => "Wi-Fi ancien".into(),
        o => o.to_string(),
    }
}
/// bitrate est en 1/10 Mbit/s dans l'API (−1 si inconnu).
fn bitrate(s: &Value, k: &str) -> Option<f64> {
    s.get(k).and_then(|x| num(x, "bitrate")).filter(|b| *b > 0.0).map(|b| b / 10.0)
}
pub fn stations(list: &Value, ap: &Ap) -> Vec<Station> {
    arr(list)
        .iter()
        .filter_map(|s| {
            let mac = norm_mac(&st(s, "mac")?);
            Some(Station {
                mac,
                ap_id: ap.id,
                band: ap.band.clone(),
                ssid: s.get("bss").and_then(|b| st(b, "ssid")).or(st(s, "ssid")),
                signal: dbm(num(s, "signal")),
                phy_down: bitrate(s, "last_tx"),
                phy_up: bitrate(s, "last_rx"),
                // Point de vue de la borne : tx = vers l'appareil.
                bytes_to_dev: num(s, "tx_bytes"),
                bytes_from_dev: num(s, "rx_bytes"),
                rate_to_dev: num(s, "tx_rate"),
                rate_from_dev: num(s, "rx_rate"),
                standard: std_from_flags(s),
                conn_duration: num(s, "conn_duration").map(|d| d as i64),
                host_id: s.get("host").and_then(|h| st(h, "id")),
            })
        })
        .collect()
}

// ---------------- Appareils ----------------
#[derive(Serialize, Clone, Default, Debug)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub vendor: Option<String>,
    pub kind: String,
    pub mac: String,
    pub ip: Option<String>,
    pub active: bool,
    pub reachable: bool,
    pub last_activity: i64,
    pub conn: String,
    pub band: Option<String>,
    pub ssid: Option<String>,
    pub signal: Option<f64>,
    pub phy_down: Option<f64>,
    pub phy_up: Option<f64>,
    pub standard: Option<String>,
    pub ap_id: Option<i64>,
    pub via_repeater: bool,
    pub eth_speed: Option<f64>,
    pub port: Option<i64>,
    pub conn_duration: Option<i64>,
    pub down_mbps: Option<f64>,
    pub up_mbps: Option<f64>,
    pub today_down: f64,
    pub today_up: f64,
    pub first_seen: Option<i64>,
    #[serde(skip)]
    pub ap_bytes: Option<(f64, f64)>,
    #[serde(skip)]
    pub ap_rates: Option<(f64, f64)>,
}

fn l2_mac(h: &Value) -> Option<String> {
    let l2 = h.get("l2ident")?;
    let o = if l2.is_array() { l2.get(0)? } else { l2 };
    st(o, "id").map(|m| norm_mac(&m))
}

pub fn hosts(list: &Value) -> Vec<Host> {
    arr(list)
        .iter()
        .filter_map(|h| {
            let mac = l2_mac(h)?;
            let ip = arr(h.get("l3connectivities").unwrap_or(&Value::Null))
                .iter()
                .filter(|c| st(c, "af").as_deref() == Some("ipv4"))
                .max_by_key(|c| (boo(c, "active") == Some(true), num(c, "last_activity").unwrap_or(0.0) as i64))
                .and_then(|c| st(c, "addr"));
            let mut host = Host {
                id: st(h, "id").unwrap_or_else(|| mac.clone()),
                name: st(h, "primary_name").unwrap_or_else(|| mac.clone()),
                vendor: st(h, "vendor_name"),
                kind: st(h, "host_type").unwrap_or_else(|| "other".into()),
                mac,
                ip,
                active: boo(h, "active").unwrap_or(false),
                reachable: boo(h, "reachable").unwrap_or(false),
                last_activity: num(h, "last_activity").unwrap_or(0.0) as i64,
                conn: "unknown".into(),
                ..Default::default()
            };
            // Versions récentes : point d'accès auquel l'appareil est relié (box ou répéteur).
            if let Some(ap) = h.get("access_point").filter(|a| a.is_object()) {
                let ctype = st(ap, "connectivity_type").or(st(ap, "type")).unwrap_or_default().to_lowercase();
                host.via_repeater = st(ap, "type").map(|t| t.to_lowercase().contains("repeater")).unwrap_or(false);
                if ctype.contains("wifi") {
                    host.conn = "wifi".into();
                } else if ctype.contains("ethernet") {
                    host.conn = "ethernet".into();
                }
                if let Some(w) = ap.get("wifi_information").filter(|w| w.is_object()) {
                    host.conn = "wifi".into();
                    host.band = st(w, "band").map(|b| band_of(&b));
                    host.ssid = st(w, "ssid");
                    host.signal = dbm(num(w, "signal"));
                    host.phy_down = num(w, "phy_tx_rate").filter(|x| *x > 0.0).map(|x| if x > 100_000.0 { x / 1e6 } else { x });
                    host.phy_up = num(w, "phy_rx_rate").filter(|x| *x > 0.0).map(|x| if x > 100_000.0 { x / 1e6 } else { x });
                    host.standard = st(w, "standard").map(|s| pretty_std(&s));
                }
                if let Some(e) = ap.get("ethernet_information").filter(|e| e.is_object()) {
                    host.conn = "ethernet".into();
                    host.eth_speed = num(e, "speed");
                }
                if let (Some(tx), Some(rx)) = (num(ap, "tx_bytes"), num(ap, "rx_bytes")) {
                    host.ap_bytes = Some((tx, rx));
                }
                if let (Some(tx), Some(rx)) = (num(ap, "tx_rate"), num(ap, "rx_rate")) {
                    host.ap_rates = Some((tx, rx));
                }
            }
            Some(host)
        })
        .collect()
}

// ---------------- Switch (ports Ethernet) ----------------
#[derive(Serialize, Clone, Default, Debug)]
pub struct Port {
    pub id: i64,
    pub link: String,
    pub speed: Option<f64>,
    pub duplex: Option<String>,
    pub macs: Vec<String>,
}
pub fn ports(list: &Value) -> Vec<Port> {
    arr(list)
        .iter()
        .map(|p| Port {
            id: num(p, "id").unwrap_or(0.0) as i64,
            link: st(p, "link").unwrap_or_default(),
            speed: num(p, "speed"),
            duplex: st(p, "duplex"),
            macs: arr(p.get("mac_list").unwrap_or(&Value::Null)).iter().filter_map(|m| st(m, "mac")).map(|m| norm_mac(&m)).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn parses_host_with_access_point() {
        let v = json!([{ "id": "ether-aa", "primary_name": "PS5", "host_type": "vg_console", "l2ident": {"id": "AA:BB:CC:00:11:22", "type": "mac_address"},
            "active": true, "l3connectivities": [{"addr": "192.168.1.20", "af": "ipv4", "active": true}],
            "access_point": {"connectivity_type": "wifi", "wifi_information": {"band": "5g", "signal": -71, "ssid": "Freebox-X", "standard": "ax", "phy_tx_rate": 864}}}]);
        let h = &hosts(&v)[0];
        assert_eq!(h.mac, "aa:bb:cc:00:11:22");
        assert_eq!(h.band.as_deref(), Some("5"));
        assert_eq!(h.signal, Some(-71.0));
        assert_eq!(h.standard.as_deref(), Some("Wi-Fi 6"));
        assert_eq!(h.ip.as_deref(), Some("192.168.1.20"));
    }
    #[test]
    fn signal_attenuation_becomes_dbm() {
        assert_eq!(dbm(Some(65.0)), Some(-65.0));
        assert_eq!(dbm(Some(-48.0)), Some(-48.0));
        assert_eq!(dbm(Some(0.0)), None);
    }
    #[test]
    fn wan_units() {
        let w = wan(&json!({"rate_down": 12_500_000, "bandwidth_down": 1_000_000_000, "state": "up"}));
        assert!((w.down_mbps - 100.0).abs() < 1e-9);
        assert!((w.bw_down_mbps - 1000.0).abs() < 1e-9);
    }
}
