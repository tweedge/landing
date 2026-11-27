//! The build engine: config -> assets -> rendered site, single-file export,
//! redirect pages, manifest, service worker, and checksums.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::config::{self, Config};
use crate::csp;
use crate::favicon;
use crate::icons;
use crate::pwa::generate_icon;
use crate::templates;
use crate::theme;

pub struct BuildOptions {
    pub config_path: PathBuf,
    pub out_dir: Option<PathBuf>,
    pub single_file: Option<bool>,
    pub quiet: bool,
    pub refresh_favicons: bool,
    pub root: PathBuf,
}

pub struct BuildResult {
    pub build_hash: String,
    pub config_hash: String,
    pub warnings: Vec<String>,
    pub out: PathBuf,
    pub config: Config,
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Canonical JSON for hashing / embedding (sorted keys, compact).
pub fn canonical_json<T: Serialize>(value: &T) -> Result<String> {
    let v = serde_json::to_value(value)?;
    Ok(serde_json::to_string(&sort_value(v))?)
}

fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut sorted = serde_json::Map::new();
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            for key in keys {
                sorted.insert(key.clone(), sort_value(map[&key].clone()));
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_value).collect()),
        other => other,
    }
}

pub fn config_hash(config: &Config) -> Result<String> {
    let json = canonical_json(config)?;
    Ok(sha256_hex(json.as_bytes()))
}

fn version_from_hash(value: &str) -> String {
    let hex = &value[..8.min(value.len())];
    let n = u64::from_str_radix(hex, 16).unwrap_or(0);
    format!("1.0.{n}")
}

struct Assets {
    favicon_url: String,
    manifest_url: String,
    styles_url: String,
    app_js_url: String,
    background_url: Option<String>,
    pwa_icons: Vec<Value>,
}

fn copy_asset(source: &Path, site_dir: &Path, dest_rel: &str) -> Result<String> {
    let dest = site_dir.join(dest_rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, &dest)?;
    Ok(dest_rel.replace('\\', "/"))
}

