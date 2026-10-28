use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl Method {
    pub const ALL: [Method; 7] = [
        Method::Get,
        Method::Post,
        Method::Put,
        Method::Patch,
        Method::Delete,
        Method::Head,
        Method::Options,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
            Method::Options => "OPTIONS",
        }
    }

    pub fn parse(s: &str) -> Option<Method> {
        Method::ALL
            .into_iter()
            .find(|m| m.as_str().eq_ignore_ascii_case(s))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyValue {
    pub enabled: bool,
    pub key: String,
    pub value: String,
}

impl KeyValue {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            enabled: true,
            key: key.into(),
            value: value.into(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.key.is_empty() && self.value.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyKind {
    None,
    Json,
    Text,
    FormUrlEncoded,
}

impl BodyKind {
    pub fn label(self) -> &'static str {
        match self {
            BodyKind::None => "none",
            BodyKind::Json => "raw JSON",
            BodyKind::Text => "raw text",
            BodyKind::FormUrlEncoded => "x-www-form-urlencoded",
        }
    }

    pub fn content_type(self) -> Option<&'static str> {
        match self {
            BodyKind::None => None,
            BodyKind::Json => Some("application/json"),
            BodyKind::Text => Some("text/plain"),
            BodyKind::FormUrlEncoded => Some("application/x-www-form-urlencoded"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub name: String,
    pub method: Method,
    pub url: String,
    pub params: Vec<KeyValue>,
    pub headers: Vec<KeyValue>,
    pub body_kind: BodyKind,
    pub body: String,
    pub form: Vec<KeyValue>,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            name: String::new(),
            method: Method::Get,
            url: String::new(),
            params: Vec::new(),
            headers: Vec::new(),
            body_kind: BodyKind::None,
            body: String::new(),
            form: Vec::new(),
        }
    }
}

impl Request {
    pub fn title(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        let base = self.url.split('?').next().unwrap_or("");
        let short = base
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        if short.is_empty() {
            "Новый запрос".to_owned()
        } else if short.chars().count() > 28 {
            format!("{}…", short.chars().take(28).collect::<String>())
        } else {
            short.to_owned()
        }
    }

    /// URL edited by hand: rebuild the params table from its query string,
    /// keeping disabled params (they are not part of the URL).
    pub fn sync_params_from_url(&mut self) {
        let mut params: Vec<KeyValue> = match self.url.split_once('?') {
            Some((_, query)) => parse_query(query),
            None => Vec::new(),
        };
        params.extend(self.params.iter().filter(|p| !p.enabled).cloned());
        self.params = params;
    }

    /// Params table edited: rewrite the query part of the URL.
    pub fn sync_url_from_params(&mut self) {
        let base = self.url.split('?').next().unwrap_or("").to_owned();
        let query = build_query(&self.params);
        self.url = if query.is_empty() {
            base
        } else {
            format!("{base}?{query}")
        };
    }

    /// Final body and the content type implied by the body kind.
    pub fn body_payload(&self) -> Option<String> {
        match self.body_kind {
            BodyKind::None => None,
            BodyKind::Json | BodyKind::Text => Some(self.body.clone()),
            BodyKind::FormUrlEncoded => Some(
                self.form
                    .iter()
                    .filter(|f| f.enabled && !f.key.is_empty())
                    .map(|f| format!("{}={}", url_encode(&f.key), url_encode(&f.value)))
                    .collect::<Vec<_>>()
                    .join("&"),
            ),
        }
    }

    pub fn has_header(&self, name: &str) -> bool {
        self.headers
            .iter()
            .any(|h| h.enabled && h.key.eq_ignore_ascii_case(name))
    }
}

/// Query is kept raw (not decoded), like Postman does, so the table shows
/// exactly what is in the URL.
pub fn parse_query(query: &str) -> Vec<KeyValue> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => KeyValue::new(k, v),
            None => KeyValue::new(pair, ""),
        })
        .collect()
}

pub fn build_query(params: &[KeyValue]) -> String {
    params
        .iter()
        .filter(|p| p.enabled && !p.is_empty())
        .map(|p| {
            if p.value.is_empty() {
                p.key.clone()
            } else {
                format!("{}={}", p.key, p.value)
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(v) => {
                        out.push(v);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_params_roundtrip() {
        let mut r = Request {
            url: "https://api.test/items?page=2&q=hello%20world&flag".into(),
            ..Default::default()
        };
        r.sync_params_from_url();
        assert_eq!(r.params.len(), 3);
        assert_eq!(r.params[1].key, "q");
        assert_eq!(r.params[1].value, "hello%20world");
        assert_eq!(r.params[2].key, "flag");

        r.params[0].enabled = false;
        r.sync_url_from_params();
        assert_eq!(r.url, "https://api.test/items?q=hello%20world&flag");

        // disabled param survives a manual URL edit
        r.url.push_str("&x=1");
        r.sync_params_from_url();
        assert_eq!(r.params.len(), 4);
        assert!(r.params.iter().any(|p| p.key == "page" && !p.enabled));
    }

    #[test]
    fn decode() {
        assert_eq!(url_decode("a+b%20c%2F"), "a b c/");
        assert_eq!(url_encode("a b/c"), "a+b%2Fc");
    }
}
