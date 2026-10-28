use base64::Engine;

use crate::model::{parse_query, url_decode, BodyKind, KeyValue, Method, Request};

/// Parse a curl command (as copied from browser devtools / Postman) into a request.
pub fn parse(cmd: &str) -> Result<Request, String> {
    // Join line continuations: `\` + newline (bash) and `^` + newline (cmd.exe).
    let joined = cmd
        .replace("\\\r\n", " ")
        .replace("\\\n", " ")
        .replace("^\r\n", " ")
        .replace("^\n", " ");
    let tokens = shell_words::split(joined.trim()).map_err(|e| format!("Ошибка разбора: {e}"))?;

    let mut iter = tokens.into_iter().peekable();
    match iter.next() {
        Some(first) if first == "curl" || first.ends_with("/curl") => {}
        _ => return Err("Команда должна начинаться с curl".into()),
    }

    let mut req = Request::default();
    let mut method: Option<Method> = None;
    let mut data: Vec<String> = Vec::new();
    let mut force_get = false;
    let mut head = false;

    while let Some(tok) = iter.next() {
        // Support --flag=value form for long options.
        let (flag, inline) = match tok.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_owned(), Some(v.to_owned())),
            _ => (tok.clone(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| iter.next())
                .ok_or_else(|| format!("Флаг {name} без значения"))
        };

        match flag.as_str() {
            "-X" | "--request" => {
                let m = value(&flag)?;
                method = Some(Method::parse(&m).ok_or_else(|| format!("Неизвестный метод {m}"))?);
            }
            "-H" | "--header" => {
                let h = value(&flag)?;
                if let Some((k, v)) = h.split_once(':') {
                    req.headers.push(KeyValue::new(k.trim(), v.trim()));
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii" => {
                data.push(value(&flag)?);
            }
            "--data-urlencode" => {
                let v = value(&flag)?;
                let encoded = match v.split_once('=') {
                    Some((k, val)) => format!("{k}={}", crate::model::url_encode(val)),
                    None => crate::model::url_encode(&v),
                };
                data.push(encoded);
            }
            "--json" => {
                data.push(value(&flag)?);
                if !req.headers.iter().any(|h| h.key.eq_ignore_ascii_case("content-type")) {
                    req.headers.push(KeyValue::new("Content-Type", "application/json"));
                }
            }
            "-u" | "--user" => {
                let creds = value(&flag)?;
                let b64 = base64::engine::general_purpose::STANDARD.encode(creds);
                req.headers.push(KeyValue::new("Authorization", format!("Basic {b64}")));
            }
            "-b" | "--cookie" => {
                req.headers.push(KeyValue::new("Cookie", value(&flag)?));
            }
            "-A" | "--user-agent" => {
                req.headers.push(KeyValue::new("User-Agent", value(&flag)?));
            }
            "-e" | "--referer" => {
                req.headers.push(KeyValue::new("Referer", value(&flag)?));
            }
            "--url" => req.url = value(&flag)?,
            "-G" | "--get" => force_get = true,
            "-I" | "--head" => head = true,
            "-F" | "--form" => {
                return Err("multipart/form-data (-F) пока не поддерживается".into());
            }
            // Options that take a value but do not matter for us.
            "-o" | "--output" | "-m" | "--max-time" | "--connect-timeout" | "-x" | "--proxy"
            | "-w" | "--write-out" | "--retry" | "-c" | "--cookie-jar" => {
                value(&flag)?;
            }
            f if f.starts_with('-') => {} // --compressed, -L, -k, -s, -i, -v ...
            _ => {
                if req.url.is_empty() {
                    req.url = tok;
                }
            }
        }
    }

    if req.url.is_empty() {
        return Err("В команде не найден URL".into());
    }

    let body = data.join("&");
    if force_get && !body.is_empty() {
        let sep = if req.url.contains('?') { '&' } else { '?' };
        req.url = format!("{}{sep}{body}", req.url);
    } else if !body.is_empty() {
        let ct = req
            .headers
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case("content-type"))
            .map(|h| h.value.to_ascii_lowercase())
            .unwrap_or_default();
        let looks_json = matches!(body.trim_start().chars().next(), Some('{') | Some('['));

        if ct.contains("json") || (ct.is_empty() && looks_json) {
            req.body_kind = BodyKind::Json;
            req.body = pretty_if_json(&body);
        } else if ct.contains("x-www-form-urlencoded") || ct.is_empty() {
            req.body_kind = BodyKind::FormUrlEncoded;
            req.form = parse_query(&body)
                .into_iter()
                .map(|kv| KeyValue::new(url_decode(&kv.key), url_decode(&kv.value)))
                .collect();
        } else {
            req.body_kind = BodyKind::Text;
            req.body = body;
        }
    }

    req.method = match (method, head) {
        (Some(m), _) => m,
        (None, true) => Method::Head,
        (None, false) if req.body_kind != BodyKind::None => Method::Post,
        _ => Method::Get,
    };

    req.sync_params_from_url();
    Ok(req)
}

/// Build a copy-pasteable curl command (bash quoting).
pub fn build(req: &Request) -> String {
    let mut parts = vec![format!("curl -X {} {}", req.method.as_str(), quote(&req.url))];
    for h in req.headers.iter().filter(|h| h.enabled && !h.key.is_empty()) {
        parts.push(format!("-H {}", quote(&format!("{}: {}", h.key, h.value))));
    }
    if let Some(body) = req.body_payload() {
        if let Some(ct) = req.body_kind.content_type() {
            if !req.has_header("content-type") {
                parts.push(format!("-H {}", quote(&format!("Content-Type: {ct}"))));
            }
        }
        if !body.is_empty() {
            parts.push(format!("--data-raw {}", quote(&body)));
        }
    }
    parts.join(" \\\n  ")
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn pretty_if_json(s: &str) -> String {
    serde_json::from_str::<serde_json::Value>(s)
        .and_then(|v| serde_json::to_string_pretty(&v))
        .unwrap_or_else(|_| s.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_devtools_curl() {
        let cmd = r#"curl 'https://api.example.com/v1/users?page=1&limit=20' \
  -H 'accept: application/json' \
  -H 'authorization: Bearer abc' \
  --data-raw '{"name":"it'\''s me"}' \
  --compressed"#;
        let r = parse(cmd).unwrap();
        assert_eq!(r.method, Method::Post);
        assert_eq!(r.url, "https://api.example.com/v1/users?page=1&limit=20");
        assert_eq!(r.params.len(), 2);
        assert_eq!(r.headers.len(), 2);
        assert_eq!(r.body_kind, BodyKind::Json);
        assert!(r.body.contains("it's me"));
    }

    #[test]
    fn form_and_get() {
        let r = parse("curl -X PUT https://x.io/a -d 'a=1' -d 'b=hello+world'").unwrap();
        assert_eq!(r.method, Method::Put);
        assert_eq!(r.body_kind, BodyKind::FormUrlEncoded);
        assert_eq!(r.form[1].value, "hello world");

        let r = parse("curl -G https://x.io/a --data-urlencode 'q=a b'").unwrap();
        assert_eq!(r.method, Method::Get);
        assert_eq!(r.url, "https://x.io/a?q=a+b");
    }

    #[test]
    fn roundtrip() {
        let original = parse(
            "curl -X PATCH 'https://x.io/a?z=1' -H 'Content-Type: application/json' --data-raw '{\"k\":\"it'\\''s\"}'",
        )
        .unwrap();
        let again = parse(&build(&original)).unwrap();
        assert_eq!(again.method, Method::Patch);
        assert_eq!(again.url, original.url);
        assert_eq!(again.body, original.body);
    }
}
