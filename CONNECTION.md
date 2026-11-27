# Connection handling

Landing reports two independent facts:

- **Location** — are we on the home LAN? (`Home` / `Away`)
- **Connectivity** — is the Internet reachable? (`Online` / `Offline`)

They are checked independently and shown as two pills, so one never waits on
the other. Location is decided by probing the configured LAN URLs; connectivity
by probing the public Internet URLs. A captive portal forces `Away`.

Each service is then resolved from its own probe result plus these two facts.
A service's state is one of:

| State | Meaning |
| --- | --- |
| **healthy** | Its preferred URL is reachable. |
| **degraded** | Its preferred URL is not reachable, but a fallback URL is, and we are not offline. |
| **unreachable** | No URL for it is reachable. |

"Preferred URL" is the LAN URL when Home, the public URL when Away. A service
with only a LAN URL has no fallback; a service with only a public URL has no
LAN preference.

## Matrix

Rows are the two facts; columns are the service's configured URLs.

### Home + Online

| Service has | Result | Link target |
| --- | --- | --- |
| LAN + public, LAN reachable | healthy | LAN URL |
| LAN + public, LAN down, public reachable | degraded | public URL |
| LAN + public, both down | unreachable | none (greyed) |
| LAN only, reachable | healthy | LAN URL |
| LAN only, down | unreachable | none (greyed) |
| public only, reachable | healthy | public URL |
| public only, down | unreachable | none (greyed) |

### Home + Offline

| Service has | Result | Link target |
| --- | --- | --- |
| LAN + public, LAN reachable | healthy | LAN URL |
| LAN + public, LAN down | unreachable | none (greyed) — public is unreachable offline |
| LAN only, reachable | healthy | LAN URL |
| LAN only, down | unreachable | none (greyed) |
| public only | unreachable | none (greyed) |

Offline does **not** rule out the home LAN. If LAN resources are reachable and
healthy while offline, they stay usable.

### Away + Online

| Service has | Result | Link target |
| --- | --- | --- |
| LAN + public, public reachable | healthy | public URL |
| LAN + public, public down | unreachable | none (greyed) — LAN is not reachable away |
| LAN only | unreachable | none (greyed) |
| public only, reachable | healthy | public URL |
| public only, down | unreachable | none (greyed) |

### Away + Offline

| Service has | Result | Link target |
| --- | --- | --- |
| any | unreachable | none (greyed) |

## Redirects (`/go/<slug>/`)

A redirect checks **only its own target**, not every service, for speed. It
resolves the target with the same matrix using the current Location and
Connectivity, then navigates. If the target is unreachable, it explains why and
links home.

## Pills

- **Home/Away** — from the LAN probe. Home when the majority of LAN checks pass
  (and no captive portal). Each service still keeps its own result, so a single
  down host is reported as unreachable/degraded rather than flipping the pill.
- **Online/Offline** — from the Internet probe, independent of the LAN probe.

Both pills are informational; the per-service state is what drives links.
