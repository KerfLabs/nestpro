//! A small JSON-over-HTTPS client for integrations: bearer token, timeouts,
//! and errors in words a person can act on. TLS is Windows' own (schannel)
//! on Windows and rustls elsewhere (Cargo.toml).

use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

/// Largest JSON answer read (a long nestable-jobs page is a few MB).
const JSON_LIMIT: u64 = 64 * 1024 * 1024;
/// Largest drawing downloaded.
const FILE_LIMIT: u64 = 100 * 1024 * 1024;

pub struct Client {
    agent: ureq::Agent,
    base: String,
    token: String,
    /// Name for messages ("Fulcrum").
    system: &'static str,
}

impl Client {
    pub fn new(base: &str, token: String, system: &'static str) -> Self {
        #[cfg(windows)]
        let provider = ureq::tls::TlsProvider::NativeTls;
        #[cfg(not(windows))]
        let provider = ureq::tls::TlsProvider::Rustls;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .tls_config(ureq::tls::TlsConfig::builder().provider(provider).build())
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_global(Some(Duration::from_secs(120)))
            .user_agent(concat!("nestpro/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Client { agent, base: base.trim_end_matches('/').to_string(), token, system }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn auth(&self) -> String {
        format!("Bearer {}", self.token)
    }

    pub fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let r = self.agent.get(&self.url(path)).header("Authorization", &self.auth()).header("Accept", "application/json").call();
        self.read_json(path, r)
    }

    pub fn post_json<B: Serialize, T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)], body: &B) -> Result<T, String> {
        let mut rq = self.agent.post(&self.url(path)).header("Authorization", &self.auth()).header("Accept", "application/json");
        for (k, v) in query {
            rq = rq.query(*k, v);
        }
        let text = serde_json::to_string(body).map_err(|e| e.to_string())?;
        let r = rq.content_type("application/json").send(text);
        self.read_json(path, r)
    }

    pub fn put_json<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T, String> {
        let text = serde_json::to_string(body).map_err(|e| e.to_string())?;
        let r = self.agent.put(&self.url(path)).header("Authorization", &self.auth()).header("Accept", "application/json").content_type("application/json").send(text);
        self.read_json(path, r)
    }

    pub fn get_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        let mut r = self.agent.get(&self.url(path)).header("Authorization", &self.auth()).call().map_err(|e| self.transport(e))?;
        let status = r.status().as_u16();
        if !(200..300).contains(&status) {
            let body = r.body_mut().with_config().limit(JSON_LIMIT).read_to_string().unwrap_or_default();
            return Err(self.status_error(path, status, &body));
        }
        r.body_mut().with_config().limit(FILE_LIMIT).read_to_vec().map_err(|e| format!("{}: download stopped ({e})", self.system))
    }

    fn read_json<T: DeserializeOwned>(&self, path: &str, r: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<T, String> {
        let mut r = r.map_err(|e| self.transport(e))?;
        let status = r.status().as_u16();
        let body = r.body_mut().with_config().limit(JSON_LIMIT).read_to_string().map_err(|e| format!("{}: couldn't read the answer ({e})", self.system))?;
        if !(200..300).contains(&status) {
            return Err(self.status_error(path, status, &body));
        }
        serde_json::from_str(&body).map_err(|e| format!("{} answered {path} in a form this app doesn't understand ({e})", self.system))
    }

    fn transport(&self, e: ureq::Error) -> String {
        match e {
            ureq::Error::Timeout(_) => format!("{} didn't answer in time — check the internet connection", self.system),
            ureq::Error::HostNotFound => format!("Couldn't find {} at {} — check the API address and the internet connection", self.system, self.base),
            ureq::Error::Io(e) => format!("Couldn't reach {} ({e})", self.system),
            e => format!("Couldn't talk to {} ({e})", self.system),
        }
    }

    fn status_error(&self, path: &str, status: u16, body: &str) -> String {
        let detail = problem_text(body);
        let what = match status {
            401 => format!("{} refused the API token (401) — check it in Settings → ERP / MES", self.system),
            403 => format!("The API token isn't allowed to do this in {} (403)", self.system),
            404 => format!("{} has no {path} (404)", self.system),
            429 => format!("{} is limiting requests (429) — wait a minute and try again", self.system),
            s if s >= 500 => format!("{} had a problem ({s})", self.system),
            s => format!("{} turned down {path} ({s})", self.system),
        };
        if detail.is_empty() { what } else { format!("{what}: {detail}") }
    }
}

/// The readable part of an error answer: ASP.NET problem details
/// (title, detail, errors{field: [..]}), else the start of the text.
pub fn problem_text(body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        return String::new();
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        let mut parts: Vec<String> = vec![];
        for k in ["title", "detail", "message"] {
            if let Some(s) = v.get(k).and_then(|x| x.as_str()) {
                if !s.trim().is_empty() && !parts.iter().any(|p| p == s) {
                    parts.push(s.trim().to_string());
                }
            }
        }
        if let Some(errs) = v.get("errors").and_then(|e| e.as_object()) {
            for (field, msgs) in errs {
                let m: Vec<&str> = msgs.as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
                parts.push(if m.is_empty() { field.clone() } else { format!("{field}: {}", m.join("; ")) });
            }
        }
        if !parts.is_empty() {
            return parts.join(" — ");
        }
    }
    let mut s: String = body.chars().take(300).collect();
    if body.chars().count() > 300 {
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_answers_read_like_sentences() {
        let pd = r#"{"type":"x","title":"One or more validation errors occurred.","status":400,"errors":{"Operations":["At least one operation is required."]}}"#;
        assert_eq!(problem_text(pd), "One or more validation errors occurred. — Operations: At least one operation is required.");
        assert_eq!(problem_text("no tenant"), "no tenant");
        assert_eq!(problem_text(""), "");
        let c = Client::new("https://example.invalid/", "t".into(), "Fulcrum");
        assert!(c.status_error("/api/x", 401, "").contains("refused the API token"));
        assert!(c.status_error("/api/x", 400, "no tenant").ends_with(": no tenant"));
    }
}
