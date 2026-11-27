# AGENTS.md

Guidance for agents and contributors working on Landing. Read this before changing code, and follow it so changes stay consistent and verifiable.

## What this project is

Landing is a local-first homelab start page implemented as a single Rust binary. A user installs it (prebuilt release or `cargo build`), runs `landing init` to write `config/landing.yml`, edits it, and runs `landing` to build, install, and serve it on `localhost`. There is no cloud build, CDN, extension, or userscript.

Authoritative behavior lives in [`SPEC.md`](SPEC.md). If code and spec disagree, treat the spec as intent and either fix the code or update the spec deliberately — do not leave them inconsistent.

## Repository map

| Path | Purpose |
| --- | --- |
| `src/main.rs` | CLI entrypoint (clap): `build`, `install`, `start`, `stop`, `restart`, `status`, `health`, `logs`, `uninstall`, `init`, `check-idempotent`, `serve`. |
| `src/lib.rs` | Library root re-exporting the modules below; embeds the example config as `TEMPLATE_CONFIG`. |
| `src/config.rs` | Typed config model, normalization (serde defaults), and validation. |
| `src/build.rs` | Build engine: config -> assets -> rendered site, single-file export, redirect pages, manifest, service worker, CSP headers, checksums, idempotency check. |
| `src/serve.rs` | Local static server (tiny_http): loopback binding, security headers, `/healthz`, `/api/location`, `/api/connectivity`, `/api/network`, `/api/suggest` server-side proxies. |
| `src/lifecycle.rs` | XDG paths, install, systemd unit, foreground process fallback, healthcheck, status, logs, uninstall, `init`. |
| `src/favicon.rs` | Build-time favicon fetch (reqwest, self-signed accepted), HTML icon discovery, image normalization, cache. |
| `src/theme.rs` | Theme CSS generation and color helpers. |
| `src/csp.rs` | Content-Security-Policy construction and hashing. |
| `src/pwa.rs` | Generated cat-mark PWA icons. |
| `src/templates.rs` | minijinja rendering (Jinja2-compatible escaping) of embedded templates. |
| `src/icons.rs` | Built-in SVG icon set. |
| `src/js/core.js` | Pure helpers: slug normalization, state classification, link resolution, redirect parsing. |
| `src/js/site.js` | Landing page runtime: probes, link rewriting, search, dialogs, theme, redirect controller. |
| `src/js/redirect.js` | Minimal `/go/<slug>/` page runtime. Must not depend on `site.js`. Checks only its target. |
| `src/js/icons.js` | Built-in icon set (rendered client-side). |
| `src/js/sw.js` | Service worker template (placeholders filled at build). |
| `src/styles/main.css` | All styles, including the minimal redirect page. |
| `src/templates/` | Jinja2 templates: `index.html.j2`, `redirect.html.j2`, `404.html.j2`. |
| `config/landing.example.yml` | Tracked example/template config (also embedded in the binary). |
| `config/landing.yml` | Your personal config (gitignored; write it with `landing init`). |
| `tests/build_tests.rs` | Rust integration tests (`cargo test`). |
| `tests/smoke_test.py` | Chromium smoke tests for the built site, single-file export, and redirect pages. |
| `scripts/install.sh` | tldr-style release installer (checksum-verified). |
| `scripts/build-release.sh` | Cross-builds `landing_linux_<arch>.tar.gz` + `SHA256SUMS`. |
| `assets/favicons/` | Build-time favicon cache (local, gitignored; never commit). |

## Environment

- Rust (stable; edition 2021). Build with `cargo build` / `cargo test`.
- Smoke tests need Chromium on `PATH` and `websocket-client` (`python3 -m pip install websocket-client`).
- No Node.js is required. Do not add a JS build step; the frontend is embedded with `include_str!`.

## Development workflow

1. Read the relevant part of `SPEC.md` before changing behavior.
2. Make the smallest change that satisfies the spec.
3. Rebuild and run the tests (below).
4. If you change behavior, update `SPEC.md` and `README.md` in the same change.
5. Do not commit `dist/`, `target/`, or `assets/favicons/` (gitignored).

### Build

```bash
cargo build                             # compile the binary
cargo run -- build                      # build dist/site and dist/landing.html
cargo run -- build --refresh-favicons
cargo run -- check-idempotent
```

