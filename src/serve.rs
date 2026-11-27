//! Local loopback server: static files, health, and the probe/suggest APIs.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Method, Response, Server, StatusCode};

pub struct ServeOptions {
    pub root: PathBuf,
    pub config_path: PathBuf,
}

const DEFAULT_HEADERS: &[(&str, &str)] = &[
    ("X-Content-Type-Options", "nosniff"),
    ("Referrer-Policy", "no-referrer"),
    (
        "Permissions-Policy",
        "camera=(), microphone=(), geolocation=(), payment=(), usb=()",
    ),
];

fn load_headers(root: &Path) -> BTreeMap<String, String> {
    let mut headers: BTreeMap<String, String> = DEFAULT_HEADERS
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let header_file = root.join("_headers");
    let Ok(text) = std::fs::read_to_string(header_file) else {
        return headers;
    };
    for line in text.lines() {
        let stripped = line.trim();
        if stripped.is_empty() || stripped.starts_with('#') || stripped.starts_with('/') {
            continue;
        }
        let Some((name, value)) = stripped.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if matches!(
            name,
            "Content-Security-Policy"
                | "Permissions-Policy"
                | "Referrer-Policy"
                | "X-Content-Type-Options"
        ) {
            headers.insert(name.to_string(), value.trim().to_string());
        }
    }
    headers
}

fn build_info(root: &Path) -> (String, String) {
    let info_file = root.parent().map(|p| p.join("build-info.json"));
    let Some(info_file) = info_file else {
        return (String::new(), String::new());
    };
    let Ok(text) = std::fs::read_to_string(info_file) else {
        return (String::new(), String::new());
    };
    let Ok(data) = serde_json::from_str::<Value>(&text) else {
        return (String::new(), String::new());
    };
    let get = |key: &str| {
        data.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    (get("version"), get("build_hash"))
}

fn http_client(timeout: Duration, accept_untrusted: bool) -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(timeout)
        .danger_accept_invalid_certs(accept_untrusted)
        .user_agent("Landing-Probe/2.0")
        .build()?)
}

struct ProbeOutcome {
    ok: bool,
    pin_mismatch: bool,
}

fn probe_url(client: &reqwest::blocking::Client, url: &str, expect: &str) -> ProbeOutcome {
    match client.get(url).send() {
        Ok(response) => {
            let status = response.status().as_u16();
            if !(200..500).contains(&status) {
                return ProbeOutcome {
                    ok: false,
                    pin_mismatch: false,
                };
            }
            if !expect.is_empty() {
                let mut body = String::new();
                let _ = response.take(65536).read_to_string(&mut body);
                return ProbeOutcome {
                    ok: body.contains(expect),
                    pin_mismatch: false,
                };
            }
            ProbeOutcome {
                ok: true,
                pin_mismatch: false,
            }
        }
        Err(_) => ProbeOutcome {
            ok: false,
            pin_mismatch: false,
        },
    }
}

/// A certificate verifier that accepts any chain but records the leaf, so a
/// pin can be computed from self-signed LAN services (equivalent to Python's
/// `CERT_NONE` context, which still exposed `getpeercert(binary_form=True)`).
#[derive(Debug)]
struct CapturingVerifier {
    leaf: std::sync::Mutex<Option<Vec<u8>>>,
}

