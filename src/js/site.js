(function (global) {
  "use strict";

  var Core = global.LandingCore;
  var Icons = global.LandingIcons;
  if (!Core || !Icons) return;

  var STORAGE = {
    override: "landing.override",
    probe: "landing.probe"
  };

  var state = {
    config: null,
    linkMap: Object.create(null),
    location: "checking",       // home | away | checking
    connectivity: "checking",   // online | offline | checking
    lanOk: Object.create(null), // slug -> boolean
    lanTried: [],
    captivePortal: false,
    locationOverride: "auto",   // auto | home | away
    connectivityOverride: "auto", // auto | online | offline
    suggestions: [],
    suggestionIndex: -1,
    suggestTimer: null,
    suggestSeq: 0,
    checking: false,
    lastCheck: 0,
    probing: null,
    redirect: null,
    toastTimer: null
  };

  var els = {};
  var basePath = "/";

  function $(selector, root) {
    return (root || document).querySelector(selector);
  }

  function $all(selector, root) {
    return Array.prototype.slice.call((root || document).querySelectorAll(selector));
  }

  function safeStorageGet(key) {
    try {
      return global.localStorage.getItem(key);
    } catch (error) {
      return null;
    }
  }

  function safeStorageSet(key, value) {
    try {
      global.localStorage.setItem(key, value);
    } catch (error) {
      /* ignore */
    }
  }

  function safeSessionGet(key) {
    try {
      return global.sessionStorage.getItem(key);
    } catch (error) {
      return null;
    }
  }

  function safeSessionSet(key, value) {
    try {
      global.sessionStorage.setItem(key, value);
    } catch (error) {
      /* ignore */
    }
  }

  function readConfig() {
    if (global.__LANDING_CONFIG_OVERRIDE__ && Array.isArray(global.__LANDING_CONFIG_OVERRIDE__.groups)) {
      return global.__LANDING_CONFIG_OVERRIDE__;
    }
    var node = document.getElementById("landing-config");
    if (!node) return null;
    try {
      return JSON.parse(node.textContent);
    } catch (error) {
      return null;
    }
  }

  function buildLinkMap(config) {
    var map = Object.create(null);
    if (!config || !Array.isArray(config.groups)) return map;
    config.groups.forEach(function (group) {
      (group.links || []).forEach(function (link) {
        map[link.id] = link;
      });
    });
    return map;
  }

  function allLinks() {
    var links = [];
    if (!state.config || !Array.isArray(state.config.groups)) return links;
    state.config.groups.forEach(function (group) {
      (group.links || []).forEach(function (link) {
        links.push(link);
      });
    });
    return links;
  }

  function safeHref(value) {
    try {
      var parsed = new URL(String(value || ""), global.location.href);
      if (parsed.protocol === "http:" || parsed.protocol === "https:") return parsed.href;
    } catch (error) {
      /* invalid */
    }
    return "#";
  }

  function safeImage(value) {
    var text = String(value || "");
    if (/^https?:\/\//i.test(text) || /^data:image\//i.test(text)) return text;
    return "";
  }

  function iconNode(item, label) {
    var span = document.createElement("span");
    span.className = "card-icon";
    var iconUrl = safeImage(item && item.icon_url);
    if (iconUrl) {
      var img = document.createElement("img");
      img.src = iconUrl;
      img.alt = "";
      span.appendChild(img);
      return span;
    }
    var remoteUrl = item && item.icon ? safeImage(item.icon) : "";
    if (remoteUrl) {
      var remote = document.createElement("img");
      remote.src = remoteUrl;
      remote.alt = "";
      span.appendChild(remote);
      return span;
    }
    var iconName = item && item.icon;
    if (iconName === "auto" || iconName === "none" || !iconName) {
      iconName = (state.config.defaults && state.config.defaults.icon) || "link";
    }
    span.innerHTML = Icons.html(iconName, label);
    return span;
  }

  function groupIconNode(group) {
    var span = document.createElement("span");
    span.className = "group-icon";
    var iconUrl = safeImage(group.icon_url);
    if (iconUrl) {
      var img = document.createElement("img");
      img.src = iconUrl;
      img.alt = "";
      span.appendChild(img);
    } else {
      span.innerHTML = Icons.html(group.icon, group.name);
    }
    return span;
  }

  function renderGroups() {
    var container = $("#landing-groups");
    if (!container) return;
    container.textContent = "";
    state.linkMap = buildLinkMap(state.config);
    (state.config.groups || []).forEach(function (group) {
      var section = document.createElement("section");
      section.className = "group";
      section.dataset.groupId = group.id;
      section.dataset.showWhen = (group.show_when || []).join(",");
      section.dataset.hideWhen = (group.hide_when || []).join(",");

      var header = document.createElement("header");
      header.className = "group-header";
      header.appendChild(groupIconNode(group));
      var h2 = document.createElement("h2");
      h2.textContent = group.name || group.id;
      header.appendChild(h2);
      section.appendChild(header);

      var cards = document.createElement("div");
      cards.className = "cards";

      (group.links || []).forEach(function (link) {
        if (link.hidden) return;
        var card = document.createElement("a");
        card.className = "card";
        card.dataset.linkId = link.id;
        card.dataset.url = link.url || "";
        card.dataset.lanUrl = link.lan_url || "";
        card.dataset.availability = link.availability || "always";
        card.dataset.prefer = link.prefer || "auto";
        card.dataset.probe = link.probe === false ? "false" : "true";
        card.dataset.probeUrl = link.probe_url || "";
        var homepageProbe = !!(state.config.probe && state.config.probe.homepage);
        if (homepageProbe) {
          card.href = safeHref(link.url || link.lan_url || "#");
          card.target = link.target || "_blank";
        } else {
          card.href = basePath + "go/" + encodeURIComponent(link.slug) + "/";
          card.target = "_self";
        }
        card.rel = card.target === "_blank" ? "noopener noreferrer" : "";

        // Clicks go straight to the service, but a drag carries the /go/ URL so
        // the card can be dragged to the bookmarks bar as a network-aware link.
        // Gecko fills text/x-moz-url with the anchor's raw href before dragstart
        // and Places reads that flavor before uri-list/plain, so overwrite it
        // too or the bookmarks bar stores the direct service URL.
        card.addEventListener("dragstart", function (event) {
          var goUrl = new URL(basePath + "go/" + encodeURIComponent(link.slug) + "/", global.location.origin).href;
          if (!event.dataTransfer) return;
          var title = link.name || link.slug || goUrl;
          event.dataTransfer.effectAllowed = "copyLink";
          event.dataTransfer.setData("text/x-moz-url", goUrl + "\n" + title);
          event.dataTransfer.setData("text/x-moz-url-data", goUrl);
          event.dataTransfer.setData("text/x-moz-url-desc", title);
          event.dataTransfer.setData("text/uri-list", goUrl);
          event.dataTransfer.setData("text/plain", goUrl);
        });

        card.appendChild(iconNode(link, link.name));

        var cardTitle = document.createElement("span");
        cardTitle.className = "card-title";
        cardTitle.textContent = link.name;
        card.appendChild(cardTitle);

        var stateBadge = document.createElement("span");
        stateBadge.className = "card-state";
        stateBadge.dataset.role = "state";
        stateBadge.hidden = true;
        card.appendChild(stateBadge);

        if (link.hotkey) {
          var hotkey = document.createElement("span");
          hotkey.className = "card-hotkey";
          hotkey.setAttribute("aria-hidden", "true");
          hotkey.textContent = link.hotkey;
          card.appendChild(hotkey);
        }
        cards.appendChild(card);
      });

      section.appendChild(cards);
      container.appendChild(section);
    });
    applyState();
  }

  function readOverrides() {
    var raw = safeStorageGet(STORAGE.override);
    var result = { location: "auto", connectivity: "auto" };
    if (!raw) return result;
    try {
      var parsed = JSON.parse(raw);
      if (parsed.location === "home" || parsed.location === "away") result.location = parsed.location;
      if (parsed.connectivity === "online" || parsed.connectivity === "offline") result.connectivity = parsed.connectivity;
    } catch (error) {
      /* ignore */
    }
    return result;
  }

  function saveOverrides() {
    var value = { location: state.locationOverride, connectivity: state.connectivityOverride };
    if (value.location === "auto" && value.connectivity === "auto") {
      try { global.localStorage.removeItem(STORAGE.override); } catch (error) { /* ignore */ }
    } else {
      safeStorageSet(STORAGE.override, JSON.stringify(value));
    }
  }

  function effectiveLocation() {
    return state.locationOverride !== "auto" ? state.locationOverride : state.location;
  }

  function effectiveConnectivity() {
    return state.connectivityOverride !== "auto" ? state.connectivityOverride : state.connectivity;
  }

  function applyTheme(value) {
    var theme = value === "light" || value === "dark" ? value : "dark";
    document.documentElement.dataset.theme = theme;
  }

  function searchEngineUrl(query) {
    var engine = (state.config.search && state.config.search.engine) || "";
    if (!engine || engine.indexOf("{query}") === -1) return "";
    return engine.replace("{query}", encodeURIComponent(query));
  }

  function submitSearch(raw) {
    var query = (raw || "").trim();
    if (!query) return;
    var url = searchEngineUrl(query);
    if (!url) return;
    var newTab = !!(state.config.search && state.config.search.open_in_new_tab);
    if (newTab) global.open(url, "_blank", "noopener,noreferrer");
    else global.location.href = url;
  }

  function updateSearchHint() {
    var hint = $("#landing-search-hint");
    if (!hint) return;
    var name = (state.config.search && state.config.search.engine_name) || "the web";
    hint.textContent = "Enter to search " + name;
  }

  function renderSuggestions(items) {
    var list = $("#landing-suggestions");
    var input = $("#landing-search");
    if (!list || !input) return;
    state.suggestions = items;
    state.suggestionIndex = -1;
    list.textContent = "";
    if (!items.length) {
      list.hidden = true;
      input.setAttribute("aria-expanded", "false");
      return;
    }
    items.forEach(function (item, index) {
      var li = document.createElement("li");
      li.className = "suggestion";
      li.id = "landing-suggestion-" + index;
      li.setAttribute("role", "option");
      li.dataset.index = String(index);
      var label = document.createElement("span");
      label.className = "suggestion-label";
      label.textContent = item.label;
      li.appendChild(label);
      li.addEventListener("mousedown", function (event) {
        event.preventDefault();
        chooseSuggestion(index);
      });
      list.appendChild(li);
    });
    list.hidden = false;
    input.setAttribute("aria-expanded", "true");
  }

  function highlightSuggestion(index) {
    var list = $("#landing-suggestions");
    var input = $("#landing-search");
    if (!list) return;
    var options = $all(".suggestion", list);
    if (!options.length) return;
    state.suggestionIndex = Math.max(0, Math.min(options.length - 1, index));
    options.forEach(function (option, i) {
      var active = i === state.suggestionIndex;
      option.classList.toggle("is-active", active);
      option.setAttribute("aria-selected", active ? "true" : "false");
    });
    if (input) input.setAttribute("aria-activedescendant", "landing-suggestion-" + state.suggestionIndex);
  }

  function closeSuggestions() {
    var list = $("#landing-suggestions");
    var input = $("#landing-search");
    state.suggestions = [];
    state.suggestionIndex = -1;
    if (list) {
      list.hidden = true;
      list.textContent = "";
    }
    if (input) {
      input.setAttribute("aria-expanded", "false");
      input.removeAttribute("aria-activedescendant");
    }
  }

  function chooseSuggestion(index) {
    var item = state.suggestions[index];
    if (!item) return;
    var input = $("#landing-search");
    if (input) input.value = item.label;
    closeSuggestions();
    submitSearch(item.label);
  }

  function fetchRemoteSuggestions(query) {
    var search = state.config.search || {};
    if (!search.suggest_url) return;
    var seq = ++state.suggestSeq;
    fetch(basePath + "api/suggest?q=" + encodeURIComponent(query), {
      cache: "no-store",
      credentials: "omit",
      signal: AbortSignal.timeout(3000)
    }).then(function (response) {
      return response.ok ? response.json() : null;
    }).then(function (data) {
      if (seq !== state.suggestSeq) return;
      var remote = (data && data.suggestions) || [];
      var seen = {};
      var merged = [];
      remote.forEach(function (label) {
        if (!label || seen[label.toLowerCase()]) return;
        seen[label.toLowerCase()] = true;
        merged.push({ label: label, kind: "web" });
      });
      renderSuggestions(merged.slice(0, search.suggest_limit || 6));
    }).catch(function () {
      if (seq !== state.suggestSeq) return;
      renderSuggestions([]);
    });
  }

  function updateSuggestions() {
    var input = $("#landing-search");
    if (!input) return;
    var query = input.value.trim();
    if (!query) {
      closeSuggestions();
      return;
    }
    var search = state.config.search || {};
    if (!search.suggest_url) {
      closeSuggestions();
      return;
    }
    if (state.suggestTimer) clearTimeout(state.suggestTimer);
    state.suggestTimer = setTimeout(function () {
      fetchRemoteSuggestions(query);
    }, 180);
  }

  function setBanner(kind, text, actionLabel, action, secondaryLabel, secondaryAction) {
    var banner = $("#landing-banner");
    if (!banner) return;
    banner.textContent = "";
    if (!text) {
      banner.hidden = true;
      return;
    }
    banner.hidden = false;
    banner.dataset.kind = kind || "info";
    banner.appendChild(document.createTextNode(text + " "));
    function addAction(label, handler) {
      if (!label || typeof handler !== "function") return;
      banner.appendChild(document.createTextNode(" "));
      var button = document.createElement("button");
      button.type = "button";
      button.textContent = label;
      button.addEventListener("click", handler, { once: true });
      banner.appendChild(button);
    }
    addAction(actionLabel, action);
    addAction(secondaryLabel, secondaryAction);
  }

  function updateBanner() {
    if (state.captivePortal) {
      setBanner("warn", "A captive portal is intercepting this network, so home LAN URLs are not used. Sign in to the network, then recheck.", "Recheck", function () {
        recheck(true);
      });
      return;
    }
    if (state.locationOverride !== "auto" || state.connectivityOverride !== "auto") {
      var parts = [];
      if (state.locationOverride !== "auto") parts.push("location " + state.locationOverride);
      if (state.connectivityOverride !== "auto") parts.push("connectivity " + state.connectivityOverride);
      setBanner("info", "Forced: " + parts.join(", ") + ".", "Auto", function () {
        state.locationOverride = "auto";
        state.connectivityOverride = "auto";
        saveOverrides();
        recheck(true);
      });
      return;
    }
    if (effectiveLocation() === "home" && effectiveConnectivity() === "offline") {
      setBanner("warn", "You are on the home LAN but offline. LAN services stay available; public-only services are disabled.");
      return;
    }
    if (effectiveLocation() === "away" && effectiveConnectivity() === "offline") {
      setBanner("danger", "You are away and offline. External links are unavailable.", "Recheck", function () {
        recheck(true);
      });
      return;
    }
    setBanner(null, "");
  }

  function applyState() {
    var location = effectiveLocation();
    var connectivity = effectiveConnectivity();
    document.documentElement.dataset.location = state.checking ? "checking" : location;
    document.documentElement.dataset.connectivity = state.checking ? "checking" : connectivity;

    var locationEl = $("#landing-location");
    if (locationEl) {
      locationEl.textContent = state.checking && state.location === "checking"
        ? "Checking…"
        : Core.locationLabel(location) + (state.locationOverride !== "auto" ? " · forced" : "");
    }
    var connectivityEl = $("#landing-connectivity");
    if (connectivityEl) {
      connectivityEl.textContent = state.checking && state.connectivity === "checking"
        ? "Checking…"
        : Core.connectivityLabel(connectivity) + (state.connectivityOverride !== "auto" ? " · forced" : "");
    }

    var detail = $("#landing-location-detail");
    if (detail) {
      var tried = state.lanTried || [];
      if (!tried.length) {
        detail.hidden = true;
        detail.textContent = "";
      } else {
        var okCount = tried.filter(function (item) { return item.ok; }).length;
        detail.hidden = false;
        detail.textContent = "LAN: " + okCount + "/" + tried.length + " reachable";
        detail.title = tried.map(function (item) {
          return (item.ok ? "OK " : "FAIL ") + Core.hostOf(item.url);
        }).join("\n");
      }
    }

    var groups = $("#landing-groups");
    if (groups) groups.setAttribute("aria-busy", state.checking ? "true" : "false");

    var homepageProbe = !!(state.config.probe && state.config.probe.homepage);
    $all(".card").forEach(function (card) {
      if (!homepageProbe) return;
      var link = state.linkMap[card.dataset.linkId];
      if (!link) return;
      var resolved = Core.resolveLink(link, {
        location: location,
        connectivity: connectivity,
        lanOk: state.lanOk[link.slug]
      });
      var badge = $('[data-role="state"]', card);
      card.dataset.resolvedState = resolved.state;
      card.dataset.reason = resolved.reason || "";
      card.title = resolved.reason || "";
      if (resolved.available) {
        card.href = safeHref(resolved.href);
        card.removeAttribute("aria-disabled");
        card.classList.remove("is-unavailable");
        card.classList.toggle("is-degraded", resolved.state === "degraded");
        if (badge) {
          badge.hidden = resolved.state !== "degraded";
          badge.dataset.kind = resolved.state;
          badge.textContent = resolved.state === "degraded" ? "Degraded" : "";
        }
      } else {
        card.removeAttribute("href");
        card.setAttribute("aria-disabled", "true");
        card.classList.add("is-unavailable");
        card.classList.remove("is-degraded");
        if (badge) {
          badge.hidden = false;
          badge.dataset.kind = "unreachable";
          badge.textContent = "Unreachable";
        }
      }
    });

    $all(".group").forEach(function (group) {
      var show = (group.dataset.showWhen || "").split(",").filter(Boolean);
      var hide = (group.dataset.hideWhen || "").split(",").filter(Boolean);
      var visible = true;
      if (show.length && show.indexOf(location) === -1) visible = false;
      if (hide.length && hide.indexOf(location) !== -1) visible = false;
      group.classList.toggle("is-state-hidden", !visible);
    });

    updateBanner();
  }

  function setLocationOverride(mode) {
    if (["auto", "home", "away"].indexOf(mode) === -1) return;
    state.locationOverride = mode;
    saveOverrides();
    applyState();
    if (mode === "auto") recheck(true);
  }

  function setConnectivityOverride(mode) {
    if (["auto", "online", "offline"].indexOf(mode) === -1) return;
    state.connectivityOverride = mode;
    saveOverrides();
    applyState();
    if (mode === "auto") recheck(true);
  }

  function navigate(href, target) {
    var safe = safeHref(href);
    if (!safe || safe === "#") return;
    if (target === "_blank") {
      global.open(safe, "_blank", "noopener,noreferrer");
    } else {
      global.location.assign(safe);
    }
  }

  function browserName() {
    var ua = navigator.userAgent || "";
    if (/Electron\//.test(ua)) return "electron";
    if (/Firefox\//.test(ua)) return "firefox";
    if (/Edg\//.test(ua)) return "edge";
    if (/Chrome\//.test(ua) || /Chromium\//.test(ua)) return "chromium";
    if (/Safari\//.test(ua)) return "safari";
    return "unknown";
  }

  function detectHttpsUpgrade(requested) {
    if (!global.performance || !performance.getEntriesByType) return false;
    var entries = performance.getEntriesByType("resource");
    for (var i = 0; i < entries.length; i += 1) {
      var name = entries[i].name || "";
      if (name.indexOf("https://") !== 0) continue;
      for (var j = 0; j < requested.length; j += 1) {
        if (requested[j].indexOf("http://") !== 0) continue;
        var host = Core.hostOf(requested[j]);
        if (host && Core.hostOf(name) === host) return true;
      }
    }
    return false;
  }

  function firstLanUrl() {
    var urls = lanProbeUrls();
    return urls[0] || "";
  }

  function showToast(text, ms) {
    var toast = $("#landing-toast");
    if (!toast) return;
    toast.textContent = text;
    toast.hidden = false;
    if (state.toastTimer) clearTimeout(state.toastTimer);
    state.toastTimer = setTimeout(function () {
      toast.hidden = true;
      state.toastTimer = null;
    }, ms || 3200);
  }

  function probeImage(url, timeout) {
    return new Promise(function (resolve) {
      var image = new Image();
      var timer = setTimeout(function () {
        resolve(false);
      }, timeout || 2500);
      image.onload = function () {
        clearTimeout(timer);
        resolve(true);
      };
      image.onerror = function () {
        clearTimeout(timer);
        resolve(false);
      };
      image.src = Core.withProbeQuery(url, true);
    });
  }

  function probe(url, options) {
    return probeFetch(url, options);
  }

  function probeFetch(url, options) {
    var opts = options || {};
    var timeout = opts.timeout || 2500;
    var controller = new AbortController();
    var timer = setTimeout(function () {
      controller.abort();
    }, timeout);
    var init = {
      mode: opts.mode || "no-cors",
      cache: "no-store",
      credentials: "omit",
      redirect: "follow",
      signal: controller.signal
    };
    return fetch(url, init)
      .then(function (response) {
        if (init.mode === "cors") {
          if (!response.ok) return false;
          if (opts.token) {
            return response.text().then(function (text) {
              return text.indexOf(opts.token) !== -1;
            });
          }
        }
        return true;
      })
      .catch(function () {
        return false;
      })
      .finally(function () {
        clearTimeout(timer);
      });
  }

  function lanProbeUrls() {
    var lan = (state.config.probe && state.config.probe.lan) || {};
    var source = lan.source || "links";
    var urls = [];
    if (source === "links" || source === "both") {
      var candidates = allLinks().filter(function (link) {
        return link.lan_url && link.probe !== false;
      });
      candidates.sort(function (a, b) {
        return (b.lan_url.indexOf("https:") === 0 ? 1 : 0) - (a.lan_url.indexOf("https:") === 0 ? 1 : 0);
      });
      candidates.slice(0, lan.max_links || 5).forEach(function (link) {
        urls.push(link.probe_url || link.lan_url);
      });
    }
    if (source === "explicit" || source === "both") {
      (lan.urls || []).forEach(function (url) {
        urls.push(url);
      });
    }
    urls = Core.unique(urls);
    if (lan.random_subdomain && lan.domain) {
      var scheme = urls.length && urls[0].indexOf("https:") === 0 ? "https" : "http";
      urls.push(scheme + "://probe-" + Core.randomToken() + "." + lan.domain + "/");
    }
    return urls;
  }

  function lanImageUrls() {
    var lan = (state.config.probe && state.config.probe.lan) || {};
    var source = lan.source || "links";
    var urls = [];
    function add(value) {
      if (!value) return;
      try {
        urls.push(new URL(value, global.location.href).toString());
      } catch (error) {
        /* ignore invalid */
      }
    }
    if (source === "links" || source === "both") {
      var candidates = allLinks().filter(function (link) {
        return link.lan_url && link.probe !== false;
      });
      candidates.slice(0, lan.max_links || 5).forEach(function (link) {
        if (link.probe_image) {
          add(link.probe_image);
        } else {
          try {
            add(new URL("/favicon.ico", link.probe_url || link.lan_url).toString());
          } catch (error) {
            /* ignore */
          }
        }
      });
    }
    if (source === "explicit" || source === "both") {
      (lan.urls || []).forEach(function (url) {
        try {
          add(new URL("/favicon.ico", url).toString());
        } catch (error) {
          /* ignore */
        }
      });
    }
    return Core.unique(urls);
  }

  async function fetchJson(path) {
    try {
      var response = await fetch(basePath + path, {
        cache: "no-store",
        credentials: "omit",
        signal: AbortSignal.timeout(8000)
      });
      if (!response.ok) return null;
      return await response.json();
    } catch (error) {
      return null;
    }
  }

  async function probeLocation() {
    var data = await fetchJson("api/location");
    if (data && typeof data.home === "boolean") {
      var lanOk = Object.create(null);
      (data.tried || []).forEach(function (item) {
        if (item.slug) lanOk[item.slug] = !!item.ok;
      });
      return {
        location: data.home ? "home" : "away",
        lanOk: lanOk,
        tried: data.tried || [],
        captivePortal: !!data.captivePortal
      };
    }
    // Browser fallback: probe LAN URLs directly.
    var lan = (state.config.probe && state.config.probe.lan) || {};
    var urls = lanProbeUrls();
    if (!urls.length) return { location: "away", lanOk: Object.create(null), tried: [], captivePortal: false };
    var timeout = lan.timeout_ms || 2500;
    var results = await Promise.all(urls.map(function (url) {
      var target = Core.withProbeQuery(url, lan.random_query !== false);
      return probeFetch(target, { mode: "no-cors", timeout: timeout }).then(function (ok) {
        return { url: target, ok: ok };
      });
    }));
    var okCount = results.filter(function (r) { return r.ok; }).length;
    return {
      location: okCount * 2 > results.length ? "home" : "away",
      lanOk: Object.create(null),
      tried: results,
      captivePortal: false
    };
  }

  async function probeConnectivity() {
    var data = await fetchJson("api/connectivity");
    if (data && typeof data.online === "boolean") {
      return { connectivity: data.online ? "online" : "offline", captivePortal: !!data.captivePortal };
    }
    if (navigator.onLine === false) return { connectivity: "offline", captivePortal: false };
    var probeConfig = (state.config.probe && state.config.probe.internet) || {};
    var urls = probeConfig.urls || [];
    for (var i = 0; i < urls.length; i += 1) {
      var ok = await probe(urls[i], {
        mode: probeConfig.mode || "no-cors",
        timeout: probeConfig.timeout_ms || 2500,
        token: probeConfig.token || ""
      });
      if (ok) return { connectivity: "online", captivePortal: false };
    }
    return { connectivity: "offline", captivePortal: false };
  }

  function cacheState() {
    try {
      safeSessionSet(STORAGE.probe, JSON.stringify({
        location: state.location,
        connectivity: state.connectivity,
        lanOk: state.lanOk,
        captivePortal: state.captivePortal,
        ts: state.lastCheck
      }));
    } catch (error) {
      /* ignore */
    }
  }

  function restoreCachedState() {
    var raw = safeSessionGet(STORAGE.probe);
    if (!raw) return;
    try {
      var cached = JSON.parse(raw);
      var ttl = (state.config.probe && state.config.probe.cache_ttl_ms) || 30000;
      if (!cached || !cached.ts || Date.now() - cached.ts > ttl) return;
      state.location = cached.location || "checking";
      state.connectivity = cached.connectivity || "checking";
      state.lanOk = cached.lanOk || Object.create(null);
      state.captivePortal = !!cached.captivePortal;
      state.lastCheck = cached.ts;
      applyState();
    } catch (error) {
      /* ignore */
    }
  }

  function recheck(force) {
    if (state.probing) return state.probing;
    var ttl = (state.config.probe && state.config.probe.cache_ttl_ms) || 30000;
    if (!force && state.lastCheck && Date.now() - state.lastCheck < ttl && state.location !== "checking") {
      applyState();
      return Promise.resolve();
    }

    state.checking = true;
    applyState();
    // Location and connectivity are independent; do not wait for each other.
    var locationPromise = probeLocation().then(function (result) {
      state.location = result.location;
      state.lanOk = result.lanOk || Object.create(null);
      state.lanTried = result.tried || [];
      state.captivePortal = state.captivePortal || !!result.captivePortal;
      state.lastCheck = Date.now();
      applyState();
    }).catch(function () {
      state.location = "away";
      applyState();
    });
    var connectivityPromise = probeConnectivity().then(function (result) {
      state.connectivity = result.connectivity;
      state.captivePortal = state.captivePortal || !!result.captivePortal;
      state.lastCheck = Date.now();
      applyState();
    }).catch(function () {
      state.connectivity = "offline";
      applyState();
    });

    state.probing = Promise.all([locationPromise, connectivityPromise]).then(function () {
      state.checking = false;
      state.probing = null;
      cacheState();
      applyState();
      if (state.redirect && !state.redirect.unavailable) {
        updateRedirectView();
      }
    });
    return state.probing;
  }

  function setDocumentIcon(url) {
    if (!url) return;
    var icon = document.querySelector('link[rel="icon"]');
    if (!icon) {
      icon = document.createElement("link");
      icon.rel = "icon";
      document.head.appendChild(icon);
    }
    icon.href = url;
  }

  function renderRedirectIcon(link) {
    var holder = $('[data-role="redirect-icon"]');
    if (!holder) return;
    holder.textContent = "";
    var node = iconNode(link, link.name);
    node.className = "";
    holder.appendChild(node.firstChild || document.createTextNode(""));
  }

  function redirectResolved() {
    if (!state.redirect) return null;
    var link = state.redirect.link;
    var redirect = state.redirect.redirect;
    var override = redirect.mode && redirect.mode !== "auto" ? redirect.mode : "auto";
    return Core.resolveLink(link, {
      location: effectiveLocation(),
      connectivity: effectiveConnectivity(),
      lanOk: state.lanOk[link.slug]
    }, override);
  }

  function updateRedirectView() {
    if (!state.redirect) return;
    var redirect = state.redirect.redirect;
    var link = state.redirect.link;
    var resolved = redirectResolved();
    var eyebrow = $('[data-role="redirect-eyebrow"]');
    var reason = $('[data-role="redirect-reason"]');
    var host = $('[data-role="redirect-host"]');
    var go = $('[data-role="redirect-go"]');
    var retry = $('[data-role="redirect-retry"]');
    var countdown = $('[data-role="redirect-countdown"]');

    if (!resolved || !resolved.available) {
      state.redirect.unavailable = true;
      if (eyebrow) eyebrow.textContent = "Not available";
      if (reason) reason.textContent = (resolved && resolved.reason ? resolved.reason : "Unavailable") + " · " + Core.locationLabel(effectiveLocation()) + " · " + Core.connectivityLabel(effectiveConnectivity());
      if (host) host.textContent = Core.hostOf(link.url || link.lan_url);
      if (go) go.disabled = true;
      if (retry) retry.hidden = false;
      if (countdown) countdown.textContent = "—";
      return;
    }

    state.redirect.unavailable = false;
    state.redirect.target = resolved.href;
    if (eyebrow) eyebrow.textContent = "Taking you there";
    if (reason) reason.textContent = resolved.reason;
    if (host) host.textContent = Core.hostOf(resolved.href);
    if (go) go.disabled = false;
    if (retry) retry.hidden = true;
    if (countdown) countdown.textContent = (redirect.delay / 1000).toFixed(1);
  }

  function startRedirectCountdown() {
    if (!state.redirect || !state.redirect.target) return;
    var redirect = state.redirect.redirect;
    state.redirect.remaining = redirect.delay;
    state.redirect.paused = document.hidden;
    if (state.redirect.timer) clearInterval(state.redirect.timer);
    if (redirect.delay <= 0) {
      navigateRedirect();
      return;
    }
    state.redirect.timer = setInterval(function () {
      if (!state.redirect) return;
      if (state.redirect.paused) return;
      state.redirect.remaining -= 100;
      var countdown = $('[data-role="redirect-countdown"]');
      if (countdown) countdown.textContent = Math.max(0, state.redirect.remaining / 1000).toFixed(1);
      if (state.redirect.remaining <= 0) navigateRedirect();
    }, 100);
  }

  function navigateRedirect() {
    if (!state.redirect || !state.redirect.target) return;
    var target = safeHref(state.redirect.target);
    if (!target || target === "#") {
      showToast("This service has an invalid target URL.");
      return;
    }
    if (state.redirect.timer) clearInterval(state.redirect.timer);
    state.redirect = null;
    global.location.replace(target);
  }

  function cancelRedirect() {
    if (!state.redirect) return;
    if (state.redirect.timer) clearInterval(state.redirect.timer);
    state.redirect = null;
    var view = $("#redirect-view");
    if (view) view.hidden = true;
    document.body.classList.remove("is-redirecting");
    try {
      global.history.replaceState({}, "", basePath);
    } catch (error) {
      /* ignore */
    }
    applyState();
    var search = $("#landing-search");
    if (search) search.focus();
  }

  async function retryRedirect() {
    if (!state.redirect) return;
    await recheck(true);
    updateRedirectView();
    if (state.redirect && !state.redirect.unavailable) startRedirectCountdown();
  }

  async function startRedirect(redirect) {
    if (!redirect) return;
    if (redirect.error) {
      showToast("Unknown or invalid service.");
      return;
    }
    var link = Core.findLink(state.config, redirect.slug);
    if (!link) {
      showToast("Unknown service: " + redirect.slug);
      return;
    }
    state.redirect = {
      redirect: redirect,
      link: link,
      target: null,
      timer: null,
      remaining: redirect.delay,
      paused: false,
      unavailable: false
    };
    document.body.classList.add("is-redirecting");
    var name = $('[data-role="redirect-name"]');
    if (name) name.textContent = link.name;
    renderRedirectIcon(link);
    setDocumentIcon(safeImage(link.icon_url));
    var host = $('[data-role="redirect-host"]');
    if (host) host.textContent = Core.hostOf(link.url || link.lan_url);
    var reason = $('[data-role="redirect-reason"]');
    if (reason) reason.textContent = "Checking network…";
    var go = $('[data-role="redirect-go"]');
    if (go) go.disabled = true;
    var retry = $('[data-role="redirect-retry"]');
    if (retry) retry.hidden = true;

    // Avoid flashing the interstitial for fast probes: reveal it only if the
    // check takes a moment, or if the target turns out to be unavailable.
    var revealTimer = setTimeout(function () {
      var view = $("#redirect-view");
      if (view && state.redirect) view.hidden = false;
    }, 150);

    await recheck(true);
    clearTimeout(revealTimer);
    if (!state.redirect) return;
    updateRedirectView();
    if (state.redirect.unavailable) {
      var view = $("#redirect-view");
      if (view) view.hidden = false;
      return;
    }
    if (state.redirect.redirect.delay <= 0) {
      navigateRedirect();
      return;
    }
    var view = $("#redirect-view");
    if (view) view.hidden = false;
    startRedirectCountdown();
  }

  function registerServiceWorker() {
    if (!("serviceWorker" in navigator) || !/^https?:$/.test(global.location.protocol)) return;
    var hadController = !!navigator.serviceWorker.controller;
    navigator.serviceWorker.register(basePath + "sw.js", { scope: basePath, updateViaCache: "none" }).then(function (registration) {
      registration.addEventListener("updatefound", function () {
        var installing = registration.installing;
        if (!installing) return;
        installing.addEventListener("statechange", function () {
          if (installing.state === "installed" && navigator.serviceWorker.controller) {
            showToast("A new version is available. Reload to update.", 8000);
          }
        });
      });
    }).catch(function () {
      /* service worker is optional */
    });
    navigator.serviceWorker.addEventListener("controllerchange", function () {
      if (hadController) showToast("Landing updated. Reload to use the new version.", 8000);
    });
  }

  function applyConfig(config) {
    if (!config || !Array.isArray(config.groups)) return false;
    state.config = config;
    state.linkMap = buildLinkMap(config);
    renderGroups();
    recheck(true);
    return true;
  }

  function cacheElements() {
    els = {
      groups: $("#landing-groups"),
      banner: $("#landing-banner"),
      location: $("#landing-location"),
      connectivity: $("#landing-connectivity"),
      search: $("#landing-search"),
      toast: $("#landing-toast"),
      redirect: $("#redirect-view")
    };
  }

  function focusCard(delta) {
    var cards = $all(".card").filter(function (card) {
      return !card.hidden && card.getClientRects().length && card.getAttribute("aria-disabled") !== "true";
    });
    if (!cards.length) return;
    var active = document.activeElement && document.activeElement.closest ? document.activeElement.closest(".card") : null;
    var index = cards.indexOf(active);
    var next = cards[Math.max(0, Math.min(cards.length - 1, index + delta))];
    if (next) next.focus();
  }

  function openDialog(id) {
    var dialog = document.getElementById(id);
    if (!dialog || dialog.open) return;
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.hidden = false;
  }

  function closeDialog(id) {
    var dialog = document.getElementById(id);
    if (!dialog || !dialog.open) return;
    if (typeof dialog.close === "function") dialog.close();
    else dialog.hidden = true;
  }

  function redirectUrl(link) {
    var slug = encodeURIComponent(link.slug);
    if (global.location.protocol === "file:") {
      return global.location.href.split("#")[0].split("?")[0] + "#go/" + slug;
    }
    var base = basePath || "/";
    return new URL(base + "go/" + slug + "/", global.location.origin).href;
  }

  function copyText(text, button) {
    function done() {
      if (!button) return;
      var old = button.textContent;
      button.textContent = "Copied";
      setTimeout(function () { button.textContent = old; }, 1200);
    }
    function fallback() {
      var area = document.createElement("textarea");
      area.value = text;
      area.setAttribute("readonly", "");
      area.style.position = "fixed";
      area.style.opacity = "0";
      document.body.appendChild(area);
      area.select();
      try { document.execCommand("copy"); done(); } catch (error) { /* ignore */ }
      area.remove();
    }
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(done).catch(fallback);
    } else {
      fallback();
    }
  }

  function openHelp() {
    openDialog("help-dialog");
  }

  function bindEvents() {
    var search = $("#landing-search");
    var searchForm = $("#landing-search-form");
    if (search) {
      search.addEventListener("input", function () {
            updateSuggestions();
      });
      search.addEventListener("keydown", function (event) {
        if (event.key === "Escape") {
          if (state.suggestions.length) {
            closeSuggestions();
            return;
          }
          search.value = "";
                search.blur();
          return;
        }
        if (event.key === "ArrowDown" && state.suggestions.length) {
          event.preventDefault();
          highlightSuggestion(state.suggestionIndex + 1);
          return;
        }
        if (event.key === "ArrowUp" && state.suggestions.length) {
          event.preventDefault();
          highlightSuggestion(state.suggestionIndex - 1);
          return;
        }
        if (event.key === "Enter") {
          event.preventDefault();
          if (state.suggestionIndex >= 0) {
            chooseSuggestion(state.suggestionIndex);
          } else {
            closeSuggestions();
            submitSearch(search.value);
          }
        }
      });
      search.addEventListener("blur", function () {
        setTimeout(closeSuggestions, 120);
      });
    }
    if (searchForm) {
      searchForm.addEventListener("submit", function (event) {
        event.preventDefault();
        submitSearch(search ? search.value : "");
      });
    }

    function bindPill(buttonSel, menuSel, handler) {
      var button = $(buttonSel);
      var menu = $(menuSel);
      if (!button || !menu) return;
      button.addEventListener("click", function (event) {
        event.stopPropagation();
        menu.hidden = !menu.hidden;
        button.setAttribute("aria-expanded", menu.hidden ? "false" : "true");
      });
      menu.addEventListener("click", function (event) {
        var item = event.target.closest("button");
        if (!item) return;
        if (item.id === "recheck-button") {
          recheck(true);
        } else {
          handler(item);
        }
        menu.hidden = true;
        button.setAttribute("aria-expanded", "false");
      });
      document.addEventListener("click", function (event) {
        if (!event.target.closest(".network-wrap")) {
          menu.hidden = true;
          button.setAttribute("aria-expanded", "false");
        }
      });
    }

    bindPill("#location-button", "#location-menu", function (item) {
      if (item.dataset.location) setLocationOverride(item.dataset.location);
    });
    bindPill("#connectivity-button", "#connectivity-menu", function (item) {
      if (item.dataset.connectivity) setConnectivityOverride(item.dataset.connectivity);
    });

    var groups = $("#landing-groups");
    if (groups) {
      groups.addEventListener("click", function (event) {
        var card = event.target.closest("a.card");
        if (!card) return;
        if (!(state.config.probe && state.config.probe.homepage)) {
          // Cards link to /go/<slug>/; let the browser navigate normally.
          return;
        }
        if (card.getAttribute("aria-disabled") === "true") {
          event.preventDefault();
          showToast(card.dataset.reason || "This link is unavailable right now.");
          return;
        }
        if (state.checking || state.location === "checking" || state.connectivity === "checking") {
          event.preventDefault();
          recheck(true).then(function () {
            var link = state.linkMap[card.dataset.linkId];
            if (!link) return;
            var resolved = Core.resolveLink(link, {
              location: effectiveLocation(),
              connectivity: effectiveConnectivity(),
              lanOk: state.lanOk[link.slug]
            });
            if (!resolved.available) {
              showToast(resolved.reason || "This link is unavailable right now.");
              return;
            }
            navigate(resolved.href, link.target || "_blank");
          });
        }
      });
    }

    document.addEventListener("keydown", function (event) {
      if (event.key === "Escape" && state.redirect) {
        event.preventDefault();
        cancelRedirect();
      }
      var target = event.target;
      var tag = target && target.tagName ? target.tagName.toLowerCase() : "";
      var typing = tag === "input" || tag === "textarea" || (target && target.isContentEditable);
      if (typing) return;
      if (event.key === "/" && !event.ctrlKey && !event.metaKey && !event.altKey) {
        var searchInput = $("#landing-search");
        if (searchInput) {
          event.preventDefault();
          searchInput.focus();
        }
        return;
      }
      if (event.key === "?") {
        event.preventDefault();
        openHelp();
        return;
      }
      if (event.key === "ArrowDown" || event.key === "ArrowRight") {
        event.preventDefault();
        focusCard(1);
        return;
      }
      if (event.key === "ArrowUp" || event.key === "ArrowLeft") {
        event.preventDefault();
        focusCard(-1);
        return;
      }
      if (/^[a-zA-Z0-9]$/.test(event.key) && !event.ctrlKey && !event.metaKey && !event.altKey) {
        var hotkeyCard = $all(".card").filter(function (card) {
          var hotkey = card.querySelector(".card-hotkey");
          return hotkey && hotkey.textContent === event.key && !card.hidden;
        })[0];
        if (hotkeyCard) {
          event.preventDefault();
          hotkeyCard.click();
        }
      }
    });

    var helpButton = $("#help-button");
    if (helpButton) helpButton.addEventListener("click", openHelp);

    var helpDialog = $("#help-dialog");
    if (helpDialog) {
      ["help-close", "help-close-2"].forEach(function (id) {
        var button = document.getElementById(id);
        if (button) button.addEventListener("click", function () { closeDialog("help-dialog"); });
      });
      helpDialog.addEventListener("click", function (event) {
        if (event.target === helpDialog) closeDialog("help-dialog");
      });
    }

    var go = $('[data-role="redirect-go"]');
    if (go) go.addEventListener("click", function () {
      if (state.redirect && state.redirect.target) navigateRedirect();
      else retryRedirect();
    });
    var cancel = $('[data-role="redirect-cancel"]');
    if (cancel) cancel.addEventListener("click", cancelRedirect);
    var retry = $('[data-role="redirect-retry"]');
    if (retry) retry.addEventListener("click", retryRedirect);

    global.addEventListener("online", function () {
      recheck(true);
    });
    global.addEventListener("offline", function () {
      recheck(true);
    });
    global.addEventListener("focus", function () {
      if (state.lastCheck && Date.now() - state.lastCheck > 30000) recheck(true);
    });
    document.addEventListener("visibilitychange", function () {
      if (state.redirect) {
        state.redirect.paused = document.hidden;
        if (!document.hidden && state.lastCheck && Date.now() - state.lastCheck > 30000) {
          recheck(true);
        }
      } else if (!document.hidden && state.lastCheck && Date.now() - state.lastCheck > 30000) {
        recheck(true);
      }
    });
    global.addEventListener("landing:recheck", function () {
      recheck(true);
    });
    document.addEventListener("landing:recheck", function () {
      recheck(true);
    });
    document.addEventListener("landing:config", function (event) {
      try {
        var config = typeof event.detail === "string" ? JSON.parse(event.detail) : event.detail;
        applyConfig(config);
      } catch (error) {
        /* ignore malformed config */
      }
    });
  }

  function hideNetworkUi() {
    $all(".network-wrap").forEach(function (wrap) { wrap.hidden = true; });
    var groups = $("#landing-groups");
    if (groups) groups.setAttribute("aria-busy", "false");
  }

  function init() {
    basePath = (document.body && document.body.dataset.basePath) || "/";
    if (basePath.charAt(basePath.length - 1) !== "/") basePath += "/";
    state.config = readConfig();
    if (!state.config) {
      showToast("Landing configuration failed to load.");
      return;
    }
    state.linkMap = buildLinkMap(state.config);
    updateSearchHint();
    var overrides = readOverrides();
    state.locationOverride = overrides.location;
    state.connectivityOverride = overrides.connectivity;
    var params = new URLSearchParams(global.location.search);
    var queryMode = params.get("mode");
    if (queryMode === "lan") state.locationOverride = "home";
    else if (queryMode === "wan") state.locationOverride = "away";
    else if (queryMode === "offline") state.connectivityOverride = "offline";
    state.redirect = Core.parseRedirect(global.location.pathname, global.location.search, basePath, state.config, global.location.hash);
    applyTheme((state.config.theme && state.config.theme.mode) || "dark");
    cacheElements();
    renderGroups();
    bindEvents();
    if (!state.redirect) {
      if (params.get("help") === "1") openHelp();
    }
    registerServiceWorker();
    if (state.redirect) {
      startRedirect(state.redirect);
    } else if (state.config.probe && state.config.probe.homepage) {
      restoreCachedState();
      recheck(true);
    } else {
      // Homepage probing is disabled: cards already link to /go/<slug>/,
      // so no network detection is needed here.
      hideNetworkUi();
    }
    global.Landing = {
      applyConfig: applyConfig,
      recheck: function () { return recheck(true); },
      getConfig: function () { return state.config; },
      getState: function () {
        return {
          location: effectiveLocation(),
          connectivity: effectiveConnectivity(),
          lanOk: state.lanOk,
          lanTried: state.lanTried,
          captivePortal: state.captivePortal,
          locationOverride: state.locationOverride,
          connectivityOverride: state.connectivityOverride,
          checking: state.checking
        };
      }
    };
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init, { once: true });
  } else {
    init();
  }
})(window);
