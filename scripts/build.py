#!/usr/bin/env python3
"""Build the Landing static site and single-file fallback.

Usage:
    python3 scripts/build.py
    python3 scripts/build.py --config config/landing.yml --out dist
    python3 scripts/build.py --check-idempotent

The build is deterministic: the same inputs produce the same output files.
"""

from __future__ import annotations

import argparse
import base64
import io
import ipaddress
import os
import hashlib
import html
import json
import re
import shutil
import ssl
import sys
import tempfile
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any
from urllib.parse import quote, urljoin, urlparse

import yaml
from jinja2 import Environment, FileSystemLoader, select_autoescape
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
DEFAULT_CONFIG = ROOT / "config" / "landing.yml"

RESERVED_SLUGS = {
    "go", "assets", "sw.js", "manifest.webmanifest", "probe", "api", "health",
    "favicon.ico", "favicon.png", "robots.txt", "404.html", "landing.html",
    "landing.user.js", "index.html", "styles.css", "app.js",
}

SLUG_RE = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$")

ICONS: dict[str, str] = {
    "home": '<path d="M3 10.5 12 3l9 7.5"/><path d="M5 9.5V21h14V9.5"/><path d="M9 21v-6h6v6"/>',
    "router": '<rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 16h.01M11 16h.01M17 16h.01"/><path d="M7 13V9M12 13V7M17 13V9"/>',
    "book": '<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z"/>',
    "play": '<path d="m8 5 11 7-11 7Z"/>',
    "camera": '<path d="M4 7h3l2-2h6l2 2h3v12H4Z"/><circle cx="12" cy="13" r="4"/>',
    "music": '<path d="M9 18V5l10-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="16" cy="16" r="3"/>',
    "server": '<rect x="3" y="4" width="18" height="6" rx="2"/><rect x="3" y="14" width="18" height="6" rx="2"/><path d="M7 7h.01M7 17h.01"/>',
    "chart": '<path d="M4 20V10M10 20V4M16 20v-7M22 20H2"/>',
    "shield": '<path d="M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6Z"/><path d="m9 12 2 2 4-4"/>',
    "cloud": '<path d="M17.5 19H7a5 5 0 0 1-.8-9.9A6 6 0 0 1 18 8a5.5 5.5 0 0 1-.5 11Z"/>',
    "link": '<path d="M10 13a5 5 0 0 0 7.1 0l2-2a5 5 0 0 0-7.1-7.1l-1.1 1.1"/><path d="M14 11a5 5 0 0 0-7.1 0l-2 2A5 5 0 0 0 12 20.1l1.1-1.1"/>',
    "globe": '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.6 4 5.7 4 9s-1.5 6.4-4 9c-2.5-2.6-4-5.7-4-9s1.5-6.4 4-9Z"/>',
    "terminal": '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 9 3 3-3 3M13 15h4"/>',
    "lock": '<rect x="4" y="10" width="16" height="11" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3"/>',
    "download": '<path d="M12 3v12M7 10l5 5 5-5M4 21h16"/>',
    "search": '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.2-3.2"/>',
    "sun": '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>',
    "moon": '<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5Z"/>',
    "refresh": '<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 4v7h-7"/>',
    "check": '<path d="m5 12 4 4L19 6"/>',
    "alert": '<path d="M12 3 2 21h20Z"/><path d="M12 9v5M12 17h.01"/>',
    "wifi": '<path d="M5 12a10 10 0 0 1 14 0M8 15a6 6 0 0 1 8 0M11 18a2 2 0 0 1 2 0"/><circle cx="12" cy="21" r=".5"/>',
    "external": '<path d="M14 3h7v7M10 14 21 3"/><path d="M21 14v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5"/>',
    "menu": '<path d="M4 7h16M4 12h16M4 17h16"/>',
    "cat": '<path d="M12 5c.67 0 1.35.09 2 .26 1.78-2 5.03-2.84 6.42-2.26 1.4.58-.42 7-.42 7 .57 1.07 1 2.24 1 3.44C21 17.9 16.97 21 12 21s-9-3.1-9-7.56c0-1.25.43-2.42 1-3.44 0 0-1.82-6.42-.42-7 1.39-.58 4.64.26 6.42 2.26A9.7 9.7 0 0 1 12 5Z"/><path d="M8 14v.5"/><path d="M16 14v.5"/><path d="M11.25 16.25h1.5L12 17l-.75-.75Z"/>',
    "bookmark": '<path d="M6 4h12v17l-6-4-6 4Z"/>',
    "help": '<circle cx="12" cy="12" r="9"/><path d="M9.1 9a3 3 0 0 1 5.8 1c0 2-3 2-3 4"/><path d="M12 17h.01"/>',
    "copy": '<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/>',
}

DEFAULT_THEME: dict[str, Any] = {
    "preset": "slate",
    "mode": "dark",
    "accent": "#38bdf8",
    "radius": 16,
    "density": "comfortable",
    "font": "system",
    "card_style": "hairline",
    "background": {
        "image": None,
        "position": "center",
        "size": "cover",
        "overlay": "rgba(2, 6, 23, 0.55)",
        "blur": 0,
        "brightness": 1,
    },
    "colors": {
        "light": {
            "bg": "#f6f8fb",
            "surface": "#ffffff",
            "text": "#0f172a",
            "muted": "#5b6474",
            "border": "#e2e8f0",
            "accent": "#0284c7",
            "accent2": "#6366f1",
            "success": "#059669",
            "warning": "#b45309",
            "danger": "#dc2626",
        },
        "dark": {
            "bg": "#070b14",
            "surface": "#0f172a",
            "text": "#e5edf7",
            "muted": "#93a4bd",
            "border": "#1e2a3f",
            "accent": "#38bdf8",
            "accent2": "#818cf8",
            "success": "#34d399",
            "warning": "#fbbf24",
            "danger": "#f87171",
        },
    },
}

DEFAULT_PROBE = {
    "internet": {
        "urls": ["https://example.org/", "https://example.com/", "https://www.gstatic.com/generate_204"],
        "mode": "no-cors",
        "timeout_ms": 2500,
        "image": None,
    },
    "lan": {
        "source": "links",
        "urls": [],
        "max_links": 20,
        "timeout_ms": 2500,
        "random_query": True,
        "target_address_space": "local",
        "image_fallback": True,
        "ip_fallback": None,
        "random_subdomain": False,
        "domain": None,
        "accept_untrusted_cert": True,
        "pin_certificates": False,
        "pins": {},
    },
    "captive_portal": {
        "enabled": True,
        "urls": [
            {"url": "http://connectivitycheck.gstatic.com/generate_204", "expect_status": 204},
            {"url": "http://captive.apple.com/hotspot-detect.html", "expect_body": "Success"},
            {"url": "http://www.msftconnecttest.com/connecttest.txt", "expect_body": "Microsoft Connect Test"},
        ],
    },
    "cache_ttl_ms": 30000,
    "recheck_on": ["online", "offline", "visibilitychange", "focus"],
    "homepage": False,
}

DEFAULT_FAVICONS = {
    "enabled": True,
    "refresh": False,
    "timeout_ms": 5000,
    "max_bytes": 1048576,
    "allow_private_hosts": False,
    "cache_dir": "assets/favicons",
    "prefer": ["svg", "png", "ico"],
}


class ConfigError(RuntimeError):
    pass


def deep_merge(base: dict[str, Any], override: dict[str, Any]) -> dict[str, Any]:
    result = json.loads(json.dumps(base))
    for key, value in (override or {}).items():
        if isinstance(value, dict) and isinstance(result.get(key), dict):
            result[key] = deep_merge(result[key], value)
        else:
            result[key] = value
    return result


def load_yaml(path: Path) -> dict[str, Any]:
    try:
        with path.open("r", encoding="utf-8") as handle:
            data = yaml.safe_load(handle)
    except FileNotFoundError as exc:
        raise ConfigError(f"Config not found: {path}") from exc
    except yaml.YAMLError as exc:
        raise ConfigError(f"Invalid YAML in {path}: {exc}") from exc
    if not isinstance(data, dict):
        raise ConfigError("Config root must be a mapping")
    return data


def normalize_slug(value: Any) -> str:
    text = str(value or "").strip().lower()
    return text


def parse_url(value: str, label: str) -> None:
    parsed = urlparse(value)
    if parsed.scheme not in ("http", "https") or not parsed.netloc:
        raise ConfigError(f"{label} must be an absolute http(s) URL: {value!r}")
    if parsed.username or parsed.password:
        raise ConfigError(f"{label} must not contain credentials: {value!r}")


