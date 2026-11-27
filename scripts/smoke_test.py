#!/usr/bin/env python3
"""Smoke-test the built Landing site and single file in Chromium.

Usage:
    python3 scripts/build.py
    python3 scripts/smoke_test.py

Requires Chromium and websocket-client.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread
from typing import Any

try:
    import websocket  # type: ignore
except ImportError:
    raise SystemExit("Missing dependency: pip install websocket-client")

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
CHROME = shutil.which("chromium") or shutil.which("chromium-browser")
if not CHROME:
    raise SystemExit("Chromium not found on PATH")


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, fmt: str, *args: Any) -> None:
        return


class CDP:
    def __init__(self, ws_url: str):
        self.ws = websocket.create_connection(ws_url, timeout=20)
        self._id = 0
        self.events: list[dict[str, Any]] = []

    def cmd(self, method: str, params: dict[str, Any] | None = None, timeout: float = 30) -> dict[str, Any]:
        self._id += 1
        request_id = self._id
        self.ws.send(json.dumps({"id": request_id, "method": method, "params": params or {}}))
        deadline = time.time() + timeout
        while time.time() < deadline:
            message = json.loads(self.ws.recv())
            if message.get("id") == request_id:
                if "error" in message:
                    raise RuntimeError(f"{method}: {message['error']}")
                return message
            self.events.append(message)
        raise TimeoutError(method)

    def evaluate(self, expression: str, await_promise: bool = True, timeout: float = 30) -> Any:
        result = self.cmd(
            "Runtime.evaluate",
            {"expression": expression, "awaitPromise": await_promise, "returnByValue": True, "userGesture": True},
            timeout=timeout,
        )
        payload = result.get("result", {})
        if "exceptionDetails" in payload:
            details = payload["exceptionDetails"]
            raise RuntimeError(details.get("exception", {}).get("description") or details.get("text"))
        return payload.get("result", {}).get("value")

    def close(self) -> None:
        try:
            self.cmd("Browser.close", timeout=5)
        except Exception:
            pass
        try:
            self.ws.close()
        except Exception:
            pass


class Browser:
    def __init__(self, url: str):
        tmp_root = ROOT / "research" / ".tmp"
        tmp_root.mkdir(parents=True, exist_ok=True)
        self.profile = Path(tempfile.mkdtemp(prefix="landing-smoke-", dir=str(tmp_root)))
        args = [
            CHROME,
            "--headless=new",
            "--no-sandbox",
            "--disable-gpu",
            "--disable-dev-shm-usage",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-networking",
            "--remote-debugging-port=0",
            "--remote-allow-origins=*",
            f"--user-data-dir={self.profile}",
            url,
        ]
        self.process = subprocess.Popen(
            args,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
        port_file = self.profile / "DevToolsActivePort"
        deadline = time.time() + 20
        while time.time() < deadline and not port_file.exists():
            if self.process.poll() is not None:
                raise RuntimeError("Chromium exited early")
            time.sleep(0.05)
        self.port = int(port_file.read_text().splitlines()[0])
        targets = []
        deadline = time.time() + 15
        while time.time() < deadline:
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{self.port}/json/list", timeout=1) as response:
                    targets = json.load(response)
            except Exception:
                pass
            pages = [target for target in targets if target.get("type") == "page"]
            if pages:
                break
            time.sleep(0.05)
        else:
            raise TimeoutError("Chromium did not expose a page")
        self.cdp = CDP(pages[0]["webSocketDebuggerUrl"])
        self.cdp.cmd("Runtime.enable")
        self.cdp.cmd("Page.enable")

    def wait_for(self, expression: str, timeout: float = 12) -> None:
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                if self.cdp.evaluate(f"Boolean({expression})", await_promise=False):
                    return
            except Exception:
                pass
            time.sleep(0.1)
        raise TimeoutError(f"Condition not met: {expression}")

    def close(self) -> None:
        self.cdp.close()
        try:
            self.process.wait(timeout=5)
        except Exception:
            pass
        try:
            os.killpg(os.getpgid(self.process.pid), signal.SIGTERM)
        except Exception:
            pass
        try:
            self.process.wait(timeout=2)
        except Exception:
            try:
                os.killpg(os.getpgid(self.process.pid), signal.SIGKILL)
            except Exception:
                pass
        shutil.rmtree(self.profile, ignore_errors=True)

    def __enter__(self) -> "Browser":
        return self

    def __exit__(self, *_: Any) -> None:
        self.close()


def check(name: str, condition: bool, detail: Any = "") -> None:
    status = "PASS" if condition else "FAIL"
    print(f"{status} {name}" + (f" :: {detail}" if detail and not condition else ""))
    if not condition:
        raise AssertionError(name)


def scenario(url: str, checks: list[tuple[str, str, Any]]) -> None:
    with Browser(url) as browser:
        browser.wait_for("window.Landing && !window.Landing.getState().checking", timeout=15)
        for name, expression, expected in checks:
            actual = browser.cdp.evaluate(expression, await_promise=False)
            check(name, actual == expected, f"expected {expected!r}, got {actual!r}")
        exceptions = [
            event.get("params", {}).get("exceptionDetails", {}).get("text")
            for event in browser.cdp.events
            if event.get("method") == "Runtime.exceptionThrown"
        ]
        check("no runtime exceptions", not exceptions, exceptions)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dist", type=Path, default=DIST)
    args = parser.parse_args()
    site = args.dist / "site"
    if not (site / "index.html").exists():
        raise SystemExit("Build output missing; run python3 scripts/build.py first")

    server = ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=str(site)))
    port = server.server_address[1]
    Thread(target=server.serve_forever, daemon=True).start()
    base = f"http://127.0.0.1:{port}"
    single = (args.dist / "landing.html").as_uri()
    try:
        # Homepage: renders groups and shows the network pill.
        with Browser(base + "/") as browser:
            browser.wait_for("window.Landing")
            check(
                "site: renders groups",
                browser.cdp.evaluate("document.querySelectorAll('.group').length", await_promise=False) >= 1,
            )
            check(
                "site: both pills visible",
                browser.cdp.evaluate("!document.getElementById('location-button').closest('.network-wrap').hidden && !document.getElementById('connectivity-button').closest('.network-wrap').hidden", await_promise=False),
            )

        with Browser(base + "/") as browser:
            browser.wait_for("window.Landing")
            check(
                "ui: search is focused",
                browser.cdp.evaluate("document.activeElement && document.activeElement.id", await_promise=False) == "landing-search",
            )
            check(
                "ui: search does not filter by default",
                browser.cdp.evaluate("var i=document.getElementById('landing-search'); i.value='jelly'; i.dispatchEvent(new Event('input')); Array.from(document.querySelectorAll('.card')).filter(c=>!c.hidden).length === document.querySelectorAll('.card').length", await_promise=False),
            )
            browser.cdp.evaluate("var i=document.getElementById('landing-search'); i.value=''; i.dispatchEvent(new Event('input'))", await_promise=False)
            check(
                "ui: help button opens help",
                browser.cdp.evaluate("document.getElementById('help-button').click(); document.getElementById('help-dialog').open", await_promise=False),
            )
            browser.cdp.evaluate("document.getElementById('help-close').click()", await_promise=False)
            check(
                "ui: no header or footer",
                browser.cdp.evaluate("!document.querySelector('.site-header') && !document.querySelector('.site-footer')", await_promise=False),
            )
            check(
                "ui: cards resolve to a URL",
                (browser.cdp.evaluate("document.querySelector('[data-link-id=ha]').getAttribute('href')", await_promise=False) or "").startswith(("http", "/go/")),
            )
            check(
                "ui: drag payload is the /go/ link",
                "/go/ha/" in browser.cdp.evaluate(
                    "(function(){var c=document.querySelector('[data-link-id=ha]');var dt=new DataTransfer();"
                    "c.dispatchEvent(new DragEvent('dragstart',{bubbles:true,cancelable:true,dataTransfer:dt}));"
                    "return dt.getData('text/uri-list');})()",
                    await_promise=False,
                ),
            )
            check(
                "ui: drag sets text/x-moz-url for Firefox Places",
                "/go/ha/" in browser.cdp.evaluate(
                    "(function(){var c=document.querySelector('[data-link-id=ha]');var dt=new DataTransfer();"
                    "c.dispatchEvent(new DragEvent('dragstart',{bubbles:true,cancelable:true,dataTransfer:dt}));"
                    "return dt.getData('text/x-moz-url');})()",
                    await_promise=False,
                ),
            )

        # Minimal redirect page: never loads the landing UI.
        with Browser(base + "/go/ha/?mode=wan&delay=5000") as browser:
            browser.wait_for("document.getElementById('redirect-status') && document.getElementById('redirect-status').textContent.indexOf('Home Assistant') !== -1", timeout=15)
            check("redirect: no landing UI", browser.cdp.evaluate("!document.querySelector('.site-header') && !document.getElementById('landing-groups')", await_promise=False))
            check("redirect: status names service", "Home Assistant" in browser.cdp.evaluate("document.getElementById('redirect-status').textContent", await_promise=False))
            # The static smoke server has no /api, so the check resolves to the
            # failure panel -- still with no landing UI.
            check("redirect: boot or failure visible", browser.cdp.evaluate("document.getElementById('redirect-boot').getClientRects().length > 0 || !document.getElementById('redirect-fail').hidden", await_promise=False))

        # Unavailable target explains why and links back to the landing root.
        with Browser(base + "/go/jaeger/?mode=wan") as browser:
            browser.wait_for("!document.getElementById('redirect-fail').hidden", timeout=15)
            check("redirect: failure explains", "Home LAN only" in browser.cdp.evaluate("document.getElementById('redirect-fail-reason').textContent", await_promise=False))
            check("redirect: failure links home", browser.cdp.evaluate("document.querySelector('#redirect-fail a').getAttribute('href')", await_promise=False) == "/")
            check("redirect: failure has no landing UI", browser.cdp.evaluate("!document.querySelector('.site-header')", await_promise=False))

        # Single-file export still renders and links to /go/.
        with Browser(single) as browser:
            browser.wait_for("window.Landing")
            check(
                "single: renders groups",
                browser.cdp.evaluate("document.querySelectorAll('.group').length", await_promise=False) >= 1,
            )
            check(
                "single: cards resolve to a URL",
                (browser.cdp.evaluate("document.querySelector('[data-link-id=ha]').getAttribute('href')", await_promise=False) or "").startswith(("http", "/go/")),
            )
    finally:
        server.shutdown()
        server.server_close()
    print("Smoke tests passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
