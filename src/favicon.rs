//! Build-time favicon fetching and asset resolution.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine};
use regex::Regex;
use url::Url;

use crate::config::Config;
use crate::icons;
/// A resolved favicon: normalized bytes and the extension to save with.
pub struct FetchedIcon {
    pub data: Vec<u8>,
    pub ext: &'static str,
    pub warning: Option<String>,
}

fn is_private_host(host: &str) -> bool {
    if host.is_empty() {
        return true;
    }
    if host == "localhost" || host.ends_with(".local") || host.ends_with(".lan") {
        return true;
    }
    // Rust's std has no IP classifier; handle the common ranges manually.
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
            // Loopback, unique-local (fc00::/7), and link-local (fe80::/10).
            // Checked via the first segment to avoid newer std APIs (MSRV 1.75).
            std::net::IpAddr::V6(v6) => {
                let seg0 = v6.segments()[0];
                v6.is_loopback() || (seg0 & 0xfe00) == 0xfc00 || (seg0 & 0xffc0) == 0xfe80
            }
        };
    }
    false
}

pub fn is_private_host_pub(host: &str) -> bool {
    is_private_host(host)
}

fn http_client(timeout: Duration) -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(timeout)
        .danger_accept_invalid_certs(true)
        .user_agent("Landing-Favicon/2.0")
        .build()?)
}

struct Fetch {
    data: Vec<u8>,
    content_type: String,
}

fn fetch_bytes(client: &reqwest::blocking::Client, url: &str, max_bytes: usize) -> Result<Fetch> {
    let response = client.get(url).send()?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = response.bytes()?;
    if bytes.len() > max_bytes {
        anyhow::bail!("response too large");
    }
    Ok(Fetch {
        data: bytes.to_vec(),
        content_type,
    })
}

// Compiled once and shared across every link/page (OnceLock keeps MSRV at 1.75).
fn link_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)<link\b[^>]*>").unwrap())
}
fn rel_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)rel\s*=\s*["']([^"']*)["']"#).unwrap())
}
fn href_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)href\s*=\s*["']([^"']*)["']"#).unwrap())
}
fn sizes_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)sizes\s*=\s*["']([^"']*)["']"#).unwrap())
}
fn danger_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)<script|on\w+\s*=|javascript:").unwrap())
}

