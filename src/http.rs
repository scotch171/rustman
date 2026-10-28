use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use crate::model::Request;

pub struct Response {
    pub status: u16,
    pub reason: String,
    pub elapsed: Duration,
    pub size: usize,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub json: Option<serde_json::Value>,
    /// Pretty-printed JSON, computed once.
    pub pretty: Option<String>,
}

pub type Pending = Receiver<Result<Response, String>>;

/// Run the request on a background thread; the UI polls the receiver.
pub fn send(req: Request, repaint: impl Fn() + Send + 'static) -> Pending {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(execute(&req));
        repaint();
    });
    rx
}

fn execute(req: &Request) -> Result<Response, String> {
    let url = normalize_url(&req.url)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("rustman/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;

    let method = reqwest::Method::from_bytes(req.method.as_str().as_bytes()).unwrap();
    let mut builder = client.request(method, url);
    for h in req.headers.iter().filter(|h| h.enabled && !h.key.is_empty()) {
        builder = builder.header(h.key.trim(), h.value.trim());
    }
    if let Some(body) = req.body_payload() {
        if let Some(ct) = req.body_kind.content_type() {
            if !req.has_header("content-type") {
                builder = builder.header("Content-Type", ct);
            }
        }
        builder = builder.body(body);
    }

    let started = Instant::now();
    let resp = builder.send().map_err(describe_error)?;
    let status = resp.status();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
        .collect();
    let bytes = resp.bytes().map_err(describe_error)?;
    let elapsed = started.elapsed();

    let body = String::from_utf8_lossy(&bytes).into_owned();
    let json = serde_json::from_slice::<serde_json::Value>(&bytes).ok();
    let pretty = json.as_ref().and_then(|v| serde_json::to_string_pretty(v).ok());

    Ok(Response {
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or("").to_owned(),
        elapsed,
        size: bytes.len(),
        headers,
        body,
        json,
        pretty,
    })
}

fn normalize_url(raw: &str) -> Result<String, String> {
    let url = raw.trim();
    if url.is_empty() {
        return Err("Введите URL".into());
    }
    if url.contains("://") {
        Ok(url.to_owned())
    } else {
        Ok(format!("http://{url}"))
    }
}

fn describe_error(e: reqwest::Error) -> String {
    let mut msg = e.to_string();
    let mut source = std::error::Error::source(&e);
    while let Some(s) = source {
        msg.push_str(&format!("\n  -> {s}"));
        source = s.source();
    }
    msg
}
