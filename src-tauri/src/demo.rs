//! Freebox simulée : renvoie des réponses au format de l'API Freebox OS,
//! avec un foyer réaliste (streaming le soir, téléchargements de console, sauvegarde nocturne…).
//! Sert au mode démo et aux tests : tout le reste de l'application ne voit pas la différence.

use crate::store::{now, Store};
use chrono::{Local, TimeZone, Timelike};
use serde_json::{json, Value};

#[derive(Clone, Copy, PartialEq)]
enum Link {
    W24,
    W5,
    Eth(i64, i64), // port, vitesse
}

struct Dev {
    mac: &'static str,
    name: &'static str,
    kind: &'static str,
    vendor: &'static str,
    link: Link,
    signal: f64,
    phy: f64,
    std: &'static str,
    new: bool,
}

const DEVS: &[Dev] = &[
    Dev { mac: "a8:23:fe:10:00:01", name: "TV Salon", kind: "television", vendor: "LG Electronics", link: Link::W5, signal: -58.0, phy: 480.0, std: "ac", new: false },
    Dev { mac: "f8:46:1c:10:00:02", name: "PS5", kind: "vg_console", vendor: "Sony Interactive", link: Link::W5, signal: -71.0, phy: 290.0, std: "ax", new: false },
    Dev { mac: "d4:81:d7:10:00:03", name: "PC bureau", kind: "workstation", vendor: "Dell", link: Link::Eth(1, 100), signal: 0.0, phy: 0.0, std: "", new: false },
    Dev { mac: "3c:22:fb:10:00:04", name: "MacBook de Léa", kind: "laptop", vendor: "Apple", link: Link::W5, signal: -52.0, phy: 866.0, std: "ax", new: false },
    Dev { mac: "3c:22:fb:10:00:05", name: "iPhone de Léa", kind: "smartphone", vendor: "Apple", link: Link::W5, signal: -61.0, phy: 573.0, std: "ax", new: false },
    Dev { mac: "58:24:29:10:00:06", name: "Pixel de Thomas", kind: "smartphone", vendor: "Google", link: Link::W24, signal: -63.0, phy: 72.0, std: "n", new: false },
    Dev { mac: "3c:22:fb:10:00:07", name: "iPad", kind: "tablet", vendor: "Apple", link: Link::W5, signal: -66.0, phy: 351.0, std: "ac", new: false },
    Dev { mac: "78:8b:2a:10:00:08", name: "Caméra jardin", kind: "ip_camera", vendor: "Xiaomi", link: Link::W24, signal: -82.0, phy: 6.5, std: "n", new: false },
    Dev { mac: "00:11:32:10:00:09", name: "NAS Synology", kind: "nas", vendor: "Synology", link: Link::Eth(2, 1000), signal: 0.0, phy: 0.0, std: "", new: false },
    Dev { mac: "34:27:92:10:00:0a", name: "Freebox Player", kind: "freebox_player", vendor: "Freebox SAS", link: Link::Eth(3, 1000), signal: 0.0, phy: 0.0, std: "", new: false },
    Dev { mac: "5c:aa:fd:10:00:0b", name: "Enceinte Sonos", kind: "multimedia_device", vendor: "Sonos", link: Link::W24, signal: -55.0, phy: 65.0, std: "n", new: false },
    Dev { mac: "ec:b5:fa:10:00:0c", name: "Pont Hue", kind: "networking_device", vendor: "Philips Lighting", link: Link::Eth(4, 1000), signal: 0.0, phy: 0.0, std: "", new: false },
    Dev { mac: "98:4b:e1:10:00:0d", name: "Switch", kind: "vg_console", vendor: "Nintendo", link: Link::W24, signal: -70.0, phy: 39.0, std: "n", new: false },
    Dev { mac: "24:0a:c4:10:00:0e", name: "ESP_3F21A0", kind: "other", vendor: "Espressif", link: Link::W24, signal: -74.0, phy: 24.0, std: "n", new: true },
];

/// Hachage déterministe pour des « événements » stables dans le temps (ex. une mise à jour de jeu).
fn h(a: u64, b: u64) -> f64 {
    let mut x = a.wrapping_mul(0x9E3779B97F4A7C15) ^ b.wrapping_mul(0xC2B2AE3D27D4EB4F);
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58476D1CE4E5B9);
    x ^= x >> 29;
    (x % 10_000) as f64 / 10_000.0
}