impl rustls::client::danger::ServerCertVerifier for CapturingVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        *self.leaf.lock().unwrap() = Some(end_entity.as_ref().to_vec());
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Probe LAN entries concurrently. Returns each entry's URL, outcome, and the
/// captured certificate fingerprint when a pin was requested. Order matches
/// `entries` so callers can zip results back to their config.
fn probe_lan_entries(
    client: &reqwest::blocking::Client,
    entries: &[Value],
    pin_enabled: bool,
    pins: &Value,
    timeout: Duration,
) -> Vec<(String, ProbeOutcome, Option<String>)> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = entries
            .iter()
            .map(|entry| {
                scope.spawn(move || {
                    let url = entry
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let expect = entry.get("expect").and_then(|v| v.as_str()).unwrap_or("");
                    let mut outcome = probe_url(client, &url, expect);
                    let mut fingerprint = None;
                    if pin_enabled && outcome.ok {
                        if let Some(expected) = pins.get(&url).and_then(|v| v.as_str()) {
                            fingerprint = cert_fingerprint(&url, timeout);
                            if fingerprint.as_deref() != Some(expected) {
                                outcome.ok = false;
                                outcome.pin_mismatch = true;
                            }
                        }
                    }
                    (url, outcome, fingerprint)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join().unwrap_or_else(|_| {
                    (
                        String::new(),
                        ProbeOutcome {
                            ok: false,
                            pin_mismatch: false,
                        },
                        None,
                    )
                })
            })
            .collect()
    })
}

fn cert_fingerprint(url: &str, timeout: Duration) -> Option<String> {
    // reqwest does not expose the peer certificate; use rustls directly for pins.
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    let host = parsed.host_str()?.to_string();
    let port = parsed.port_or_known_default().unwrap_or(443);
    let verifier = Arc::new(CapturingVerifier {
        leaf: std::sync::Mutex::new(None),
    });
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    let server_name = rustls::pki_types::ServerName::try_from(host).ok()?;
    let mut conn = rustls::ClientConnection::new(Arc::new(config), server_name).ok()?;
    let mut sock = std::net::TcpStream::connect((parsed.host_str()?, port)).ok()?;
    sock.set_read_timeout(Some(timeout)).ok()?;
    // Drive the TLS handshake until rustls reports it complete.
    while conn.is_handshaking() {
        if conn.wants_write() {
            conn.write_tls(&mut sock).ok()?;
        }
        if conn.wants_read() {
            conn.read_tls(&mut sock).ok()?;
            conn.process_new_packets().ok()?;
        } else if !conn.is_handshaking() {
            break;
        }
    }
    let leaf = verifier.leaf.lock().unwrap().clone()?;
    let mut hasher = Sha256::new();
    hasher.update(&leaf);
    Some(format!("{:x}", hasher.finalize()))
}

fn detect_captive_portal(probe: &Value, client: &reqwest::blocking::Client) -> bool {
    let captive = probe.get("captive_portal").cloned().unwrap_or(Value::Null);
    if !captive
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
    {
        return false;
    }
    let Some(checks) = captive.get("urls").and_then(|v| v.as_array()) else {
        return false;
    };
    for check in checks {
        let Some(url) = check.get("url").and_then(|v| v.as_str()) else {
            continue;
        };
        let (status, body) = match client.get(url).send() {
            Ok(response) => {
                let status = response.status().as_u16();
                let mut body = String::new();
                let _ = response.take(65536).read_to_string(&mut body);
                (Some(status), body)
            }
            Err(_) => (None, String::new()),
        };
        let Some(status) = status else {
            continue;
        };
        if let Some(expect) = check.get("expect_status").and_then(|v| v.as_i64()) {
            if status as i64 != expect {
                return true;
            }
        }
        if let Some(expect_body) = check.get("expect_body").and_then(|v| v.as_str()) {
            if !body.contains(expect_body) {
                return true;
            }
        }
    }
    false
}

