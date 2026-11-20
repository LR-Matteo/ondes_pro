//! Client de l'API Freebox OS (réseau local).
//!
//! Flux d'authentification (doc officielle dev.freebox.fr/sdk/os/login) :
//! 1. POST login/authorize/ → app_token + track_id ; l'utilisateur valide sur l'écran de la box.
//! 2. GET login/authorize/{track_id} → status: pending | granted | denied | timeout | unknown.
//! 3. GET login/ → challenge ; password = HMAC-SHA1(clé = app_token, message = challenge) en hexadécimal.
//! 4. POST login/session/ → session_token, à envoyer dans l'en-tête X-Fbx-App-Auth.
//! La session expire : sur `auth_required`, on rouvre une session et on rejoue la requête une fois.

use anyhow::{anyhow, Result};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha1::Sha1;
use std::time::Duration;

pub const APP_ID: &str = "fr.ondes.diagnostic";
pub const APP_NAME: &str = "Ondes";

/// Erreur renvoyée par la Freebox, avec son code (`insufficient_rights`, `auth_required`...).
#[derive(Debug)]
pub struct ApiError {
    pub code: String,
    pub msg: String,
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.msg)
    }
}
impl std::error::Error for ApiError {}

pub fn api_code(e: &anyhow::Error) -> Option<&str> {
    e.downcast_ref::<ApiError>().map(|a| a.code.as_str())
}

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .connect_timeout(Duration::from_secs(4))
        .build()
        .expect("client HTTP")
}

pub fn password(app_token: &str, challenge: &str) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(app_token.as_bytes()).expect("clé HMAC");
    mac.update(challenge.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Informations publiques de la box (GET /api_version, sans authentification).
#[derive(Clone, Debug)]
pub struct BoxInfo {
    pub base: String,
    pub api_base: String,
    pub major: u32,
    pub raw: Value,
}

impl BoxInfo {
    pub fn url(&self, path: &str) -> String {
        format!("{}{}v{}/{}", self.base, self.api_base, self.major, path.trim_start_matches('/'))
    }
    pub fn model(&self) -> String {
        self.raw.get("box_model_name").or(self.raw.get("device_name")).and_then(|v| v.as_str()).unwrap_or("Freebox").to_string()
    }
}

pub async fn discover(http: &reqwest::Client, base: &str) -> Result<BoxInfo> {
    let base = base.trim_end_matches('/').to_string();
    let raw: Value = http
        .get(format!("{base}/api_version"))
        .send()
        .await
        .map_err(|e| anyhow!("unreachable: Freebox injoignable à {base} ({e})"))?
        .json()
        .await
        .map_err(|_| anyhow!("not_freebox: l'adresse {base} ne répond pas comme une Freebox"))?;
    let version = raw.get("api_version").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("not_freebox: champ api_version absent"))?;
    let major: u32 = version.split('.').next().and_then(|m| m.parse().ok()).ok_or_else(|| anyhow!("not_freebox: version d'API illisible ({version})"))?;
    let api_base = raw.get("api_base_url").and_then(|v| v.as_str()).unwrap_or("/api/").to_string();
    Ok(BoxInfo { base, api_base, major, raw })
}

/// Lit une réponse API Freebox { success, result, error_code, msg }.
async fn unwrap(resp: reqwest::Response) -> Result<Value> {
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|e| anyhow!("bad_response: réponse illisible (HTTP {status}, {e})"))?;
    if body.get("success").and_then(|v| v.as_bool()) == Some(true) {
        return Ok(body.get("result").cloned().unwrap_or(Value::Null));
    }
    Err(ApiError {
        code: body.get("error_code").and_then(|v| v.as_str()).unwrap_or("http_error").to_string(),
        msg: body.get("msg").and_then(|v| v.as_str()).unwrap_or("erreur inconnue").to_string(),
    }
    .into())
}

pub async fn request_authorization(http: &reqwest::Client, info: &BoxInfo, device_name: &str) -> Result<(String, i64)> {
    let r = http
        .post(info.url("login/authorize/"))
        .json(&json!({ "app_id": APP_ID, "app_name": APP_NAME, "app_version": env!("CARGO_PKG_VERSION"), "device_name": device_name }))
        .send()
        .await?;
    let res = unwrap(r).await?;
    let token = res.get("app_token").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("bad_response: app_token absent"))?;
    let track = res.get("track_id").and_then(|v| v.as_i64()).ok_or_else(|| anyhow!("bad_response: track_id absent"))?;
    Ok((token.to_string(), track))
}

pub async fn track_authorization(http: &reqwest::Client, info: &BoxInfo, track_id: i64) -> Result<String> {
    let r = http.get(info.url(&format!("login/authorize/{track_id}"))).send().await?;
    let res = unwrap(r).await?;
    Ok(res.get("status").and_then(|v| v.as_str()).unwrap_or("unknown").to_string())
}

pub struct Client {
    pub http: reqwest::Client,
    pub info: BoxInfo,
    app_token: String,
    session: Option<String>,
    pub permissions: Value,
}

impl Client {
    pub fn new(http: reqwest::Client, info: BoxInfo, app_token: String) -> Self {
        Self { http, info, app_token, session: None, permissions: Value::Null }
    }

    pub async fn login(&mut self) -> Result<()> {
        let ch = unwrap(self.http.get(self.info.url("login/")).send().await?).await?;
        let challenge = ch.get("challenge").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("bad_response: challenge absent"))?;
        let body = json!({ "app_id": APP_ID, "password": password(&self.app_token, challenge) });
        let res = unwrap(self.http.post(self.info.url("login/session/")).json(&body).send().await?).await?;
        self.session = Some(res.get("session_token").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("bad_response: session_token absent"))?.to_string());
        self.permissions = res.get("permissions").cloned().unwrap_or(Value::Null);
        Ok(())
    }

    async fn call(&mut self, method: reqwest::Method, path: &str, body: Option<&Value>) -> Result<Value> {
        for attempt in 0..2 {
            if self.session.is_none() {
                self.login().await?;
            }
            let mut req = self.http.request(method.clone(), self.info.url(path)).header("X-Fbx-App-Auth", self.session.as_deref().unwrap_or(""));
            if let Some(b) = body {
                req = req.json(b);
            }
            match unwrap(req.send().await?).await {
                Err(e) if attempt == 0 && api_code(&e) == Some("auth_required") => self.session = None,
                other => return other,
            }
        }
        unreachable!()
    }

    pub async fn get(&mut self, path: &str) -> Result<Value> {
        self.call(reqwest::Method::GET, path, None).await
    }
    pub async fn post(&mut self, path: &str, body: &Value) -> Result<Value> {
        self.call(reqwest::Method::POST, path, Some(body)).await
    }
    pub async fn logout(&mut self) {
        if self.session.is_some() {
            let _ = self.post("login/logout/", &json!({})).await;
            self.session = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hmac_matches_openssl() {
        // echo -n "challenge" | openssl dgst -sha1 -hmac "token"
        assert_eq!(password("token", "challenge"), "0c534563a85c1e77106850f4fe745daa1be1ebea");
    }
}