/// Débit (Mb/s) vers / depuis l'appareil i à l'instant t ; `internet` = part qui passe par la ligne.
fn rates(i: usize, t: i64) -> (f64, f64) {
    let lt = Local.timestamp_opt(t, 0).single().unwrap_or_else(Local::now);
    let hr = lt.hour() as f64 + lt.minute() as f64 / 60.0;
    let wd = chrono::Datelike::weekday(&lt).num_days_from_monday();
    let weekend = wd >= 5;
    let n = 0.85 + 0.3 * h(t as u64 / 7, i as u64);
    let between = |a: f64, b: f64| hr >= a && hr < b;
    let slot = (t / 600) as u64; // fenêtres de 10 min
    let r = match DEVS[i].name {
        "TV Salon" if between(19.5, 23.5) || (weekend && between(14.0, 17.0)) => (24.0 * n, 0.4),
        "PS5" => {
            if h(slot, 77) > 0.99 && between(8.0, 24.0) {
                (620.0 + 240.0 * h(slot, 5), 4.0)
            } else if between(20.0, 23.0) && h(slot, 9) > 0.4 {
                (4.0 * n, 1.5)
            } else {
                (0.05, 0.02)
            }
        }
        "PC bureau" if !weekend && between(9.0, 18.0) => {
            if h(slot, 3) > 0.6 { (4.0 * n, 3.2 * n) } else { (2.0 * n, 0.4) }
        }
        "MacBook de Léa" if between(18.0, 23.0) || (weekend && between(10.0, 18.0)) => (if h(slot, 11) > 0.85 { 180.0 * n } else { 8.0 * n }, 1.2 * n),
        "iPhone de Léa" if between(7.0, 23.5) => (if h(slot, 13) > 0.7 { 12.0 * n } else { 1.5 * n }, 0.5),
        "Pixel de Thomas" if between(7.0, 24.0) => (if h(slot, 17) > 0.7 { 9.0 * n } else { 1.0 * n }, 0.4),
        "iPad" if between(17.0, 21.0) => (6.0 * n, 0.3),
        "Caméra jardin" => (0.05, 2.4 * n),
        "NAS Synology" if between(2.0, 4.0) => (2.0, 160.0 * n),
        "Freebox Player" if between(19.0, 23.5) => (14.0 * n, 0.3),
        "Enceinte Sonos" if between(8.0, 10.0) || between(18.0, 20.0) => (0.35, 0.05),
        "Switch" if weekend && between(15.0, 19.0) => (3.0 * n, 1.0),
        "ESP_3F21A0" => (0.02, 0.15 * n),
        "Pont Hue" => (0.01, 0.01),
        _ => (0.01 * n, 0.01),
    };
    r
}

pub struct Demo {
    last: i64,
    to_dev: Vec<f64>,
    from_dev: Vec<f64>,
    wan_down_bytes: f64,
    wan_up_bytes: f64,
    started: i64,
}

pub const BW_DOWN: f64 = 1000.0;
pub const BW_UP: f64 = 600.0;

impl Demo {
    pub fn new() -> Self {
        Self { last: now(), to_dev: vec![3e9; DEVS.len()], from_dev: vec![4e8; DEVS.len()], wan_down_bytes: 8.1e11, wan_up_bytes: 9.4e10, started: now() - 86400 * 9 }
    }
    fn wan_now(t: i64) -> (f64, f64) {
        let (mut d, mut u) = (0.0, 0.0);
        for i in 0..DEVS.len() {
            let (a, b) = rates(i, t);
            d += a;
            u += b;
        }
        (d.min(BW_DOWN * 0.97), u.min(BW_UP * 0.97))
    }
    fn advance(&mut self) {
        let t = now();
        let dt = (t - self.last) as f64;
        if dt <= 0.0 {
            return;
        }
        for i in 0..DEVS.len() {
            let (d, u) = rates(i, t);
            self.to_dev[i] += d * 1e6 / 8.0 * dt;
            self.from_dev[i] += u * 1e6 / 8.0 * dt;
        }
        let (d, u) = Self::wan_now(t);
        self.wan_down_bytes += d * 1e6 / 8.0 * dt;
        self.wan_up_bytes += u * 1e6 / 8.0 * dt;
        self.last = t;
    }

