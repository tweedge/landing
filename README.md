# landing

A local-first start page for your homelab. Clone it, edit it, run one script, and it builds, installs, and serves itself on `localhost`.

Landing lists grouped links, detects Internet and home-LAN reachability in the browser, uses LAN URLs on the LAN, public URLs away, and LAN URLs when offline at home. It also supports `/go/<service>/` redirects for favorites-bar bookmarks, and it fetches each service's real favicon at build time.

There is no cloud build, no CDN, no browser extension, and no userscript. The build and install happen on your machine.

## Quick start

```bash
git clone <your-fork> landing
cd landing
./landing init          # writes config/landing.yml from the example
$EDITOR config/landing.yml
./landing
```

`./landing` builds the site, installs it under XDG paths, installs and starts a systemd user service when available, and runs a healthcheck. Open <http://localhost:8080/>.

`config/landing.yml` is gitignored, so your real hosts and URLs are never committed. The tracked [`config/landing.example.yml`](config/landing.example.yml) is the template and the config the test suite builds against.

## Entrypoint

| Command | What it does |
| --- | --- |
| `./landing` | Build, install, start, and healthcheck (same as `install`). |
| `./landing build` | Build `dist/` from `config/landing.yml`. |
| `./landing install` | Build, install to XDG paths, install/enable the systemd user unit, start, and healthcheck. |
| `./landing start` | Start the installed service. Use `--foreground` without systemd. |
| `./landing stop` | Stop the service. |
| `./landing restart` | Restart after a rebuild. |
| `./landing status` | Show service state, bind address, installed paths, and last healthcheck. |
| `./landing health` | Request the health path; non-zero exit on failure. |
| `./landing logs` | Show recent service logs. |
| `./landing uninstall` | Stop/disable and remove installed files; keep config unless `--purge`. |

Re-running `./landing` after editing YAML is safe and idempotent.

## Install layout

| Path | Contents |
| --- | --- |
| `~/.local/share/landing/site/` | Built static site served by the local server. |
| `~/.local/share/landing/state/` | Runtime state and PID file for foreground mode. |
| `~/.config/landing/landing.yml` | Installed copy of your configuration. |
| `~/.config/systemd/user/landing.service` | systemd user unit. |
| `~/.local/state/landing/` | Logs when not using the journal. |

Paths respect `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, and `XDG_STATE_HOME`.

## Favicons

By default, each link's icon is the service's real favicon, fetched and cached at build time:

```yaml
links:
  - id: ha
    name: Home Assistant
    icon: auto          # default: fetch the real favicon
    url: https://ha.example.com/
    lan_url: http://ha.home.arpa/
```

- The build parses `<link rel="icon">`, `shortcut icon`, `apple-touch-icon`, and `mask-icon`, then falls back to `/favicon.ico`.
- It tries `favicon_url`, then `url`, then `lan_url`, so LAN-only services can still get icons when `allow_private_hosts` is enabled.
- It accepts self-signed certificates, since LAN services often use them.
- If a configured URL only answers on the other scheme (http↔https), the build still fetches the icon but prints a warning telling you to update the URL in your config.
- Icons are cached in `assets/favicons/` (gitignored) so repeated builds are fast and deterministic. A fresh clone fetches them on first build.
- `./landing build --refresh-favicons` re-fetches all icons; `favicon_refresh: true` re-fetches one link.
- Icon ladder: a real favicon fetched at build wins; otherwise a custom `icon` (path or built-in name); otherwise `defaults.icon` (default `link`). Never a letter.
- Set `icon_background` on a link to give its icon a custom tile color (for example a warm light tile behind a dark mark). There is no automatic detection.
- Group and category icons are always explicit; they are never autoloaded.
- `/go/` bookmarks and the redirect interstitial use the same resolved icon as the card.

Configure it globally:

```yaml
favicons:
  enabled: true
  refresh: false
  timeout_ms: 5000
  max_bytes: 1048576
  allow_private_hosts: false   # opt in to fetch icons from LAN/private addresses
  cache_dir: assets/favicons
  prefer: [svg, png, ico]