Build output:

- `dist/site/` — multi-file site served by the local server.
- `dist/landing.html` — optional single-file export.
- `dist/config.resolved.json`, `dist/server.json`, `dist/build-info.json`, `dist/checksums.txt`.

### Install and run locally

```bash
cargo run --           # build + install + start + healthcheck
cargo run -- init      # write config/landing.yml from the embedded example
cargo run -- status
cargo run -- health
cargo run -- logs
cargo run -- restart
cargo run -- stop
cargo run -- uninstall [--purge]
```

Installed paths (XDG): site in `$XDG_DATA_HOME/landing/site`, config in `$XDG_CONFIG_HOME/landing`, systemd user unit in `$XDG_CONFIG_HOME/systemd/user/landing.service`.

## Testing and validation

Run all of these before declaring work done.

### 1. Unit and build tests

```bash
cargo test
```

These must pass offline. They build from `config/landing.example.yml`; do not make tests depend on network access.

### 2. Idempotency

```bash
cargo run -- check-idempotent
```

Two builds from the same inputs must produce identical hashes.

### 3. Smoke tests (browser)

```bash
cargo run -- build
python3 tests/smoke_test.py
```

Covers: group rendering, card links, the `/go/` drag payload, help UI, the minimal `/go/` page (no landing UI, status text, failure explanation + link home), and the single-file export.

### 4. Visual and layout verification (required for any UI change)

Do not rely on "it looks fine" or on a single screenshot. Measure, and compare regions against each other.

**Capture screenshots in both themes and at multiple widths.** Use the smoke harness (`tests/smoke_test.py` exposes `Browser` and `CDP`) or Chromium headless:

```python
import sys; sys.path.insert(0, "scripts")
import smoke_test as st
with st.Browser("http://127.0.0.1:8080/") as b:
    b.cdp.cmd("Emulation.setDeviceMetricsOverride",
              {"width": 1920, "height": 1080, "deviceScaleFactor": 1, "mobile": False})
    b.wait_for("window.Landing", timeout=15)
    for theme in ("light", "dark"):
        b.cdp.evaluate(f"document.documentElement.dataset.theme='{theme}'", await_promise=False)
        shot = b.cdp.cmd("Page.captureScreenshot", {"format": "png"})
        open(f"/tmp/landing-{theme}.png", "wb").write(base64.b64decode(shot["result"]["data"]))
```

Test at least **390px (mobile), 780px, and 1920px**, in **light and dark**.

**Check for color bands and seams.** A common failure is a strip at the top (or any edge) that differs from the page background because a transparent canvas, margin collapse, or an overlay shows through. Sample pixels and compare, do not eyeball:

```python
from PIL import Image
im = Image.open("/tmp/landing-dark.png").convert("RGB")
w, h = im.size
top = im.getpixel((w // 2, 2))
mid = im.getpixel((w // 2, h // 2))
assert top == mid, f"top {top} != mid {mid}"
# Also scan the first 60 rows for any discontinuity:
prev = None
for y in range(60):
    px = im.getpixel((w // 2, y))
    if prev and sum(abs(px[i] - prev[i]) for i in range(3)) > 4:
        print("edge at y=", y, prev, "->", px)
    prev = px
```

**Check alignment numerically.** Compare bounding boxes, not impressions. For a centered element, its center must equal the viewport center; for a left-aligned element, its left edge must equal the container's left edge. For a row, the vertical centers of the icon, input, and button must match:

```python
b.cdp.evaluate("""(function(){
  function r(sel){var e=document.querySelector(sel);var b=e.getBoundingClientRect();
    return {l:b.left,r:b.right,t:b.top,b:b.bottom,cx:(b.left+b.right)/2,cy:(b.top+b.bottom)/2};}
  return JSON.stringify({form:r('.hero-search'),input:r('#landing-search'),
    icon:r('.hero-search .search-icon'),go:r('.hero-search-go')});
})()""", await_promise=False)
```

**Check the DOM for leftovers.** After removing a component, confirm no markup, CSS, or JS remains:

```bash
grep -rn "site-header\|site-footer\|bookmarks\|header-bg\|brand" src/ scripts/ config/
grep -o "site-header\|site-footer\|bookmarks" dist/site/styles.css dist/site/app.js dist/site/index.html
```

