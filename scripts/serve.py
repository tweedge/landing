#!/usr/bin/env python3
"""Serve the installed Landing site on loopback with a healthcheck.

Usage:
    python3 serve.py --root /path/to/site --config /path/to/server.json
"""

from __future__ import annotations

import argparse
import json
import mimetypes
import hashlib
import os
import socket
import ssl
import sys
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import quote, unquote, urlsplit

DEFAULT_HEADERS = {
    "X-Content-Type-Options": "nosniff",
    "Referrer-Policy": "no-referrer",
    "Permissions-Policy": "camera=(), microphone=(), geolocation=(), payment=(), usb=()",
}


def load_headers(root: Path) -> dict[str, str]:
    headers = dict(DEFAULT_HEADERS)
    header_file = root / "_headers"
    if not header_file.is_file():
        return headers
    for line in header_file.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or stripped.startswith("/"):
            continue
        if ":" not in stripped:
            continue
        name, value = stripped.split(":", 1)
        name = name.strip()
        if name in ("Content-Security-Policy", "Permissions-Policy", "Referrer-Policy", "X-Content-Type-Options"):
            headers[name] = value.strip()
    return headers


def build_info(root: Path) -> dict[str, str]:
    info_file = root.parent / "build-info.json"
    if not info_file.is_file():
        return {}
    try:
        data = json.loads(info_file.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return {key: str(data.get(key, "")) for key in ("version", "build_hash", "config_hash")}


def _ssl_context(accept_untrusted: bool) -> ssl.SSLContext:
    context = ssl.create_default_context()
    if accept_untrusted:
        context.check_hostname = False
        context.verify_mode = ssl.CERT_NONE
    return context


def _cert_fingerprint(url: str, timeout: float) -> str | None:
    parsed = urlsplit(url)
    if parsed.scheme != "https" or not parsed.hostname:
        return None
    port = parsed.port or 443
    try:
        context = ssl.create_default_context()
        context.check_hostname = False
        context.verify_mode = ssl.CERT_NONE
        with socket.create_connection((parsed.hostname, port), timeout=timeout) as sock:
            with context.wrap_socket(sock, server_hostname=parsed.hostname) as tls:
                der = tls.getpeercert(binary_form=True)
        return hashlib.sha256(der).hexdigest() if der else None
    except (OSError, ssl.SSLError, ValueError):
        return None


def probe_url(url: str, timeout: float, accept_untrusted: bool = True, expect: str = "") -> bool:
    request = urllib.request.Request(url, headers={"User-Agent": "Landing-Probe/0.2"})
    context = _ssl_context(accept_untrusted)
    try:
        with urllib.request.urlopen(request, timeout=timeout, context=context) as response:
            if not (200 <= response.status < 500):
                return False
            if expect:
                body = response.read(65536).decode("utf-8", "replace")
                return expect in body
            return True
    except urllib.error.HTTPError as exc:
        return 200 <= exc.code < 500
    except (urllib.error.URLError, OSError, ValueError):
        return False


def detect_captive_portal(config: dict, timeout: float) -> bool:
    """True when a captive portal is intercepting traffic.

    Each configured check must return its exact expected result. A portal
    typically redirects or returns its own HTML, so any mismatch means we are
    behind a portal and must not claim the home LAN."""
    captive = config.get("captive_portal") or {}
    if not captive.get("enabled", True):
        return False
    checks = captive.get("urls") or []
    if not checks:
        return False
    context = _ssl_context(True)
    for check in checks:
        url = str(check.get("url") or "")
        if not url:
            continue
        request = urllib.request.Request(url, headers={"User-Agent": "Landing-Probe/0.2"})
        try:
            with urllib.request.urlopen(request, timeout=timeout, context=context) as response:
                status = response.status
                body = response.read(65536).decode("utf-8", "replace")
        except urllib.error.HTTPError as exc:
            status = exc.code
            body = ""
        except (urllib.error.URLError, OSError, ValueError):
            # Unreachable check is inconclusive, not proof of a portal.
            continue
        expect_status = check.get("expect_status")
        expect_body = check.get("expect_body")
        if expect_status is not None and status != int(expect_status):
            return True
        if expect_body and expect_body not in body:
            return True
    return False


def _probe_lan_entries(probe: dict, entries: list[dict], timeout: float) -> list[dict]:
    accept_untrusted = bool(probe.get("accept_untrusted_cert", True))
    pin_enabled = bool(probe.get("pin_certificates", False))
    pins = probe.get("pins") or {}
    with ThreadPoolExecutor(max_workers=8) as pool:
        futures = {
            entry["url"]: pool.submit(probe_url, entry["url"], timeout, accept_untrusted, entry.get("expect") or "")
            for entry in entries
        }
        results = [
            {"slug": entry.get("slug", ""), "url": url, "ok": future.result()}
            for entry, (url, future) in zip(entries, futures.items())
        ]
    if pin_enabled:
        for item in results:
            if not item["ok"]:
                continue
            expected = pins.get(item["url"])
            if not expected:
                continue
            if _cert_fingerprint(item["url"], timeout) != expected:
                item["ok"] = False
                item["pin_mismatch"] = True
    return results


def probe_location(config: dict, slug: str | None = None) -> dict:
    """Home/Away. Probes LAN URLs; with a slug, only that service's URL."""
    probe = config.get("probe") or {}
    timeout = float(probe.get("timeout_ms") or 2500) / 1000.0
    entries = [item for item in (probe.get("lan") or []) if isinstance(item, dict) and item.get("url")]
    if slug:
        entries = [item for item in entries if item.get("slug") == slug]
    captive = detect_captive_portal(probe.get("captive_portal") or {}, timeout)
    results = _probe_lan_entries(probe, entries, timeout)
    ok_count = sum(1 for item in results if item["ok"])
    if slug:
        home = bool(results) and results[0]["ok"]
    else:
        home = bool(results) and ok_count * 2 > len(results)
    if captive:
        home = False
    return {
        "home": home,
        "captivePortal": captive,
        "tried": results,
        "okCount": ok_count,
        "total": len(results),
    }


def probe_connectivity(config: dict) -> dict:
    """Online/Offline. Independent of the LAN probe."""
    probe = config.get("probe") or {}
    timeout = float(probe.get("timeout_ms") or 2500) / 1000.0
    accept_untrusted = bool(probe.get("accept_untrusted_cert", True))
    internet_urls = [str(url) for url in (probe.get("internet") or []) if url]
    captive = detect_captive_portal(probe.get("captive_portal") or {}, timeout)
    with ThreadPoolExecutor(max_workers=4) as pool:
        futures = [pool.submit(probe_url, url, timeout, accept_untrusted) for url in internet_urls]
        online = any(future.result() for future in futures) if futures else None
    if captive:
        online = False
    return {"online": online, "captivePortal": captive}


def probe_network(config: dict) -> dict:
    """Combined probe kept for compatibility."""
    location = probe_location(config)
    connectivity = probe_connectivity(config)
    return {
        "internet": connectivity["online"],
        "lan": location["home"],
        "lanTried": location["tried"],
        "captivePortal": location["captivePortal"] or connectivity["captivePortal"],
    }


def load_icon_map(root: Path) -> dict[str, str]:
    config_file = root.parent / "config.resolved.json"
    if not config_file.is_file():
        return {}
    try:
        config = json.loads(config_file.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    icons: dict[str, str] = {}
    for group in config.get("groups") or []:
        for link in group.get("links") or []:
            slug = link.get("slug")
            icon_url = link.get("icon_url")
            if slug and icon_url:
                icons[str(slug)] = str(icon_url)
    return icons


def fetch_suggestions(config: dict, query: str) -> list[str]:
    search = config.get("search") or {}
    template = str(search.get("suggest_url") or "")
    if not template or "{query}" not in template:
        return []
    limit = int(search.get("suggest_limit") or 6)
    url = template.replace("{query}", quote(query))
    request = urllib.request.Request(url, headers={"User-Agent": "Landing-Suggest/0.2"})
    try:
        with urllib.request.urlopen(request, timeout=3) as response:
            data = response.read(65536)
    except (urllib.error.URLError, OSError, ValueError):
        return []
    try:
        payload = json.loads(data.decode("utf-8", "replace"))
    except ValueError:
        return []
    items: list[str] = []
    if isinstance(payload, list):
        # OpenSearch shape: ["query", ["suggestion", ...], ...]
        if len(payload) >= 2 and isinstance(payload[1], list):
            items.extend(item for item in payload[1] if isinstance(item, str))
        else:
            for entry in payload:
                if isinstance(entry, str):
                    items.append(entry)
                elif isinstance(entry, list) and entry and isinstance(entry[0], str):
                    items.append(entry[0])
    elif isinstance(payload, dict):
        for key in ("suggestions", "results", "data"):
            value = payload.get(key)
            if isinstance(value, list):
                for entry in value:
                    if isinstance(entry, str):
                        items.append(entry)
                    elif isinstance(entry, dict):
                        label = entry.get("phrase") or entry.get("suggestion") or entry.get("query")
                        if isinstance(label, str):
                            items.append(label)
    seen: set[str] = set()
    unique: list[str] = []
    for item in items:
        key = item.strip().lower()
        if not key or key in seen:
            continue
        seen.add(key)
        unique.append(item.strip())
    return unique[:limit]


class LandingHandler(BaseHTTPRequestHandler):
    server_version = "Landing/0.2"
    root: Path
    health_path: str
    response_headers: dict[str, str]
    server_config: dict
    icon_map: dict[str, str]
    info: dict[str, str]

    def log_message(self, fmt: str, *args: object) -> None:
        sys.stderr.write("%s - %s\n" % (self.address_string(), fmt % args))

    def do_HEAD(self) -> None:
        self._respond(send_body=False)

    def do_GET(self) -> None:
        self._respond(send_body=True)

    def _respond(self, send_body: bool) -> None:
        path = unquote(urlsplit(self.path).path)
        if path == self.health_path:
            self._health(send_body)
            return
        if path == "/api/network":
            self._network(send_body)
            return
        if path == "/api/location":
            self._location(send_body)
            return
        if path == "/api/connectivity":
            self._connectivity(send_body)
            return
        if path == "/api/suggest":
            self._suggest(send_body)
            return
        target = self._resolve(path)
        if target is None:
            self._error(HTTPStatus.NOT_FOUND, send_body)
            return
        try:
            body = target.read_bytes()
        except OSError:
            self._error(HTTPStatus.INTERNAL_SERVER_ERROR, send_body)
            return
        content_type = mimetypes.guess_type(target.name)[0] or "application/octet-stream"
        if target.suffix == ".webmanifest":
            content_type = "application/manifest+json"
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        for name, value in self.response_headers.items():
            if name == "Content-Security-Policy" and path.startswith("/go/"):
                continue
            self.send_header(name, value)
        if target.name in ("index.html", "sw.js", "styles.css", "app.js", "manifest.webmanifest"):
            self.send_header("Cache-Control", "no-cache")
        else:
            self.send_header("Cache-Control", "public, max-age=86400")
        self.end_headers()
        if send_body:
            self.wfile.write(body)

    def _network(self, send_body: bool) -> None:
        try:
            payload = probe_network(self.server_config)
        except Exception as exc:  # noqa: BLE001
            payload = {"error": str(exc)}
        body = (json.dumps(payload, sort_keys=True) + "\n").encode("utf-8")
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        for name, value in self.response_headers.items():
            self.send_header(name, value)
        self.end_headers()
        if send_body:
            self.wfile.write(body)

    def _json(self, payload: dict, send_body: bool) -> None:
        body = (json.dumps(payload, sort_keys=True) + "\n").encode("utf-8")
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        for name, value in self.response_headers.items():
            self.send_header(name, value)
        self.end_headers()
        if send_body:
            self.wfile.write(body)

    def _location(self, send_body: bool) -> None:
        slug = ""
        for part in urlsplit(self.path).query.split("&"):
            if part.startswith("slug="):
                slug = unquote(part[5:]).strip()
                break
        try:
            payload = probe_location(self.server_config, slug or None)
        except Exception as exc:  # noqa: BLE001
            payload = {"error": str(exc)}
        self._json(payload, send_body)

    def _connectivity(self, send_body: bool) -> None:
        try:
            payload = probe_connectivity(self.server_config)
        except Exception as exc:  # noqa: BLE001
            payload = {"error": str(exc)}
        self._json(payload, send_body)

    def _suggest(self, send_body: bool) -> None:
        query = ""
        for part in urlsplit(self.path).query.split("&"):
            if part.startswith("q="):
                query = unquote(part[2:]).strip()
                break
        items = fetch_suggestions(self.server_config, query) if query else []
        body = (json.dumps({"suggestions": items}, sort_keys=True) + "\n").encode("utf-8")
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        for name, value in self.response_headers.items():
            self.send_header(name, value)
        self.end_headers()
        if send_body:
            self.wfile.write(body)

    def _resolve(self, path: str) -> Path | None:
        relative = path.lstrip("/")
        candidate = (self.root / relative).resolve()
        try:
            candidate.relative_to(self.root)
        except ValueError:
            return None
        if candidate.is_dir():
            candidate = candidate / "index.html"
        if candidate.is_file():
            return candidate
        return None

    def _health(self, send_body: bool) -> None:
        index = self.root / "index.html"
        healthy = index.is_file() and os.access(index, os.R_OK)
        payload = {
            "status": "ok" if healthy else "error",
            "site": self.info.get("version", ""),
            "build": self.info.get("build_hash", ""),
        }
        body = (json.dumps(payload, sort_keys=True) + "\n").encode("utf-8")
        self.send_response(HTTPStatus.OK if healthy else HTTPStatus.SERVICE_UNAVAILABLE)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        for name, value in self.response_headers.items():
            self.send_header(name, value)
        self.end_headers()
        if send_body:
            self.wfile.write(body)

    def _error(self, status: HTTPStatus, send_body: bool) -> None:
        page = self.root / "404.html"
        body = page.read_bytes() if status == HTTPStatus.NOT_FOUND and page.is_file() else b"Not found\n"
        self.send_response(status)
        self.send_header("Content-Type", "text/html; charset=utf-8" if page.is_file() else "text/plain")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        for name, value in self.response_headers.items():
            self.send_header(name, value)
        self.end_headers()
        if send_body:
            self.wfile.write(body)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Serve Landing locally")
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    args = parser.parse_args(argv)

    root = args.root.resolve()
    if not (root / "index.html").is_file():
        print(f"error: no index.html in {root}", file=sys.stderr)
        return 2
    try:
        config = json.loads(args.config.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        print(f"error: cannot read {args.config}: {exc}", file=sys.stderr)
        return 2

    host = str(config.get("host") or "127.0.0.1")
    port = int(config.get("port") or 8080)
    health_path = str(config.get("health_path") or "/healthz")

    LandingHandler.root = root
    LandingHandler.health_path = health_path
    LandingHandler.response_headers = load_headers(root)
    LandingHandler.info = build_info(root)
    LandingHandler.server_config = config
    LandingHandler.icon_map = load_icon_map(root)

    server = ThreadingHTTPServer((host, port), LandingHandler)
    print(f"Landing serving {root} at http://{host}:{port}/ (health {health_path})", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