    pub fn api_version() -> Value {
        json!({ "uid": "demo0000000000000000000000000000", "device_name": "Freebox Server", "api_version": "14.0", "api_base_url": "/api/",
                "device_type": "FreeboxServer8,1", "box_model": "fbxgw8-r1", "box_model_name": "Freebox Pop (démo)", "https_available": true })
    }

    pub fn get(&mut self, path: &str) -> anyhow::Result<Value> {
        self.advance();
        let t = now();
        let p = path.trim_matches('/');
        let jitter = |k: u64| 0.9 + 0.2 * h(t as u64, k);
        Ok(match p {
            "connection" => {
                let (d, u) = Self::wan_now(t);
                json!({ "type": "ethernet", "media": "ftth", "state": "up", "ipv4": "82.64.0.42",
                        "rate_down": (d * jitter(1) * 1e6 / 8.0) as i64, "rate_up": (u * jitter(2) * 1e6 / 8.0) as i64,
                        "bandwidth_down": (BW_DOWN * 1e6) as i64, "bandwidth_up": (BW_UP * 1e6) as i64,
                        "bytes_down": self.wan_down_bytes as i64, "bytes_up": self.wan_up_bytes as i64 })
            }
            "connection/ftth" => json!({ "sfp_present": true, "sfp_alim_ok": true, "link": true, "sfp_has_signal": true, "sfp_pwr_rx": -2140, "sfp_pwr_tx": 212, "sfp_model": "demo-gpon" }),
            "system" => json!({ "firmware_version": "4.9.14", "mac": "34:27:92:00:00:01", "uptime_val": t - self.started, "board_name": "fbxgw8r",
                                "model_info": { "pretty_name": "Freebox Pop (démo)", "name": "fbxgw8-r1", "wifi_type": "2d4_5g" },
                                "sensors": [ { "id": "temp_cpu0", "name": "Température CPU", "value": 61 }, { "id": "temp_t1", "name": "Température carte", "value": 49 } ],
                                "fans": [] }),
            "lan/browser/interfaces" => json!([{ "name": "pub", "host_count": DEVS.len() }]),
            "lan/browser/pub" => Value::Array(
                DEVS.iter()
                    .enumerate()
                    .map(|(i, d)| {
                        let active = rates(i, t).0 + rates(i, t).1 > 0.05 || matches!(d.link, Link::Eth(..)) || d.kind == "smartphone";
                        let ip = format!("192.168.1.{}", 20 + i);
                        let mut v = json!({ "id": format!("ether-{}", d.mac), "primary_name": d.name, "host_type": d.kind, "primary_name_manual": true,
                            "vendor_name": d.vendor, "persistent": true, "reachable": true, "active": active,
                            "last_activity": if active { t - 3 } else { t - 3600 }, "last_time_reachable": t - 2,
                            "l2ident": { "id": d.mac.to_uppercase(), "type": "mac_address" },
                            "l3connectivities": [ { "addr": ip, "af": "ipv4", "active": true, "reachable": true, "last_activity": t - 3 } ] });
                        let ap = match d.link {
                            Link::Eth(_, sp) => json!({ "connectivity_type": "ethernet", "type": "gateway", "ethernet_information": { "speed": sp, "duplex": "full", "link": "up" } }),
                            Link::W24 | Link::W5 => {
                                let band = if d.link == Link::W24 { "2d4g" } else { "5g" };
                                json!({ "connectivity_type": "wifi", "type": "gateway", "wifi_information": { "band": band, "ssid": "Freebox-Demo", "signal": d.signal + (h(t as u64 / 5, i as u64) * 4.0 - 2.0).round(), "standard": d.std, "phy_tx_rate": d.phy, "phy_rx_rate": (d.phy * 0.8).round() } })
                            }
                        };
                        v["access_point"] = ap;
                        v
                    })
                    .collect(),
            ),
            "wifi/ap" => json!([
                { "id": 0, "name": "2.4G", "config": { "band": "2d4g", "channel_width": "40", "primary_channel": 0, "secondary_channel": 0 },
                  "status": { "state": "active", "channel_width": "40", "primary_channel": 6, "secondary_channel": 10 } },
                { "id": 1, "name": "5G", "config": { "band": "5g", "channel_width": "80", "primary_channel": 0, "secondary_channel": 0 },
                  "status": { "state": "active", "channel_width": "80", "primary_channel": 36, "secondary_channel": 0 } }
            ]),
            "wifi/bss" => json!([{ "id": "34:27:92:00:00:02", "phy_id": 0, "config": { "ssid": "Freebox-Demo", "enabled": true }, "status": { "state": "active", "sta_count": 6 } }]),
            p if p.starts_with("wifi/ap/") && p.ends_with("/stations") => {
                let ap: i64 = p.split('/').nth(2).and_then(|x| x.parse().ok()).unwrap_or(0);
                let want = if ap == 0 { Link::W24 } else { Link::W5 };
                Value::Array(
                    DEVS.iter()
                        .enumerate()
                        .filter(|(_, d)| d.link == want)
                        .map(|(i, d)| {
                            let (dn, up) = rates(i, t);
                            json!({ "id": d.mac, "mac": d.mac.to_uppercase(), "bssid": "34:27:92:00:00:02", "hostname": d.name, "state": "authorized",
                                    "inactive": 1, "conn_duration": 3600 + i * 917, "signal": d.signal + (h(t as u64 / 5, i as u64) * 4.0 - 2.0).round(),
                                    "tx_bytes": self.to_dev[i] as i64, "rx_bytes": self.from_dev[i] as i64,
                                    "tx_rate": (dn * 1e6 / 8.0) as i64, "rx_rate": (up * 1e6 / 8.0) as i64,
                                    "last_tx": { "bitrate": (d.phy * 10.0) as i64 }, "last_rx": { "bitrate": (d.phy * 8.0) as i64 },
                                    "flags": { "legacy": d.std.is_empty(), "ht": true, "vht": d.std == "ac" || d.std == "ax", "he": d.std == "ax", "authorized": true },
                                    "host": { "id": format!("ether-{}", d.mac), "primary_name": d.name } })
                        })
                        .collect(),
                )
            }
            p if p.starts_with("wifi/ap/") && p.ends_with("/neighbors") => {
                let ap: i64 = p.split('/').nth(2).and_then(|x| x.parse().ok()).unwrap_or(0);
                if ap == 0 {
                    let n: [(&str, i64, i64); 12] = [("Livebox-7A20", 1, -67), ("Livebox-1C3F", 6, -58), ("SFR_88E0", 6, -64), ("Bbox-41B2C3", 11, -73), ("Freebox-5B9A21", 6, -61),
                             ("DIRECT-HP-Print", 7, -70), ("Livebox-E5D1", 11, -79), ("SFR WiFi FON", 1, -81), ("Bbox-77AA12", 3, -69), ("iPhone de Marc", 6, -76),
                             ("Livebox-9F3C", 9, -72), ("TP-Link_2.4", 13, -84)];
                    Value::Array(n.iter().map(|(s, c, g)| json!({ "ssid": s, "bssid": format!("02:00:00:00:{:02x}:{:02x}", c, g.abs()), "band": "2d4g", "channel": c, "channel_width": 20, "signal": g })).collect())
                } else {
                    let n: [(&str, i64, i64); 6] = [("Livebox-1C3F", 36, -66), ("SFR_88E0_5GHZ", 40, -71), ("Freebox-5B9A21", 44, -69), ("Bbox-41B2C3", 100, -80), ("Livebox-7A20", 36, -74), ("Livebox-9F3C", 112, -83)];
                    Value::Array(n.iter().map(|(s, c, g)| json!({ "ssid": s, "bssid": format!("02:00:00:01:{:02x}:{:02x}", c, g.abs()), "band": "5g", "channel": c, "channel_width": 80, "signal": g })).collect())
                }
            }
            p if p.starts_with("wifi/ap/") && p.ends_with("/channel_usage") => {
                let ap: i64 = p.split('/').nth(2).and_then(|x| x.parse().ok()).unwrap_or(0);
                if ap == 0 {
                    Value::Array((1..=13).map(|c| {
                        let base: f64 = match c { 6 | 7 => 52.0, 5 | 8 => 40.0, 1 | 11 => 28.0, 2 | 10 => 22.0, _ => 14.0 };
                        json!({ "channel": c, "band": "2d4g", "rx_busy_percent": (base * jitter(c as u64)).round(), "tx_percent": if c == 6 { 9 } else { 0 }, "noise_level": -92 })
                    }).collect())
                } else {
                    Value::Array([36, 40, 44, 48, 52, 56, 60, 64, 100, 104, 108, 112, 116, 120, 124, 128, 132, 136, 140].iter().map(|&c| {
                        let base: f64 = match c { 36 => 24.0, 40 | 44 => 18.0, 48 => 10.0, 100 | 112 => 6.0, _ => 2.0 };
                        json!({ "channel": c, "band": "5g", "rx_busy_percent": (base * jitter(c as u64)).round(), "tx_percent": if c == 36 { 6 } else { 0 }, "noise_level": -95 })
                    }).collect())
                }
            }
            "switch/status" => Value::Array((1..=4).map(|port| {
                let macs: Vec<Value> = DEVS.iter().filter(|d| matches!(d.link, Link::Eth(p, _) if p == port)).map(|d| json!({ "mac": d.mac.to_uppercase(), "hostname": d.name })).collect();
                let speed = DEVS.iter().find_map(|d| match d.link { Link::Eth(p, s) if p == port => Some(s), _ => None });
                json!({ "id": port, "link": if speed.is_some() { "up" } else { "down" }, "speed": speed.map(|s| s.to_string()).unwrap_or_default(), "duplex": "full", "mac_list": macs })
            }).collect()),
            p if p.starts_with("switch/port/") && p.ends_with("/stats") => {
                let port: i64 = p.split('/').nth(2).and_then(|x| x.parse().ok()).unwrap_or(0);
                let (mut d, mut u) = (0.0, 0.0);
                for (i, dv) in DEVS.iter().enumerate() {
                    if matches!(dv.link, Link::Eth(p, _) if p == port) {
                        let r = rates(i, t);
                        d += r.0;
                        u += r.1;
                    }
                }
                json!({ "tx_bytes_rate": (d * 1e6 / 8.0) as i64, "rx_bytes_rate": (u * 1e6 / 8.0) as i64 })
            }
            "repeater" => json!([]),
            other => anyhow::bail!("not_found: chemin inconnu en démo ({other})"),
        })
    }