def normalize_config(raw: dict[str, Any]) -> dict[str, Any]:
    version = raw.get("version", 1)
    if version != 1:
        raise ConfigError(f"Unsupported config version: {version!r}")

    site = {
        "title": "Landing",
        "subtitle": "",
        "description": "",
        "lang": "en",
        "noindex": True,
        "favicon": None,
    }
    site.update(raw.get("site") or {})

    theme = deep_merge(DEFAULT_THEME, raw.get("theme") or {})
    theme["meta_color"] = theme.get("meta_color") or theme["colors"]["dark"]["bg"]

    layout = {
        "columns": 3,
        "max_width": 1400,
        "groups_max_width": 1080,
    }
    layout.update(raw.get("layout") or {})

    defaults = {"target": "_blank", "icon_style": "flat", "availability": "always", "prefer": "auto", "icon": "link"}
    defaults.update(raw.get("defaults") or {})

    search = {
        "enabled": True,
        "placeholder": "Search the web…",
        "hotkey": "/",
        "focus": True,
        "engine": "https://duckduckgo.com/?q={query}",
        "engine_name": "DuckDuckGo",
        "open_in_new_tab": False,
        "suggestions": True,
        "suggest_url": "https://duckduckgo.com/ac/?q={query}&type=list",
        "suggest_limit": 6,
    }
    search.update(raw.get("search") or {})

    probe = deep_merge(DEFAULT_PROBE, raw.get("probe") or {})
    probe["internet"]["urls"] = [str(item) for item in (probe["internet"].get("urls") or [])]
    probe["lan"]["urls"] = [str(item) for item in (probe["lan"].get("urls") or [])]
    probe["lan"]["pins"] = {str(k): str(v) for k, v in (probe["lan"].get("pins") or {}).items()}
    captive = probe.get("captive_portal") or {}
    captive["urls"] = [
        item for item in (captive.get("urls") or [])
        if isinstance(item, dict) and item.get("url")
    ]
    probe["captive_portal"] = captive

    redirect = {
        "enabled": True,
        "delay_ms": 0,
        "max_delay_ms": 5000,
        "allow_mode_override": True,
        "allow_delay_override": True,
        "show_target_host": True,
        "cancel": True,
        "generate_static_pages": True,
    }
    redirect.update(raw.get("redirect") or {})

    pwa = {
        "enabled": True,
        "name": site.get("title", "Landing"),
        "short_name": "Landing",
        "description": site.get("description", ""),
        "theme_color": theme["colors"]["dark"]["bg"],
        "background_color": theme["colors"]["dark"]["bg"],
        "display": "standalone",
        "start_url": "/",
        "scope": "/",
        "icons": [],
        "shortcuts": [],
    }
    pwa.update(raw.get("pwa") or {})

    server = {
        "host": "127.0.0.1",
        "port": 8080,
        "health_path": "/healthz",
        "probe": True,
    }
    server.update(raw.get("server") or {})

    favicons = deep_merge(DEFAULT_FAVICONS, raw.get("favicons") or {})
    favicons["prefer"] = [str(item) for item in (favicons.get("prefer") or [])]

    build = {
        "base_url": "https://landing.example.net/",
        "base_path": "/",
        "output_dir": "dist",
        "single_file": True,
        "minify": False,
        "noindex": True,
        "allow_remote_assets": False,
        "source_date_epoch": 0,
    }
    build.update(raw.get("build") or {})
    build["base_url"] = str(build.get("base_url") or "https://landing.example.net/")
    if not build["base_url"].endswith("/"):
        build["base_url"] += "/"
    base_path = str(build.get("base_path") or "/")
    if not base_path.startswith("/"):
        base_path = "/" + base_path
    if not base_path.endswith("/"):
        base_path += "/"
    build["base_path"] = base_path

    groups_raw = raw.get("groups") or []
    if not isinstance(groups_raw, list) or not groups_raw:
        raise ConfigError("At least one group is required")

    groups: list[dict[str, Any]] = []
    for index, group_raw in enumerate(groups_raw):
        if not isinstance(group_raw, dict):
            raise ConfigError(f"groups[{index}] must be a mapping")
        group_id = str(group_raw.get("id") or "").strip()
        if not group_id:
            raise ConfigError(f"groups[{index}] is missing id")
        group = {
            "id": group_id,
            "name": str(group_raw.get("name") or group_id),
            "icon": str(group_raw.get("icon") or "link"),
            "color": group_raw.get("color"),
            "order": group_raw.get("order", index * 10),
            "collapsed": bool(group_raw.get("collapsed", False)),
            "show_when": list(group_raw.get("show_when") or []),
            "hide_when": list(group_raw.get("hide_when") or []),
            "links": [],
        }
        links_raw = group_raw.get("links") or []
        if not isinstance(links_raw, list):
            raise ConfigError(f"groups[{index}].links must be a list")
        for link_index, link_raw in enumerate(links_raw):
            if not isinstance(link_raw, dict):
                raise ConfigError(f"groups[{index}].links[{link_index}] must be a mapping")
            link_id = str(link_raw.get("id") or "").strip()
            if not link_id:
                raise ConfigError(f"groups[{index}].links[{link_index}] is missing id")
            link = {
                "id": link_id,
                "slug": normalize_slug(link_raw.get("slug") or link_id),
                "name": str(link_raw.get("name") or link_id),
                "icon": str(link_raw.get("icon") or "auto"),
                "icon_background": str(link_raw.get("icon_background") or "").strip(),
                "url": str(link_raw.get("url") or "").strip(),
                "lan_url": str(link_raw.get("lan_url") or "").strip(),
                "target": link_raw.get("target") or defaults.get("target", "_blank"),
                "availability": link_raw.get("availability") or defaults.get("availability", "always"),
                "prefer": link_raw.get("prefer") or defaults.get("prefer", "auto"),
                "hotkey": str(link_raw.get("hotkey") or ""),
                "hidden": bool(link_raw.get("hidden", False)),
                "probe": link_raw.get("probe", True) is not False,
                "probe_url": str(link_raw.get("probe_url") or "").strip(),
                "probe_expect": str(link_raw.get("probe_expect") or "").strip(),
                "probe_image": str(link_raw.get("probe_image") or "").strip(),
                "favicon_url": str(link_raw.get("favicon_url") or "").strip(),
                "favicon_refresh": bool(link_raw.get("favicon_refresh", False)),
                "order": link_raw.get("order", link_index * 10),
                "icon_url": None,
            }
            group["links"].append(link)
        group["links"].sort(key=lambda item: (item["order"], item["id"]))
        groups.append(group)

    groups.sort(key=lambda item: (item["order"], item["id"]))

    return {
        "version": version,
        "site": site,
        "theme": theme,
        "layout": layout,
        "defaults": defaults,
        "search": search,
        "probe": probe,
        "redirect": redirect,
        "pwa": pwa,
        "server": server,
        "favicons": favicons,
        "build": build,
        "groups": groups,
    }