**Check computed styles, not just pixels.** Verify the actual background, overlay, and variables:

```python
b.cdp.evaluate("JSON.stringify({html:getComputedStyle(document.documentElement).backgroundColor,"
                "body:getComputedStyle(document.body).backgroundColor,"
                "after:getComputedStyle(document.body,'::after').backgroundColor})", await_promise=False)
```

**Functional checks:**

- `http://localhost:8080/` renders and links resolve for the current network.
- `http://localhost:8080/go/ha/` shows only the minimal "Opening …" page, then redirects.
- An unavailable target (e.g. `/go/router/?mode=wan`) explains why and links to `/`.
- No CSP errors in the browser console.
- The configured theme (`theme.mode`, default `dark`) applies; reduced-motion still works.
- Search is focused on load, suggestions appear, and Enter searches the provider.
- Floating controls do not overlap content at any tested width.

**When a user reports a visual issue, reproduce it at their viewport and theme first.** Do not dismiss it because a default-size screenshot looks fine. Compare the exact region they mention against a reference region and report the measured values.

### 5. Debugging a persistent visual bug efficiently

A recurring top-band bug took several rounds because each fix addressed a
plausible cause without first proving which layer produced the wrong pixels.
Follow this order so a bug like that is solved in one or two passes.

1. **Get the user's artifact, and measure it.** Ask for a screenshot if one is
   not provided. Sample the exact problem region and a known-good region, in the
   user's theme. Compare numeric RGB values, do not eyeball.
2. **Identify *which layer* owns the wrong color.** A color that is neutral gray
   or the browser's default is the **canvas** (`html`/viewport), not our
   `body`/`--bg`. Our warm palette colors are ours; a neutral color is the UA.
   This one distinction (canvas vs. our layer) usually names the bug.
3. **Measure geometry, not just color.** Print the bounding rects of `html`,
   `body`, the first child, and `document.scrollingElement`. If `body.top > 0`,
   a margin collapsed through it and the strip above is the canvas. Check
   `getComputedStyle` for `margin`, `padding`, `background`, and `zi`-index.
4. **Reproduce in a harness at the user's viewport and device pixel ratio**, not
   a default 780px window. Use `Emulation.setDeviceMetricsOverride` with their
   width/height and `deviceScaleFactor`.
5. **Rule out stale delivery before changing code.** Confirm what is actually
   served with `curl`, what is installed on disk, and what the service worker
   cached. Compare hashes. "I cleared cache" does not clear CacheStorage, and an
   extension can inject its own styles/scripts. If the served bytes are already
   correct, the bug is in the browser's environment, not the build.
6. **Prefer a fix that does not depend on the failing layer.** For a canvas that
   may not paint the way we expect, put the solid background on a fixed,
   viewport-covering layer (`body::before`) rather than relying on `html`. Test
   the fix with that layer forced off to prove independence.
7. **Fix the root cause too.** After the robust workaround, also remove the
   trigger (for example, replace a collapsing `margin-top` with `padding-top`),
   then re-measure at every tested width and theme.
8. **Re-read the user's reports literally.** Their words ("top 5% is a different
   color", "neutral gray") contain the diagnosis. Match their observation to a
   layer; do not substitute a nearby plausible theory.

Common trap: sampling only the interior (`mid` pixel) and declaring success. A
band at the edge only shows when you compare the **first few rows** against the
**middle** and against a **margin column** (`x = 3px`), not just the center line.

**Reproduce in the user's actual browser, not just Chromium.** When a user says
"it still happens in <browser>", run that browser headless against the served
page and screenshot it:

```bash
librewolf --headless --no-remote --profile /tmp/lw --screenshot /tmp/lw.png \
  --window-size=1920,1080 http://127.0.0.1:8080/
```

Then measure the screenshot. A fresh profile rules out cache and service
workers. This is how the GitHub icon bug was found: the tile was correct, but
`loading="lazy"` on the `<img>` left the icon blank in LibreWolf. Do not assume
a Chromium-only result generalizes.

### 6. Verifying native browser behavior (drag, clipboard, bookmarks)

Synthetic `DragEvent`s are not proof. `new DragEvent(...)` with a fresh
`DataTransfer` skips the browser's own drag machinery, so it cannot reveal which
flavor a real drop consumes. The `/go/` bookmark bug is the cautionary case: a
Chromium `DataTransfer` check passed while LibreWolf still bookmarked the direct
service URL, because Gecko injects its own flavor that the synthetic test never
creates.

