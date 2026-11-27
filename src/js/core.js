(function (global) {
  "use strict";

  var STATES = ["checking", "lan", "lan-offline", "wan", "offline"];
  var OVERRIDES = ["auto", "lan", "wan", "offline"];

  function normalizeSlug(value) {
    if (typeof value !== "string") return "";
    var slug = value.trim().toLowerCase();
    if (!/^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/.test(slug)) return "";
    return slug;
  }

  function classify(input) {
    // Legacy combined classifier, kept for the redirect page.
    var internet = input && input.internet;
    var lan = input && input.lan;
    if (internet === null || typeof internet === "undefined") {
      if (lan === null || typeof lan === "undefined") return "checking";
    }
    if (lan === true) {
      if (internet === true) return "lan";
      if (internet === false) return "lan-offline";
      return "checking";
    }
    if (lan === false) {
      if (internet === true) return "wan";
      if (internet === false) return "offline";
      return "checking";
    }
    if (internet === true) return "wan";
    if (internet === false) return "offline";
    return "checking";
  }

  function modeLabel(mode) {
    if (mode === "lan") return "Home LAN";
    if (mode === "lan-offline") return "Home LAN · offline";
    if (mode === "wan") return "Away";
    if (mode === "offline") return "Offline";
    if (mode === "checking") return "Checking…";
    return "Auto";
  }

  function locationLabel(location) {
    return location === "home" ? "Home" : location === "away" ? "Away" : "Checking…";
  }

  function connectivityLabel(connectivity) {
    return connectivity === "online" ? "Online" : connectivity === "offline" ? "Offline" : "Checking…";
  }

  // Resolve a link from the two independent facts plus its own LAN result.
  // See CONNECTION.md for the matrix.
  function resolveLink(link, facts, override) {
    var location = (facts && facts.location) || "checking";
    var connectivity = (facts && facts.connectivity) || "checking";
    var lanOk = facts && typeof facts.lanOk === "boolean" ? facts.lanOk : null;
    if (override && override !== "auto") {
      if (override === "lan") location = "home";
      else if (override === "wan") location = "away";
      else if (override === "offline") connectivity = "offline";
    }
    var hasLan = !!(link && link.lan_url);
    var hasPublic = !!(link && link.url);
    var availability = (link && link.availability) || "always";

    if (location === "checking" || connectivity === "checking") {
      return {
        href: hasPublic ? link.url : (hasLan ? link.lan_url : "#"),
        available: true,
        state: "checking",
        reason: "Checking network…"
      };
    }

    if (availability === "lan_only") {
      if (location === "home" && lanOk !== false) {
        return { href: link.lan_url, available: true, state: "healthy", reason: "Home LAN" };
      }
      return { href: "", available: false, state: "unreachable", reason: "Home LAN only" };
    }

    if (availability === "wan_only") {
      if (connectivity === "online" && hasPublic) {
        return { href: link.url, available: true, state: "healthy", reason: "Online" };
      }
      return { href: "", available: false, state: "unreachable", reason: "Needs Internet" };
    }

    if (location === "home") {
      if (lanOk !== false && hasLan) {
        return { href: link.lan_url, available: true, state: "healthy", reason: "Home LAN" };
      }
      // LAN not reachable (or no LAN URL): fall back to public if online.
      if (hasPublic && connectivity === "online") {
        return {
          href: link.url,
          available: true,
          state: lanOk === false ? "degraded" : "healthy",
          reason: lanOk === false ? "LAN unreachable · using public" : "Online"
        };
      }
      return { href: "", available: false, state: "unreachable", reason: "Unreachable" };
    }

    // Away
    if (hasPublic && connectivity === "online") {
      return { href: link.url, available: true, state: "healthy", reason: "Online" };
    }
    return { href: "", available: false, state: "unreachable", reason: "Away · unreachable" };
  }

  function findLink(config, slug) {
    var wanted = normalizeSlug(slug);
    if (!wanted || !config || !Array.isArray(config.groups)) return null;
    for (var i = 0; i < config.groups.length; i += 1) {
      var group = config.groups[i];
      var links = group.links || [];
      for (var j = 0; j < links.length; j += 1) {
        if (links[j].slug === wanted) return links[j];
      }
    }
    return null;
  }

  function parseRedirect(pathname, search, basePath, config, hash) {
    if (!config || !config.redirect || config.redirect.enabled === false) return null;
    var base = basePath || "/";
    var rest = null;
    if (pathname.indexOf(base) === 0) {
      rest = pathname.slice(base.length);
    }
    var match = rest === null ? null : /^go\/([^/]+)\/?$/.exec(rest);
    if (!match && hash) {
      match = /^#go\/([^/]+)\/?$/.exec(hash);
    }
    if (!match) return null;
    var raw = decodeURIComponent(match[1]);
    var slug = normalizeSlug(raw);
    if (!slug) return { error: "invalid", slug: raw };

    var params = new URLSearchParams(search || "");
    var mode = "auto";
    var requestedMode = params.get("mode");
    if (config.redirect.allow_mode_override !== false && OVERRIDES.indexOf(requestedMode) !== -1) {
      mode = requestedMode;
    }

    var delay = Number(config.redirect.delay_ms);
    if (!Number.isFinite(delay)) delay = 1500;
    var maxDelay = Number(config.redirect.max_delay_ms);
    if (!Number.isFinite(maxDelay)) maxDelay = 5000;
    var requestedDelay = Number(params.get("delay"));
    if (config.redirect.allow_delay_override !== false && params.has("delay") && Number.isFinite(requestedDelay)) {
      delay = requestedDelay;
    }
    if (params.get("now") === "1") delay = 0;
    delay = Math.max(0, Math.min(maxDelay, delay));

    return { slug: slug, mode: mode, delay: delay, now: delay === 0 };
  }

  function hostOf(url) {
    try {
      var parsed = new URL(url, global.location && global.location.href ? global.location.href : "https://example.org/");
      return parsed.host;
    } catch (error) {
      return "";
    }
  }

  function randomToken() {
    try {
      var bytes = new Uint8Array(8);
      global.crypto.getRandomValues(bytes);
      return Array.prototype.map.call(bytes, function (byte) {
        return byte.toString(16).padStart(2, "0");
      }).join("");
    } catch (error) {
      return String(Date.now()) + Math.random().toString(16).slice(2);
    }
  }

  function withProbeQuery(url, enabled) {
    if (!enabled) return url;
    try {
      var parsed = new URL(url, global.location && global.location.href ? global.location.href : "https://example.org/");
      parsed.searchParams.set("_landing_probe", randomToken());
      return parsed.toString();
    } catch (error) {
      return url;
    }
  }

  function isHttp(url) {
    return /^https?:\/\//i.test(String(url || ""));
  }

  function unique(values) {
    var seen = Object.create(null);
    var result = [];
    (values || []).forEach(function (value) {
      if (!value || seen[value]) return;
      seen[value] = true;
      result.push(value);
    });
    return result;
  }

  function clamp(value, min, max) {
    return Math.max(min, Math.min(max, value));
  }

  global.LandingCore = {
    STATES: STATES,
    OVERRIDES: OVERRIDES,
    normalizeSlug: normalizeSlug,
    classify: classify,
    locationLabel: locationLabel,
    connectivityLabel: connectivityLabel,
    modeLabel: modeLabel,
    resolveLink: resolveLink,
    findLink: findLink,
    parseRedirect: parseRedirect,
    hostOf: hostOf,
    randomToken: randomToken,
    withProbeQuery: withProbeQuery,
    isHttp: isHttp,
    unique: unique,
    clamp: clamp
  };
})(window);