fn probe_location(config: &Value, slug: Option<&str>) -> Value {
    let probe = config.get("probe").cloned().unwrap_or(Value::Null);
    let timeout = Duration::from_millis(
        probe
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(2500),
    );
    let accept_untrusted = probe
        .get("accept_untrusted_cert")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let client = match http_client(timeout, accept_untrusted) {
        Ok(c) => c,
        Err(_) => return json!({"error": "client"}),
    };
    let empty = Vec::new();
    let entries = probe
        .get("lan")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty)
        .iter()
        .filter(|e| e.get("url").and_then(|v| v.as_str()).is_some())
        .filter(|e| match slug {
            Some(s) => e.get("slug").and_then(|v| v.as_str()) == Some(s),
            None => true,
        })
        .cloned()
        .collect::<Vec<_>>();

    let captive = detect_captive_portal(&probe, &client);
    let pin_enabled = probe
        .get("pin_certificates")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let pins = probe.get("pins").cloned().unwrap_or(json!({}));

    // Probe LAN entries concurrently, bounded like the original Python pool, so
    // an "away" result costs ~one timeout instead of N sequential timeouts.
    let outcomes = probe_lan_entries(&client, &entries, pin_enabled, &pins, timeout);
    let ok_count = outcomes.iter().filter(|(_, o, _)| o.ok).count();
    let tried: Vec<Value> = entries
        .iter()
        .zip(outcomes)
        .map(|(entry, (url, outcome, expected_pin))| {
            let _ = expected_pin;
            json!({
                "slug": entry.get("slug").and_then(|v| v.as_str()).unwrap_or(""),
                "url": url,
                "ok": outcome.ok,
                "pin_mismatch": outcome.pin_mismatch,
            })
        })
        .collect();

    let home = if slug.is_some() {
        !tried.is_empty()
            && tried[0]
                .get("ok")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
    } else {
        !tried.is_empty() && ok_count * 2 > tried.len()
    };
    let home = home && !captive;
    json!({
        "home": home,
        "captivePortal": captive,
        "tried": tried,
        "okCount": ok_count,
        "total": entries.len(),
    })
}

fn probe_connectivity(config: &Value) -> Value {
    let probe = config.get("probe").cloned().unwrap_or(Value::Null);
    let timeout = Duration::from_millis(
        probe
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(2500),
    );
    let accept_untrusted = probe
        .get("accept_untrusted_cert")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let client = match http_client(timeout, accept_untrusted) {
        Ok(c) => c,
        Err(_) => return json!({"error": "client"}),
    };
    let captive = detect_captive_portal(&probe, &client);
    let empty = Vec::new();
    let urls: Vec<String> = probe
        .get("internet")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty)
        .iter()
        .filter_map(|u| u.as_str().map(|s| s.to_string()))
        .collect();
    // Probe all internet URLs concurrently and take any(ok); the offline case
    // then costs ~one timeout instead of N.
    let online = if urls.is_empty() {
        None
    } else {
        let any_ok = std::thread::scope(|scope| {
            let handles: Vec<_> = urls
                .iter()
                .map(|u| {
                    let client = &client;
                    scope.spawn(move || probe_url(client, u, "").ok)
                })
                .collect();
            handles.into_iter().any(|h| h.join().unwrap_or(false))
        });
        Some(any_ok)
    };
    let online = if captive { Some(false) } else { online };
    json!({ "online": online, "captivePortal": captive })
}

