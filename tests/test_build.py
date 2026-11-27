import copy
import hashlib
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yaml

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

import build as landing_build  # noqa: E402


SAMPLE = ROOT / "config" / "landing.example.yml"

# Tests must be independent of the caller's CI/deployment environment.
for _key in (
    "LANDING_BASE_URL",
    "LANDING_BASE_PATH",
):
    os.environ.pop(_key, None)


class BuildTests(unittest.TestCase):
    def setUp(self):
        # Keep tests offline and deterministic: favicon fetching is covered separately.
        self._favicon_patch = patch.object(landing_build, "resolve_favicons", return_value={})
        self._favicon_patch.start()
        self.addCleanup(self._favicon_patch.stop)

    def test_sample_build_outputs(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            result = landing_build.build(SAMPLE, out, quiet=True)
            sample = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
            self.assertEqual(result["config"]["site"]["title"], sample["site"]["title"])
            for path in (
                out / "site" / "index.html",
                out / "site" / "styles.css",
                out / "site" / "app.js",
                out / "site" / "sw.js",
                out / "site" / "manifest.webmanifest",
                out / "site" / "404.html",
                out / "site" / "robots.txt",
                out / "site" / "_headers",
                out / "site" / "Caddyfile.example",
                out / "site" / "nginx.conf.example",
                out / "site" / "go" / "ha" / "index.html",
                out / "landing.html",
                out / "config.resolved.json",
                out / "server.json",
                out / "build-info.json",
                out / "checksums.txt",
            ):
                self.assertTrue(path.exists(), f"missing {path}")

            index = (out / "site" / "index.html").read_text(encoding="utf-8")
            self.assertIn('data-link-id="ha"', index)
            self.assertIn("https://ha.example.com/", index)
            self.assertIn("http://ha.lan/", index)
            self.assertIn('data-group-id="home"', index)
            server = json.loads((out / "server.json").read_text(encoding="utf-8"))
            self.assertEqual(server["host"], "127.0.0.1")
            self.assertEqual(server["port"], 8080)
            self.assertEqual(server["health_path"], "/healthz")
            headers = (out / "site" / "_headers").read_text(encoding="utf-8")
            self.assertIn("Content-Security-Policy:", headers)
            self.assertIn("Permissions-Policy:", headers)

            manifest = json.loads((out / "site" / "manifest.webmanifest").read_text(encoding="utf-8"))
            self.assertEqual(manifest["start_url"], "/")
            self.assertGreaterEqual(len(manifest["icons"]), 3)

    def test_single_file_is_self_contained(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            landing_build.build(SAMPLE, out, quiet=True)
            single = (out / "landing.html").read_text(encoding="utf-8")
            self.assertIn("<style>", single)
            self.assertIn("<script>", single)
            self.assertNotIn('<link rel="stylesheet"', single)
            self.assertNotIn("<script src=", single)
            self.assertNotIn('rel="manifest"', single)
            self.assertNotIn("/styles.css", single)
            self.assertNotIn("/app.js", single)
            self.assertIn("https://example.org/", single)
            self.assertIn("http://ha.lan/", single)

    def test_idempotent(self):
        with tempfile.TemporaryDirectory() as first, tempfile.TemporaryDirectory() as second:
            landing_build.build(SAMPLE, Path(first), quiet=True)
            landing_build.build(SAMPLE, Path(second), quiet=True)
            first_hash = landing_build.hash_tree(Path(first))
            second_hash = landing_build.hash_tree(Path(second))
            self.assertEqual(first_hash, second_hash)

    def test_duplicate_slug_rejected(self):
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        raw = copy.deepcopy(raw)
        raw["groups"][0]["links"].append({"id": "dup", "name": "Dup", "url": "https://example.org/", "slug": "ha"})
        config = landing_build.normalize_config(raw)
        with self.assertRaises(landing_build.ConfigError):
            landing_build.validate_config(config)

    def test_missing_urls_rejected(self):
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        raw = copy.deepcopy(raw)
        link = raw["groups"][0]["links"][0]
        link.pop("url", None)
        link.pop("lan_url", None)
        config = landing_build.normalize_config(raw)
        with self.assertRaises(landing_build.ConfigError):
            landing_build.validate_config(config)

    def test_bad_scheme_rejected(self):
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        raw = copy.deepcopy(raw)
        raw["groups"][0]["links"][0]["url"] = "javascript:alert(1)"
        config = landing_build.normalize_config(raw)
        with self.assertRaises(landing_build.ConfigError):
            landing_build.validate_config(config)

    def test_random_subdomain_requires_domain(self):
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        raw = copy.deepcopy(raw)
        raw["probe"]["lan"]["random_subdomain"] = True
        raw["probe"]["lan"]["domain"] = None
        config = landing_build.normalize_config(raw)
        with self.assertRaises(landing_build.ConfigError):
            landing_build.validate_config(config)

    def test_config_hash_stable(self):
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        config = landing_build.normalize_config(raw)
        first = landing_build.config_hash(config)
        second = landing_build.config_hash(landing_build.normalize_config(raw))
        self.assertEqual(first, second)
        self.assertEqual(len(first), 64)

    def test_favicon_discovery_and_validation(self):
        page = '<html><head><link rel="icon" href="/static/icons/favicon.ico"><link rel="apple-touch-icon" href="/apple.png"></head></html>'
        urls = landing_build.discover_icon_urls("http://ha.lan/", page)
        self.assertEqual(urls[0], "http://ha.lan/static/icons/favicon.ico")
        self.assertIn("http://ha.lan/apple.png", urls)
        self.assertIsNone(landing_build.normalize_icon(b"<html>not an image</html>", "text/html", "http://x/favicon.ico"))
        self.assertIsNone(landing_build.normalize_icon(b'<svg onload="alert(1)"></svg>', "image/svg+xml", "http://x/i.svg"))

    def test_favicon_private_host_policy(self):
        link = {"slug": "ha", "url": "http://ha.lan/", "lan_url": "http://ha.lan/"}
        result, warning = landing_build.fetch_favicon(link, {"allow_private_hosts": False})
        self.assertIsNone(result)
        self.assertIsNone(warning)
        self.assertTrue(landing_build.is_private_host("192.168.1.1"))
        self.assertTrue(landing_build.is_private_host("ha.lan"))
        self.assertFalse(landing_build.is_private_host("example.org"))

    def test_server_json_includes_probe_targets(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            landing_build.build(SAMPLE, out, quiet=True)
            server = json.loads((out / "server.json").read_text(encoding="utf-8"))
            self.assertTrue(server["probe"])
            self.assertIn("http://ha.lan/", [item["url"] for item in server["probe"]["lan"]])
            self.assertIn("https://example.org/", server["probe"]["internet"])

    def test_redirect_page_uses_service_icon(self):
        icon = ROOT / "assets" / "favicons" / "ha.png"
        if not icon.is_file():
            self.skipTest("no cached ha favicon")
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        raw["groups"][0]["links"][0]["icon"] = "auto"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "landing.yml"
            path.write_text(yaml.safe_dump(raw), encoding="utf-8")
            out = Path(tmp) / "out"
            with patch.object(landing_build, "resolve_favicons", return_value={"ha": "assets/favicons/ha.png"}):
                landing_build.build(path, out, quiet=True)
            page = (out / "site" / "go" / "ha" / "index.html").read_text(encoding="utf-8")
            self.assertIn('<link rel="icon"', page)
            self.assertIn("data:image/png;base64,", page)

    def test_search_engine_requires_query_placeholder(self):
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        raw["search"]["engine"] = "https://example.org/search"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "landing.yml"
            path.write_text(yaml.safe_dump(raw), encoding="utf-8")
            with self.assertRaises(landing_build.ConfigError):
                landing_build.build(path, Path(tmp) / "out", quiet=True)

    def test_search_engine_rendered(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            landing_build.build(SAMPLE, out, quiet=True)
            html = (out / "site" / "index.html").read_text(encoding="utf-8")
            self.assertIn('id="landing-search-form"', html)
            self.assertIn("autofocus", html)
            self.assertIn("DuckDuckGo", html)

    def test_suggest_url_requires_query_placeholder(self):
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        raw["search"]["suggest_url"] = "https://example.org/suggest"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "landing.yml"
            path.write_text(yaml.safe_dump(raw), encoding="utf-8")
            with self.assertRaises(landing_build.ConfigError):
                landing_build.build(path, Path(tmp) / "out", quiet=True)

    def test_server_json_includes_search_suggestions(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            landing_build.build(SAMPLE, out, quiet=True)
            server = json.loads((out / "server.json").read_text(encoding="utf-8"))
            self.assertIn("search", server)
            self.assertIn("suggest_url", server["search"])

    def test_suggestion_parser_handles_opensearch(self):
        import serve
        payload = b'["goog", ["google", "google maps", "google docs"]]'
        with patch.object(serve.urllib.request, "urlopen") as urlopen:
            urlopen.return_value.__enter__.return_value.read.return_value = payload
            items = serve.fetch_suggestions(
                {"search": {"suggest_url": "https://example.org/?q={query}", "suggest_limit": 5}},
                "goog",
            )
        self.assertEqual(items, ["google", "google maps", "google docs"])

    def test_cards_link_to_go_by_default(self):
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        raw["probe"]["homepage"] = False
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "landing.yml"
            path.write_text(yaml.safe_dump(raw), encoding="utf-8")
            out = Path(tmp) / "out"
            landing_build.build(path, out, quiet=True)
            html = (out / "site" / "index.html").read_text(encoding="utf-8")
            self.assertIn('href="/go/ha/"', html)
            self.assertNotIn("or filter services", html)

    def test_homepage_probe_restores_direct_links(self):
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        raw["probe"]["homepage"] = True
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "landing.yml"
            path.write_text(yaml.safe_dump(raw), encoding="utf-8")
            out = Path(tmp) / "out"
            landing_build.build(path, out, quiet=True)
            html = (out / "site" / "index.html").read_text(encoding="utf-8")
            self.assertIn('href="https://ha.example.com/"', html)

    def test_icon_ladder_prefers_favicon_then_custom(self):
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        raw["groups"][0]["links"][0]["icon"] = "router"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "landing.yml"
            path.write_text(yaml.safe_dump(raw), encoding="utf-8")
            out = Path(tmp) / "out"
            with patch.object(landing_build, "resolve_favicons", return_value={"ha": "assets/favicons/ha.png"}):
                landing_build.build(path, out, quiet=True)
            config = json.loads((out / "config.resolved.json").read_text(encoding="utf-8"))
            ha = config["groups"][0]["links"][0]
            self.assertTrue(ha["icon_url"], "fetched favicon should win over a custom icon")

    def test_icon_ladder_falls_back_to_custom(self):
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        raw["groups"][0]["links"][0]["icon"] = "router"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "landing.yml"
            path.write_text(yaml.safe_dump(raw), encoding="utf-8")
            out = Path(tmp) / "out"
            with patch.object(landing_build, "resolve_favicons", return_value={}):
                landing_build.build(path, out, quiet=True)
            config = json.loads((out / "config.resolved.json").read_text(encoding="utf-8"))
            ha = config["groups"][0]["links"][0]
            self.assertIsNone(ha["icon_url"])
            self.assertEqual(ha["icon"], "router")

    def test_icon_background_rendered(self):
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        raw["groups"][0]["links"][0]["icon_background"] = "#f3e3d3"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "landing.yml"
            path.write_text(yaml.safe_dump(raw), encoding="utf-8")
            out = Path(tmp) / "out"
            landing_build.build(path, out, quiet=True)
            css = (out / "site" / "styles.css").read_text(encoding="utf-8")
            self.assertIn("background:#f3e3d3", css)

    def test_icon_background_must_be_hex(self):
        raw = yaml.safe_load(SAMPLE.read_text(encoding="utf-8"))
        raw["groups"][0]["links"][0]["icon_background"] = "red"
        config = landing_build.normalize_config(raw)
        with self.assertRaises(landing_build.ConfigError):
            landing_build.validate_config(config)

    def test_captive_portal_detection(self):
        import serve
        # A portal returns 200 HTML instead of the expected 204.
        class FakeResponse:
            status = 200
            def read(self, n=-1): return b"<html>Sign in</html>"
            def __enter__(self): return self
            def __exit__(self, *a): return False
        with patch.object(serve.urllib.request, "urlopen", return_value=FakeResponse()):
            captive = serve.detect_captive_portal(
                {"captive_portal": {"enabled": True, "urls": [
                    {"url": "http://x/generate_204", "expect_status": 204}
                ]}}, 1.0)
        self.assertTrue(captive)

    def test_captive_portal_clean(self):
        import serve
        class FakeResponse:
            status = 204
            def read(self, n=-1): return b""
            def __enter__(self): return self
            def __exit__(self, *a): return False
        with patch.object(serve.urllib.request, "urlopen", return_value=FakeResponse()):
            captive = serve.detect_captive_portal(
                {"captive_portal": {"enabled": True, "urls": [
                    {"url": "http://x/generate_204", "expect_status": 204}
                ]}}, 1.0)
        self.assertFalse(captive)

    def test_probe_expect_marker(self):
        import serve
        class FakeResponse:
            status = 200
            def read(self, n=-1): return b"welcome to my homelab"
            def __enter__(self): return self
            def __exit__(self, *a): return False
        with patch.object(serve.urllib.request, "urlopen", return_value=FakeResponse()):
            self.assertTrue(serve.probe_url("http://x/", 1.0, True, "homelab"))
            self.assertFalse(serve.probe_url("http://x/", 1.0, True, "not-there"))

    def test_checksums_cover_artifacts(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            landing_build.build(SAMPLE, out, quiet=True)
            lines = (out / "checksums.txt").read_text(encoding="utf-8").splitlines()
            names = {line.split("  ", 1)[1] for line in lines}
            self.assertIn("site/index.html", names)
            self.assertIn("landing.html", names)
            for line in lines:
                digest, name = line.split("  ", 1)
                actual = hashlib.sha256((out / name).read_bytes()).hexdigest()
                self.assertEqual(digest, actual, name)


if __name__ == "__main__":
    unittest.main()
