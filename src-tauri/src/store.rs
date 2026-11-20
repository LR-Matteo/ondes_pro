//! Historique local (SQLite) : agrégats par minute de la ligne et de chaque appareil.

use anyhow::Result;
use chrono::{Datelike, Local, TimeZone, Timelike};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::Path;

pub struct Store {
    pub db: Connection,
}

#[derive(Default, Clone, Debug)]
pub struct WanMinute {
    pub ts: i64,
    pub down_avg: f64,
    pub down_max: f64,
    pub up_avg: f64,
    pub up_max: f64,
    pub bw_down: f64,
    pub bw_up: f64,
}
#[derive(Default, Clone, Debug)]
pub struct HostMinute {
    pub ts: i64,
    pub host: String,
    pub down_bytes: f64,
    pub up_bytes: f64,
    pub down_max: f64,
    pub up_max: f64,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path)?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS wan_min(ts INTEGER PRIMARY KEY, down_avg REAL, down_max REAL, up_avg REAL, up_max REAL, bw_down REAL, bw_up REAL);
             CREATE TABLE IF NOT EXISTS host_min(ts INTEGER, host TEXT, down_bytes REAL, up_bytes REAL, down_max REAL, up_max REAL, PRIMARY KEY(ts, host));
             CREATE INDEX IF NOT EXISTS host_min_host ON host_min(host, ts);
             CREATE TABLE IF NOT EXISTS hosts(mac TEXT PRIMARY KEY, name TEXT, vendor TEXT, kind TEXT, first_seen INTEGER, last_seen INTEGER);
             CREATE TABLE IF NOT EXISTS events(ts INTEGER, kind TEXT, host TEXT, detail TEXT);
             CREATE TABLE IF NOT EXISTS speedtests(ts INTEGER, down REAL, up REAL, ping REAL, jitter REAL, loaded REAL, conn TEXT, room TEXT, box_down REAL);",
        )?;
        Ok(Self { db })
    }

    pub fn purge(&self, keep_days: i64) -> Result<()> {
        let lim = now() - keep_days * 86400;
        self.db.execute("DELETE FROM wan_min WHERE ts < ?1", [lim])?;
        self.db.execute("DELETE FROM host_min WHERE ts < ?1", [lim])?;
        self.db.execute("DELETE FROM events WHERE ts < ?1", [lim])?;
        Ok(())
    }

    pub fn write_minute(&mut self, w: &WanMinute, hosts: &[HostMinute]) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO wan_min VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![w.ts, w.down_avg, w.down_max, w.up_avg, w.up_max, w.bw_down, w.bw_up],
        )?;
        for h in hosts {
            tx.execute(
                "INSERT OR REPLACE INTO host_min VALUES(?1,?2,?3,?4,?5,?6)",
                params![h.ts, h.host, h.down_bytes, h.up_bytes, h.down_max, h.up_max],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Enregistre l'appareil ; renvoie true s'il est nouveau.
    pub fn upsert_host(&self, mac: &str, name: &str, vendor: Option<&str>, kind: &str, ts: i64) -> Result<bool> {
        let n = self.db.execute(
            "UPDATE hosts SET name=?2, vendor=COALESCE(?3, vendor), kind=?4, last_seen=?5 WHERE mac=?1",
            params![mac, name, vendor, kind, ts],
        )?;
        if n == 0 {
            self.db.execute("INSERT INTO hosts VALUES(?1,?2,?3,?4,?5,?5)", params![mac, name, vendor, kind, ts])?;
            return Ok(true);
        }
        Ok(false)
    }
    pub fn first_seen(&self, mac: &str) -> Option<i64> {
        self.db.query_row("SELECT first_seen FROM hosts WHERE mac=?1", [mac], |r| r.get(0)).ok()
    }
    pub fn host_count(&self) -> i64 {
        self.db.query_row("SELECT COUNT(*) FROM hosts", [], |r| r.get(0)).unwrap_or(0)
    }
    pub fn event(&self, kind: &str, host: &str, detail: &str) -> Result<()> {
        self.db.execute("INSERT INTO events VALUES(?1,?2,?3,?4)", params![now(), kind, host, detail])?;
        Ok(())
    }

    pub fn today_bytes(&self) -> Result<std::collections::HashMap<String, (f64, f64)>> {
        let mid = local_midnight();
        let mut q = self.db.prepare("SELECT host, SUM(down_bytes), SUM(up_bytes) FROM host_min WHERE ts >= ?1 GROUP BY host")?;
        let rows = q.query_map([mid], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, f64>(1)?, r.get::<_, f64>(2)?))))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Série de la ligne, regroupée pour rester sous ~ `points` points.
    pub fn wan_series(&self, range: i64, points: i64) -> Result<Value> {
        let from = now() - range;
        let bucket = ((range / points.max(1)) / 60).max(1) * 60;
        let mut q = self.db.prepare(
            "SELECT (ts/?2)*?2 AS b, AVG(down_avg), MAX(down_max), AVG(up_avg), MAX(up_max), MAX(bw_down), MAX(bw_up)
             FROM wan_min WHERE ts >= ?1 GROUP BY b ORDER BY b",
        )?;
        let rows: Vec<Value> = q
            .query_map(params![from, bucket], |r| {
                Ok(json!([r.get::<_, i64>(0)?, r.get::<_, f64>(1)?, r.get::<_, f64>(2)?, r.get::<_, f64>(3)?, r.get::<_, f64>(4)?, r.get::<_, f64>(5)?, r.get::<_, f64>(6)?]))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(json!({ "bucket": bucket, "fields": ["ts", "down_avg", "down_max", "up_avg", "up_max", "bw_down", "bw_up"], "rows": rows }))
    }

    pub fn host_series(&self, host: &str, range: i64, points: i64) -> Result<Value> {
        let from = now() - range;
        let bucket = ((range / points.max(1)) / 60).max(1) * 60;
        let mut q = self.db.prepare(
            "SELECT (ts/?3)*?3 AS b, SUM(down_bytes), SUM(up_bytes), MAX(down_max), MAX(up_max)
             FROM host_min WHERE host=?1 AND ts >= ?2 GROUP BY b ORDER BY b",
        )?;
        let rows: Vec<Value> = q
            .query_map(params![host, from, bucket], |r| {
                let b: i64 = r.get(0)?;
                let d: f64 = r.get(1)?;
                let u: f64 = r.get(2)?;
                // débit moyen sur le seau, en Mb/s
                Ok(json!([b, d * 8.0 / 1e6 / bucket as f64, u * 8.0 / 1e6 / bucket as f64, r.get::<_, f64>(3)?, r.get::<_, f64>(4)?]))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(json!({ "bucket": bucket, "fields": ["ts", "down_avg", "up_avg", "down_max", "up_max"], "rows": rows }))
    }

    pub fn top_consumers(&self, range: i64, limit: i64) -> Result<Value> {
        let from = now() - range;
        let mut q = self.db.prepare(
            "SELECT m.host, COALESCE(h.name, m.host), h.kind, SUM(m.down_bytes) d, SUM(m.up_bytes) u, MAX(m.down_max)
             FROM host_min m LEFT JOIN hosts h ON h.mac = m.host WHERE m.ts >= ?1 GROUP BY m.host ORDER BY d + u DESC LIMIT ?2",
        )?;
        let rows: Vec<Value> = q
            .query_map(params![from, limit], |r| {
                Ok(json!({ "mac": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "kind": r.get::<_, Option<String>>(2)?,
                           "down": r.get::<_, f64>(3)?, "up": r.get::<_, f64>(4)?, "peak": r.get::<_, f64>(5)? }))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(Value::Array(rows))
    }

    /// Moyenne de débit descendant par jour de semaine × heure (heure locale), sur `days` jours.
    pub fn heatmap(&self, days: i64) -> Result<Value> {
        let from = now() - days * 86400;
        let mut sum = [[0f64; 24]; 7];
        let mut cnt = [[0f64; 24]; 7];
        let mut q = self.db.prepare("SELECT ts, down_avg FROM wan_min WHERE ts >= ?1")?;
        let rows = q.query_map([from], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))?;
        for (ts, d) in rows.flatten() {
            if let Some(t) = Local.timestamp_opt(ts, 0).single() {
                let wd = t.weekday().num_days_from_monday() as usize;
                let h = t.hour() as usize;
                sum[wd][h] += d;
                cnt[wd][h] += 1.0;
            }
        }
        let grid: Vec<Vec<Option<f64>>> = (0..7).map(|d| (0..24).map(|h| if cnt[d][h] > 0.0 { Some(sum[d][h] / cnt[d][h]) } else { None }).collect()).collect();
        Ok(json!(grid))
    }

    /// Épisodes de saturation : minutes où la ligne dépasse 90 % de sa capacité, regroupées, avec les principaux responsables.
    pub fn saturation(&self, range: i64) -> Result<Value> {
        let from = now() - range;
        let mut q = self.db.prepare("SELECT ts, down_max, bw_down, up_max, bw_up FROM wan_min WHERE ts >= ?1 ORDER BY ts")?;
        let mins: Vec<(i64, bool, bool)> = q
            .query_map([from], |r| {
                let (ts, dm, bd, um, bu): (i64, f64, f64, f64, f64) = (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?);
                Ok((ts, bd > 0.0 && dm >= 0.9 * bd, bu > 0.0 && um >= 0.9 * bu))
            })?
            .filter_map(|r| r.ok())
            .collect();
        let total_minutes = mins.len();
        let mut episodes: Vec<(i64, i64, bool, bool)> = vec![];
        for (ts, d, u) in mins.into_iter().filter(|m| m.1 || m.2) {
            match episodes.last_mut() {
                Some(e) if ts - e.1 <= 120 => {
                    e.1 = ts;
                    e.2 |= d;
                    e.3 |= u;
                }
                _ => episodes.push((ts, ts, d, u)),
            }
        }
        let mut out = vec![];
        let mut sat_minutes = 0;
        for (a, b, d, u) in &episodes {
            sat_minutes += (b - a) / 60 + 1;
            let mut q = self.db.prepare(
                "SELECT m.host, COALESCE(h.name, m.host), SUM(m.down_bytes), SUM(m.up_bytes) FROM host_min m LEFT JOIN hosts h ON h.mac=m.host
                 WHERE m.ts BETWEEN ?1 AND ?2 GROUP BY m.host ORDER BY SUM(m.down_bytes)+SUM(m.up_bytes) DESC LIMIT 3",
            )?;
            let top: Vec<Value> = q
                .query_map(params![a, b], |r| Ok(json!({"mac": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "down": r.get::<_, f64>(2)?, "up": r.get::<_, f64>(3)?})))?
                .filter_map(|r| r.ok())
                .collect();
            out.push(json!({ "start": a, "end": b + 60, "down": d, "up": u, "top": top }));
        }
        Ok(json!({ "episodes": out, "saturated_minutes": sat_minutes, "observed_minutes": total_minutes }))
    }

    pub fn events(&self, range: i64) -> Result<Value> {
        let mut q = self.db.prepare("SELECT ts, kind, host, detail FROM events WHERE ts >= ?1 ORDER BY ts DESC LIMIT 200")?;
        let rows: Vec<Value> = q
            .query_map([now() - range], |r| Ok(json!({"ts": r.get::<_, i64>(0)?, "kind": r.get::<_, String>(1)?, "host": r.get::<_, String>(2)?, "detail": r.get::<_, String>(3)?})))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(Value::Array(rows))
    }

    pub fn add_speedtest(&self, v: &Value) -> Result<()> {
        let f = |k: &str| v.get(k).and_then(|x| x.as_f64());
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string());
        self.db.execute(
            "INSERT INTO speedtests VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![now(), f("down"), f("up"), f("ping"), f("jitter"), f("loadedPing"), s("conn"), s("room"), f("boxDown")],
        )?;
        Ok(())
    }
    pub fn speedtests(&self) -> Result<Value> {
        let mut q = self.db.prepare("SELECT ts, down, up, ping, jitter, loaded, conn, room, box_down FROM speedtests ORDER BY ts DESC LIMIT 100")?;
        let rows: Vec<Value> = q
            .query_map([], |r| {
                Ok(json!({"ts": r.get::<_, i64>(0)?, "down": r.get::<_, Option<f64>>(1)?, "up": r.get::<_, Option<f64>>(2)?, "ping": r.get::<_, Option<f64>>(3)?,
                    "jitter": r.get::<_, Option<f64>>(4)?, "loadedPing": r.get::<_, Option<f64>>(5)?, "conn": r.get::<_, Option<String>>(6)?,
                    "room": r.get::<_, Option<String>>(7)?, "boxDown": r.get::<_, Option<f64>>(8)?}))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(Value::Array(rows))
    }
    pub fn delete_speedtest(&self, ts: i64) -> Result<()> {
        self.db.execute("DELETE FROM speedtests WHERE ts=?1", [ts])?;
        Ok(())
    }
    pub fn coverage(&self) -> Result<i64> {
        Ok(self.db.query_row("SELECT COALESCE(MIN(ts), 0) FROM wan_min", [], |r| r.get(0))?)
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
pub fn local_midnight() -> i64 {
    let n = Local::now();
    Local
        .with_ymd_and_hms(n.year(), n.month(), n.day(), 0, 0, 0)
        .single()
        .map(|t| t.timestamp())
        .unwrap_or_else(|| now() - 86400)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saturation_episodes_and_top() {
        let dir = std::env::temp_dir().join(format!("ondes-test-{}.db", now()));
        let mut s = Store::open(&dir).unwrap();
        let t0 = (now() / 60 - 30) * 60;
        for i in 0..20 {
            let sat = (5..9).contains(&i);
            let w = WanMinute { ts: t0 + i * 60, down_avg: 100.0, down_max: if sat { 950.0 } else { 300.0 }, up_avg: 5.0, up_max: 10.0, bw_down: 1000.0, bw_up: 700.0 };
            let h = HostMinute { ts: w.ts, host: "aa".into(), down_bytes: if sat { 6e9 } else { 1e6 }, up_bytes: 0.0, down_max: 900.0, up_max: 0.0 };
            s.write_minute(&w, &[h]).unwrap();
        }
        s.upsert_host("aa", "PS5", None, "vg_console", t0).unwrap();
        let r = s.saturation(3600).unwrap();
        assert_eq!(r["episodes"].as_array().unwrap().len(), 1);
        assert_eq!(r["saturated_minutes"], 4);
        assert_eq!(r["episodes"][0]["top"][0]["name"], "PS5");
        let _ = std::fs::remove_file(dir);
    }
}