def validate_config(config: dict[str, Any]) -> list[str]:
    warnings: list[str] = []
    site = config["site"]
    if not str(site.get("title") or "").strip():
        raise ConfigError("site.title is required")

    base_url = config["build"]["base_url"]
    parse_url(base_url, "build.base_url")
    if not config["build"]["base_path"].startswith("/") or not config["build"]["base_path"].endswith("/"):
        raise ConfigError("build.base_path must start and end with '/'")

    for field in ("favicon",):
        value = site.get(field)
        if value in ICONS:
            continue
        if value and str(value).startswith(("http://", "https://")):
            if not config["build"].get("allow_remote_assets"):
                raise ConfigError(f"site.{field} remote URL requires build.allow_remote_assets: {value}")
        elif value:
            if not (ROOT / str(value)).exists():
                raise ConfigError(f"site.{field} not found: {value}")

    background = config["theme"].get("background") or {}
    background_image = background.get("image")
    if background_image and str(background_image).startswith(("http://", "https://")):
        if not config["build"].get("allow_remote_assets"):
            raise ConfigError(f"theme.background.image remote URL requires build.allow_remote_assets: {background_image}")
    elif background_image:
        if not (ROOT / str(background_image)).exists():
            raise ConfigError(f"theme.background.image not found: {background_image}")

    group_ids: set[str] = set()
    link_ids: set[str] = set()
    slugs: set[str] = set()
    hotkeys: set[str] = set()
    lan_count = 0
    ip_lan_count = 0

    for group in config["groups"]:
        if not SLUG_RE.match(group["id"]):
            raise ConfigError(f"Invalid group id: {group['id']!r}")
        if group["id"] in group_ids:
            raise ConfigError(f"Duplicate group id: {group['id']}")
        group_ids.add(group["id"])
        for link in group["links"]:
            label = f"{group['id']}/{link['id']}"
            if not SLUG_RE.match(link["id"]):
                raise ConfigError(f"Invalid link id: {link['id']!r}")
            if link["id"] in link_ids:
                raise ConfigError(f"Duplicate link id: {link['id']}")
            link_ids.add(link["id"])
            if not SLUG_RE.match(link["slug"]):
                raise ConfigError(f"Invalid slug for {label}: {link['slug']!r}")
            if link["slug"] in RESERVED_SLUGS:
                raise ConfigError(f"Reserved slug for {label}: {link['slug']!r}")
            if link["slug"] in slugs:
                raise ConfigError(f"Duplicate slug: {link['slug']}")
            slugs.add(link["slug"])
            if link["hotkey"]:
                if not re.match(r"^[a-zA-Z0-9]$", link["hotkey"]):
                    raise ConfigError(f"Link {label} hotkey must be a single letter or digit")
                if link["hotkey"] in hotkeys:
                    raise ConfigError(f"Duplicate hotkey: {link['hotkey']}")
                hotkeys.add(link["hotkey"])
            if not link["url"] and not link["lan_url"]:
                raise ConfigError(f"Link {label} needs url and/or lan_url")
            for field in ("url", "lan_url", "probe_url", "probe_image"):
                value = link[field]
                if value:
                    parse_url(value, f"{label}.{field}")
            if link["availability"] not in ("always", "lan_only", "wan_only"):
                raise ConfigError(f"Link {label} has invalid availability: {link['availability']}")
            if link["prefer"] not in ("auto", "lan", "wan"):
                raise ConfigError(f"Link {label} has invalid prefer: {link['prefer']}")
            if link["target"] not in ("_self", "_blank"):
                raise ConfigError(f"Link {label} target must be _self or _blank")
            if link["lan_url"]:
                lan_count += 1
                host = urlparse(link["lan_url"]).hostname or ""
                if re.match(r"^\d{1,3}(?:\.\d{1,3}){3}$", host) or ":" in host:
                    ip_lan_count += 1
            if link["icon"] in ("auto", "none"):
                pass
            elif link["icon"].startswith(("http://", "https://")):
                if not config["build"]["allow_remote_assets"]:
                    raise ConfigError(f"Link {label} remote icon requires build.allow_remote_assets: {link['icon']}")
            elif link["icon"] not in ICONS:
                icon_path = ROOT / link["icon"]
                if not icon_path.exists():
                    raise ConfigError(f"Link {label} icon not found: {link['icon']}")
            if link["favicon_url"]:
                parse_url(link["favicon_url"], f"{label}.favicon_url")
            if link["icon_background"] and not hex_to_rgb(link["icon_background"]):
                raise ConfigError(f"Link {label} icon_background must be a hex color: {link['icon_background']}")

    if not lan_count:
        warnings.append("No lan_url is configured; LAN detection is disabled.")
    elif ip_lan_count == lan_count:
        warnings.append("All LAN URLs are IP addresses; the probe tests reachability, not DNS resolution.")

    probe = config["probe"]
    if not probe["internet"]["urls"]:
        raise ConfigError("probe.internet.urls must not be empty")
    for url in probe["internet"]["urls"]:
        parse_url(url, "probe.internet.urls[]")
    if probe["internet"]["mode"] not in ("no-cors", "cors"):
        raise ConfigError("probe.internet.mode must be no-cors or cors")
    if probe["internet"].get("image"):
        image_value = str(probe["internet"]["image"])
        if image_value.startswith(("http://", "https://")) and not config["build"].get("allow_remote_assets"):
            raise ConfigError("probe.internet.image remote URL requires build.allow_remote_assets")
    for url in probe["lan"]["urls"]:
        parse_url(url, "probe.lan.urls[]")
    if probe["lan"].get("ip_fallback"):
        parse_url(probe["lan"]["ip_fallback"], "probe.lan.ip_fallback")
    if probe["lan"].get("source") not in ("links", "explicit", "both"):
        raise ConfigError("probe.lan.source must be links, explicit, or both")
    if probe["lan"].get("random_subdomain"):
        domain = str(probe["lan"].get("domain") or "").strip()
        if not domain:
            raise ConfigError("probe.lan.domain is required when random_subdomain is true")
        if not re.match(r"^[a-zA-Z0-9.-]+$", domain):
            raise ConfigError("probe.lan.domain is invalid")

    search = config["search"]
    if search.get("engine"):
        engine = str(search["engine"])
        if "{query}" not in engine:
            raise ConfigError("search.engine must contain the {query} placeholder")
        parse_url(engine.replace("{query}", "test"), "search.engine")
    if search.get("suggest_url"):
        suggest = str(search["suggest_url"])
        if "{query}" not in suggest:
            raise ConfigError("search.suggest_url must contain the {query} placeholder")
        parse_url(suggest.replace("{query}", "test"), "search.suggest_url")

    redirect = config["redirect"]
    if not isinstance(redirect["delay_ms"], int) or redirect["delay_ms"] < 0:
        raise ConfigError("redirect.delay_ms must be a non-negative integer")
    if not isinstance(redirect["max_delay_ms"], int) or redirect["max_delay_ms"] < redirect["delay_ms"]:
        raise ConfigError("redirect.max_delay_ms must be >= delay_ms")

    def scan_secrets(value: Any, path: str) -> None:
        if isinstance(value, dict):
            for key, item in value.items():
                key_text = str(key).lower()
                if any(marker in key_text for marker in ("password", "secret", "api_key", "apikey", "private_key", "access_token")):
                    warnings.append(f"Config key looks secret-like: {path}.{key}")
                scan_secrets(item, f"{path}.{key}" if path else str(key))
        elif isinstance(value, list):
            for index, item in enumerate(value):
                scan_secrets(item, f"{path}[{index}]")
        elif isinstance(value, str):
            if re.search(r"(?i)(password|secret|api[_-]?key|private[_-]?key)\s*[:=]\s*\S+", value):
                warnings.append(f"Config value looks secret-like at {path}")

    scan_secrets(config, "config")

    server = config["server"]
    try:
        port = int(server.get("port", 8080))
    except (TypeError, ValueError) as exc:
        raise ConfigError("server.port must be an integer") from exc
    if not 1 <= port <= 65535:
        raise ConfigError("server.port must be between 1 and 65535")
    if not str(server.get("host") or "").strip():
        raise ConfigError("server.host must not be empty")
    if not str(server.get("health_path") or "").startswith("/"):
        raise ConfigError("server.health_path must start with '/'")

    return warnings


