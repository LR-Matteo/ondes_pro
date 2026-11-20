//! Cœur de l'application : configuration, source de données (Freebox ou démo), collecte continue,
//! historique, et une seule fonction `dispatch` appelée par l'interface (Tauri ou pont de dev).

use crate::demo::Demo;
use crate::freebox::{self, api_code, BoxInfo};
use crate::normalize::{self as n, Ap, Host, Neighbor, Port, Station};
use crate::store::{now, HostMinute, Store, WanMinute};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub base_url: String,
    pub interval: u64,
    pub close_to_tray: bool,
    pub retention_days: i64,
    pub demo: bool,
    pub paired: bool,
    pub token_fallback: Option<String>,
    pub box_uid: Option<String>,
    pub box_name: Option<String>,
    pub subscribed_down: Option<f64>,
    pub subscribed_up: Option<f64>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "http://mafreebox.freebox.fr".into(),
            interval: 5,
            close_to_tray: true,
            retention_days: 90,
            demo: false,
            paired: false,
            token_fallback: None,
            box_uid: None,
            box_name: None,
            subscribed_down: None,
            subscribed_up: None,
        }
    }
}

enum Source {
    Box(freebox::Client),
    Demo(Demo),
}
impl Source {
    async fn get(&mut self, p: &str) -> Result<Value> {
        match self {
            Source::Box(c) => c.get(p).await,
            Source::Demo(d) => d.get(p),
        }
    }
    async fn post(&mut self, p: &str, body: &Value) -> Result<Value> {
        match self {
            Source::Box(c) => c.post(p, body).await,
            Source::Demo(_) => Ok(Value::Null),
        }
    }
    fn is_demo(&self) -> bool {
        matches!(self, Source::Demo(_))
    }
}

#[derive(Default)]
struct Live {
    wan: n::Wan,
    wan_series: VecDeque<(i64, f64, f64)>,
    host_series: HashMap<String, VecDeque<(i64, f64, f64)>>,
    counters: HashMap<String, (f64, f64, f64)>,
    hosts: Vec<Host>,
    aps: Vec<Ap>,
    ports: Vec<Port>,
    port_rates: HashMap<i64, (f64, f64)>,
    neighbors: Vec<Neighbor>,
    neighbors_ts: i64,
    system: Value,
    ftth: Value,
    last_slow: i64,
    last_poll: f64,
    minute: i64,
    acc_wan: Vec<(f64, f64)>,
    acc_bw: (f64, f64),
    acc_hosts: HashMap<String, (f64, f64, f64, f64)>,
    today: HashMap<String, (f64, f64)>,
    errors: BTreeMap<String, String>,
    connected: bool,
    last_error: Option<String>,
    last_ok: i64,
    last_purge: i64,
    box_info: Value,
    permissions: Value,
}

struct Pairing {
    info: BoxInfo,
    token: String,
    track: i64,
}

struct Inner {
    dir: PathBuf,
    cfg: Mutex<Config>,
    live: Mutex<Live>,
    src: tokio::sync::Mutex<Option<Source>>,
    store: Mutex<Option<Store>>,
    raw: Mutex<BTreeMap<String, Value>>,
    pairing: Mutex<Option<Pairing>>,
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    http: reqwest::Client,
}

#[derive(Clone)]
pub struct Engine(Arc<Inner>);

const KEYRING_SERVICE: &str = "Ondes";

fn monotonic() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}
fn device_name() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "PC".into())
}
fn arg_i(a: &Value, k: &str, d: i64) -> i64 {
    a.get(k).and_then(|v| v.as_i64()).unwrap_or(d)
}