fn resolve_assets(
    config: &mut Config,
    root: &Path,
    site_dir: &Path,
    single_file: bool,
    favicon_map: &BTreeMap<String, String>,
) -> Result<Assets> {
    let base_path = config.build.base_path.clone();
    fs::create_dir_all(site_dir.join("assets"))?;

    let dark_bg = config.theme.colors.dark.bg.clone();
    let theme_accent = if config.theme.accent.is_empty() {
        config.theme.colors.dark.accent.clone()
    } else {
        config.theme.accent.clone()
    };

    let mut favicon_url = String::new();
    if let Some(favicon) = &config.site.favicon {
        if icons::body(favicon).is_some() {
            favicon_url = favicon::icon_data_uri(favicon, &dark_bg, &theme_accent);
        } else if favicon.starts_with("http://") || favicon.starts_with("https://") {
            favicon_url = favicon.clone();
        } else {
            let source = root.join(favicon);
            if source.exists() {
                if single_file {
                    favicon_url = favicon::data_uri(&source)?;
                } else {
                    let ext = source
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();
                    let rel = copy_asset(&source, site_dir, &format!("assets/favicon.{ext}"))?;
                    favicon_url = format!("{base_path}{rel}");
                }
            }
        }
    }

    let mut background_url = None;
    if let Some(image) = &config.theme.background.image {
        if image.starts_with("http://") || image.starts_with("https://") {
            background_url = Some(image.clone());
        } else {
            let source = root.join(image);
            if source.exists() {
                if single_file && source.metadata().map(|m| m.len()).unwrap_or(0) <= 250_000 {
                    background_url = Some(favicon::data_uri(&source)?);
                } else if !single_file {
                    let ext = source
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();
                    let rel = copy_asset(&source, site_dir, &format!("assets/background.{ext}"))?;
                    background_url = Some(format!("{base_path}{rel}"));
                }
            }
        }
    }

    let accent = if config.theme.accent.is_empty() {
        config.theme.colors.dark.accent.clone()
    } else {
        config.theme.accent.clone()
    };

    let mut pwa_icons: Vec<Value> = Vec::new();
    if config.pwa.enabled {
        let configured = config.pwa.icons.clone();
        if !configured.is_empty() {
            for icon in configured {
                let source = root.join(&icon.src);
                if !source.exists() {
                    bail!("PWA icon not found: {}", icon.src);
                }
                let name = source
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "icon.png".into());
                let rel = copy_asset(&source, site_dir, &format!("icons/{name}"))?;
                pwa_icons.push(json!({
                    "src": format!("{base_path}{rel}"),
                    "sizes": icon.sizes,
                    "type": icon.kind,
                    "purpose": icon.purpose,
                }));
            }
        } else {
            let icons_dir = site_dir.join("icons");
            fs::create_dir_all(&icons_dir)?;
            for (size, name, maskable) in [
                (192u32, "icon-192.png", false),
                (512u32, "icon-512.png", false),
                (512u32, "icon-maskable-512.png", true),
            ] {
                generate_icon(&icons_dir.join(name), size, &accent, maskable)?;
                pwa_icons.push(json!({
                    "src": format!("{base_path}icons/{name}"),
                    "sizes": format!("{size}x{size}"),
                    "type": "image/png",
                    "purpose": if maskable { "maskable" } else { "any" },
                }));
            }
        }
    }

    // Resolve local group/link icon URLs.
    for group in &mut config.groups {
        group.icon_url = resolve_icon_value(
            &group.icon,
            root,
            site_dir,
            single_file,
            &base_path,
            config.build.allow_remote_assets,
        )?;
        for link in &mut group.links {
            let cached = favicon_map.get(&link.slug);
            if let Some(cached) = cached {
                let source = root.join(cached);
                if source.exists() {
                    if single_file {
                        link.icon_url = Some(favicon::data_uri(&source)?);
                    } else {
                        let name = source
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| format!("{}.png", link.slug));
                        let rel =
                            copy_asset(&source, site_dir, &format!("assets/favicons/{name}"))?;
                        link.icon_url = Some(format!("{base_path}{rel}"));
                    }
                    continue;
                }
            }
            if link.icon == "auto" || link.icon == "none" {
                link.icon_url = None;
            } else {
                link.icon_url = resolve_icon_value(
                    &link.icon,
                    root,
                    site_dir,
                    single_file,
                    &base_path,
                    config.build.allow_remote_assets,
                )?;
            }
        }
    }

    Ok(Assets {
        favicon_url,
        manifest_url: format!("{base_path}manifest.webmanifest"),
        styles_url: format!("{base_path}styles.css"),
        app_js_url: format!("{base_path}app.js"),
        background_url,
        pwa_icons,
    })
}

fn resolve_icon_value(
    icon: &str,
    root: &Path,
    site_dir: &Path,
    single_file: bool,
    base_path: &str,
    allow_remote: bool,
) -> Result<Option<String>> {
    if icon.is_empty() || icons::body(icon).is_some() {
        return Ok(None);
    }
    if icon.starts_with("http://") || icon.starts_with("https://") {
        return Ok(if allow_remote {
            Some(icon.into())
        } else {
            None
        });
    }
    let source = root.join(icon);
    if !source.exists() {
        return Ok(None);
    }
    if single_file {
        Ok(Some(favicon::data_uri(&source)?))
    } else {
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "icon".into());
        let rel = copy_asset(&source, site_dir, &format!("assets/icons/{name}"))?;
        Ok(Some(format!("{base_path}{rel}")))
    }
}

fn icon_html(name: &str, label: &str, icon_url: Option<&str>, class_name: &str) -> String {
    if let Some(url) = icon_url {
        return format!(
            "<img class=\"{}\" src=\"{}\" alt=\"\">",
            csp::html_escape_attr(class_name),
            csp::html_escape_attr(url)
        );
    }
    if let Some(svg) = icons::svg(name) {
        return svg;
    }
    format!(
        "<span class=\"icon-letter\">{}</span>",
        csp::html_escape_attr(&icons::initials(label))
    )
}

