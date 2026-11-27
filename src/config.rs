//! Configuration model and normalization.
//!
//! Mirrors `scripts/build.py::normalize_config` and `validate_config`. Values
//! are deserialized loosely (many fields optional) and then normalized into a
//! canonical shape so the rest of the program never has to reason about
//! missing keys.

use std::collections::{BTreeMap, HashSet};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const RESERVED_SLUGS: &[&str] = &[
    "go",
    "assets",
    "sw.js",
    "manifest.webmanifest",
    "probe",
    "api",
    "health",
    "favicon.ico",
    "favicon.png",
    "robots.txt",
    "404.html",
    "landing.html",
    "landing.user.js",
    "index.html",
    "styles.css",
    "app.js",
];

/// A collection of linkable services.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Config {
    pub version: u32,
    pub site: Site,
    pub theme: Theme,
    pub layout: Layout,
    pub defaults: Defaults,
    pub search: Search,
    pub probe: Probe,
    pub redirect: Redirect,
    pub pwa: Pwa,
    pub favicons: Favicons,
    pub server: Server,
    pub build: Build,
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Site {
    pub title: String,
    pub subtitle: String,
    pub description: String,
    pub lang: String,
    pub noindex: bool,
    pub favicon: Option<String>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Theme {
    pub preset: String,
    pub mode: String,
    pub accent: String,
    pub radius: i64,
    pub density: String,
    pub font: String,
    pub card_style: String,
    pub meta_color: String,
    pub background: Background,
    pub colors: Colors,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Background {
    pub image: Option<String>,
    pub position: String,
    pub size: String,
    pub overlay: String,
    pub blur: i64,
    pub brightness: f64,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Colors {
    pub light: Palette,
    pub dark: Palette,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Palette {
    pub bg: String,
    pub surface: String,
    pub text: String,
    pub muted: String,
    pub border: String,
    pub accent: String,
    pub accent2: String,
    pub success: String,
    pub warning: String,
    pub danger: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Layout {
    pub columns: i64,
    pub max_width: i64,
    pub groups_max_width: i64,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Defaults {
    pub target: String,
    pub icon_style: String,
    pub availability: String,
    pub prefer: String,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Search {
    pub enabled: bool,
    pub placeholder: String,
    pub hotkey: String,
    pub focus: bool,
    pub engine: String,
    pub engine_name: String,
    pub open_in_new_tab: bool,
    pub suggestions: bool,
    pub suggest_url: String,
    pub suggest_limit: i64,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Probe {
    pub internet: Internet,
    pub lan: Lan,
    pub captive_portal: CaptivePortal,
    pub cache_ttl_ms: i64,
    pub recheck_on: Vec<String>,
    pub homepage: bool,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Internet {
    pub urls: Vec<String>,
    pub mode: String,
    pub timeout_ms: i64,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Lan {
    pub source: String,
    pub urls: Vec<String>,
    pub max_links: i64,
    pub timeout_ms: i64,
    pub random_query: bool,
    pub target_address_space: String,
    pub image_fallback: bool,
    pub ip_fallback: Option<String>,
    pub random_subdomain: bool,
    pub domain: Option<String>,
    pub accept_untrusted_cert: bool,
    pub pin_certificates: bool,
    pub pins: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CaptivePortal {
    pub enabled: bool,
    pub urls: Vec<CaptiveCheck>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CaptiveCheck {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect_status: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect_body: Option<String>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Redirect {
    pub enabled: bool,
    pub delay_ms: i64,
    pub max_delay_ms: i64,
    pub allow_mode_override: bool,
    pub allow_delay_override: bool,
    pub show_target_host: bool,
    pub cancel: bool,
    pub generate_static_pages: bool,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Pwa {
    pub enabled: bool,
    pub name: String,
    pub short_name: String,
    pub description: String,
    pub theme_color: String,
    pub background_color: String,
    pub display: String,
    pub start_url: String,
    pub scope: String,
    pub icons: Vec<PwaIcon>,
    pub shortcuts: Vec<PwaShortcut>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PwaIcon {
    pub src: String,
    #[serde(default)]
    pub sizes: String,
    #[serde(rename = "type", default = "default_icon_type")]
    pub kind: String,
    #[serde(default = "default_icon_purpose")]
    pub purpose: String,
}

fn default_icon_type() -> String {
    "image/png".into()
}
fn default_icon_purpose() -> String {
    "any".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PwaShortcut {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Favicons {
    pub enabled: bool,
    pub refresh: bool,
    pub timeout_ms: i64,
    pub max_bytes: i64,
    pub allow_private_hosts: bool,
    pub cache_dir: String,
    pub prefer: Vec<String>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Server {
    pub host: String,
    pub port: u16,
    pub health_path: String,
    pub probe: bool,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Build {
    pub base_url: String,
    pub base_path: String,
    pub output_dir: String,
    pub single_file: bool,
    pub minify: bool,
    pub noindex: bool,
    pub allow_remote_assets: bool,
    pub source_date_epoch: i64,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub color: Option<String>,
    pub order: i64,
    pub collapsed: bool,
    pub show_when: Vec<String>,
    pub hide_when: Vec<String>,
    pub icon_url: Option<String>,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Link {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub icon: String,
    pub icon_background: String,
    pub url: String,
    pub lan_url: String,
    pub target: String,
    pub availability: String,
    pub prefer: String,
    pub hotkey: String,
    pub hidden: bool,
    pub probe: bool,
    pub probe_url: String,
    pub probe_expect: String,
    pub probe_image: String,
    pub favicon_url: String,
    pub favicon_refresh: bool,
    pub order: i64,
    pub icon_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Raw (loosely typed) input shapes. Every field is optional so a partial YAML
// document merges cleanly onto the defaults below.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    site: Option<RawSite>,
    #[serde(default)]
    theme: Option<RawTheme>,
    #[serde(default)]
    layout: Option<Layout>,
    #[serde(default)]
    defaults: Option<Defaults>,
    #[serde(default)]
    search: Option<Search>,
    #[serde(default)]
    probe: Option<RawProbe>,
    #[serde(default)]
    redirect: Option<Redirect>,
    #[serde(default)]
    pwa: Option<RawPwa>,
    #[serde(default)]
    favicons: Option<RawFavicons>,
    #[serde(default)]
    server: Option<Server>,
    #[serde(default)]
    build: Option<RawBuild>,
    #[serde(default)]
    groups: Vec<RawGroup>,
}

#[derive(Debug, Deserialize, Default)]
struct RawSite {
    title: Option<String>,
    subtitle: Option<String>,
    description: Option<String>,
    lang: Option<String>,
    noindex: Option<bool>,
    favicon: Option<Option<String>>,
}

#[derive(Debug, Deserialize, Default)]
struct RawTheme {
    preset: Option<String>,
    mode: Option<String>,
    accent: Option<String>,
    radius: Option<i64>,
    density: Option<String>,
    font: Option<String>,
    card_style: Option<String>,
    meta_color: Option<String>,
    background: Option<RawBackground>,
    colors: Option<RawColors>,
}

#[derive(Debug, Deserialize, Default)]
struct RawBackground {
    image: Option<Option<String>>,
    position: Option<String>,
    size: Option<String>,
    overlay: Option<String>,
    blur: Option<i64>,
    brightness: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
struct RawColors {
    light: Option<RawPalette>,
    dark: Option<RawPalette>,
}

#[derive(Debug, Deserialize, Default)]
struct RawPalette {
    bg: Option<String>,
    surface: Option<String>,
    text: Option<String>,
    muted: Option<String>,
    border: Option<String>,
    accent: Option<String>,
    accent2: Option<String>,
    success: Option<String>,
    warning: Option<String>,
    danger: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawProbe {
    internet: Option<RawInternet>,
    lan: Option<RawLan>,
    captive_portal: Option<RawCaptive>,
    cache_ttl_ms: Option<i64>,
    recheck_on: Option<Vec<String>>,
    homepage: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct RawInternet {
    urls: Option<Vec<String>>,
    mode: Option<String>,
    timeout_ms: Option<i64>,
    image: Option<Option<String>>,
}

#[derive(Debug, Deserialize, Default)]
struct RawLan {
    source: Option<String>,
    urls: Option<Vec<String>>,
    max_links: Option<i64>,
    timeout_ms: Option<i64>,
    random_query: Option<bool>,
    target_address_space: Option<String>,
    image_fallback: Option<bool>,
    ip_fallback: Option<Option<String>>,
    random_subdomain: Option<bool>,
    domain: Option<Option<String>>,
    accept_untrusted_cert: Option<bool>,
    pin_certificates: Option<bool>,
    pins: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize, Default)]
struct RawCaptive {
    enabled: Option<bool>,
    urls: Option<Vec<CaptiveCheck>>,
}

#[derive(Debug, Deserialize, Default)]
struct RawPwa {
    enabled: Option<bool>,
    name: Option<String>,
    short_name: Option<String>,
    description: Option<String>,
    theme_color: Option<String>,
    background_color: Option<String>,
    display: Option<String>,
    start_url: Option<String>,
    scope: Option<String>,
    icons: Option<Vec<PwaIcon>>,
    shortcuts: Option<Vec<PwaShortcut>>,
}

#[derive(Debug, Deserialize, Default)]
struct RawFavicons {
    enabled: Option<bool>,
    refresh: Option<bool>,
    timeout_ms: Option<i64>,
    max_bytes: Option<i64>,
    allow_private_hosts: Option<bool>,
    cache_dir: Option<String>,
    prefer: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
struct RawBuild {
    base_url: Option<String>,
    base_path: Option<String>,
    output_dir: Option<String>,
    single_file: Option<bool>,
    minify: Option<bool>,
    noindex: Option<bool>,
    allow_remote_assets: Option<bool>,
    source_date_epoch: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
struct RawGroup {
    id: Option<String>,
    name: Option<String>,
    icon: Option<String>,
    color: Option<String>,
    order: Option<i64>,
    collapsed: Option<bool>,
    show_when: Option<Vec<String>>,
    hide_when: Option<Vec<String>>,
    links: Option<Vec<RawLink>>,
}

#[derive(Debug, Deserialize, Default)]
struct RawLink {
    id: Option<String>,
    slug: Option<String>,
    name: Option<String>,
    icon: Option<String>,
    icon_background: Option<String>,
    url: Option<String>,
    lan_url: Option<String>,
    target: Option<String>,
    availability: Option<String>,
    prefer: Option<String>,
    hotkey: Option<String>,
    hidden: Option<bool>,
    probe: Option<bool>,
    probe_url: Option<String>,
    probe_expect: Option<String>,
    probe_image: Option<String>,
    favicon_url: Option<String>,
    favicon_refresh: Option<bool>,
    order: Option<i64>,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            columns: 3,
            max_width: 1400,
            groups_max_width: 1080,
        }
    }
}
impl Default for Defaults {
    fn default() -> Self {
        Defaults {
            target: "_blank".into(),
            icon_style: "flat".into(),
            availability: "always".into(),
            prefer: "auto".into(),
            icon: "link".into(),
        }
    }
}
impl Default for Search {
    fn default() -> Self {
        Search {
            enabled: true,
            placeholder: "Search the web…".into(),
            hotkey: "/".into(),
            focus: true,
            engine: "https://duckduckgo.com/?q={query}".into(),
            engine_name: "DuckDuckGo".into(),
            open_in_new_tab: false,
            suggestions: true,
            suggest_url: "https://duckduckgo.com/ac/?q={query}&type=list".into(),
            suggest_limit: 6,
        }
    }
}
impl Default for Redirect {
    fn default() -> Self {
        Redirect {
            enabled: true,
            delay_ms: 0,
            max_delay_ms: 5000,
            allow_mode_override: true,
            allow_delay_override: true,
            show_target_host: true,
            cancel: true,
            generate_static_pages: true,
        }
    }
}
impl Default for Server {
    fn default() -> Self {
        Server {
            host: "127.0.0.1".into(),
            port: 8080,
            health_path: "/healthz".into(),
            probe: true,
        }
    }
}

fn default_palette_light() -> Palette {
    Palette {
        bg: "#f6f8fb".into(),
        surface: "#ffffff".into(),
        text: "#0f172a".into(),
        muted: "#5b6474".into(),
        border: "#e2e8f0".into(),
        accent: "#0284c7".into(),
        accent2: "#6366f1".into(),
        success: "#059669".into(),
        warning: "#b45309".into(),
        danger: "#dc2626".into(),
    }
}

fn default_palette_dark() -> Palette {
    Palette {
        bg: "#070b14".into(),
        surface: "#0f172a".into(),
        text: "#e5edf7".into(),
        muted: "#93a4bd".into(),
        border: "#1e2a3f".into(),
        accent: "#38bdf8".into(),
        accent2: "#818cf8".into(),
        success: "#34d399".into(),
        warning: "#fbbf24".into(),
        danger: "#f87171".into(),
    }
}

fn merge_palette(base: Palette, raw: RawPalette) -> Palette {
    Palette {
        bg: raw.bg.unwrap_or(base.bg),
        surface: raw.surface.unwrap_or(base.surface),
        text: raw.text.unwrap_or(base.text),
        muted: raw.muted.unwrap_or(base.muted),
        border: raw.border.unwrap_or(base.border),
        accent: raw.accent.unwrap_or(base.accent),
        accent2: raw.accent2.unwrap_or(base.accent2),
        success: raw.success.unwrap_or(base.success),
        warning: raw.warning.unwrap_or(base.warning),
        danger: raw.danger.unwrap_or(base.danger),
    }
}

fn normalize_slug(value: &str) -> String {
    value.trim().to_lowercase()
}

/// Normalize raw YAML into a fully populated [`Config`].
fn normalize(raw: RawConfig) -> Result<Config> {
    let version = raw.version.unwrap_or(1);
    if version != 1 {
        bail!("Unsupported config version: {version}");
    }

    let site_raw = raw.site.unwrap_or_default();
    let site = Site {
        title: site_raw.title.unwrap_or_else(|| "Landing".into()),
        subtitle: site_raw.subtitle.unwrap_or_default(),
        description: site_raw.description.unwrap_or_default(),
        lang: site_raw.lang.unwrap_or_else(|| "en".into()),
        noindex: site_raw.noindex.unwrap_or(true),
        favicon: site_raw.favicon.unwrap_or(None),
    };

    let theme_raw = raw.theme.unwrap_or_default();
    let bg_raw = theme_raw.background.unwrap_or_default();
    let background = Background {
        image: bg_raw.image.unwrap_or(None),
        position: bg_raw.position.unwrap_or_else(|| "center".into()),
        size: bg_raw.size.unwrap_or_else(|| "cover".into()),
        overlay: bg_raw
            .overlay
            .unwrap_or_else(|| "rgba(2, 6, 23, 0.55)".into()),
        blur: bg_raw.blur.unwrap_or(0),
        brightness: bg_raw.brightness.unwrap_or(1.0),
    };
    let colors_raw = theme_raw.colors.unwrap_or_default();
    let colors = Colors {
        light: merge_palette(
            default_palette_light(),
            colors_raw.light.unwrap_or_default(),
        ),
        dark: merge_palette(default_palette_dark(), colors_raw.dark.unwrap_or_default()),
    };
    let meta_color = theme_raw
        .meta_color
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| colors.dark.bg.clone());
    let theme = Theme {
        preset: theme_raw.preset.unwrap_or_else(|| "slate".into()),
        mode: theme_raw.mode.unwrap_or_else(|| "dark".into()),
        accent: theme_raw
            .accent
            .unwrap_or_else(|| colors.dark.accent.clone()),
        radius: theme_raw.radius.unwrap_or(16),
        density: theme_raw.density.unwrap_or_else(|| "comfortable".into()),
        font: theme_raw.font.unwrap_or_else(|| "system".into()),
        card_style: theme_raw.card_style.unwrap_or_else(|| "hairline".into()),
        meta_color,
        background,
        colors,
    };

    let layout = raw.layout.unwrap_or_default();

    let defaults = raw.defaults.unwrap_or_default();

    let search = raw.search.unwrap_or_default();

    let probe_raw = raw.probe.unwrap_or_default();
    let internet_raw = probe_raw.internet.unwrap_or_default();
    let lan_raw = probe_raw.lan.unwrap_or_default();
    let captive_raw = probe_raw.captive_portal.unwrap_or_default();
    let probe = Probe {
        internet: Internet {
            urls: internet_raw.urls.unwrap_or_else(|| {
                vec![
                    "https://example.org/".into(),
                    "https://example.com/".into(),
                    "https://www.gstatic.com/generate_204".into(),
                ]
            }),
            mode: internet_raw.mode.unwrap_or_else(|| "no-cors".into()),
            timeout_ms: internet_raw.timeout_ms.unwrap_or(2500),
            image: internet_raw.image.unwrap_or(None),
        },
        lan: Lan {
            source: lan_raw.source.unwrap_or_else(|| "links".into()),
            urls: lan_raw.urls.unwrap_or_default(),
            max_links: lan_raw.max_links.unwrap_or(20),
            timeout_ms: lan_raw.timeout_ms.unwrap_or(2500),
            random_query: lan_raw.random_query.unwrap_or(true),
            target_address_space: lan_raw
                .target_address_space
                .unwrap_or_else(|| "local".into()),
            image_fallback: lan_raw.image_fallback.unwrap_or(true),
            ip_fallback: lan_raw.ip_fallback.unwrap_or(None),
            random_subdomain: lan_raw.random_subdomain.unwrap_or(false),
            domain: lan_raw.domain.unwrap_or(None),
            accept_untrusted_cert: lan_raw.accept_untrusted_cert.unwrap_or(true),
            pin_certificates: lan_raw.pin_certificates.unwrap_or(false),
            pins: lan_raw.pins.unwrap_or_default(),
        },
        captive_portal: CaptivePortal {
            enabled: captive_raw.enabled.unwrap_or(true),
            urls: captive_raw.urls.unwrap_or_else(default_captive_checks),
        },
        cache_ttl_ms: probe_raw.cache_ttl_ms.unwrap_or(30000),
        recheck_on: probe_raw.recheck_on.unwrap_or_else(|| {
            vec![
                "online".into(),
                "offline".into(),
                "visibilitychange".into(),
                "focus".into(),
            ]
        }),
        homepage: probe_raw.homepage.unwrap_or(false),
    };

    let redirect = raw.redirect.unwrap_or_default();

    let pwa_raw = raw.pwa.unwrap_or_default();
    let dark_bg_for_pwa = theme.colors.dark.bg.clone();
    let pwa = Pwa {
        enabled: pwa_raw.enabled.unwrap_or(true),
        name: pwa_raw.name.unwrap_or_else(|| site.title.clone()),
        short_name: pwa_raw.short_name.unwrap_or_else(|| "Landing".into()),
        description: pwa_raw
            .description
            .unwrap_or_else(|| site.description.clone()),
        theme_color: pwa_raw
            .theme_color
            .unwrap_or_else(|| dark_bg_for_pwa.clone()),
        background_color: pwa_raw
            .background_color
            .unwrap_or_else(|| dark_bg_for_pwa.clone()),
        display: pwa_raw.display.unwrap_or_else(|| "standalone".into()),
        start_url: pwa_raw.start_url.unwrap_or_else(|| "/".into()),
        scope: pwa_raw.scope.unwrap_or_else(|| "/".into()),
        icons: pwa_raw.icons.unwrap_or_default(),
        shortcuts: pwa_raw.shortcuts.unwrap_or_default(),
    };

    let fav_raw = raw.favicons.unwrap_or_default();
    let favicons = Favicons {
        enabled: fav_raw.enabled.unwrap_or(true),
        refresh: fav_raw.refresh.unwrap_or(false),
        timeout_ms: fav_raw.timeout_ms.unwrap_or(5000),
        max_bytes: fav_raw.max_bytes.unwrap_or(1048576),
        allow_private_hosts: fav_raw.allow_private_hosts.unwrap_or(false),
        cache_dir: fav_raw
            .cache_dir
            .unwrap_or_else(|| "assets/favicons".into()),
        prefer: fav_raw
            .prefer
            .unwrap_or_else(|| vec!["svg".into(), "png".into(), "ico".into()]),
    };

    let server = raw.server.unwrap_or_default();

    let build_raw = raw.build.unwrap_or_default();
    let mut base_url = build_raw
        .base_url
        .unwrap_or_else(|| "https://landing.example.net/".into());
    if !base_url.ends_with('/') {
        base_url.push('/');
    }
    let mut base_path = build_raw.base_path.unwrap_or_else(|| "/".into());
    if !base_path.starts_with('/') {
        base_path.insert(0, '/');
    }
    if !base_path.ends_with('/') {
        base_path.push('/');
    }
    let build = Build {
        base_url,
        base_path,
        output_dir: build_raw.output_dir.unwrap_or_else(|| "dist".into()),
        single_file: build_raw.single_file.unwrap_or(true),
        minify: build_raw.minify.unwrap_or(false),
        noindex: build_raw.noindex.unwrap_or(true),
        allow_remote_assets: build_raw.allow_remote_assets.unwrap_or(false),
        source_date_epoch: build_raw.source_date_epoch.unwrap_or(0),
    };

    if raw.groups.is_empty() {
        bail!("At least one group is required");
    }

    let mut groups = Vec::with_capacity(raw.groups.len());
    for (index, group_raw) in raw.groups.into_iter().enumerate() {
        let id = group_raw.id.unwrap_or_default().trim().to_string();
        if id.is_empty() {
            bail!("groups[{index}] is missing id");
        }
        let mut links = Vec::new();
        for (link_index, link_raw) in group_raw.links.unwrap_or_default().into_iter().enumerate() {
            let link_id = link_raw.id.unwrap_or_default().trim().to_string();
            if link_id.is_empty() {
                bail!("groups[{index}].links[{link_index}] is missing id");
            }
            let slug = normalize_slug(
                &link_raw
                    .slug
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| link_id.clone()),
            );
            links.push(Link {
                id: link_id.clone(),
                name: link_raw.name.filter(|s| !s.is_empty()).unwrap_or(link_id),
                slug,
                icon: link_raw.icon.unwrap_or_else(|| "auto".into()),
                icon_background: link_raw
                    .icon_background
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
                url: link_raw.url.unwrap_or_default().trim().to_string(),
                lan_url: link_raw.lan_url.unwrap_or_default().trim().to_string(),
                target: link_raw.target.unwrap_or_else(|| defaults.target.clone()),
                availability: link_raw
                    .availability
                    .unwrap_or_else(|| defaults.availability.clone()),
                prefer: link_raw.prefer.unwrap_or_else(|| defaults.prefer.clone()),
                hotkey: link_raw.hotkey.unwrap_or_default(),
                hidden: link_raw.hidden.unwrap_or(false),
                probe: link_raw.probe.unwrap_or(true),
                probe_url: link_raw.probe_url.unwrap_or_default().trim().to_string(),
                probe_expect: link_raw.probe_expect.unwrap_or_default().trim().to_string(),
                probe_image: link_raw.probe_image.unwrap_or_default().trim().to_string(),
                favicon_url: link_raw.favicon_url.unwrap_or_default().trim().to_string(),
                favicon_refresh: link_raw.favicon_refresh.unwrap_or(false),
                order: link_raw.order.unwrap_or(link_index as i64 * 10),
                icon_url: None,
            });
        }
        links.sort_by(|a, b| (a.order, &a.id).cmp(&(b.order, &b.id)));
        groups.push(Group {
            name: group_raw.name.unwrap_or_else(|| id.clone()),
            id,
            icon: group_raw.icon.unwrap_or_else(|| "link".into()),
            color: group_raw.color,
            order: group_raw.order.unwrap_or(index as i64 * 10),
            collapsed: group_raw.collapsed.unwrap_or(false),
            show_when: group_raw.show_when.unwrap_or_default(),
            hide_when: group_raw.hide_when.unwrap_or_default(),
            icon_url: None,
            links,
        });
    }
    groups.sort_by(|a, b| (a.order, &a.id).cmp(&(b.order, &b.id)));

    Ok(Config {
        version,
        site,
        theme,
        layout,
        defaults,
        search,
        probe,
        redirect,
        pwa,
        favicons,
        server,
        build,
        groups,
    })
}

fn default_captive_checks() -> Vec<CaptiveCheck> {
    vec![
        CaptiveCheck {
            url: "http://connectivitycheck.gstatic.com/generate_204".into(),
            expect_status: Some(204),
            expect_body: None,
        },
        CaptiveCheck {
            url: "http://captive.apple.com/hotspot-detect.html".into(),
            expect_status: None,
            expect_body: Some("Success".into()),
        },
        CaptiveCheck {
            url: "http://www.msftconnecttest.com/connecttest.txt".into(),
            expect_status: None,
            expect_body: Some("Microsoft Connect Test".into()),
        },
    ]
}

/// Parse YAML text into a normalized config.
pub fn parse(yaml: &str) -> Result<Config> {
    let raw: RawConfig =
        serde_yaml_ng::from_str(yaml).map_err(|e| anyhow::anyhow!("Invalid YAML: {e}"))?;
    normalize(raw)
}

/// Validation mirrors `validate_config`, returning warnings on success.
pub fn validate(config: &Config) -> Result<Vec<String>> {
    let mut warnings = Vec::new();

    if config.site.title.trim().is_empty() {
        bail!("site.title is required");
    }
    parse_url(&config.build.base_url, "build.base_url")?;
    if !config.build.base_path.starts_with('/') || !config.build.base_path.ends_with('/') {
        bail!("build.base_path must start and end with '/'");
    }

    if let Some(favicon) = &config.site.favicon {
        if !crate::icons::ICONS.iter().any(|(name, _)| name == favicon) {
            check_asset(favicon, "site.favicon", config.build.allow_remote_assets)?;
        }
    }
    if let Some(image) = &config.theme.background.image {
        check_asset(
            image,
            "theme.background.image",
            config.build.allow_remote_assets,
        )?;
    }

    let mut group_ids = HashSet::new();
    let mut link_ids = HashSet::new();
    let mut slugs = HashSet::new();
    let mut hotkeys = HashSet::new();
    let mut lan_count = 0;
    let mut ip_lan_count = 0;

    for group in &config.groups {
        if !is_slug(&group.id) {
            bail!("Invalid group id: {:?}", group.id);
        }
        if !group_ids.insert(group.id.clone()) {
            bail!("Duplicate group id: {}", group.id);
        }
        for link in &group.links {
            let label = format!("{}/{}", group.id, link.id);
            if !is_slug(&link.id) {
                bail!("Invalid link id: {:?}", link.id);
            }
            if !link_ids.insert(link.id.clone()) {
                bail!("Duplicate link id: {}", link.id);
            }
            if !is_slug(&link.slug) {
                bail!("Invalid slug for {label}: {:?}", link.slug);
            }
            if RESERVED_SLUGS.contains(&link.slug.as_str()) {
                bail!("Reserved slug for {label}: {:?}", link.slug);
            }
            if !slugs.insert(link.slug.clone()) {
                bail!("Duplicate slug: {}", link.slug);
            }
            if !link.hotkey.is_empty() {
                if link.hotkey.len() != 1
                    || !link.hotkey.chars().next().unwrap().is_ascii_alphanumeric()
                {
                    bail!("Link {label} hotkey must be a single letter or digit");
                }
                if !hotkeys.insert(link.hotkey.clone()) {
                    bail!("Duplicate hotkey: {}", link.hotkey);
                }
            }
            if link.url.is_empty() && link.lan_url.is_empty() {
                bail!("Link {label} needs url and/or lan_url");
            }
            for (field, value) in [
                ("url", &link.url),
                ("lan_url", &link.lan_url),
                ("probe_url", &link.probe_url),
                ("probe_image", &link.probe_image),
            ] {
                if !value.is_empty() {
                    parse_url(value, &format!("{label}.{field}"))?;
                }
            }
            if !["always", "lan_only", "wan_only"].contains(&link.availability.as_str()) {
                bail!(
                    "Link {label} has invalid availability: {}",
                    link.availability
                );
            }
            if !["auto", "lan", "wan"].contains(&link.prefer.as_str()) {
                bail!("Link {label} has invalid prefer: {}", link.prefer);
            }
            if !["_self", "_blank"].contains(&link.target.as_str()) {
                bail!("Link {label} target must be _self or _blank");
            }
            if !link.lan_url.is_empty() {
                lan_count += 1;
                if let Ok(parsed) = url::Url::parse(&link.lan_url) {
                    if let Some(host) = parsed.host_str() {
                        let is_ip = host.chars().all(|c| c.is_ascii_digit() || c == '.')
                            && host.contains('.');
                        if is_ip || host.contains(':') {
                            ip_lan_count += 1;
                        }
                    }
                }
            }
            if !["auto", "none"].contains(&link.icon.as_str()) {
                if link.icon.starts_with("http://") || link.icon.starts_with("https://") {
                    if !config.build.allow_remote_assets {
                        bail!(
                            "Link {label} remote icon requires build.allow_remote_assets: {}",
                            link.icon
                        );
                    }
                } else if !crate::icons::ICONS
                    .iter()
                    .any(|(name, _)| name == &link.icon)
                {
                    check_asset(&link.icon, &format!("{label}.icon"), false)?;
                }
            }
            if !link.favicon_url.is_empty() {
                parse_url(&link.favicon_url, &format!("{label}.favicon_url"))?;
            }
            if !link.icon_background.is_empty()
                && crate::theme::hex_to_rgb(&link.icon_background).is_none()
            {
                bail!(
                    "Link {label} icon_background must be a hex color: {}",
                    link.icon_background
                );
            }
        }
    }

    if lan_count == 0 {
        warnings.push("No lan_url is configured; LAN detection is disabled.".into());
    } else if ip_lan_count == lan_count {
        warnings.push(
            "All LAN URLs are IP addresses; the probe tests reachability, not DNS resolution."
                .into(),
        );
    }

    if config.probe.internet.urls.is_empty() {
        bail!("probe.internet.urls must not be empty");
    }
    for url in &config.probe.internet.urls {
        parse_url(url, "probe.internet.urls[]")?;
    }
    if !["no-cors", "cors"].contains(&config.probe.internet.mode.as_str()) {
        bail!("probe.internet.mode must be no-cors or cors");
    }
    for url in &config.probe.lan.urls {
        parse_url(url, "probe.lan.urls[]")?;
    }
    if let Some(fallback) = &config.probe.lan.ip_fallback {
        parse_url(fallback, "probe.lan.ip_fallback")?;
    }
    if !["links", "explicit", "both"].contains(&config.probe.lan.source.as_str()) {
        bail!("probe.lan.source must be links, explicit, or both");
    }
    if config.probe.lan.random_subdomain {
        match &config.probe.lan.domain {
            Some(domain) if !domain.trim().is_empty() => {
                if !domain
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
                {
                    bail!("probe.lan.domain is invalid");
                }
            }
            _ => bail!("probe.lan.domain is required when random_subdomain is true"),
        }
    }

    if !config.search.engine.is_empty() {
        if !config.search.engine.contains("{query}") {
            bail!("search.engine must contain the {{query}} placeholder");
        }
        parse_url(
            &config.search.engine.replace("{query}", "test"),
            "search.engine",
        )?;
    }
    if !config.search.suggest_url.is_empty() {
        if !config.search.suggest_url.contains("{query}") {
            bail!("search.suggest_url must contain the {{query}} placeholder");
        }
        parse_url(
            &config.search.suggest_url.replace("{query}", "test"),
            "search.suggest_url",
        )?;
    }

    if config.redirect.delay_ms < 0 {
        bail!("redirect.delay_ms must be a non-negative integer");
    }
    if config.redirect.max_delay_ms < config.redirect.delay_ms {
        bail!("redirect.max_delay_ms must be >= delay_ms");
    }

    // Server binding must be sane: a 0 port binds an ephemeral one, and an empty
    // host can bind all interfaces. health_path is used to answer probes.
    if config.server.port == 0 {
        bail!("server.port must be between 1 and 65535");
    }
    if config.server.host.trim().is_empty() {
        bail!("server.host must not be empty");
    }
    if !config.server.health_path.starts_with('/') {
        bail!("server.health_path must start with '/'");
    }
    let host = config.server.host.trim();
    let is_loopback = host == "127.0.0.1"
        || host == "::1"
        || host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false);
    if !is_loopback {
        warnings.push(format!(
            "server.host is {host}, not loopback; the site is reachable from other machines."
        ));
    }

    Ok(warnings)
}

fn check_asset(value: &str, label: &str, allow_remote: bool) -> Result<()> {
    if value.starts_with("http://") || value.starts_with("https://") {
        if !allow_remote {
            bail!("{label} remote URL requires build.allow_remote_assets: {value}");
        }
    } else if !std::path::Path::new(value).exists() {
        bail!("{label} not found: {value}");
    }
    Ok(())
}

pub fn parse_url(value: &str, label: &str) -> Result<()> {
    match url::Url::parse(value) {
        Ok(parsed) => {
            if parsed.scheme() != "http" && parsed.scheme() != "https" {
                bail!("{label} must be an absolute http(s) URL: {value:?}");
            }
            if !parsed.username().is_empty() || parsed.password().is_some() {
                bail!("{label} must not contain credentials: {value:?}");
            }
            Ok(())
        }
        Err(_) => bail!("{label} must be an absolute http(s) URL: {value:?}"),
    }
}

fn is_slug(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
        return false;
    }
    let last = bytes[bytes.len() - 1];
    if !last.is_ascii_lowercase() && !last.is_ascii_digit() {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}