    /// Remplit la base avec `days` jours d'historique simulé.
    pub fn seed(store: &mut Store, days: i64) -> anyhow::Result<()> {
        let end = now() / 60 * 60;
        let start = end - days * 86400;
        let t0 = start - 86400 * 14;
        for (i, d) in DEVS.iter().enumerate() {
            let first = if d.new { end - 3600 * 30 } else { t0 + i as i64 * 3600 };
            store.db.execute("INSERT OR REPLACE INTO hosts VALUES(?1,?2,?3,?4,?5,?6)", rusqlite::params![d.mac, d.name, d.vendor, d.kind, first, end])?;
        }
        store.db.execute("INSERT INTO events VALUES(?1,'new_device',?2,?3)", rusqlite::params![end - 3600 * 30, DEVS[13].mac, "ESP_3F21A0 (Espressif) a rejoint le réseau"])?;
        let tx = store.db.transaction()?;
        {
            let mut wq = tx.prepare("INSERT OR REPLACE INTO wan_min VALUES(?1,?2,?3,?4,?5,?6,?7)")?;
            let mut hq = tx.prepare("INSERT OR REPLACE INTO host_min VALUES(?1,?2,?3,?4,?5,?6)")?;
            let mut ts = start;
            while ts < end {
                let mut dsum = 0.0;
                let mut usum = 0.0;
                let mut dmax: f64 = 0.0;
                let mut umax: f64 = 0.0;
                for (i, d) in DEVS.iter().enumerate() {
                    let (a, b) = rates(i, ts + 20);
                    let (a2, b2) = rates(i, ts + 50);
                    let (da, ua) = ((a + a2) / 2.0, (b + b2) / 2.0);
                    dsum += da;
                    usum += ua;
                    dmax = dmax.max(a.max(a2));
                    umax = umax.max(b.max(b2));
                    if da + ua > 0.03 {
                        hq.execute(rusqlite::params![ts, d.mac, da * 1e6 / 8.0 * 60.0, ua * 1e6 / 8.0 * 60.0, a.max(a2), b.max(b2)])?;
                    }
                }
                let dm = (dsum * 1.12).max(dmax).min(BW_DOWN * 0.97);
                let um = (usum * 1.12).max(umax).min(BW_UP * 0.97);
                wq.execute(rusqlite::params![ts, dsum.min(BW_DOWN), dm, usum.min(BW_UP), um, BW_DOWN, BW_UP])?;
                ts += 60;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