fn fetch_suggestions(config: &Value, query: &str) -> Vec<String> {
    let search = config.get("search").cloned().unwrap_or(Value::Null);
    let template = search
        .get("suggest_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if template.is_empty() || !template.contains("{query}") {
        return Vec::new();
    }
    let limit = search
        .get("suggest_limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(6) as usize;
    let url = template.replace(
        "{query}",
        &percent_encoding::utf8_percent_encode(query, percent_encoding::NON_ALPHANUMERIC)
            .to_string(),
    );
    // Provider TLS should follow the same verification policy as probes;
    // only skip verification when the user explicitly allows untrusted certs.
    let accept_untrusted = config
        .get("probe")
        .and_then(|p| p.get("accept_untrusted_cert"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let client = match http_client(Duration::from_secs(3), accept_untrusted) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let Ok(response) = client.get(&url).send() else {
        return Vec::new();
    };
    let Ok(payload) = response.json::<Value>() else {
        return Vec::new();
    };
    let mut items: Vec<String> = Vec::new();
    match payload {
        Value::Array(arr) => {
            if arr.len() >= 2 && arr[1].is_array() {
                for item in arr[1].as_array().unwrap() {
                    if let Some(s) = item.as_str() {
                        items.push(s.to_string());
                    }
                }
            } else {
                for entry in arr {
                    match entry {
                        Value::String(s) => items.push(s),
                        Value::Array(a) => {
                            if let Some(Value::String(s)) = a.first() {
                                items.push(s.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Value::Object(map) => {
            for key in ["suggestions", "results", "data"] {
                if let Some(Value::Array(list)) = map.get(key) {
                    for entry in list {
                        match entry {
                            Value::String(s) => items.push(s.clone()),
                            Value::Object(o) => {
                                for k in ["phrase", "suggestion", "query"] {
                                    if let Some(Value::String(s)) = o.get(k) {
                                        items.push(s.clone());
                                        break;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        _ => {}
    }
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for item in items {
        let trimmed = item.trim();
        let key = trimmed.to_lowercase();
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        unique.push(trimmed.to_string());
        if unique.len() >= limit {
            break;
        }
    }
    unique
}

fn mime_for(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "webmanifest" => "application/manifest+json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn resolve_path(root: &Path, path: &str) -> Option<PathBuf> {
    let rel = path.trim_start_matches('/');
    let candidate = root.join(rel);
    let canonical = candidate.canonicalize().ok()?;
    let root_canon = root.canonicalize().ok()?;
    if !canonical.starts_with(&root_canon) {
        return None;
    }
    let target = if canonical.is_dir() {
        canonical.join("index.html")
    } else {
        canonical
    };
    if target.is_file() {
        Some(target)
    } else {
        None
    }
}

pub fn serve(opts: ServeOptions) -> Result<()> {
    let root = opts.root.canonicalize()?;
    if !root.join("index.html").is_file() {
        anyhow::bail!("no index.html in {}", root.display());
    }
    let config_text = std::fs::read_to_string(&opts.config_path)?;
    let config: Value = serde_json::from_str(&config_text)?;
    let host = config
        .get("host")
        .and_then(|v| v.as_str())
        .unwrap_or("127.0.0.1");
    let port = config.get("port").and_then(|v| v.as_u64()).unwrap_or(8080);
    let health_path = config
        .get("health_path")
        .and_then(|v| v.as_str())
        .unwrap_or("/healthz")
        .to_string();

    let headers = load_headers(&root);
    let (version, build_hash) = build_info(&root);
    let server = Server::http((host, port as u16))
        .map_err(|e| anyhow::anyhow!("cannot bind {host}:{port}: {e}"))?;
    println!(
        "Landing serving {} at http://{host}:{port}/ (health {health_path})",
        root.display()
    );

    let config = Arc::new(Mutex::new(config));
    for request in server.incoming_requests() {
        let root = root.clone();
        let headers = headers.clone();
        let version = version.clone();
        let build_hash = build_hash.clone();
        let health_path = health_path.clone();
        let config = Arc::clone(&config);
        std::thread::spawn(move || {
            handle(
                request,
                &root,
                &headers,
                &version,
                &build_hash,
                &health_path,
                &config,
            );
        });
    }
    Ok(())
}

fn json_response(
    payload: &Value,
    headers: &BTreeMap<String, String>,
) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response_with_status(payload, headers, StatusCode(200))
}

fn json_response_with_status(
    payload: &Value,
    headers: &BTreeMap<String, String>,
    status: StatusCode,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_string(payload).unwrap_or_default() + "\n";
    let mut response = Response::from_string(body).with_status_code(status);
    add_headers(&mut response, headers);
    response.add_header(header("Content-Type", "application/json"));
    response.add_header(header("Cache-Control", "no-store"));
    response
}

fn add_headers(
    response: &mut Response<std::io::Cursor<Vec<u8>>>,
    headers: &BTreeMap<String, String>,
) {
    for (name, value) in headers {
        response.add_header(header(name, value));
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

#[allow(clippy::too_many_arguments)]
fn handle(
    request: tiny_http::Request,
    root: &Path,
    headers: &BTreeMap<String, String>,
    version: &str,
    build_hash: &str,
    health_path: &str,
    config: &Mutex<Value>,
) {
    let method = request.method().clone();
    if method != Method::Get && method != Method::Head {
        let _ = request.respond(Response::empty(StatusCode(405)));
        return;
    }
    let send_body = method == Method::Get;
    let url = request.url().to_string();
    let (raw_path, raw_query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    let path = percent_encoding::percent_decode_str(raw_path)
        .decode_utf8_lossy()
        .to_string();

    let response = if path == health_path {
        let healthy = root.join("index.html").is_file();
        let payload = json!({
            "status": if healthy { "ok" } else { "error" },
            "site": version,
            "build": build_hash,
        });
        let status = if healthy {
            StatusCode(200)
        } else {
            StatusCode(503)
        };
        json_response_with_status(&payload, headers, status)
    } else if path == "/api/network" || path == "/api/location" || path == "/api/connectivity" {
        let cfg = config.lock().unwrap().clone();
        // server.probe: false disables server-side probing; return an empty
        // object so the client falls back to its own browser probes.
        let enabled = cfg
            .get("server_probe")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if !enabled {
            json_response(&json!({}), headers)
        } else if path == "/api/network" {
            let payload = json!({
                "internet": probe_connectivity(&cfg).get("online").cloned().unwrap_or(Value::Null),
                "lan": probe_location(&cfg, None).get("home").cloned().unwrap_or(Value::Null),
            });
            json_response(&payload, headers)
        } else if path == "/api/location" {
            let slug = query_param(raw_query, "slug");
            json_response(&probe_location(&cfg, slug.as_deref()), headers)
        } else {
            json_response(&probe_connectivity(&cfg), headers)
        }
    } else if path == "/api/suggest" {
        let cfg = config.lock().unwrap().clone();
        let query = query_param(raw_query, "q").unwrap_or_default();
        let items = if query.is_empty() {
            Vec::new()
        } else {
            fetch_suggestions(&cfg, &query)
        };
        json_response(&json!({ "suggestions": items }), headers)
    } else {
        match resolve_path(root, &path) {
            Some(target) => {
                let body = std::fs::read(&target).unwrap_or_default();
                let mut response = Response::from_data(body).with_status_code(StatusCode(200));
                response.add_header(header(
                    "Content-Type",
                    mime_for(&target.display().to_string()),
                ));
                for (name, value) in headers {
                    if name == "Content-Security-Policy" && path.starts_with("/go/") {
                        continue;
                    }
                    response.add_header(header(name, value));
                }
                let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let cache = if matches!(
                    name,
                    "index.html" | "sw.js" | "styles.css" | "app.js" | "manifest.webmanifest"
                ) {
                    "no-cache"
                } else {
                    "public, max-age=86400"
                };
                response.add_header(header("Cache-Control", cache));
                response
            }
            None => {
                let page = root.join("404.html");
                let (body, ctype) = if page.is_file() {
                    (
                        std::fs::read(&page).unwrap_or_default(),
                        "text/html; charset=utf-8",
                    )
                } else {
                    (b"Not found\n".to_vec(), "text/plain; charset=utf-8")
                };
                let mut response = Response::from_data(body).with_status_code(StatusCode(404));
                response.add_header(header("Content-Type", ctype));
                for (name, value) in headers {
                    response.add_header(header(name, value));
                }
                response.add_header(header("Cache-Control", "no-store"));
                response
            }
        }
    };

    if send_body {
        let _ = request.respond(response);
    } else {
        // HEAD: send no body but preserve every header (Content-Type, CSP, and
        // the security headers) so HEAD clients see the same policy as GET.
        let status = response.status_code();
        let mut head = Response::empty(status);
        for header in response.headers() {
            if header.field.equiv("Content-Length") {
                continue;
            }
            head.add_header(header.clone());
        }
        let _ = request.respond(head);
    }
}

fn query_param(query: &str, key: &str) -> Option<String> {
    for part in query.split('&') {
        if let Some(value) = part.strip_prefix(&format!("{key}=")) {
            return Some(
                percent_encoding::percent_decode_str(value)
                    .decode_utf8_lossy()
                    .to_string(),
            );
        }
    }
    None
}