Facts worth remembering about Gecko drag data:

- For an `<a>`, Firefox fills `text/x-moz-url` (`"<url>\n<title>"`),
  `text/x-moz-url-data`, `text/x-moz-url-desc`, `text/uri-list`, and
  `text/plain` **before** dispatching `dragstart`
  (`EventStateManager::DetermineDragTargetAndDefaultData` →
  `nsContentAreaDragDrop::AddStringsToDataTransfer`). A `dragstart` handler can
  still overwrite them: `eDragStart` is `Mode::ReadWrite`, and `text/x-moz-url*`
  is exempt from the internal-flavor write block in `DataTransfer.cpp`.
- Firefox Places chooses the drop flavor by fixed priority:
  `text/x-moz-url`, tab, `text/plain` (`PlacesUIUtils.URI_FLAVORS`).
  **`text/uri-list` is not in that list.** So a handler that sets only
  `text/uri-list`/`text/plain` loses to the pre-filled `text/x-moz-url`.
  Overwrite `text/x-moz-url*` too if Firefox/LibreWolf must honor your payload.

Prove it with a real pointer drag and then read the result from disk:

```bash
# 1. Isolated display and fresh profile (rules out cache/SW).
Xvfb :99 -screen 0 1920x1080x24 &
cat > /tmp/lwtest/user.js <<'EOF'
user_pref("marionette.port", 2829);
user_pref("dom.security.https_only_mode", false);
user_pref("dom.security.https_only_mode.upgrade_local", false);
EOF

# 2. Launch LibreWolf on that display, with Marionette for geometry.
DISPLAY=:99 librewolf --no-remote --profile /tmp/lwtest --marionette \
  --new-instance "http://127.0.0.1:8080/" &
```

Get the source and target coordinates from the page, not by guessing. Marionette
reports CSS pixels relative to the viewport; `xdotool` uses root/screen pixels:

```python
from marionette_driver.marionette import Marionette
m = Marionette(host="127.0.0.1", port=2829); m.start_session()
print(m.execute_script("""
  var b=document.querySelectorAll('.card')[0].getBoundingClientRect();
  return JSON.stringify({cx:b.left+b.width/2, cy:b.top+b.height/2,
    ix:window.mozInnerScreenX, iy:window.mozInnerScreenY});
"""))
```

Convert with `screen = viewport + mozInnerScreenX/Y`. Confirm the offset first by
hovering and checking `document.querySelectorAll('.card:hover').length` — a wrong
offset makes the drag grab empty space and silently do nothing.

Perform the drag with the real pointer, slowing down and pausing over the
toolbar so Firefox registers a drop target:

```bash
DISPLAY=:99 xdotool mousemove $SRC_X $SRC_Y; sleep 0.4
DISPLAY=:99 xdotool mousedown 1; sleep 0.5
for x in ...; do DISPLAY=:99 xdotool mousemove $x 220; sleep 0.15; done
DISPLAY=:99 xdotool mousemove $TARGET_X $TARGET_Y; sleep 1.2
DISPLAY=:99 xdotool mouseup 1; sleep 2
```

Read the outcome from the profile, not from the UI. `places.sqlite` is locked
while LibreWolf runs, so copy `places.sqlite` **and** `places.sqlite-wal` before
querying (a WAL-only write is invisible if you copy the main file alone):

```bash
cp /tmp/lwtest/places.sqlite* /tmp/db/
sqlite3 /tmp/db/places.sqlite \
  "select b.title,p.url from moz_bookmarks b
   left join moz_places p on p.id=b.fk where b.type=1 order by b.id desc limit 5;"
```

Always run the **control**: temporarily remove the fix, rebuild, and repeat the
drag on a fresh profile. If the control stores the wrong value and the fixed
build stores the right one, you have proven causation rather than correlation.

## Invariants to preserve