impl Engine {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let cfg: Config = std::fs::read_to_string(dir.join("config.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Engine(Arc::new(Inner {
            dir,
            cfg: Mutex::new(cfg),
            live: Mutex::new(Live::default()),
            src: tokio::sync::Mutex::new(None),
            store: Mutex::new(None),
            raw: Mutex::new(BTreeMap::new()),
            pairing: Mutex::new(None),
            task: Mutex::new(None),
            http: freebox::http_client(),
        }))
    }

    pub fn cfg(&self) -> Config {
        self.0.cfg.lock().unwrap().clone()
    }
    fn save_cfg(&self) {
        let c = self.cfg();
        let _ = std::fs::write(self.0.dir.join("config.json"), serde_json::to_string_pretty(&c).unwrap_or_default());
    }
    fn with_store<T>(&self, f: impl FnOnce(&mut Store) -> Result<T>) -> Result<T> {
        let mut g = self.0.store.lock().unwrap();
        let s = g.as_mut().ok_or_else(|| anyhow!("no_store: historique indisponible"))?;
        f(s)
    }

    // ---------- secret ----------
    fn token_key(&self) -> String {
        format!("freebox-app-token-{}", self.cfg().box_uid.unwrap_or_else(|| "default".into()))
    }
    fn save_token(&self, token: &str) {
        let ok = keyring::Entry::new(KEYRING_SERVICE, &self.token_key()).and_then(|e| e.set_password(token)).is_ok();
        let mut c = self.0.cfg.lock().unwrap();
        // Repli si le coffre de Windows est indisponible : stocké dans la configuration locale.
        c.token_fallback = if ok { None } else { Some(token.to_string()) };
    }
    fn load_token(&self) -> Option<String> {
        keyring::Entry::new(KEYRING_SERVICE, &self.token_key()).and_then(|e| e.get_password()).ok().or(self.cfg().token_fallback)
    }
    fn delete_token(&self) {
        if let Ok(e) = keyring::Entry::new(KEYRING_SERVICE, &self.token_key()) {
            let _ = e.delete_credential();
        }
        self.0.cfg.lock().unwrap().token_fallback = None;
    }

    // ---------- démarrage ----------
    pub fn start(&self) {
        let c = self.cfg();
        if c.demo {
            self.open_store(true);
        } else if c.paired {
            self.open_store(false);
        }
        self.spawn_collector();
    }

    fn open_store(&self, demo: bool) {
        let path = self.0.dir.join(if demo { "ondes-demo.db" } else { "ondes.db" });
        if demo {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(path.with_extension("db-wal"));
            let _ = std::fs::remove_file(path.with_extension("db-shm"));
        }
        match Store::open(&path) {
            Ok(mut s) => {
                if demo {
                    let _ = Demo::seed(&mut s, 8);
                }
                let _ = s.purge(self.cfg().retention_days);
                if let Ok(td) = s.today_bytes() {
                    self.0.live.lock().unwrap().today = td;
                }
                *self.0.store.lock().unwrap() = Some(s);
            }
            Err(e) => self.0.live.lock().unwrap().last_error = Some(format!("no_store: {e}")),
        }
    }

    fn spawn_collector(&self) {
        let me = self.clone();
        let h = tauri::async_runtime::spawn(async move {
            let mut tick: u64 = 0;
            loop {
                let c = me.cfg();
                if c.demo || c.paired {
                    if let Err(e) = me.ensure_source().await {
                        let mut l = me.0.live.lock().unwrap();
                        l.connected = false;
                        l.last_error = Some(e.to_string());
                    } else if let Err(e) = me.poll(tick).await {
                        let code = api_code(&e).map(|s| s.to_string());
                        {
                            let mut l = me.0.live.lock().unwrap();
                            l.connected = false;
                            l.last_error = Some(e.to_string());
                        }
                        if matches!(code.as_deref(), Some("invalid_token")) {
                            // Jeton révoqué depuis Freebox OS : il faut réassocier.
                            me.0.cfg.lock().unwrap().paired = false;
                            me.save_cfg();
                        }
                        *me.0.src.lock().await = None; // reconnexion au prochain tour
                    }
                    tick += 1;
                }
                tokio::time::sleep(Duration::from_secs(c.interval.clamp(2, 60))).await;
            }
        });
        if let Some(old) = self.0.task.lock().unwrap().replace(h) {
            old.abort();
        }
    }

    async fn ensure_source(&self) -> Result<()> {
        let mut g = self.0.src.lock().await;
        if g.is_some() {
            return Ok(());
        }
        let c = self.cfg();
        if c.demo {
            *g = Some(Source::Demo(Demo::new()));
            let mut l = self.0.live.lock().unwrap();
            l.box_info = Demo::api_version();
            l.permissions = json!({"settings": true, "explorer": true});
            return Ok(());
        }
        let token = self.load_token().ok_or_else(|| anyhow!("no_token: aucune autorisation enregistrée, il faut associer la Freebox"))?;
        let info = freebox::discover(&self.0.http, &c.base_url).await?;
        let mut client = freebox::Client::new(self.0.http.clone(), info.clone(), token);
        client.login().await?;
        {
            let mut l = self.0.live.lock().unwrap();
            l.box_info = info.raw.clone();
            l.permissions = client.permissions.clone();
        }
        *g = Some(Source::Box(client));
        Ok(())
    }

    // ---------- collecte ----------
    async fn poll(&self, tick: u64) -> Result<()> {
        let t = now();
        let mut errors: BTreeMap<String, String> = BTreeMap::new();
        let mut raw: BTreeMap<String, Value> = BTreeMap::new();
        let (slow, need_neighbors, prev_aps, prev_ports) = {
            let l = self.0.live.lock().unwrap();
            (t - l.last_slow >= 60 || l.aps.is_empty(), t - l.neighbors_ts >= 900, l.aps.clone(), l.ports.clone())
        };
        let mut g = self.0.src.lock().await;
        let src = g.as_mut().ok_or_else(|| anyhow!("no_source"))?;
        let demo = src.is_demo();

        macro_rules! fetch {
            ($path:expr) => {{
                let p: String = $path;
                match src.get(&p).await {
                    Ok(v) => {
                        raw.insert(p.clone(), v.clone());
                        Some(v)
                    }
                    Err(e) => {
                        errors.insert(p.clone(), e.to_string());
                        None
                    }
                }
            }};
        }

        let conn = src.get("connection/").await?; // indispensable : une erreur ici coupe la collecte
        raw.insert("connection/".into(), conn.clone());

        let mut aps = prev_aps;
        let mut system = None;
        let mut ftth = None;
        let mut ports = prev_ports;
        if slow {
            if let Some(v) = fetch!("wifi/ap/".to_string()) {
                aps = n::aps(&v);
                for ap in aps.iter_mut() {
                    if let Some(u) = fetch!(format!("wifi/ap/{}/channel_usage/", ap.id)) {
                        ap.usage = n::channel_usage(&u, &ap.band);
                    }
                }
            }
            system = fetch!("system/".to_string());
            if n::st(&conn, "media").as_deref() == Some("ftth") {
                ftth = fetch!("connection/ftth/".to_string());
            }
            if let Some(v) = fetch!("switch/status/".to_string()) {
                ports = n::ports(&v);
            }
        }
        let hosts_raw = fetch!("lan/browser/pub/".to_string());
        let _ = tick;
        let mut stations: Vec<Station> = vec![];
        for ap in aps.iter() {
            if let Some(v) = fetch!(format!("wifi/ap/{}/stations/", ap.id)) {
                stations.extend(n::stations(&v, ap));
            }
        }
        let mut port_rates = HashMap::new();
        for p in ports.iter().filter(|p| p.link == "up" && !p.macs.is_empty()) {
            if let Some(v) = fetch!(format!("switch/port/{}/stats/", p.id)) {
                if let (Some(tx), Some(rx)) = (n::num(&v, "tx_bytes_rate"), n::num(&v, "rx_bytes_rate")) {
                    port_rates.insert(p.id, (tx, rx));
                }
            }
        }
        let mut neighbors = None;
        if need_neighbors {
            let mut all = vec![];
            for ap in aps.iter() {
                if let Some(v) = fetch!(format!("wifi/ap/{}/neighbors/", ap.id)) {
                    all.extend(n::neighbors(&v, &ap.band));
                }
            }
            neighbors = Some(all);
        }
        drop(g);

        // ----- fusion et calculs -----
        let mut raw_map = self.0.raw.lock().unwrap();
        raw_map.extend(raw);
        drop(raw_map);

        let mut flush: Option<(WanMinute, Vec<HostMinute>, Vec<(String, String, Option<String>, String)>)> = None;
        {
            let mut l = self.0.live.lock().unwrap();
            let tf = monotonic();
            let dt = if l.last_poll > 0.0 { (tf - l.last_poll).min(120.0) } else { 0.0 };
            l.last_poll = tf;
            l.wan = n::wan(&conn);
            let (wd, wu) = (l.wan.down_mbps, l.wan.up_mbps);
            l.wan_series.push_back((t, wd, wu));
            while l.wan_series.front().map(|x| x.0 < t - 900).unwrap_or(false) {
                l.wan_series.pop_front();
            }
            if slow {
                l.last_slow = t;
                l.aps = aps.clone();
                l.ports = ports.clone();
            }
            l.port_rates = port_rates;
            if let Some(s) = system {
                l.system = s;
            }
            if let Some(f) = ftth {
                l.ftth = f;
            }
            if let Some(nb) = neighbors {
                l.neighbors = nb;
                l.neighbors_ts = t;
            }
            if let Some(h) = hosts_raw {
                let fresh = n::hosts(&h);
                // On garde les débits calculés du tour précédent si le navigateur LAN n'a pas été relu.
                l.hosts = fresh;
            }
            // Wi-Fi : on enrichit les appareils avec les stations
            let st_by_mac: HashMap<String, &Station> = stations.iter().map(|s| (s.mac.clone(), s)).collect();
            let ports_c = l.ports.clone();
            let port_rates_c = l.port_rates.clone();
            let mut counters = std::mem::take(&mut l.counters);
            let mut acc = std::mem::take(&mut l.acc_hosts);
            let today = l.today.clone();
            for h in l.hosts.iter_mut() {
                if let Some(s) = st_by_mac.get(&h.mac) {
                    h.conn = "wifi".into();
                    h.band = Some(s.band.clone());
                    h.ap_id = Some(s.ap_id);
                    h.signal = s.signal.or(h.signal);
                    h.phy_down = s.phy_down.or(h.phy_down);
                    h.phy_up = s.phy_up.or(h.phy_up);
                    h.standard = s.standard.clone().or(h.standard.clone());
                    h.ssid = s.ssid.clone().or(h.ssid.clone());
                    h.conn_duration = s.conn_duration;
                }
                let mut port_single = None;
                for p in ports_c.iter() {
                    if p.macs.contains(&h.mac) {
                        h.conn = "ethernet".into();
                        h.port = Some(p.id);
                        if p.speed.is_some() {
                            h.eth_speed = p.speed;
                        }
                        if p.macs.len() == 1 {
                            port_single = port_rates_c.get(&p.id).copied();
                        }
                    }
                }
                // Débits : compteurs d'octets (le plus fiable), sinon débits instantanés.
                let bytes = st_by_mac.get(&h.mac).and_then(|s| s.bytes_to_dev.zip(s.bytes_from_dev)).or(h.ap_bytes);
                let inst = st_by_mac.get(&h.mac).and_then(|s| s.rate_to_dev.zip(s.rate_from_dev)).or(h.ap_rates).or(port_single);
                let mut rate: Option<(f64, f64)> = None;
                if let Some((to, from)) = bytes {
                    if let Some((pt, pto, pfrom)) = counters.get(&h.mac).copied() {
                        let d = tf - pt;
                        if d > 0.5 && to >= pto && from >= pfrom {
                            rate = Some(((to - pto) * 8.0 / 1e6 / d, (from - pfrom) * 8.0 / 1e6 / d));
                        }
                    }
                    counters.insert(h.mac.clone(), (tf, to, from));
                }
                if rate.is_none() {
                    rate = inst.map(|(a, b)| (a * 8.0 / 1e6, b * 8.0 / 1e6));
                }
                if let Some((d, u)) = rate {
                    h.down_mbps = Some(d);
                    h.up_mbps = Some(u);
                    if dt > 0.0 {
                        let e = acc.entry(h.mac.clone()).or_insert((0.0, 0.0, 0.0, 0.0));
                        e.0 += d * 1e6 / 8.0 * dt;
                        e.1 += u * 1e6 / 8.0 * dt;
                        e.2 = e.2.max(d);
                        e.3 = e.3.max(u);
                    }
                }
                let cur = acc.get(&h.mac).map(|e| (e.0, e.1)).unwrap_or((0.0, 0.0));
                let td = today.get(&h.mac).copied().unwrap_or((0.0, 0.0));
                h.today_down = td.0 + cur.0;
                h.today_up = td.1 + cur.1;
            }
            l.counters = counters;
            l.acc_hosts = acc;
            for h in l.hosts.clone().iter() {
                if let (Some(d), Some(u)) = (h.down_mbps, h.up_mbps) {
                    let s = l.host_series.entry(h.mac.clone()).or_default();
                    s.push_back((t, d, u));
                    while s.front().map(|x| x.0 < t - 900).unwrap_or(false) {
                        s.pop_front();
                    }
                }
            }
            // stations par borne
            for ap in l.aps.iter_mut() {
                ap.stations = stations.iter().filter(|s| s.ap_id == ap.id).count();
                ap.busy = ap.usage.iter().find(|u| u.channel == ap.channel).and_then(|u| u.busy);
            }
            l.errors = errors;
            l.connected = true;
            l.last_error = None;
            l.last_ok = t;

            // agrégat par minute
            let minute = t / 60 * 60;
            if l.minute == 0 {
                l.minute = minute;
            }
            if minute != l.minute {
                let samples = std::mem::take(&mut l.acc_wan);
                if !samples.is_empty() {
                    let k = samples.len() as f64;
                    let w = WanMinute {
                        ts: l.minute,
                        down_avg: samples.iter().map(|s| s.0).sum::<f64>() / k,
                        down_max: samples.iter().map(|s| s.0).fold(0.0, f64::max),
                        up_avg: samples.iter().map(|s| s.1).sum::<f64>() / k,
                        up_max: samples.iter().map(|s| s.1).fold(0.0, f64::max),
                        bw_down: l.acc_bw.0,
                        bw_up: l.acc_bw.1,
                    };
                    let hm: Vec<HostMinute> = std::mem::take(&mut l.acc_hosts)
                        .into_iter()
                        .filter(|(_, v)| v.0 + v.1 > 1000.0)
                        .map(|(mac, v)| HostMinute { ts: l.minute, host: mac, down_bytes: v.0, up_bytes: v.1, down_max: v.2, up_max: v.3 })
                        .collect();
                    let dirs = l.hosts.iter().filter(|h| h.active || h.down_mbps.unwrap_or(0.0) > 0.0).map(|h| (h.mac.clone(), h.name.clone(), h.vendor.clone(), h.kind.clone())).collect();
                    flush = Some((w, hm, dirs));
                }
                l.minute = minute;
            }
            l.acc_wan.push((wd, wu));
            l.acc_bw = (l.wan.bw_down_mbps, l.wan.bw_up_mbps);
            if demo {
                l.acc_bw = (crate::demo::BW_DOWN, crate::demo::BW_UP);
            }
        }
        if let Some((w, hm, dirs)) = flush {
            let today = self.with_store(|s| {
                s.write_minute(&w, &hm)?;
                let known = s.host_count();
                for (mac, name, vendor, kind) in dirs.iter() {
                    if s.upsert_host(mac, name, vendor.as_deref(), kind, w.ts)? && known > 0 {
                        let label = match vendor { Some(v) => format!("{name} ({v}) a rejoint le réseau"), None => format!("{name} a rejoint le réseau") };
                        s.event("new_device", mac, &label)?;
                    }
                }
                s.today_bytes()
            });
            if let Ok(td) = today {
                let mut l = self.0.live.lock().unwrap();
                l.today = td;
                let purge = t - l.last_purge > 86400;
                if purge {
                    l.last_purge = t;
                }
                drop(l);
                if purge {
                    let days = self.cfg().retention_days;
                    let _ = self.with_store(|s| s.purge(days));
                }
            }
        }
        Ok(())
    }

    // ---------- vues ----------
    fn live_json(&self) -> Value {
        let l = self.0.live.lock().unwrap();
        let c = self.cfg();
        let first_seen: HashMap<String, Option<i64>> = {
            let g = self.0.store.lock().unwrap();
            match g.as_ref() {
                Some(s) => l.hosts.iter().map(|h| (h.mac.clone(), s.first_seen(&h.mac))).collect(),
                None => HashMap::new(),
            }
        };
        let mut hosts = l.hosts.clone();
        for h in hosts.iter_mut() {
            h.first_seen = first_seen.get(&h.mac).copied().flatten();
        }
        hosts.sort_by(|a, b| {
            (b.down_mbps.unwrap_or(0.0) + b.up_mbps.unwrap_or(0.0)).partial_cmp(&(a.down_mbps.unwrap_or(0.0) + a.up_mbps.unwrap_or(0.0))).unwrap_or(std::cmp::Ordering::Equal).then(b.active.cmp(&a.active))
        });
        let ftth = if l.ftth.is_object() {
            json!({ "rx_dbm": n::num(&l.ftth, "sfp_pwr_rx").map(|x| x / 100.0), "tx_dbm": n::num(&l.ftth, "sfp_pwr_tx").map(|x| x / 100.0),
                    "link": n::boo(&l.ftth, "link"), "signal": n::boo(&l.ftth, "sfp_has_signal") })
        } else {
            Value::Null
        };
        let sys = &l.system;
        let mut temps: Vec<Value> = n::arr(sys.get("sensors").unwrap_or(&Value::Null)).iter().map(|s| json!({"name": n::st(s, "name"), "value": n::num(s, "value")})).collect();
        for (k, label) in [("temp_cpub", "CPU"), ("temp_cpum", "CPU (M)"), ("temp_sw", "Switch")] {
            if let Some(v) = n::num(sys, k) {
                temps.push(json!({"name": label, "value": v}));
            }
        }
        let model = sys.get("model_info").and_then(|m| n::st(m, "pretty_name")).or_else(|| n::st(&l.box_info, "box_model_name")).unwrap_or_else(|| "Freebox".into());
        json!({
            "ts": now(),
            "demo": c.demo,
            "connected": l.connected,
            "last_error": l.last_error,
            "last_ok": l.last_ok,
            "interval": c.interval,
            "box": { "model": model, "firmware": n::st(sys, "firmware_version"), "uptime": n::num(sys, "uptime_val"),
                     "api": n::st(&l.box_info, "api_version"), "temps": temps },
            "permissions": l.permissions,
            "wan": l.wan,
            "wan_series": l.wan_series.iter().map(|x| json!([x.0, x.1, x.2])).collect::<Vec<_>>(),
            "hosts": hosts,
            "aps": l.aps,
            "ports": l.ports,
            "ftth": ftth,
            "errors": l.errors,
            "subscribed": { "down": c.subscribed_down, "up": c.subscribed_up },
        })
    }

    fn status_json(&self) -> Value {
        let c = self.cfg();
        let l = self.0.live.lock().unwrap();
        let since = self.with_store(|s| s.coverage()).unwrap_or(0);
        json!({
            "configured": c.demo || c.paired, "demo": c.demo, "paired": c.paired, "connected": l.connected,
            "last_error": l.last_error, "base_url": c.base_url, "interval": c.interval, "close_to_tray": c.close_to_tray,
            "retention_days": c.retention_days, "subscribed_down": c.subscribed_down, "subscribed_up": c.subscribed_up,
            "box_name": c.box_name, "permissions": l.permissions, "api": n::st(&l.box_info, "api_version"),
            "data_since": since, "token_in_file": c.token_fallback.is_some(), "data_dir": self.0.dir.to_string_lossy(),
            "version": env!("CARGO_PKG_VERSION"),
        })
    }

    async fn reset_source(&self) {
        if let Some(Source::Box(mut c)) = self.0.src.lock().await.take() {
            c.logout().await;
        }
        *self.0.live.lock().unwrap() = Live::default();
        self.0.raw.lock().unwrap().clear();
    }

    // ---------- point d'entrée unique ----------
    pub async fn dispatch(&self, cmd: &str, a: Value) -> Result<Value> {
        match cmd {
            "status" => Ok(self.status_json()),
            "live" => Ok(self.live_json()),
            "discover" => {
                let base = a.get("base").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(self.cfg().base_url);
                let info = freebox::discover(&self.0.http, &base).await?;
                Ok(json!({ "base": info.base, "api": n::st(&info.raw, "api_version"), "model": info.model(), "raw": info.raw }))
            }
            "pair_start" => {
                let base = a.get("base").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(self.cfg().base_url);
                let info = freebox::discover(&self.0.http, &base).await?;
                let (token, track) = freebox::request_authorization(&self.0.http, &info, &device_name()).await?;
                let model = info.model();
                *self.0.pairing.lock().unwrap() = Some(Pairing { info, token, track });
                Ok(json!({ "track_id": track, "model": model }))
            }
            "pair_poll" => {
                let (info, token, track) = {
                    let p = self.0.pairing.lock().unwrap();
                    let p = p.as_ref().ok_or_else(|| anyhow!("no_pairing: aucune demande en cours"))?;
                    (p.info.clone(), p.token.clone(), p.track)
                };
                let status = freebox::track_authorization(&self.0.http, &info, track).await?;
                if status == "granted" {
                    self.reset_source().await;
                    {
                        let mut c = self.0.cfg.lock().unwrap();
                        c.base_url = info.base.clone();
                        c.box_uid = n::st(&info.raw, "uid");
                        c.box_name = Some(info.model());
                        c.paired = true;
                        c.demo = false;
                    }
                    self.save_token(&token);
                    self.save_cfg();
                    *self.0.pairing.lock().unwrap() = None;
                    self.open_store(false);
                    self.spawn_collector();
                }
                Ok(json!({ "status": status }))
            }
            "unpair" => {
                self.reset_source().await;
                self.delete_token();
                {
                    let mut c = self.0.cfg.lock().unwrap();
                    c.paired = false;
                    c.demo = false;
                }
                self.save_cfg();
                Ok(self.status_json())
            }
            "demo" => {
                let on = a.get("on").and_then(|v| v.as_bool()).unwrap_or(true);
                self.reset_source().await;
                self.0.cfg.lock().unwrap().demo = on;
                self.save_cfg();
                let paired = self.cfg().paired;
                if on || paired {
                    self.open_store(on);
                } else {
                    *self.0.store.lock().unwrap() = None;
                }
                self.spawn_collector();
                Ok(self.status_json())
            }
            "wifi" => {
                let l = self.0.live.lock().unwrap();
                Ok(json!({ "aps": l.aps, "neighbors": l.neighbors, "neighbors_ts": l.neighbors_ts }))
            }
            "scan_neighbors" => {
                let ids: Vec<i64> = self.0.live.lock().unwrap().aps.iter().map(|a| a.id).collect();
                let mut g = self.0.src.lock().await;
                if let Some(src) = g.as_mut() {
                    for id in ids {
                        src.post(&format!("wifi/ap/{id}/neighbors/scan/"), &json!({})).await?;
                    }
                }
                drop(g);
                // relecture des voisins dans ~10 s (le scan de la box prend quelques secondes)
                self.0.live.lock().unwrap().neighbors_ts = now() - 900 + 10;
                Ok(json!({ "ok": true }))
            }
            "wan_history" => self.with_store(|s| s.wan_series(arg_i(&a, "range", 86400), arg_i(&a, "points", 400))),
            "host_history" => {
                let mac = a.get("mac").and_then(|v| v.as_str()).unwrap_or("").to_string();
                self.with_store(|s| s.host_series(&mac, arg_i(&a, "range", 86400), arg_i(&a, "points", 300)))
            }
            "top" => self.with_store(|s| s.top_consumers(arg_i(&a, "range", 86400), arg_i(&a, "limit", 30))),
            "heatmap" => self.with_store(|s| s.heatmap(arg_i(&a, "days", 7))),
            "saturation" => self.with_store(|s| s.saturation(arg_i(&a, "range", 7 * 86400))),
            "events" => self.with_store(|s| s.events(arg_i(&a, "range", 30 * 86400))),
            "rrd" => {
                let range = arg_i(&a, "range", 7 * 86400);
                let body = json!({ "db": "net", "date_start": now() - range, "date_end": now(), "fields": ["time", "rate_down", "rate_up", "bw_down", "bw_up"] });
                let mut g = self.0.src.lock().await;
                let src = g.as_mut().ok_or_else(|| anyhow!("no_source: pas de connexion à la box"))?;
                if src.is_demo() {
                    drop(g);
                    return self.with_store(|s| s.wan_series(range, 400));
                }
                let r = src.post("rrd/", &body).await?;
                let rows: Vec<Value> = n::arr(r.get("data").unwrap_or(&Value::Null))
                    .iter()
                    .map(|p| {
                        let m = |k: &str| n::num(p, k).unwrap_or(0.0) * 8.0 / 1e6;
                        json!([n::num(p, "time").unwrap_or(0.0) as i64, m("rate_down"), m("rate_down"), m("rate_up"), m("rate_up"), m("bw_down"), m("bw_up")])
                    })
                    .collect();
                Ok(json!({ "bucket": 0, "source": "rrd", "fields": ["ts", "down_avg", "down_max", "up_avg", "up_max", "bw_down", "bw_up"], "rows": rows }))
            }
            "speedtest_add" => {
                self.with_store(|s| s.add_speedtest(&a))?;
                Ok(json!({"ok": true}))
            }
            "speedtests" => self.with_store(|s| s.speedtests()),
            "speedtest_delete" => {
                self.with_store(|s| s.delete_speedtest(arg_i(&a, "ts", 0)))?;
                Ok(json!({"ok": true}))
            }
            "local_wifi" => crate::winnet::local_wifi(),
            "settings" => {
                {
                    let mut c = self.0.cfg.lock().unwrap();
                    if let Some(v) = a.get("interval").and_then(|v| v.as_u64()) {
                        c.interval = v.clamp(2, 60);
                    }
                    if let Some(v) = a.get("close_to_tray").and_then(|v| v.as_bool()) {
                        c.close_to_tray = v;
                    }
                    if let Some(v) = a.get("retention_days").and_then(|v| v.as_i64()) {
                        c.retention_days = v.clamp(7, 730);
                    }
                    if let Some(v) = a.get("base_url").and_then(|v| v.as_str()) {
                        c.base_url = v.trim().trim_end_matches('/').to_string();
                    }
                    if a.get("subscribed_down").is_some() {
                        c.subscribed_down = a["subscribed_down"].as_f64();
                    }
                    if a.get("subscribed_up").is_some() {
                        c.subscribed_up = a["subscribed_up"].as_f64();
                    }
                }
                self.save_cfg();
                Ok(self.status_json())
            }
            "export" => {
                let dump = json!({
                    "app": "ondes", "version": env!("CARGO_PKG_VERSION"), "exported": now(),
                    "status": self.status_json(), "live": self.live_json(),
                    "raw": *self.0.raw.lock().unwrap(),
                });
                let text = serde_json::to_string_pretty(&dump)?;
                let base = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).map(PathBuf::from).unwrap_or_else(|_| self.0.dir.clone());
                let docs = if base.join("Documents").is_dir() { base.join("Documents") } else { base };
                let path = docs.join(format!("ondes-export-{}.json", chrono::Local::now().format("%Y-%m-%d-%H%M")));
                std::fs::write(&path, &text)?;
                Ok(json!({ "path": path.to_string_lossy(), "size": text.len() }))
            }
            other => Err(anyhow!("unknown_command: {other}")),
        }
    }
}