def canonical_json(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def config_hash(config: dict[str, Any]) -> str:
    return hashlib.sha256(canonical_json(config).encode("utf-8")).hexdigest()


def version_from_hash(value: str) -> str:
    return "1.0." + str(int(value[:8], 16))


def icon_svg(name: str) -> str:
    body = ICONS.get(name)
    if not body:
        return ""
    return (
        '<svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" '
        'stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">' + body + "</svg>"
    )


def initials(label: str) -> str:
    parts = [part for part in str(label or "").strip().split() if part]
    if not parts:
        return "?"
    if len(parts) == 1:
        return parts[0][:1].upper()
    return (parts[0][:1] + parts[-1][:1]).upper()


def icon_html(name: str, label: str, icon_url: str | None = None, class_name: str = "") -> str:
    if icon_url:
        return f'<img class="{html.escape(class_name)}" src="{html.escape(icon_url)}" alt="">'
    svg = icon_svg(name)
    if svg:
        return svg
    return f'<span class="icon-letter">{html.escape(initials(label))}</span>'


def hex_to_rgb(value: str) -> tuple[int, int, int] | None:
    value = str(value or "").strip()
    if re.match(r"^#[0-9a-fA-F]{3}$", value):
        value = "#" + "".join(char * 2 for char in value[1:])
    match = re.match(r"^#[0-9a-fA-F]{6}$", value)
    if not match:
        return None
    return tuple(int(value[i : i + 2], 16) for i in (1, 3, 5))


def contrast_color(value: str) -> str:
    rgb = hex_to_rgb(value)
    if not rgb:
        return "#ffffff"
    r, g, b = rgb
    luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255
    return "#08111f" if luminance > 0.55 else "#ffffff"


def theme_css(config: dict[str, Any], background_url: str | None) -> str:
    theme = config["theme"]
    layout = config["layout"]
    light = theme["colors"]["light"]
    dark = theme["colors"]["dark"]
    accent = theme.get("accent") or dark.get("accent") or "#38bdf8"
    radius = int(theme.get("radius") or 16)
    density = theme.get("density") or "comfortable"
    space = {"compact": "12px", "comfortable": "16px", "spacious": "20px"}.get(density, "16px")
    font = (
        "system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif"
        if theme.get("font") in (None, "", "system")
        else str(theme.get("font"))
    )
    background = theme.get("background") or {}
    bg_image = f'url("{background_url}")' if background_url else "none"
    shadows_light = (
        "--shadow-sm: 0 1px 2px rgba(74,45,32,.06), 0 1px 1px rgba(74,45,32,.03);"
        "--shadow-md: 0 10px 24px rgba(74,45,32,.10), 0 2px 6px rgba(74,45,32,.05);"
        "--shadow-lg: 0 24px 60px rgba(74,45,32,.18), 0 8px 20px rgba(74,45,32,.08);"
    )
    shadows_dark = (
        "--shadow-sm: 0 1px 2px rgba(0,0,0,.35);"
        "--shadow-md: 0 12px 28px rgba(0,0,0,.4);"
        "--shadow-lg: 0 30px 70px rgba(0,0,0,.55);"
    )

    def block(colors: dict[str, Any], shadows: str) -> str:
        return (
            f"--bg: {colors.get('bg', '#f6f8fb')};"
            f"--surface: {colors.get('surface', '#ffffff')};"
            f"--text: {colors.get('text', '#0f172a')};"
            f"--muted: {colors.get('muted', '#5b6474')};"
            f"--border: {colors.get('border', '#e2e8f0')};"
            f"--accent: {colors.get('accent', accent)};"
            f"--accent2: {colors.get('accent2', '#6366f1')};"
            f"--success: {colors.get('success', '#059669')};"
            f"--warning: {colors.get('warning', '#b45309')};"
            f"--danger: {colors.get('danger', '#dc2626')};"
            f"--accent-contrast: {contrast_color(str(colors.get('accent', accent)))};"
            f"{shadows}"
        )

    base = (
        ":root{"
        + block(light, shadows_light)
        + "--accent-ring: color-mix(in srgb, var(--accent) 22%, transparent);"
        + "--surface-hover: color-mix(in srgb, var(--surface) 88%, var(--text) 12%);"
        + "--border-strong: color-mix(in srgb, var(--border) 60%, var(--text) 40%);"
        + "--tag-bg: color-mix(in srgb, var(--text) 7%, transparent);"
        + "--state-bg: color-mix(in srgb, var(--text) 6%, transparent);"
        + f"--radius: {radius}px;"
        + f"--columns: {int(layout.get('columns', 3))};"
        + f"--max-width: {int(layout.get('max_width', 1400))}px;"
        + f"--groups-max-width: {int(layout.get('groups_max_width', 1080))}px;"
        + f"--space: {space};"
        + f"--font: {font};"
        + f"--bg-image: {bg_image};"
        + f"--bg-overlay: {(background.get('overlay') or 'transparent') if bg_image and bg_image != 'none' else 'transparent'};"
        + f"--bg-blur: {int(background.get('blur') or 0)}px;"
        + f"--bg-brightness: {float(background.get('brightness') or 1)};"
        + f"--bg-position: {background.get('position') or 'center'};"
        + f"--bg-size: {background.get('size') or 'cover'};"
        + "}"
    )
    dark_block = "[data-theme=\"dark\"]{" + block(dark, shadows_dark) + "}"
    auto_block = "@media (prefers-color-scheme: dark){[data-theme=\"auto\"]{" + block(dark, shadows_dark) + "}}"
    group_styles = []
    for group in config["groups"]:
        color = group.get("color") or accent
        group_styles.append(
            f'[data-group-id="{group["id"]}"]{{--group-accent:{color};}}'
        )
        for link in group["links"]:
            group_styles.append(f'[data-link-id="{link["id"]}"]{{--card-accent:{color};}}')
            if link.get("icon_background"):
                group_styles.append(
                    f'[data-link-id="{link["id"]}"] .card-icon{{background:{link["icon_background"]};}}'
                )
    return "\n".join([base, dark_block, auto_block, "\n".join(group_styles)])


def collect_origins(config: dict[str, Any]) -> set[str]:
    origins: set[str] = set()

    def add(url: str | None) -> None:
        if not url:
            return
        parsed = urlparse(str(url))
        if parsed.scheme in ("http", "https") and parsed.netloc:
            origins.add(f"{parsed.scheme}://{parsed.netloc}")

    for url in config["probe"]["internet"]["urls"]:
        add(url)
    for group in config["groups"]:
        for link in group["links"]:
            add(link.get("lan_url"))
            add(link.get("probe_url"))
    for url in config["probe"]["lan"]["urls"]:
        add(url)
    add(config["probe"]["lan"].get("ip_fallback"))
    if config["probe"]["lan"].get("random_subdomain") and config["probe"]["lan"].get("domain"):
        domain = config["probe"]["lan"]["domain"]
        origins.add(f"https://*.{domain}")
        origins.add(f"http://*.{domain}")
    return origins


def collect_img_origins(config: dict[str, Any]) -> set[str]:
    origins: set[str] = set()
    values = [
        config["site"].get("favicon"),
        (config["theme"].get("background") or {}).get("image"),
        config["probe"]["internet"].get("image"),
    ]
    for group in config["groups"]:
        values.append(group.get("icon"))
        for link in group["links"]:
            values.append(link.get("icon"))
    for value in values:
        if value and str(value).startswith(("http://", "https://")):
            parsed = urlparse(str(value))
            if parsed.scheme in ("http", "https") and parsed.netloc:
                origins.add(f"{parsed.scheme}://{parsed.netloc}")
    return origins


def csp_policy(config: dict[str, Any], inline_css: str | None, inline_js: str | None, config_json: str) -> str:
    origins = " ".join(sorted(collect_origins(config)))
    connect = ("'self' " + origins).strip()
    img_origins = " ".join(sorted(collect_img_origins(config) | collect_origins(config)))
    img_src = ("'self' data:" + (" " + img_origins if img_origins else "")).strip()
    config_hash_b64 = base64.b64encode(hashlib.sha256(config_json.encode("utf-8")).digest()).decode("ascii")
    if inline_css is not None and inline_js is not None:
        css_hash = base64.b64encode(hashlib.sha256(inline_css.encode("utf-8")).digest()).decode("ascii")
        js_hash = base64.b64encode(hashlib.sha256(inline_js.encode("utf-8")).digest()).decode("ascii")
        return (
            "default-src 'self'; base-uri 'none'; form-action 'none'; object-src 'none'; "
            f"script-src 'sha256-{js_hash}' 'sha256-{config_hash_b64}'; "
            f"style-src 'sha256-{css_hash}'; img-src {img_src}; font-src 'self'; "
            f"connect-src {connect}; manifest-src 'self'; worker-src 'self';"
        )
    return (
        "default-src 'self'; base-uri 'none'; form-action 'none'; object-src 'none'; "
        f"script-src 'self' 'sha256-{config_hash_b64}'; style-src 'self'; "
        f"img-src {img_src}; font-src 'self'; connect-src {connect}; "
        "manifest-src 'self'; worker-src 'self';"
    )


def csp_meta(config: dict[str, Any], inline_css: str | None, inline_js: str | None, config_json: str) -> str:
    csp = csp_policy(config, inline_css, inline_js, config_json)
    return f'<meta http-equiv="Content-Security-Policy" content="{html.escape(csp, quote=True)}">'


def copy_asset(source: Path, dest_dir: Path, dest_rel: str) -> str:
    dest = dest_dir / dest_rel
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, dest)
    return dest_rel.replace("\\", "/")


def data_uri(path: Path) -> str:
    mime = {
        ".svg": "image/svg+xml",
        ".png": "image/png",
        ".jpg": "image/jpeg",
        ".jpeg": "image/jpeg",
        ".webp": "image/webp",
        ".gif": "image/gif",
        ".ico": "image/x-icon",
    }.get(path.suffix.lower(), "application/octet-stream")
    encoded = base64.b64encode(path.read_bytes()).decode("ascii")
    return f"data:{mime};base64,{encoded}"


