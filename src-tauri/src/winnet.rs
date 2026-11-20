//! Wi-Fi de ce PC (Windows) : interface connectée et réseaux visibles, via `netsh wlan`.
//! Les sorties sont lues en français comme en anglais. Sous Windows 11 récent, le scan des
//! réseaux exige que la localisation soit autorisée pour les applications de bureau.

use serde_json::{json, Value};

fn pct_to_dbm(p: f64) -> f64 {
    (p / 2.0 - 100.0).round()
}

/// Clé normalisée : minuscules, sans accents ni caractères mal décodés.
fn key(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == ' ').collect::<String>().trim().to_string()
}

fn kv(line: &str) -> Option<(String, String)> {
    let i = line.find(" : ").or_else(|| line.find(':'))?;
    let (k, v) = line.split_at(i);
    let v = v.trim_start_matches([' ', ':']).trim();
    Some((key(k), v.to_string()))
}

pub fn parse_interfaces(text: &str) -> Value {
    let mut out = vec![];
    let mut cur = serde_json::Map::new();
    for line in text.lines() {
        let Some((k, v)) = kv(line) else { continue };
        let k = k.as_str();
        let set = |cur: &mut serde_json::Map<String, Value>, name: &str, val: Value| {
            cur.insert(name.into(), val);
        };
        if (k == "name" || k == "nom") && !cur.is_empty() {
            out.push(Value::Object(std::mem::take(&mut cur)));
        }
        let num = |s: &str| s.replace(',', ".").split_whitespace().next().and_then(|x| x.trim_end_matches('%').parse::<f64>().ok());
        match k {
            "name" | "nom" => set(&mut cur, "name", json!(v)),
            "ssid" => set(&mut cur, "ssid", json!(v)),
            "ap bssid" | "bssid" => set(&mut cur, "bssid", json!(v.to_lowercase())),
            "radio type" | "type de radio" => set(&mut cur, "radio", json!(v)),
            "band" | "bande" => set(&mut cur, "band", json!(v)),
            "channel" | "canal" => set(&mut cur, "channel", json!(num(&v))),
            "signal" => {
                let p = num(&v);
                set(&mut cur, "signal_pct", json!(p));
                set(&mut cur, "signal_dbm", json!(p.map(pct_to_dbm)));
            }
            "state" | "tat" => set(&mut cur, "state", json!(v)),
            k if k.starts_with("receive rate") || k.starts_with("rception") || k.contains("ception") => set(&mut cur, "rx_mbps", json!(num(&v))),
            k if k.starts_with("transmit rate") || k.starts_with("transmission") => set(&mut cur, "tx_mbps", json!(num(&v))),
            "authentication" | "authentification" => set(&mut cur, "auth", json!(v)),
            _ => {}
        }
    }
    if !cur.is_empty() {
        out.push(Value::Object(cur));
    }
    Value::Array(out)
}

pub fn parse_networks(text: &str) -> Value {
    let mut out: Vec<Value> = vec![];
    let mut ssid = String::new();
    for line in text.lines() {
        let t = line.trim_start();
        let lower = t.to_lowercase();
        if lower.starts_with("ssid ") || lower.starts_with("ssid:") {
            if let Some((_, v)) = kv(t) {
                ssid = if v.is_empty() { "(masqué)".into() } else { v };
            }
            continue;
        }
        if lower.starts_with("bssid") {
            if let Some(i) = t.find(':') {
                let mac = t[i + 1..].trim().to_lowercase();
                out.push(json!({ "ssid": ssid, "bssid": mac, "channel": null, "band": null, "signal": null }));
            }
            continue;
        }
        let Some(last) = out.last_mut() else { continue };
        let Some((k, v)) = kv(t) else { continue };
        let n = v.replace(',', ".").split_whitespace().next().and_then(|x| x.trim_end_matches('%').parse::<f64>().ok());
        match k.as_str() {
            "signal" => last["signal"] = json!(n.map(pct_to_dbm)),
            "channel" | "canal" => last["channel"] = json!(n),
            "band" | "bande" => last["band"] = json!(n.map(|b| if b < 3.0 { "2.4" } else if b < 5.9 { "5" } else { "6" })),
            "radio type" | "type de radio" => last["radio"] = json!(v),
            _ => {}
        }
    }
    for n in out.iter_mut() {
        if n["band"].is_null() {
            if let Some(c) = n["channel"].as_f64() {
                n["band"] = json!(if c <= 14.0 { "2.4" } else { "5" });
            }
        }
    }
    Value::Array(out)
}

#[cfg(windows)]
fn netsh(args: &str) -> anyhow::Result<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("cmd")
        .args(["/C", &format!("chcp 65001 >NUL & netsh wlan {args}")])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
#[cfg(not(windows))]
fn netsh(_args: &str) -> anyhow::Result<String> {
    anyhow::bail!("windows_only: le Wi-Fi local n'est lu que sous Windows")
}

pub fn local_wifi() -> anyhow::Result<Value> {
    let itf = netsh("show interfaces")?;
    let nets = netsh("show networks mode=bssid").unwrap_or_default();
    let low = nets.to_lowercase();
    let location_blocked = low.contains("location") || low.contains("localisation");
    Ok(json!({
        "interfaces": parse_interfaces(&itf),
        "networks": if location_blocked { json!([]) } else { parse_networks(&nets) },
        "location_blocked": location_blocked,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    const ITF_FR: &str = "
Il existe 1 interface sur le système :

    Nom                    : Wi-Fi
    Description            : Intel(R) Wi-Fi 6E AX211 160MHz
    GUID                   : 1234
    Adresse physique       : 70:cd:0d:00:00:01
    Type d'interface       : Principal
    État                   : connecté
    SSID                   : Freebox-Demo
    AP BSSID               : 34:27:92:00:00:02
    Bande                  : 5 GHz
    Canal                  : 36
    Type de réseau         : Infrastructure
    Type de radio          : 802.11ax
    Authentification       : WPA2 - Personnel
    Chiffrement            : CCMP
    Mode de connexion      : Profil
    Réception (Mbits/s)    : 1200
    Transmission (Mbits/s) : 960
    Signal                 : 88%
    Profil                 : Freebox-Demo
";
    const NET_EN: &str = "
SSID 1 : Livebox-1C3F
    Network type            : Infrastructure
    Authentication          : WPA2-Personal
    BSSID 1                 : 4c:17:eb:aa:bb:01
         Signal             : 70%
         Radio type         : 802.11ax
         Band               : 2.4 GHz
         Channel            : 6
SSID 2 :
    BSSID 1                 : 4c:17:eb:aa:bb:02
         Signal             : 30%
         Channel            : 44
";
    #[test]
    fn interfaces_fr() {
        let v = parse_interfaces(ITF_FR);
        let i = &v[0];
        assert_eq!(i["ssid"], "Freebox-Demo");
        assert_eq!(i["channel"], 36.0);
        assert_eq!(i["rx_mbps"], 1200.0);
        assert_eq!(i["tx_mbps"], 960.0);
        assert_eq!(i["signal_dbm"], -56.0);
        assert_eq!(i["radio"], "802.11ax");
    }
    #[test]
    fn networks_en() {
        let v = parse_networks(NET_EN);
        assert_eq!(v.as_array().unwrap().len(), 2);
        assert_eq!(v[0]["ssid"], "Livebox-1C3F");
        assert_eq!(v[0]["band"], "2.4");
        assert_eq!(v[1]["ssid"], "(masqué)");
        assert_eq!(v[1]["band"], "5");
        assert_eq!(v[1]["signal"], -85.0);
    }
}