fn group_views(config: &Config) -> Vec<Value> {
    let base_path = &config.build.base_path;
    let use_redirect = !config.probe.homepage;
    config
        .groups
        .iter()
        .map(|group| {
            let group_icon = icon_html(
                &group.icon,
                &group.name,
                group.icon_url.as_deref(),
                "group-icon-img",
            );
            let links: Vec<Value> = group
                .links
                .iter()
                .map(|link| {
                    let icon_name = if link.icon == "auto" || link.icon == "none" {
                        config.defaults.icon.clone()
                    } else {
                        link.icon.clone()
                    };
                    let icon =
                        icon_html(&icon_name, &link.name, link.icon_url.as_deref(), "icon-img");
                    let href_default = if use_redirect {
                        format!("{base_path}go/{}/", link.slug)
                    } else if !link.url.is_empty() {
                        link.url.clone()
                    } else if !link.lan_url.is_empty() {
                        link.lan_url.clone()
                    } else {
                        "#".into()
                    };
                    let rel = if link.target == "_blank" {
                        "noopener noreferrer"
                    } else {
                        ""
                    };
                    let mut value = serde_json::to_value(link).unwrap();
                    let obj = value.as_object_mut().unwrap();
                    obj.insert("icon_html".into(), json!(icon));
                    obj.insert("href_default".into(), json!(href_default));
                    obj.insert("rel".into(), json!(rel));
                    value
                })
                .collect();
            let mut value = serde_json::to_value(group).unwrap();
            let obj = value.as_object_mut().unwrap();
            obj.insert("icon_html".into(), json!(group_icon));
            obj.insert("links".into(), Value::Array(links));
            value
        })
        .collect()
}

fn runtime_config(config: &Config) -> Value {
    json!({
        "site": config.site,
        "theme": config.theme,
        "layout": config.layout,
        "defaults": config.defaults,
        "search": config.search,
        "probe": config.probe,
        "redirect": config.redirect,
        "server": { "probe": config.server.probe },
        "groups": config.groups,
    })
}

/// Join a config-provided absolute path ("/foo/") onto the build base path, so
/// PWA start_url/scope respect both config and a sub-path deployment.
fn under_base(base_path: &str, value: &str, default: &str) -> String {
    let value = if value.trim().is_empty() {
        default
    } else {
        value.trim()
    };
    if value.starts_with('/') {
        format!("{base_path}{}", value.trim_start_matches('/'))
    } else {
        value.to_string()
    }
}

fn write_manifest(config: &Config, site_dir: &Path, pwa_icons: &[Value]) -> Result<()> {
    if !config.pwa.enabled {
        return Ok(());
    }
    let base_path = &config.build.base_path;
    let mut shortcuts = Vec::new();
    for shortcut in &config.pwa.shortcuts {
        let name = shortcut.name.trim();
        let url = shortcut.url.trim();
        if name.is_empty() || url.is_empty() {
            continue;
        }
        let url = if url.starts_with('/') {
            format!("{base_path}{}", url.trim_start_matches('/'))
        } else {
            url.to_string()
        };
        shortcuts.push(json!({ "name": name, "url": url }));
    }
    let mut manifest = serde_json::Map::new();
    manifest.insert(
        "name".into(),
        json!(if config.pwa.name.is_empty() {
            &config.site.title
        } else {
            &config.pwa.name
        }),
    );
    manifest.insert(
        "short_name".into(),
        json!(if config.pwa.short_name.is_empty() {
            "Landing"
        } else {
            &config.pwa.short_name
        }),
    );
    manifest.insert("description".into(), json!(config.pwa.description));
    manifest.insert(
        "start_url".into(),
        json!(under_base(base_path, &config.pwa.start_url, base_path)),
    );
    manifest.insert(
        "scope".into(),
        json!(under_base(base_path, &config.pwa.scope, base_path)),
    );
    manifest.insert("display".into(), json!(config.pwa.display));
    manifest.insert("theme_color".into(), json!(config.pwa.theme_color));
    manifest.insert(
        "background_color".into(),
        json!(config.pwa.background_color),
    );
    manifest.insert("icons".into(), Value::Array(pwa_icons.to_vec()));
    if !shortcuts.is_empty() {
        manifest.insert("shortcuts".into(), Value::Array(shortcuts));
    }
    let sorted = sort_value(Value::Object(manifest));
    let text = serde_json::to_string_pretty(&sorted)? + "\n";
    fs::write(site_dir.join("manifest.webmanifest"), text)?;
    Ok(())
}