def icon_data_uri(name: str, background: str, foreground: str) -> str:
    body = ICONS.get(name) or ICONS["cat"]
    svg = (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">'
        f'<rect width="24" height="24" rx="6" fill="{background}"/>'
        f'<g fill="none" stroke="{foreground}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">{body}</g>'
        "</svg>"
    )
    encoded = base64.b64encode(svg.encode("utf-8")).decode("ascii")
    return "data:image/svg+xml;base64," + encoded


def generate_icon(path: Path, size: int, accent: str, maskable: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    rgb = hex_to_rgb(accent) or (240, 160, 106)
    background = (30, 25, 23, 255)
    image = Image.new("RGBA", (size, size), background)
    draw = ImageDraw.Draw(image)
    if not maskable:
        draw.rounded_rectangle((0, 0, size - 1, size - 1), radius=int(size * 0.22), fill=background)
    pad = int(size * (0.18 if maskable else 0.14))
    inner = size - pad * 2
    head = (pad + inner * 0.08, pad + inner * 0.22, size - pad - inner * 0.08, size - pad - inner * 0.04)
    draw.ellipse(head, fill=rgb + (255,))
    ear_y = pad + inner * 0.10
    draw.polygon(
        [(pad + inner * 0.12, ear_y + inner * 0.22), (pad + inner * 0.12, ear_y), (pad + inner * 0.36, ear_y + inner * 0.08)],
        fill=rgb + (255,),
    )
    draw.polygon(
        [(size - pad - inner * 0.12, ear_y + inner * 0.22), (size - pad - inner * 0.12, ear_y), (size - pad - inner * 0.36, ear_y + inner * 0.08)],
        fill=rgb + (255,),
    )
    draw.polygon(
        [(pad + inner * 0.16, ear_y + inner * 0.18), (pad + inner * 0.16, ear_y + inner * 0.04), (pad + inner * 0.30, ear_y + inner * 0.10)],
        fill=background,
    )
    draw.polygon(
        [(size - pad - inner * 0.16, ear_y + inner * 0.18), (size - pad - inner * 0.16, ear_y + inner * 0.04), (size - pad - inner * 0.30, ear_y + inner * 0.10)],
        fill=background,
    )
    eye_r = max(2, int(size * 0.035))
    eye_y = int(size * 0.50)
    for cx in (int(size * 0.38), int(size * 0.62)):
        draw.ellipse((cx - eye_r, eye_y - eye_r, cx + eye_r, eye_y + eye_r), fill=background)
    nose_y = int(size * 0.60)
    nose_w = max(2, int(size * 0.025))
    draw.polygon([(int(size * 0.50) - nose_w, nose_y), (int(size * 0.50) + nose_w, nose_y), (int(size * 0.50), nose_y + nose_w)], fill=background)
    line_w = max(1, int(size * 0.012))
    for offset in (-1, 1):
        y = nose_y + offset * int(size * 0.035)
        draw.line((int(size * 0.42), y, int(size * 0.28), y - offset * int(size * 0.02)), fill=background, width=line_w)
        draw.line((int(size * 0.58), y, int(size * 0.72), y - offset * int(size * 0.02)), fill=background, width=line_w)
    image.save(path, format="PNG", optimize=True)


def build_theme_css(config: dict[str, Any], background_url: str | None) -> str:
    return theme_css(config, background_url)


def build_runtime_config(config: dict[str, Any]) -> dict[str, Any]:
    return {
        "site": config["site"],
        "theme": config["theme"],
        "layout": config["layout"],
        "defaults": config["defaults"],
        "search": config["search"],
        "probe": config["probe"],
        "redirect": config["redirect"],
        "server": {"probe": config["server"].get("probe", True)},
        "groups": config["groups"],
    }


ICON_LINK_RE = re.compile(
    r"<link\b[^>]*>", re.IGNORECASE
)
ICON_REL_RE = re.compile(r"rel\s*=\s*[\"']([^\"']*)[\"']", re.IGNORECASE)
ICON_HREF_RE = re.compile(r"href\s*=\s*[\"']([^\"']*)[\"']", re.IGNORECASE)
ICON_SIZES_RE = re.compile(r"sizes\s*=\s*[\"']([^\"']*)[\"']", re.IGNORECASE)
ICON_TYPES = {"icon", "shortcut icon", "apple-touch-icon", "apple-touch-icon-precomposed", "mask-icon"}


def is_private_host(host: str) -> bool:
    if not host:
        return True
    if host in ("localhost",) or host.endswith(".local") or host.endswith(".lan"):
        return True
    try:
        address = ipaddress.ip_address(host)
    except ValueError:
        return False
    return address.is_private or address.is_loopback or address.is_link_local


def fetch_bytes(url: str, timeout: float, max_bytes: int) -> tuple[bytes, str]:
    request = urllib.request.Request(url, headers={"User-Agent": "Landing-Favicon/0.2"})
    # LAN services commonly use self-signed certificates; the build only reads
    # a public icon, so accept them rather than failing the fetch.
    context = ssl.create_default_context()
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    with urllib.request.urlopen(request, timeout=timeout, context=context) as response:
        content_type = response.headers.get("Content-Type", "")
        data = response.read(max_bytes + 1)
    if len(data) > max_bytes:
        raise ValueError("response too large")
    return data, content_type


def discover_icon_urls(page_url: str, page_html: str) -> list[str]:
    found: list[tuple[int, str]] = []
    for tag in ICON_LINK_RE.findall(page_html):
        rel_match = ICON_REL_RE.search(tag)
        href_match = ICON_HREF_RE.search(tag)
        if not rel_match or not href_match:
            continue
        rel = " ".join(rel_match.group(1).lower().split())
        if rel not in ICON_TYPES:
            continue
        href = html.unescape(href_match.group(1).strip())
        if not href or href.startswith("data:"):
            continue
        score = 0
        if rel == "icon":
            score = 30
        elif rel == "shortcut icon":
            score = 25
        elif rel.startswith("apple-touch-icon"):
            score = 20
        elif rel == "mask-icon":
            score = 10
        sizes_match = ICON_SIZES_RE.search(tag)
        if sizes_match:
            for size in sizes_match.group(1).lower().split():
                if "x" in size:
                    try:
                        score += int(size.split("x", 1)[0]) // 16
                    except ValueError:
                        pass
        found.append((score, urljoin(page_url, href)))
    found.sort(key=lambda item: (-item[0], item[1]))
    return [url for _, url in found]


def normalize_icon(data: bytes, content_type: str, url: str) -> tuple[bytes, str] | None:
    lowered = content_type.lower()
    if "svg" in lowered or url.lower().endswith(".svg"):
        text = data.decode("utf-8", "replace")
        if "<svg" not in text.lower():
            return None
        if re.search(r"<script|on\w+\s*=|javascript:", text, re.IGNORECASE):
            return None
        return data, "svg"
    try:
        image = Image.open(io.BytesIO(data))
        image.load()
    except Exception:
        return None
    if image.width < 8 or image.height < 8:
        return None
    image = image.convert("RGBA")
    image.thumbnail((128, 128), Image.LANCZOS)
    buffer = io.BytesIO()
    image.save(buffer, format="PNG", optimize=True)
    return buffer.getvalue(), "png"


def fetch_favicon(link: dict[str, Any], favicons: dict[str, Any]) -> tuple[tuple[bytes, str] | None, str | None]:
    """Return ((data, ext) | None, warning | None).

    The warning is set when the configured URL's scheme is wrong (for example
    an http:// LAN URL that only answers on https://), so the user can fix the
    config instead of relying on a silent fallback."""
    sources = [link.get("favicon_url"), link.get("url"), link.get("lan_url")]
    timeout = float(favicons.get("timeout_ms") or 5000) / 1000.0
    max_bytes = int(favicons.get("max_bytes") or 1048576)
    for source in sources:
        if not source:
            continue
        parsed = urlparse(source)
        if parsed.scheme not in ("http", "https") or not parsed.netloc:
            continue
        if is_private_host(parsed.hostname or "") and not favicons.get("allow_private_hosts"):
            continue
        other = "https" if parsed.scheme == "http" else "http"
        swapped = parsed._replace(scheme=other).geturl()
        attempts = [(source, False)]
        if swapped != source:
            attempts.append((swapped, True))
        for attempt, is_fallback in attempts:
            candidates: list[str] = []
            try:
                page, content_type = fetch_bytes(attempt, timeout, max_bytes)
                if "html" in content_type.lower() or b"<html" in page[:512].lower():
                    candidates.extend(discover_icon_urls(attempt, page.decode("utf-8", "replace")))
            except (urllib.error.URLError, OSError, ValueError):
                pass
            attempt_parsed = urlparse(attempt)
            origin = f"{attempt_parsed.scheme}://{attempt_parsed.netloc}"
            candidates.append(origin + "/favicon.ico")
            seen: set[str] = set()
            for candidate in candidates:
                if candidate in seen:
                    continue
                seen.add(candidate)
                try:
                    data, content_type = fetch_bytes(candidate, timeout, max_bytes)
                except (urllib.error.URLError, OSError, ValueError):
                    continue
                normalized = normalize_icon(data, content_type, candidate)
                if normalized:
                    warning = None
                    if is_fallback:
                        warning = (
                            f"{link.get('slug')}: configured URL {source} did not respond; "
                            f"it works over {other}://. Update the config to {swapped}."
                        )
                    return normalized, warning
    return None, None


def resolve_favicons(config: dict[str, Any], refresh: bool = False, warnings: list[str] | None = None) -> dict[str, str]:
    favicons = config.get("favicons") or {}
    if not favicons.get("enabled", True):
        return {}
    cache_dir = ROOT / str(favicons.get("cache_dir") or "assets/favicons")
    cache_dir.mkdir(parents=True, exist_ok=True)
    resolved: dict[str, str] = {}
    for group in config["groups"]:
        for link in group["links"]:
            if link.get("icon") == "none":
                continue
            slug = link["slug"]
            existing = sorted(cache_dir.glob(slug + ".*"))
            if existing and not refresh and not link.get("favicon_refresh"):
                resolved[slug] = existing[0].relative_to(ROOT).as_posix()
                continue
            result, warning = fetch_favicon(link, favicons)
            if warning and warnings is not None:
                warnings.append(warning)
            if not result:
                continue
            data, extension = result
            for stale in cache_dir.glob(slug + ".*"):
                stale.unlink()
            target = cache_dir / f"{slug}.{extension}"
            target.write_bytes(data)
            resolved[slug] = target.relative_to(ROOT).as_posix()
    return resolved


def resolve_assets(config: dict[str, Any], site_dir: Path, single_dir: Path, single_file: bool, favicon_map: dict[str, str] | None = None) -> dict[str, Any]:
    """Copy/generate assets and return URLs/data URIs used by templates."""
    assets: dict[str, Any] = {
        "favicon_url": "",
        "manifest_url": config["build"]["base_path"] + "manifest.webmanifest",
        "styles_url": config["build"]["base_path"] + "styles.css",
        "app_js_url": config["build"]["base_path"] + "app.js",
        "background_url": None,
        "pwa_icons": [],
    }
    base_path = config["build"]["base_path"]
    site_assets = site_dir / "assets"
    site_assets.mkdir(parents=True, exist_ok=True)

    dark_bg = config["theme"]["colors"]["dark"]["bg"]
    theme_accent = config["theme"].get("accent") or config["theme"]["colors"]["dark"]["accent"]
    favicon = config["site"].get("favicon")
    if favicon in ICONS:
        assets["favicon_url"] = icon_data_uri(favicon, dark_bg, theme_accent)
    elif favicon:
        if str(favicon).startswith(("http://", "https://")):
            assets["favicon_url"] = str(favicon)
        else:
            source = ROOT / str(favicon)
            if source.exists():
                if single_file:
                    assets["favicon_url"] = data_uri(source)
                else:
                    rel = copy_asset(source, site_dir, "assets/favicon" + source.suffix.lower())
                    assets["favicon_url"] = base_path + rel

    background = config["theme"].get("background") or {}
    background_image = background.get("image")
    if background_image:
        if str(background_image).startswith(("http://", "https://")):
            assets["background_url"] = str(background_image)
        else:
            source = ROOT / str(background_image)
            if source.exists():
                if single_file and source.stat().st_size <= 250_000:
                    assets["background_url"] = data_uri(source)
                elif not single_file:
                    rel = copy_asset(source, site_dir, "assets/background" + source.suffix.lower())
                    assets["background_url"] = base_path + rel

    accent = config["theme"].get("accent") or config["theme"]["colors"]["dark"]["accent"]
    pwa_icons = config["pwa"].get("icons") or []
    if config["pwa"].get("enabled") and pwa_icons:
        for icon in pwa_icons:
            src = icon.get("src") if isinstance(icon, dict) else None
            if not src:
                continue
            source = ROOT / str(src)
            if not source.exists():
                raise ConfigError(f"PWA icon not found: {src}")
            rel = copy_asset(source, site_dir, f"icons/{source.name}")
            assets["pwa_icons"].append({
                "src": base_path + rel,
                "sizes": icon.get("sizes", ""),
                "type": icon.get("type", "image/png"),
                "purpose": icon.get("purpose", "any"),
            })
    elif config["pwa"].get("enabled"):
        icons_dir = site_dir / "icons"
        icons_dir.mkdir(parents=True, exist_ok=True)
        for size, name, maskable in ((192, "icon-192.png", False), (512, "icon-512.png", False), (512, "icon-maskable-512.png", True)):
            generate_icon(icons_dir / name, size, str(accent), maskable=maskable)
            assets["pwa_icons"].append({
                "src": base_path + f"icons/{name}",
                "sizes": f"{size}x{size}",
                "type": "image/png",
                "purpose": "maskable" if maskable else "any",
            })

    favicon_map = favicon_map or {}

    # Local link/group icons.
    for group in config["groups"]:
        group["icon_url"] = None
        icon = group.get("icon")
        if icon and str(icon).startswith(("http://", "https://")) and config["build"].get("allow_remote_assets"):
            group["icon_url"] = str(icon)
        elif icon and not str(icon).startswith(("http://", "https://")) and icon not in ICONS:
            source = ROOT / str(icon)
            if source.exists():
                if single_file:
                    group["icon_url"] = data_uri(source)
                else:
                    rel = copy_asset(source, site_dir, f"assets/icons/{source.name}")
                    group["icon_url"] = base_path + rel
        for link in group["links"]:
            link["icon_url"] = None
            icon = link.get("icon")

            # Ladder: real build-time favicon > user custom icon > default icon.
            cached = favicon_map.get(link["slug"])
            if cached and (ROOT / cached).exists():
                source = ROOT / cached
                if single_file:
                    link["icon_url"] = data_uri(source)
                else:
                    rel = copy_asset(source, site_dir, f"assets/favicons/{source.name}")
                    link["icon_url"] = base_path + rel
            elif icon and icon not in ("auto", "none") and str(icon).startswith(("http://", "https://")) and config["build"].get("allow_remote_assets"):
                link["icon_url"] = str(icon)
            elif icon and icon not in ("auto", "none") and not str(icon).startswith(("http://", "https://")) and icon not in ICONS:
                source = ROOT / str(icon)
                if source.exists():
                    if single_file:
                        link["icon_url"] = data_uri(source)
                    else:
                        rel = copy_asset(source, site_dir, f"assets/icons/{source.name}")
                        link["icon_url"] = base_path + rel

    return assets


def make_group_views(config: dict[str, Any], single_file: bool) -> list[dict[str, Any]]:
    base_path = config["build"]["base_path"]
    use_redirect = not config["probe"].get("homepage", False)
    groups = []
    for group in config["groups"]:
        group_view = dict(group)
        group_view["icon_html"] = icon_html(group.get("icon"), group.get("name", ""), group.get("icon_url"), "group-icon-img")
        links = []
        for link in group["links"]:
            link_view = dict(link)
            icon_name = link.get("icon")
            if icon_name in ("auto", "none"):
                icon_name = config["defaults"].get("icon") or "link"
            link_view["icon_html"] = icon_html(icon_name, link.get("name", ""), link.get("icon_url"), "icon-img")
            if use_redirect:
                link_view["href_default"] = base_path + "go/" + link["slug"] + "/"
            else:
                link_view["href_default"] = link.get("url") or link.get("lan_url") or "#"
            link_view["rel"] = "noopener noreferrer" if link.get("target") == "_blank" else ""
            links.append(link_view)
        group_view["links"] = links
        groups.append(group_view)
    return groups


def write_manifest(config: dict[str, Any], site_dir: Path, assets: dict[str, Any]) -> None:
    if not config["pwa"].get("enabled"):
        return
    base_path = config["build"]["base_path"]
    shortcuts = []
    for shortcut in config["pwa"].get("shortcuts") or []:
        if not isinstance(shortcut, dict):
            continue
        name = str(shortcut.get("name") or "").strip()
        url = str(shortcut.get("url") or "").strip()
        if not name or not url:
            continue
        if url.startswith("/"):
            url = base_path + url.lstrip("/")
        shortcuts.append({"name": name, "url": url})
    manifest = {
        "name": config["pwa"].get("name") or config["site"]["title"],
        "short_name": config["pwa"].get("short_name") or "Landing",
        "description": config["pwa"].get("description") or config["site"].get("description", ""),
        "start_url": base_path,
        "scope": base_path,
        "display": config["pwa"].get("display") or "standalone",
        "theme_color": config["pwa"].get("theme_color") or config["theme"]["colors"]["dark"]["bg"],
        "background_color": config["pwa"].get("background_color") or config["theme"]["colors"]["dark"]["bg"],
        "icons": assets["pwa_icons"],
    }
    if shortcuts:
        manifest["shortcuts"] = shortcuts
    (site_dir / "manifest.webmanifest").write_text(
        json.dumps(manifest, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def write_service_worker(config: dict[str, Any], site_dir: Path, build_hash_value: str) -> None:
    base_path = config["build"]["base_path"]
    precache = [
        base_path,
        base_path + "index.html",
        base_path + "styles.css",
        base_path + "app.js",
    ]
    if config["pwa"].get("enabled"):
        precache.append(base_path + "manifest.webmanifest")
    # Add generated PWA icons if present.
    icons_dir = site_dir / "icons"
    if icons_dir.exists():
        for path in sorted(icons_dir.glob("*.png")):
            precache.append(base_path + "icons/" + path.name)
    precache = list(dict.fromkeys(precache))
    source = (SRC / "js" / "sw.js").read_text(encoding="utf-8")
    source = source.replace("__CACHE_VERSION__", "landing-" + build_hash_value[:12])
    source = source.replace("__PRECACHE__", json.dumps(precache, ensure_ascii=False))
    source = source.replace("__BASE_PATH__", base_path)
    (site_dir / "sw.js").write_text(source, encoding="utf-8")


def write_redirect_pages(config: dict[str, Any], site_dir: Path, template: Any, assets: dict[str, Any], build_hash: str, full_css: str) -> None:
    if not config["redirect"].get("generate_static_pages"):
        return
    base_path = config["build"]["base_path"]
    redirect_js = (SRC / "js" / "redirect.js").read_text(encoding="utf-8")
    core_js = (SRC / "js" / "core.js").read_text(encoding="utf-8")
    inline_js = "\n\n".join([core_js, redirect_js])
    inline_css = full_css
    for group in config["groups"]:
        for link in group["links"]:
            slug = link["slug"]
            target_dir = site_dir / "go" / slug
            target_dir.mkdir(parents=True, exist_ok=True)
            redirect_data = {
                "link": {
                    "slug": link["slug"],
                    "name": link["name"],
                    "url": link["url"],
                    "lan_url": link["lan_url"],
                    "availability": link["availability"],
                    "prefer": link["prefer"],
                    "icon_url": link.get("icon_url"),
                },
                "config": {"redirect": config["redirect"]},
                "basePath": base_path,
                "mode": "auto",
                "delay": config["redirect"]["delay_ms"],
            }
            redirect_json = json.dumps(redirect_data, ensure_ascii=False, separators=(",", ":")).replace("</", "<\\/")
            content = template.render(
                site=config["site"],
                theme=config["theme"],
                name=link["name"],
                icon_url=link.get("icon_url") or assets["favicon_url"],
                base_path=base_path,
                csp_meta=csp_meta(config, inline_css, inline_js, redirect_json),
                inline_css=inline_css,
                inline_js=inline_js,
                redirect_json=redirect_json,
            )
            (target_dir / "index.html").write_text(content, encoding="utf-8")


def write_deployment_helpers(config: dict[str, Any], site_dir: Path, config_json: str) -> None:
    csp = csp_policy(config, None, None, config_json)
    headers = (
        "/*\n"
        "  X-Content-Type-Options: nosniff\n"
        "  Referrer-Policy: no-referrer\n"
        "  Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=(), usb=()\n"
        f"  Content-Security-Policy: {csp}\n"
        "\n"
        "/index.html\n"
        "  Cache-Control: no-cache\n"
        "/sw.js\n"
        "  Cache-Control: no-cache\n"
        "/styles.css\n"
        "  Cache-Control: no-cache\n"
        "/app.js\n"
        "  Cache-Control: no-cache\n"
        "/manifest.webmanifest\n"
        "  Cache-Control: no-cache\n"
        "/assets/*\n"
        "  Cache-Control: public, max-age=86400\n"
        "/icons/*\n"
        "  Cache-Control: public, max-age=86400\n"
    )
    (site_dir / "_headers").write_text(headers, encoding="utf-8")

    caddy = f""":8080 {{
    root * .
    file_server
    try_files {{path}} /index.html
    header {{
        X-Content-Type-Options "nosniff"
        Referrer-Policy "no-referrer"
        Permissions-Policy "camera=(), microphone=(), geolocation=(), payment=(), usb=()"
        Content-Security-Policy "{csp}"
    }}
    @no_cache path /index.html /sw.js /styles.css /app.js /manifest.webmanifest
    header @no_cache Cache-Control "no-cache"
    @hashed path /assets/* /icons/*
    header @hashed Cache-Control "public, max-age=86400"
}}
"""
    (site_dir / "Caddyfile.example").write_text(caddy, encoding="utf-8")

    nginx = f"""server {{
    listen 8080;
    server_name _;
    root /srv/landing;
    index index.html;

    location / {{
        try_files $uri $uri/ /index.html;
    }}

    add_header X-Content-Type-Options "nosniff" always;
    add_header Referrer-Policy "no-referrer" always;
    add_header Permissions-Policy "camera=(), microphone=(), geolocation=(), payment=(), usb=()" always;
    add_header Content-Security-Policy "{csp}" always;

    location = /index.html {{ add_header Cache-Control "no-cache"; }}
    location = /sw.js {{ add_header Cache-Control "no-cache"; }}
    location = /styles.css {{ add_header Cache-Control "no-cache"; }}
    location = /app.js {{ add_header Cache-Control "no-cache"; }}
    location = /manifest.webmanifest {{ add_header Cache-Control "no-cache"; }}
    location ^~ /assets/ {{ add_header Cache-Control "public, max-age=86400"; }}
    location ^~ /icons/ {{ add_header Cache-Control "public, max-age=86400"; }}
}}
"""
    (site_dir / "nginx.conf.example").write_text(nginx, encoding="utf-8")


def write_checksums(out_dir: Path) -> None:
    entries = []
    for path in sorted(out_dir.rglob("*")):
        if path.is_file() and path.name != "checksums.txt":
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            entries.append(f"{digest}  {path.relative_to(out_dir).as_posix()}")
    (out_dir / "checksums.txt").write_text("\n".join(entries) + "\n", encoding="utf-8")


def apply_env_overrides(config: dict[str, Any]) -> None:
    """Allow CI to target a deployment without editing the committed YAML."""
    base_url = os.environ.get("LANDING_BASE_URL", "").strip()
    if base_url:
        if not base_url.endswith("/"):
            base_url += "/"
        config["build"]["base_url"] = base_url
    base_path = os.environ.get("LANDING_BASE_PATH", "").strip()
    if base_path:
        if not base_path.startswith("/"):
            base_path = "/" + base_path
        if not base_path.endswith("/"):
            base_path += "/"
        config["build"]["base_path"] = base_path



def build(config_path: Path = DEFAULT_CONFIG, out_dir: Path | None = None, single_file: bool = True, quiet: bool = False, refresh_favicons: bool = False) -> dict[str, Any]:
    raw = load_yaml(config_path)
    config = normalize_config(raw)
    apply_env_overrides(config)
    warnings = validate_config(config)
    hash_value = config_hash(config)
    out = out_dir or (ROOT / str(config["build"].get("output_dir") or "dist"))
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True, exist_ok=True)
    site_dir = out / "site"
    site_dir.mkdir(parents=True, exist_ok=True)

    favicon_map = resolve_favicons(config, refresh=refresh_favicons, warnings=warnings)
    assets = resolve_assets(config, site_dir, out, single_file, favicon_map)
    theme_css_text = build_theme_css(config, assets["background_url"])
    main_css = (SRC / "styles" / "main.css").read_text(encoding="utf-8")
    full_css = theme_css_text + "\n\n" + main_css

    core_js = (SRC / "js" / "core.js").read_text(encoding="utf-8")
    icons_js = (SRC / "js" / "icons.js").read_text(encoding="utf-8")
    site_js = (SRC / "js" / "site.js").read_text(encoding="utf-8")
    full_js = "\n\n".join([core_js, icons_js, site_js])
    source_bundle = "\n".join(
        [hash_value, full_css, full_js]
        + [
            (SRC / "js" / "sw.js").read_text(encoding="utf-8"),
            (SRC / "templates" / "index.html.j2").read_text(encoding="utf-8"),
            (SRC / "templates" / "redirect.html.j2").read_text(encoding="utf-8"),
            (SRC / "templates" / "404.html.j2").read_text(encoding="utf-8"),
        ]
    )
    asset_inputs = [config["site"].get("favicon"), (config["theme"].get("background") or {}).get("image")]
    for group in config["groups"]:
        asset_inputs.append(group.get("icon"))
        for link in group["links"]:
            asset_inputs.append(link.get("icon"))
    for icon in config["pwa"].get("icons") or []:
        if isinstance(icon, dict):
            asset_inputs.append(icon.get("src"))
    asset_inputs.extend(favicon_map.values())
    for asset in asset_inputs:
        if not asset or str(asset).startswith(("http://", "https://")):
            continue
        path = ROOT / str(asset)
        if path.is_file():
            source_bundle += "\n" + hashlib.sha256(path.read_bytes()).hexdigest()
    build_hash = hashlib.sha256(source_bundle.encode("utf-8")).hexdigest()
    asset_version = build_hash[:12]
    assets["styles_url"] = assets["styles_url"] + "?v=" + asset_version
    assets["app_js_url"] = assets["app_js_url"] + "?v=" + asset_version

    runtime_config = build_runtime_config(config)
    config_json = json.dumps(runtime_config, ensure_ascii=False, separators=(",", ":"), sort_keys=True).replace("</", "<\\/")
    (out / "config.resolved.json").write_text(
        json.dumps(runtime_config, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    server_info = dict(config["server"])
    server_info["search"] = {
        "suggest_url": config["search"].get("suggest_url") or "",
        "suggest_limit": config["search"].get("suggest_limit", 6),
    }
    server_info["probe"] = {
        "internet": config["probe"]["internet"]["urls"],
        "lan": [
            {
                "slug": link["slug"],
                "url": link.get("probe_url") or link.get("lan_url"),
                "expect": link.get("probe_expect") or "",
            }
            for group in config["groups"]
            for link in group["links"]
            if link.get("lan_url") and link.get("probe") is not False
        ],
        "timeout_ms": config["probe"]["lan"].get("timeout_ms", 2500),
        "accept_untrusted_cert": config["probe"]["lan"].get("accept_untrusted_cert", True),
        "pin_certificates": config["probe"]["lan"].get("pin_certificates", False),
        "pins": config["probe"]["lan"].get("pins", {}),
        "captive_portal": config["probe"].get("captive_portal", {}),
    }
    (out / "server.json").write_text(
        json.dumps(server_info, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    group_views = make_group_views(config, single_file)

    env = Environment(
        loader=FileSystemLoader(str(SRC / "templates")),
        autoescape=select_autoescape(["html", "xml"]),
        trim_blocks=True,
        lstrip_blocks=True,
    )
    index_template = env.get_template("index.html.j2")
    redirect_template = env.get_template("redirect.html.j2")
    not_found_template = env.get_template("404.html.j2")

    # Multi-file site.
    site_csp = csp_meta(config, None, None, config_json)
    site_html = index_template.render(
        site=config["site"],
        theme=config["theme"],
        layout=config["layout"],
        search=config["search"],
        groups=group_views,
        base_path=config["build"]["base_path"],
        config_json=config_json,
        csp_meta=site_csp,
        single_file=False,
        inline_css="",
        inline_js="",
        styles_url=assets["styles_url"],
        app_js_url=assets["app_js_url"],
        manifest_url=assets["manifest_url"],
        favicon_url=assets["favicon_url"],
        noindex=bool(config["site"].get("noindex", True)),
        config_hash=build_hash,
        has_pwa=bool(config["pwa"].get("enabled")),
    )
    (site_dir / "index.html").write_text(site_html, encoding="utf-8")
    (site_dir / "styles.css").write_text(full_css, encoding="utf-8")
    (site_dir / "app.js").write_text(full_js, encoding="utf-8")
    write_manifest(config, site_dir, assets)
    write_service_worker(config, site_dir, build_hash)
    write_redirect_pages(config, site_dir, redirect_template, assets, build_hash, full_css)
    (site_dir / "404.html").write_text(
        not_found_template.render(site=config["site"], base_path=config["build"]["base_path"]),
        encoding="utf-8",
    )
    robots = "User-agent: *\nDisallow: /\n" if config["site"].get("noindex", True) else "User-agent: *\nAllow: /\n"
    (site_dir / "robots.txt").write_text(robots, encoding="utf-8")
    write_deployment_helpers(config, site_dir, config_json)

    # Single-file build.
    if single_file:
        single_csp = csp_meta(config, full_css, full_js, config_json)
        single_html = index_template.render(
            site=config["site"],
            theme=config["theme"],
            layout=config["layout"],
            search=config["search"],
            groups=group_views,
            base_path=config["build"]["base_path"],
            config_json=config_json,
            csp_meta=single_csp,
            single_file=True,
            inline_css=full_css,
            inline_js=full_js,
            styles_url="",
            app_js_url="",
            manifest_url="",
            favicon_url=assets["favicon_url"],
                noindex=bool(config["site"].get("noindex", True)),
            config_hash=build_hash,
            has_pwa=bool(config["pwa"].get("enabled")),
        )
        (out / "landing.html").write_text(single_html, encoding="utf-8")

    build_info = {
        "version": version_from_hash(build_hash),
        "config_hash": hash_value,
        "build_hash": build_hash,
        "base_url": config["build"]["base_url"],
        "base_path": config["build"]["base_path"],
        "artifacts": sorted(
            str(path.relative_to(out)).replace("\\", "/")
            for path in out.rglob("*")
            if path.is_file() and path.name not in ("build-info.json", "checksums.txt")
        ),
    }
    (out / "build-info.json").write_text(
        json.dumps(build_info, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    write_checksums(out)

    if not quiet:
        for warning in warnings:
            print(f"warning: {warning}", file=sys.stderr)
        print(f"Built {out}")
        print(f"  site:       {site_dir.relative_to(ROOT) if site_dir.is_relative_to(ROOT) else site_dir}")
        if single_file:
            print(f"  single:     {(out / 'landing.html').relative_to(ROOT) if (out / 'landing.html').is_relative_to(ROOT) else out / 'landing.html'}")
    return {
        "config": config,
        "hash": build_hash,
        "config_hash": hash_value,
        "build_hash": build_hash,
        "warnings": warnings,
        "out": out,
    }


def hash_tree(root: Path) -> dict[str, str]:
    result = {}
    for path in sorted(root.rglob("*")):
        if path.is_file():
            result[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


def check_idempotent(config_path: Path) -> bool:
    with tempfile.TemporaryDirectory() as first, tempfile.TemporaryDirectory() as second:
        build(config_path, Path(first), single_file=True, quiet=True)
        build(config_path, Path(second), single_file=True, quiet=True)
        first_hash = hash_tree(Path(first))
        second_hash = hash_tree(Path(second))
    if first_hash != second_hash:
        only_first = sorted(set(first_hash) - set(second_hash))
        only_second = sorted(set(second_hash) - set(first_hash))
        changed = sorted(key for key in first_hash.keys() & second_hash.keys() if first_hash[key] != second_hash[key])
        print("Build is not idempotent.", file=sys.stderr)
        if only_first:
            print("  only in first: " + ", ".join(only_first), file=sys.stderr)
        if only_second:
            print("  only in second: " + ", ".join(only_second), file=sys.stderr)
        if changed:
            print("  changed: " + ", ".join(changed), file=sys.stderr)
        return False
    print("Build is idempotent.")
    return True


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Build Landing")
    parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    parser.add_argument("--out", type=Path, default=None)
    parser.add_argument("--no-single-file", action="store_true")
    parser.add_argument("--check-idempotent", action="store_true")
    parser.add_argument("--refresh-favicons", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.check_idempotent:
            return 0 if check_idempotent(args.config) else 1
        build(args.config, args.out, single_file=not args.no_single_file, refresh_favicons=args.refresh_favicons)
        return 0
    except ConfigError as exc:
        print(f"config error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
