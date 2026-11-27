//! Security policy construction, mirroring `scripts/build.py` collect_* helpers.

use std::collections::BTreeSet;

use base64::{engine::general_purpose::STANDARD, Engine};
use sha2::{Digest, Sha256};

use crate::config::Config;

fn origin_of(value: &str) -> Option<String> {
    let parsed = url::Url::parse(value).ok()?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let host = parsed.host_str()?;
    match parsed.port() {
        Some(port) => Some(format!("{scheme}://{host}:{port}")),
        None => Some(format!("{scheme}://{host}")),
    }
}

/// Origins that the page connects to for probes.
pub fn collect_origins(config: &Config) -> BTreeSet<String> {
    let mut origins = BTreeSet::new();
    let mut add = |value: &str| {
        if let Some(origin) = origin_of(value) {
            origins.insert(origin);
        }
    };
    for url in &config.probe.internet.urls {
        add(url);
    }
    for group in &config.groups {
        for link in &group.links {
            add(&link.lan_url);
            add(&link.probe_url);
        }
    }
    for url in &config.probe.lan.urls {
        add(url);
    }
    if let Some(fallback) = &config.probe.lan.ip_fallback {
        add(fallback);
    }
    if config.probe.lan.random_subdomain {
        if let Some(domain) = &config.probe.lan.domain {
            if !domain.is_empty() {
                origins.insert(format!("https://*.{domain}"));
                origins.insert(format!("http://*.{domain}"));
            }
        }
    }
    origins
}

/// Origins that may serve images (icons, backgrounds).
pub fn collect_img_origins(config: &Config) -> BTreeSet<String> {
    let mut origins = BTreeSet::new();
    let mut values: Vec<&String> = Vec::new();
    if let Some(f) = &config.site.favicon {
        values.push(f);
    }
    if let Some(img) = &config.theme.background.image {
        values.push(img);
    }
    if let Some(img) = &config.probe.internet.image {
        values.push(img);
    }
    for group in &config.groups {
        values.push(&group.icon);
        for link in &group.links {
            values.push(&link.icon);
        }
    }
    for value in values {
        if value.starts_with("http://") || value.starts_with("https://") {
            if let Some(origin) = origin_of(value) {
                origins.insert(origin);
            }
        }
    }
    origins
}

fn sha256_b64(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    STANDARD.encode(hasher.finalize())
}

/// Build a Content-Security-Policy string.
pub fn csp_policy(
    config: &Config,
    inline_css: Option<&str>,
    inline_js: Option<&str>,
    config_json: &str,
) -> String {
    let origins = collect_origins(config);
    let connect = {
        let mut parts = vec!["'self'".to_string()];
        parts.extend(origins.iter().cloned());
        parts.join(" ")
    };
    let mut img_origins: BTreeSet<String> = collect_img_origins(config);
    img_origins.extend(origins.iter().cloned());
    let img_src = {
        let mut parts = vec!["'self'".to_string(), "data:".to_string()];
        parts.extend(img_origins.iter().cloned());
        parts.join(" ")
    };
    let config_hash = sha256_b64(config_json);

    match (inline_css, inline_js) {
        (Some(css), Some(js)) => {
            let css_hash = sha256_b64(css);
            let js_hash = sha256_b64(js);
            format!(
                "default-src 'self'; base-uri 'none'; form-action 'none'; object-src 'none'; script-src 'sha256-{js_hash}' 'sha256-{config_hash}'; style-src 'sha256-{css_hash}'; img-src {img_src}; font-src 'self'; connect-src {connect}; manifest-src 'self'; worker-src 'self';"
            )
        }
        _ => format!(
            "default-src 'self'; base-uri 'none'; form-action 'none'; object-src 'none'; script-src 'self' 'sha256-{config_hash}'; style-src 'self'; img-src {img_src}; font-src 'self'; connect-src {connect}; manifest-src 'self'; worker-src 'self';"
        ),
    }
}

/// Wrap a CSP string in a `<meta>` tag.
pub fn csp_meta(
    config: &Config,
    inline_css: Option<&str>,
    inline_js: Option<&str>,
    config_json: &str,
) -> String {
    let csp = csp_policy(config, inline_css, inline_js, config_json);
    format!(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"{}\">",
        html_escape_attr(&csp)
    )
}

/// Escape text for use inside an HTML attribute (double-quoted).
pub fn html_escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}