fn write_service_worker(config: &Config, site_dir: &Path, build_hash: &str) -> Result<()> {
    let base_path = &config.build.base_path;
    let mut precache = vec![
        base_path.clone(),
        format!("{base_path}index.html"),
        format!("{base_path}styles.css"),
        format!("{base_path}app.js"),
    ];
    if config.pwa.enabled {
        precache.push(format!("{base_path}manifest.webmanifest"));
    }
    let icons_dir = site_dir.join("icons");
    if icons_dir.exists() {
        let mut names: Vec<String> = fs::read_dir(&icons_dir)?
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".png") {
                    Some(name)
                } else {
                    None
                }
            })
            .collect();
        names.sort();
        for name in names {
            precache.push(format!("{base_path}icons/{name}"));
        }
    }
    // Deduplicate preserving order.
    let mut seen = std::collections::HashSet::new();
    precache.retain(|item| seen.insert(item.clone()));

    let source = templates::SW_JS
        .replace(
            "__CACHE_VERSION__",
            &format!("landing-{}", &build_hash[..build_hash.len().min(12)]),
        )
        .replace("__PRECACHE__", &serde_json::to_string(&precache)?)
        .replace("__BASE_PATH__", base_path);
    fs::write(site_dir.join("sw.js"), source)?;
    Ok(())
}

fn write_redirect_pages(
    config: &Config,
    site_dir: &Path,
    assets: &Assets,
    full_css: &str,
) -> Result<()> {
    if !config.redirect.generate_static_pages {
        return Ok(());
    }
    let base_path = &config.build.base_path;
    let inline_js = format!("{}\n\n{}", templates::CORE_JS, templates::REDIRECT_JS);
    for group in &config.groups {
        for link in &group.links {
            let redirect_data = json!({
                "link": {
                    "slug": link.slug,
                    "name": link.name,
                    "url": link.url,
                    "lan_url": link.lan_url,
                    "availability": link.availability,
                    "prefer": link.prefer,
                    "icon_url": link.icon_url,
                },
                "config": { "redirect": config.redirect },
                "basePath": base_path,
                "mode": "auto",
                "delay": config.redirect.delay_ms,
            });
            let redirect_json = serde_json::to_string(&redirect_data)?.replace("</", "<\\/");
            let csp_meta = csp::csp_meta(config, Some(full_css), Some(&inline_js), &redirect_json);
            let icon_url = link
                .icon_url
                .clone()
                .unwrap_or_else(|| assets.favicon_url.clone());
            let ctx = json!({
                "site": config.site,
                "theme": config.theme,
                "name": link.name,
                "icon_url": icon_url,
                "base_path": base_path,
                "csp_meta": csp_meta,
                "inline_css": full_css,
                "inline_js": inline_js,
                "redirect_json": redirect_json,
            });
            let content = templates::render_redirect(ctx)?;
            let dir = site_dir.join("go").join(&link.slug);
            fs::create_dir_all(&dir)?;
            fs::write(dir.join("index.html"), content)?;
        }
    }
    Ok(())
}

fn write_deployment_helpers(config: &Config, site_dir: &Path, config_json: &str) -> Result<()> {
    let csp = csp::csp_policy(config, None, None, config_json);
    let headers = format!(
        "/*\n  X-Content-Type-Options: nosniff\n  Referrer-Policy: no-referrer\n  Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=(), usb=()\n  Content-Security-Policy: {csp}\n\n/index.html\n  Cache-Control: no-cache\n/sw.js\n  Cache-Control: no-cache\n/styles.css\n  Cache-Control: no-cache\n/app.js\n  Cache-Control: no-cache\n/manifest.webmanifest\n  Cache-Control: no-cache\n/assets/*\n  Cache-Control: public, max-age=86400\n/icons/*\n  Cache-Control: public, max-age=86400\n"
    );
    fs::write(site_dir.join("_headers"), headers)?;

    let caddy = format!(
        ":8080 {{\n    root * .\n    file_server\n    try_files {{path}} /index.html\n    header {{\n        X-Content-Type-Options \"nosniff\"\n        Referrer-Policy \"no-referrer\"\n        Permissions-Policy \"camera=(), microphone=(), geolocation=(), payment=(), usb=()\"\n        Content-Security-Policy \"{csp}\"\n    }}\n    @no_cache path /index.html /sw.js /styles.css /app.js /manifest.webmanifest\n    header @no_cache Cache-Control \"no-cache\"\n    @hashed path /assets/* /icons/*\n    header @hashed Cache-Control \"public, max-age=86400\"\n}}\n"
    );
    fs::write(site_dir.join("Caddyfile.example"), caddy)?;

    let nginx = format!(
        "server {{\n    listen 8080;\n    server_name _;\n    root /srv/landing;\n    index index.html;\n\n    location / {{\n        try_files $uri $uri/ /index.html;\n    }}\n\n    add_header X-Content-Type-Options \"nosniff\" always;\n    add_header Referrer-Policy \"no-referrer\" always;\n    add_header Permissions-Policy \"camera=(), microphone=(), geolocation=(), payment=(), usb=()\" always;\n    add_header Content-Security-Policy \"{csp}\" always;\n\n    location = /index.html {{ add_header Cache-Control \"no-cache\"; }}\n    location = /sw.js {{ add_header Cache-Control \"no-cache\"; }}\n    location = /styles.css {{ add_header Cache-Control \"no-cache\"; }}\n    location = /app.js {{ add_header Cache-Control \"no-cache\"; }}\n    location = /manifest.webmanifest {{ add_header Cache-Control \"no-cache\"; }}\n    location ^~ /assets/ {{ add_header Cache-Control \"public, max-age=86400\"; }}\n    location ^~ /icons/ {{ add_header Cache-Control \"public, max-age=86400\"; }}\n}}\n"
    );
    fs::write(site_dir.join("nginx.conf.example"), nginx)?;
    Ok(())
}