- **No landing UI on `/go/` pages.** `redirect.html.j2` + `redirect.js` only. Never load `site.js` there.
- **CSP hashes must match inline content.** If you change inline CSS/JS/JSON, the build must recompute hashes. The server skips the `_headers` CSP for `/go/` paths so the page's meta CSP applies; keep that behavior.
- **Redirect pages use `full_css`** (theme variables + main CSS), not raw `main.css`, or colors break.
- **No periodic background probes.** Rechecks are event-driven only (load, online/offline, focus, visibility, redirect, manual). This is intentional to avoid passive traffic.
- **No homepage probing by default.** Cards link to `/go/<slug>/`, which resolves on demand. `probe.homepage: true` restores in-page rewriting.
- **Card drag payload is the `/go/` URL.** Cards navigate directly to the service on click, but `dragstart` sets `text/x-moz-url`, `text/x-moz-url-data`, `text/x-moz-url-desc`, `text/uri-list`, and `text/plain` to `/go/<slug>/` so a card dropped on the bookmarks bar is a network-aware link. Do not remove this handler; it is the only way to bookmark the redirect without a panel. `text/x-moz-url` is mandatory: Gecko pre-fills it from the anchor's raw `href` before `dragstart`, and Firefox's Places drop code prefers it over `text/uri-list`/`text/plain`, so without the overwrite Firefox/LibreWolf bookmarks the direct service URL even though Chromium honors the other flavors.
- **Server-side probe is preferred.** The page calls `/api/network` first; browser probes are the fallback. This bypasses browser HTTPS upgrades (e.g. LibreWolf `dom.security.https_only_mode.upgrade_local`).
- **Provider suggestions go through the server.** The browser calls `/api/suggest`; it never contacts the provider directly. Provider suggestions are on by default (`search.suggest_url`); set it to `""` to disable. Search targets the web only; it does not search or filter local services.
- **Redirects are allowlist-only.** `/go/<slug>/` resolves only configured links; never accept arbitrary URLs.
- **Favicons are build-time only.** The served page makes no third-party icon requests. Custom icons take precedence over autoloaded favicons. The cache in `assets/favicons/` is local and gitignored; never commit it.
- **Loopback by default.** `server.host` defaults to `127.0.0.1`; warn if changed.
- **Deterministic output.** No timestamps or nondeterministic ordering in built files.

## Code conventions

- Python: standard library plus the pinned deps; type hints on public functions; no new dependencies without a reason.
- JavaScript: ES5-compatible syntax in `src/js` (no build step, no modules). Keep `core.js` free of DOM access.
- CSS: single `src/styles/main.css`; use existing theme variables (`var(--bg)`, `var(--accent)`, etc.).
- Templates: Jinja2; escape user/config content; keep the redirect template minimal.
- Do not add comments unless they explain non-obvious intent.

## Common pitfalls

- **CSP blocks inline content** after editing templates/JS: rebuild so hashes update; check the browser console.
- **Stale service worker** serves old HTML/CSS: the SW is network-first for assets and versioned by build hash, but a browser "clear cache" often does not clear CacheStorage. Use DevTools → Application → Service Workers → Unregister, or "Clear site data", when testing. If you change the SW strategy, bump the cache version via the build hash.
- **Favicon cache** hides changes: use `--refresh-favicons` or delete the specific file in `assets/favicons/`. The cache is gitignored, so a fresh clone fetches on first build.
- **Browser HTTPS-Only upgrades** make LAN probes fail: the server-side `/api/network` probe is the fix; do not "fix" it by rewriting URLs.
- **`/go/ha` vs `/go/ha/`**: both resolve; keep relative asset paths working for both.
- **Top/edge color band**: a transparent `html` canvas or a collapsed margin can show the browser default color above `body`. Keep the solid `--bg` on the fixed `body::before` layer and avoid a collapsing `margin-top` on the first child (`body.top` must be 0). Verify by sampling the first rows and a margin column, not by eyeballing. See §5.
- **Overlay tint**: the background overlay must only apply when a background image is set, or it muddies the base color in both themes.

## Definition of done

- `cargo test` passes.
- `cargo run -- check-idempotent` passes.
- `python3 tests/smoke_test.py` passes (when Chromium is available).
- For UI changes: screenshots captured at 390/780/1920px in light and dark, with top/edge pixels compared to the page background and alignment measured numerically.
- `landing restart` succeeds and `landing health` returns OK.
- `SPEC.md` and `README.md` reflect any behavior change.
- No stale references to removed features (extensions, userscripts, CDN, `?go=`).