```

## Search

The search bar is the focal point of the page and is focused on load, so Landing works as a new-tab or home page as well as a homelab navigator.

- **Enter** searches the web. Local services are not searched or filtered; the cards are a static directory.
- The web search uses a configurable URL template, so it is not tied to one provider:

```yaml
search:
  enabled: true
  focus: true
  engine: https://duckduckgo.com/?q={query}
  engine_name: DuckDuckGo
  open_in_new_tab: false
```

Any provider works, for example:

```yaml
engine: https://www.google.com/search?q={query}
engine: https://kagi.com/search?q={query}
engine: https://search.brave.com/search?q={query}
engine: https://www.startpage.com/sp/search?query={query}
```

`{query}` is replaced with the URL-encoded query. A page cannot invoke the browser's own configured default search engine — there is no web API for that — so set `engine` to match your preferred provider.

### Suggestions

Typing shows provider suggestions, proxied through the local server so the browser never contacts the provider directly:

```yaml
search:
  suggestions: true
  suggest_url: https://duckduckgo.com/ac/?q={query}&type=list
  suggest_limit: 6
```

`suggest_url` must contain `{query}`. The server fetches it, normalizes common response shapes (OpenSearch, DuckDuckGo, Google), dedupes, and returns a small JSON list. Set it to `""` to disable provider suggestions. Use the arrow keys to move and Enter to choose.

## How network detection works

- **Internet:** `fetch("https://example.org/", { mode: "no-cors" })` with `example.com` and `gstatic.com/generate_204` fallbacks. No CORS, token, or deployed endpoint is required.
- **LAN/local DNS:** probes the existing `lan_url`s from your config (for example `http://ha.lan/`) with `fetch(no-cors)` and a random query. Any success means LAN. No dedicated endpoint, reverse proxy, certificate, wildcard DNS, or specific CIDR is required.
- **State:** `lan`, `lan-offline` (home but no Internet), `wan`, `offline`. Links resolve accordingly; unavailable links stay visible with a reason.
- **Override:** the status menu lets you force `Auto`, `Home LAN`, `Away`, or `Offline`.
- **Two pills:** Home/Away (LAN reachability) and Online/Offline (Internet), checked independently so neither waits on the other.
- **Per-service state:** each card is healthy, degraded (LAN down but a public fallback works), or unreachable. See [`CONNECTION.md`](CONNECTION.md) for the full matrix.
- **Rechecks are event-driven, not periodic.** The page rechecks on load, `online`/`offline`, window focus, tab visibility, before a redirect, and when you press **Recheck now**. There is deliberately no background timer in the page or the server, to avoid sending unnecessary traffic to the Internet probe and every configured LAN service.
- **Local origin:** because the page is served from `http://localhost`, it can probe HTTP LAN URLs directly in Chrome, Firefox, and Safari, with no mixed-content blocking and no Local Network Access prompt.
- **Server-side probe:** the local server also probes LAN URLs itself (`GET /api/network`) and the page prefers that result. This bypasses browser HTTPS upgrades (for example LibreWolf's `dom.security.https_only_mode.upgrade_local`), which otherwise rewrite `http://*.lan/` to `https://` and make the LAN look unreachable. Set `server.probe: false` to disable it.
- **Captive portals are never mistaken for home.** The server checks the standard captive-portal endpoints (`generate_204`, Apple, Microsoft). If any returns the wrong result, the network is treated as a portal and LAN URLs are not used. Configure under `probe.captive_portal`.
- **Certificate handling:** `probe.lan.accept_untrusted_cert` (default `true`) accepts self-signed LAN certificates. `probe.lan.pin_certificates` (default `false`) pins SHA-256 fingerprints from `probe.lan.pins`; a mismatch marks the service unreachable, which also defeats portals.
- **Optional content marker:** set `probe_expect` on a link to require a known substring in the probe response, so a portal page cannot pass.
- **No homepage probing by default.** Service cards link to `/go/<slug>/`, which resolves the network state on demand. The homepage itself sends no probes (`probe.homepage: false`), so it stays quiet. Set `probe.homepage: true` to restore in-page LAN/WAN link rewriting.

## Bookmarks and help

- **Bookmarks:** drag any service card to your bookmarks bar, or right-click it and choose **Bookmark Link**. Clicking a card opens the service directly, but dragging carries the `/go/<service>/` URL, so the bookmark is network-aware.
- The **help icon** (or the `?` key) opens help with network-state explanations, keyboard shortcuts, and hidden features. The header and footer are intentionally omitted; the only chrome is a small floating control cluster.
- Hidden features include `/go/ha/`, `?mode=lan|wan|offline`, `?delay=0..5000`, `?now=1`, and `?help=1`.
- Redirects are immediate by default (`redirect.delay_ms: 0`). `/go/` pages are a standalone minimal page: a short "Opening …" status, then the redirect. They never load the landing UI. If the target can't be reached, the page explains why and offers **Try again** and **Go to Landing**.
- Set `site.favicon` to a custom asset path if you prefer your own page icon.

## Configuration

Edit [`config/landing.yml`](config/landing.yml). Highlights:

```yaml
site:
  title: Landing
  subtitle: Homelab

theme:
  mode: dark            # dark (default) | light | auto
  accent: "#f0a06a"
  radius: 18
  density: comfortable  # comfortable | compact
  font: system
  card_style: hairline
  background:
    image: null         # local path or allowlisted URL
    overlay: "rgba(30, 20, 15, 0.16)"   # only applied when an image is set
  colors:
    dark:
      bg: "#1e1917"
      surface: "#2a2320"
      text: "#f4e8df"
      muted: "#b9a79d"
      border: "#3d322e"
      accent: "#f0a06a"
      accent2: "#e3a0a0"
      success: "#9bc49a"
      warning: "#e6c07b"
      danger: "#e88a8a"
    light:
      bg: "#f7efe7"
      surface: "#fffaf5"
      text: "#4a3b36"
      muted: "#8c7a70"
      border: "#e6d6c8"
      accent: "#c96f3b"
      accent2: "#b56a7a"
      success: "#6f9b72"
      warning: "#b9822f"
      danger: "#c95f5f"

probe:
  internet:
    urls: [https://example.org/, https://example.com/, https://www.gstatic.com/generate_204]
    mode: no-cors
  lan:
    source: links          # probe lan_urls from configured links
    max_links: 20
    random_query: true
    ip_fallback: null      # optional user-provided IP for DoH diagnosis
    random_subdomain: false
    domain: null

server:
  host: 127.0.0.1
  port: 8080
  health_path: /healthz

install:
  mode: user               # user (default) | system (optional, requires root)
  systemd: auto            # auto | true | false
  service_name: landing

layout:
  columns: 3
  max_width: 1400
  groups_max_width: 1080   # cap the card area on wide screens

groups:
  - id: home
    name: Home
    icon: home
    links:
      - id: ha
        name: Home Assistant
        url: https://ha.example.com/
        lan_url: http://ha.home.arpa/
        icon: auto
        hotkey: "1"
```

- A link needs `url` and/or `lan_url`.
- `availability: lan_only` for LAN-only services; `wan_only` for public-only.
- `probe: false` excludes a link from LAN probing; `probe_url` uses a lightweight existing path.
- `hotkey` opens a link with a digit when no input is focused.
- `/go/<slug>/` pages are generated for every link; use them for bookmarks.

## Security and privacy

- The server binds to `127.0.0.1` by default. Changing `server.host` exposes the page to the network; the entrypoint warns.
- No analytics, trackers, cookies, third-party fonts, or third-party icons at runtime.
- Probes are `GET`, credential-free, `no-store`, and side-effect-free; they target only configured URLs.
- Redirects are allowlist-only; `/go/<slug>/` never accepts arbitrary URLs.
- Favicon fetching is build-time only, bounded, credential-free, and opt-in for private hosts.
- `noindex` is enabled by default.

## Testing

```bash
python3 -m unittest discover -s tests -v
python3 scripts/build.py --check-idempotent
python3 scripts/smoke_test.py        # requires Chromium
```

## Documentation

- [`SPEC.md`](SPEC.md) — full requirements, architecture, config schema, state machine, redirect, security, local installation, and acceptance criteria.