fn write_checksums(out_dir: &Path) -> Result<()> {
    let mut files = Vec::new();
    collect_files(out_dir, &mut files)?;
    files.sort();
    let mut lines = Vec::new();
    for path in files {
        if path
            .file_name()
            .map(|n| n == "checksums.txt")
            .unwrap_or(false)
        {
            continue;
        }
        let digest = sha256_hex(&fs::read(&path)?);
        let rel = path
            .strip_prefix(out_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        lines.push(format!("{digest}  {rel}"));
    }
    fs::write(out_dir.join("checksums.txt"), lines.join("\n") + "\n")?;
    Ok(())
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn apply_env_overrides(config: &mut Config) {
    if let Ok(base_url) = std::env::var("LANDING_BASE_URL") {
        let base_url = base_url.trim();
        if !base_url.is_empty() {
            config.build.base_url = if base_url.ends_with('/') {
                base_url.to_string()
            } else {
                format!("{base_url}/")
            };
        }
    }
    if let Ok(base_path) = std::env::var("LANDING_BASE_PATH") {
        let base_path = base_path.trim();
        if !base_path.is_empty() {
            let mut value = base_path.to_string();
            if !value.starts_with('/') {
                value.insert(0, '/');
            }
            if !value.ends_with('/') {
                value.push('/');
            }
            config.build.base_path = value;
        }
    }
}

/// Run the full build.
pub fn build(opts: BuildOptions) -> Result<BuildResult> {
    let root = opts.root.clone();
    let yaml = fs::read_to_string(&opts.config_path)
        .map_err(|e| anyhow::anyhow!("Config not found: {} ({e})", opts.config_path.display()))?;
    let mut cfg = config::parse(&yaml)?;
    apply_env_overrides(&mut cfg);
    let warnings = config::validate(&cfg)?;
    let config_hash = config_hash(&cfg)?;

    let out = opts
        .out_dir
        .clone()
        .unwrap_or_else(|| root.join(&cfg.build.output_dir));
    if out.exists() {
        fs::remove_dir_all(&out)?;
    }
    fs::create_dir_all(&out)?;
    let site_dir = out.join("site");
    fs::create_dir_all(&site_dir)?;

    let mut warnings = warnings;
    let favicon_map = favicon::resolve_favicons(&cfg, &root, opts.refresh_favicons, &mut warnings)?;
    let single_file = opts.single_file.unwrap_or(cfg.build.single_file);
    let assets = resolve_assets(&mut cfg, &root, &site_dir, single_file, &favicon_map)?;

    let theme_css = theme::theme_css(&cfg, assets.background_url.as_deref());
    let full_css = format!("{theme_css}\n\n{}", templates::MAIN_CSS);
    let full_js = format!(
        "{}\n\n{}\n\n{}",
        templates::CORE_JS,
        templates::ICONS_JS,
        templates::SITE_JS
    );

    // Build hash over all inputs.
    let mut source_bundle = String::new();
    source_bundle.push_str(&config_hash);
    source_bundle.push('\n');
    source_bundle.push_str(&full_css);
    source_bundle.push('\n');
    source_bundle.push_str(&full_js);
    source_bundle.push('\n');
    source_bundle.push_str(templates::SW_JS);
    source_bundle.push_str(templates::INDEX_TEMPLATE);
    source_bundle.push_str(templates::REDIRECT_TEMPLATE);
    source_bundle.push_str(templates::NOT_FOUND_TEMPLATE);
    let mut asset_inputs: Vec<String> = Vec::new();
    if let Some(f) = &cfg.site.favicon {
        asset_inputs.push(f.clone());
    }
    if let Some(img) = &cfg.theme.background.image {
        asset_inputs.push(img.clone());
    }
    for group in &cfg.groups {
        asset_inputs.push(group.icon.clone());
        for link in &group.links {
            asset_inputs.push(link.icon.clone());
        }
    }
    for icon in &cfg.pwa.icons {
        asset_inputs.push(icon.src.clone());
    }
    asset_inputs.extend(favicon_map.values().cloned());
    for asset in &asset_inputs {
        if asset.is_empty() || asset.starts_with("http://") || asset.starts_with("https://") {
            continue;
        }
        let path = root.join(asset);
        if path.is_file() {
            source_bundle.push('\n');
            source_bundle.push_str(&sha256_hex(&fs::read(&path)?));
        }
    }
    let build_hash = sha256_hex(source_bundle.as_bytes());
    let asset_version = &build_hash[..12.min(build_hash.len())];
    let styles_url = format!("{}?v={asset_version}", assets.styles_url);
    let app_js_url = format!("{}?v={asset_version}", assets.app_js_url);

    let runtime = runtime_config(&cfg);
    let config_json = canonical_json(&runtime)?.replace("</", "<\\/");
    let config_pretty = serde_json::to_string_pretty(&sort_value(runtime.clone()))? + "\n";
    fs::write(out.join("config.resolved.json"), config_pretty)?;

    // server.json consumed by the local server.
    let lan_entries: Vec<Value> = cfg
        .groups
        .iter()
        .flat_map(|g| g.links.iter())
        .filter(|l| !l.lan_url.is_empty() && l.probe)
        .map(|l| {
            let url = if !l.probe_url.is_empty() {
                l.probe_url.clone()
            } else {
                l.lan_url.clone()
            };
            json!({ "slug": l.slug, "url": url, "expect": l.probe_expect })
        })
        .collect();
    let server_info = json!({
        "host": cfg.server.host,
        "port": cfg.server.port,
        "health_path": cfg.server.health_path,
        "server_probe": cfg.server.probe,
        "search": { "suggest_url": cfg.search.suggest_url, "suggest_limit": cfg.search.suggest_limit },
        "probe": {
            "internet": cfg.probe.internet.urls,
            "lan": lan_entries,
            "timeout_ms": cfg.probe.lan.timeout_ms,
            "accept_untrusted_cert": cfg.probe.lan.accept_untrusted_cert,
            "pin_certificates": cfg.probe.lan.pin_certificates,
            "pins": cfg.probe.lan.pins,
            "captive_portal": cfg.probe.captive_portal,
        },
    });
    fs::write(
        out.join("server.json"),
        serde_json::to_string_pretty(&sort_value(server_info.clone()))? + "\n",
    )?;

    let groups = group_views(&cfg);
    let noindex = cfg.site.noindex || cfg.build.noindex;
    let site_csp = csp::csp_meta(&cfg, None, None, &config_json);
    let index_ctx = json!({
        "site": cfg.site,
        "theme": cfg.theme,
        "layout": cfg.layout,
        "search": cfg.search,
        "groups": groups,
        "base_path": cfg.build.base_path,
        "config_json": config_json,
        "csp_meta": site_csp,
        "single_file": false,
        "inline_css": "",
        "inline_js": "",
        "styles_url": styles_url,
        "app_js_url": app_js_url,
        "manifest_url": assets.manifest_url,
        "favicon_url": assets.favicon_url,
        "noindex": noindex,
        "config_hash": build_hash,
        "has_pwa": cfg.pwa.enabled,
    });
    let site_html = templates::render_index(index_ctx)?;
    fs::write(site_dir.join("index.html"), site_html)?;
    fs::write(site_dir.join("styles.css"), &full_css)?;
    fs::write(site_dir.join("app.js"), &full_js)?;
    write_manifest(&cfg, &site_dir, &assets.pwa_icons)?;
    write_service_worker(&cfg, &site_dir, &build_hash)?;
    write_redirect_pages(&cfg, &site_dir, &assets, &full_css)?;
    let not_found = templates::render_404(json!({
        "site": cfg.site,
        "base_path": cfg.build.base_path,
    }))?;
    fs::write(site_dir.join("404.html"), not_found)?;
    let robots = if noindex {
        "User-agent: *\nDisallow: /\n"
    } else {
        "User-agent: *\nAllow: /\n"
    };
    fs::write(site_dir.join("robots.txt"), robots)?;
    write_deployment_helpers(&cfg, &site_dir, &config_json)?;

    if single_file {
        let single_csp = csp::csp_meta(&cfg, Some(&full_css), Some(&full_js), &config_json);
        let single_ctx = json!({
            "site": cfg.site,
            "theme": cfg.theme,
            "layout": cfg.layout,
            "search": cfg.search,
            "groups": group_views(&cfg),
            "base_path": cfg.build.base_path,
            "config_json": config_json,
            "csp_meta": single_csp,
            "single_file": true,
            "inline_css": full_css,
            "inline_js": full_js,
            "styles_url": "",
            "app_js_url": "",
            "manifest_url": "",
            "favicon_url": assets.favicon_url,
            "noindex": noindex,
            "config_hash": build_hash,
            "has_pwa": cfg.pwa.enabled,
        });
        let single_html = templates::render_index(single_ctx)?;
        fs::write(out.join("landing.html"), single_html)?;
    }

    // build-info.json
    let mut artifacts = Vec::new();
    let mut files = Vec::new();
    collect_files(&out, &mut files)?;
    for path in files {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "build-info.json" || name == "checksums.txt" {
            continue;
        }
        artifacts.push(
            path.strip_prefix(&out)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/"),
        );
    }
    artifacts.sort();
    let build_info = json!({
        "version": version_from_hash(&build_hash),
        "config_hash": config_hash,
        "build_hash": build_hash,
        "base_url": cfg.build.base_url,
        "base_path": cfg.build.base_path,
        "artifacts": artifacts,
    });
    fs::write(
        out.join("build-info.json"),
        serde_json::to_string_pretty(&sort_value(build_info))? + "\n",
    )?;
    write_checksums(&out)?;

    if !opts.quiet {
        for warning in &warnings {
            eprintln!("warning: {warning}");
        }
        println!("Built {}", out.display());
        println!("  site:       {}", site_dir.display());
        if single_file {
            println!("  single:     {}", out.join("landing.html").display());
        }
    }

    Ok(BuildResult {
        build_hash,
        config_hash,
        warnings,
        out,
        config: cfg,
    })
}

/// Hash every file under a directory.
pub fn hash_tree(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut files = Vec::new();
    collect_files(root, &mut files)?;
    files.sort();
    let mut result = BTreeMap::new();
    for path in files {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        result.insert(rel, sha256_hex(&fs::read(&path)?));
    }
    Ok(result)
}

/// Build twice and compare the output trees.
pub fn check_idempotent(config_path: &Path, root: &Path) -> Result<bool> {
    let tmp1 = tempdir()?;
    let tmp2 = tempdir()?;
    build(BuildOptions {
        config_path: config_path.to_path_buf(),
        out_dir: Some(tmp1.clone()),
        single_file: Some(true),
        quiet: true,
        refresh_favicons: false,
        root: root.to_path_buf(),
    })?;
    build(BuildOptions {
        config_path: config_path.to_path_buf(),
        out_dir: Some(tmp2.clone()),
        single_file: Some(true),
        quiet: true,
        refresh_favicons: false,
        root: root.to_path_buf(),
    })?;
    let first = hash_tree(&tmp1)?;
    let second = hash_tree(&tmp2)?;
    let _ = fs::remove_dir_all(&tmp1);
    let _ = fs::remove_dir_all(&tmp2);
    if first == second {
        println!("Build is idempotent.");
        Ok(true)
    } else {
        eprintln!("Build is not idempotent.");
        Ok(false)
    }
}

fn tempdir() -> Result<PathBuf> {
    let base = std::env::temp_dir();
    let unique = format!(
        "landing-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let path = base.join(unique);
    fs::create_dir_all(&path)?;
    Ok(path)
}