fn discover_icon_urls(page_url: &str, page_html: &str) -> Vec<String> {
    let link_re = link_re();
    let rel_re = rel_re();
    let href_re = href_re();
    let sizes_re = sizes_re();
    let icon_types = [
        "icon",
        "shortcut icon",
        "apple-touch-icon",
        "apple-touch-icon-precomposed",
        "mask-icon",
    ];

    let base = Url::parse(page_url).ok();
    let mut found: Vec<(i64, String)> = Vec::new();
    for tag in link_re.find_iter(page_html).map(|m| m.as_str()) {
        let (Some(rel_m), Some(href_m)) = (rel_re.captures(tag), href_re.captures(tag)) else {
            continue;
        };
        let rel = rel_m[1]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if !icon_types.contains(&rel.as_str()) {
            continue;
        }
        let href = html_unescape(href_m[1].trim());
        if href.is_empty() || href.starts_with("data:") {
            continue;
        }
        let mut score = match rel.as_str() {
            "icon" => 30,
            "shortcut icon" => 25,
            _ if rel.starts_with("apple-touch-icon") => 20,
            "mask-icon" => 10,
            _ => 0,
        };
        if let Some(sizes) = sizes_re.captures(tag) {
            for size in sizes[1].to_lowercase().split_whitespace() {
                if let Some((w, _)) = size.split_once('x') {
                    if let Ok(w) = w.parse::<i64>() {
                        score += w / 16;
                    }
                }
            }
        }
        let absolute = match &base {
            Some(base) => base
                .join(&href)
                .map(|u| u.to_string())
                .unwrap_or(href.clone()),
            None => href.clone(),
        };
        found.push((score, absolute));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    found.into_iter().map(|(_, url)| url).collect()
}

fn normalize_icon(data: &[u8], content_type: &str, url: &str) -> Option<(Vec<u8>, &'static str)> {
    let lowered = content_type.to_lowercase();
    if lowered.contains("svg") || url.to_lowercase().ends_with(".svg") {
        let text = String::from_utf8_lossy(data);
        if !text.to_lowercase().contains("<svg") {
            return None;
        }
        let danger = danger_re();
        if danger.is_match(&text) {
            return None;
        }
        return Some((data.to_vec(), "svg"));
    }
    let image = image::load_from_memory(data).ok()?;
    if image.width() < 8 || image.height() < 8 {
        return None;
    }
    let thumb = image.thumbnail(128, 128);
    let mut buffer = Cursor::new(Vec::new());
    thumb.write_to(&mut buffer, image::ImageFormat::Png).ok()?;
    Some((buffer.into_inner(), "png"))
}

fn html_unescape(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

/// Fetch one link's favicon, trying favicon_url, url, then lan_url.
pub fn fetch_favicon(link: &crate::config::Link, config: &Config) -> Result<Option<FetchedIcon>> {
    let favicons = &config.favicons;
    let timeout = Duration::from_millis(favicons.timeout_ms.max(1) as u64);
    let max_bytes = favicons.max_bytes.max(1) as usize;
    let client = http_client(timeout)?;

    let sources = [
        link.favicon_url.as_str(),
        link.url.as_str(),
        link.lan_url.as_str(),
    ];
    for source in sources {
        if source.is_empty() {
            continue;
        }
        let Ok(parsed) = Url::parse(source) else {
            continue;
        };
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            continue;
        }
        let host = parsed.host_str().unwrap_or("");
        if is_private_host(host) && !favicons.allow_private_hosts {
            continue;
        }
        let other = if parsed.scheme() == "http" {
            "https"
        } else {
            "http"
        };
        let mut swapped = parsed.clone();
        let _ = swapped.set_scheme(other);
        let mut attempts = vec![(source.to_string(), false)];
        if swapped.as_str() != source {
            attempts.push((swapped.to_string(), true));
        }

        for (attempt, is_fallback) in attempts {
            let mut candidates: Vec<String> = Vec::new();
            if let Ok(fetch) = fetch_bytes(&client, &attempt, max_bytes) {
                let head = &fetch.data[..fetch.data.len().min(512)];
                let looks_html = fetch.content_type.to_lowercase().contains("html")
                    || String::from_utf8_lossy(head)
                        .to_lowercase()
                        .contains("<html");
                if looks_html {
                    let page = String::from_utf8_lossy(&fetch.data);
                    candidates.extend(discover_icon_urls(&attempt, &page));
                }
            }
            if let Ok(parsed_attempt) = Url::parse(&attempt) {
                let origin = format!(
                    "{}://{}{}",
                    parsed_attempt.scheme(),
                    parsed_attempt.host_str().unwrap_or(""),
                    parsed_attempt
                        .port()
                        .map(|p| format!(":{p}"))
                        .unwrap_or_default()
                );
                candidates.push(format!("{origin}/favicon.ico"));
            }
            let mut seen = std::collections::HashSet::new();
            for candidate in candidates {
                if !seen.insert(candidate.clone()) {
                    continue;
                }
                let Ok(fetch) = fetch_bytes(&client, &candidate, max_bytes) else {
                    continue;
                };
                if let Some((data, ext)) =
                    normalize_icon(&fetch.data, &fetch.content_type, &candidate)
                {
                    let warning = if is_fallback {
                        Some(format!(
                            "{}: configured URL {} did not respond; it works over {}://. Update the config to {}.",
                            link.slug, source, other, swapped
                        ))
                    } else {
                        None
                    };
                    return Ok(Some(FetchedIcon { data, ext, warning }));
                }
            }
        }
    }
    Ok(None)
}

/// Resolve and cache favicons for all links. Returns slug -> path (relative to root).
pub fn resolve_favicons(
    config: &Config,
    root: &Path,
    refresh: bool,
    warnings: &mut Vec<String>,
) -> Result<std::collections::BTreeMap<String, String>> {
    let mut resolved = std::collections::BTreeMap::new();
    if !config.favicons.enabled {
        return Ok(resolved);
    }
    let cache_dir = root.join(&config.favicons.cache_dir);
    fs::create_dir_all(&cache_dir)?;

    for group in &config.groups {
        for link in &group.links {
            // Only autoload a favicon when the link explicitly asks for `auto`.
            // An explicit icon (built-in name, path, or URL) is a deliberate
            // choice and always wins, which also keeps offline builds fast and
            // deterministic.
            if link.icon != "auto" {
                continue;
            }
            let slug = &link.slug;
            let existing = existing_cached(&cache_dir, slug);
            if let Some(path) = existing {
                if !refresh && !link.favicon_refresh {
                    resolved.insert(slug.clone(), relative_path(&path, root));
                    continue;
                }
            }
            let fetched = match fetch_favicon(link, config) {
                Ok(Some(icon)) => icon,
                Ok(None) => continue,
                Err(_) => continue,
            };
            if let Some(warning) = fetched.warning {
                warnings.push(warning);
            }
            for stale in cached_files(&cache_dir, slug) {
                let _ = fs::remove_file(stale);
            }
            let target = cache_dir.join(format!("{slug}.{}", fetched.ext));
            fs::write(&target, &fetched.data)?;
            resolved.insert(slug.clone(), relative_path(&target, root));
        }
    }
    Ok(resolved)
}

fn cached_files(cache_dir: &Path, slug: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(cache_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&format!("{slug}.")) {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    files
}

fn existing_cached(cache_dir: &Path, slug: &str) -> Option<PathBuf> {
    cached_files(cache_dir, slug).into_iter().next()
}

fn relative_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Encode a file as a data URI.
pub fn data_uri(path: &Path) -> Result<String> {
    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    };
    let bytes = fs::read(path)?;
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

/// Build a data URI favicon from a built-in icon.
pub fn icon_data_uri(name: &str, background: &str, foreground: &str) -> String {
    let body = icons::body(name)
        .or_else(|| icons::body("cat"))
        .unwrap_or("");
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><rect width=\"24\" height=\"24\" rx=\"6\" fill=\"{background}\"/><g fill=\"none\" stroke=\"{foreground}\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{body}</g></svg>"
    );
    format!("data:image/svg+xml;base64,{}", STANDARD.encode(svg))
}
