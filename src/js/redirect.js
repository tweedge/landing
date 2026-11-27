(function (global) {
  "use strict";

  var Core = global.LandingCore;
  if (!Core) return;

  var dataEl = document.getElementById("landing-redirect");
  var data = {};
  try {
    data = JSON.parse(dataEl ? dataEl.textContent : "{}");
  } catch (error) {
    data = {};
  }

  var boot = document.getElementById("redirect-boot");
  var status = document.getElementById("redirect-status");
  var fail = document.getElementById("redirect-fail");
  var failTitle = document.getElementById("redirect-fail-title");
  var failReason = document.getElementById("redirect-fail-reason");
  var failIcon = document.getElementById("redirect-fail-icon");
  var retry = document.getElementById("redirect-retry");

  var link = data.link || {};
  var config = data.config || {};
  var basePath = data.basePath || "/";
  var params = new URLSearchParams(global.location.search || "");
  var mode = data.mode || "auto";
  var requestedMode = params.get("mode");
  if (config.redirect && config.redirect.allow_mode_override !== false && Core.OVERRIDES.indexOf(requestedMode) !== -1) {
    mode = requestedMode;
  }
  var delay = typeof data.delay === "number" ? data.delay : 0;
  var maxDelay = (config.redirect && config.redirect.max_delay_ms) || 5000;
  var requestedDelay = Number(params.get("delay"));
  if (config.redirect && config.redirect.allow_delay_override !== false && params.has("delay") && Number.isFinite(requestedDelay)) {
    delay = requestedDelay;
  }
  if (params.get("now") === "1") delay = 0;
  delay = Math.max(0, Math.min(maxDelay, delay));

  function setStatus(text) {
    if (status) status.textContent = text;
  }

  function showFailure(reason) {
    if (boot) boot.hidden = true;
    if (fail) fail.hidden = false;
    if (failTitle) failTitle.textContent = "Can’t open " + (link.name || "this service");
    if (failReason) failReason.textContent = reason || "The service is not reachable right now.";
    if (failIcon && link.icon_url) {
      var img = document.createElement("img");
      img.src = link.icon_url;
      img.alt = "";
      failIcon.appendChild(img);
    }
  }

  function navigate(target) {
    var safe = Core.isHttp(target) ? target : "";
    if (!safe) {
      showFailure("This service has an invalid target URL.");
      return;
    }
    global.location.replace(safe);
  }

  function resolve(facts) {
    var resolved = Core.resolveLink(link, facts, mode);
    if (!resolved || !resolved.available || !resolved.href) {
      var loc = facts.location;
      var con = facts.connectivity;
      if (mode === "lan") loc = "home";
      else if (mode === "wan") loc = "away";
      else if (mode === "offline") con = "offline";
      showFailure((resolved && resolved.reason ? resolved.reason : "Unavailable") + " · " + Core.locationLabel(loc) + " · " + Core.connectivityLabel(con));
      return;
    }
    setStatus("Opening " + (link.name || "service") + "…");
    if (delay <= 0) {
      navigate(resolved.href);
      return;
    }
    var remaining = delay;
    var timer = setInterval(function () {
      remaining -= 100;
      if (remaining <= 0) {
        clearInterval(timer);
        navigate(resolved.href);
      }
    }, 100);
  }

  async function check() {
    if (boot) boot.hidden = false;
    if (fail) fail.hidden = true;
    setStatus("Opening " + (link.name || "service") + "…");
    // Only this target is checked, for speed. Location and connectivity are
    // fetched independently so neither waits on the other.
    var location = "checking";
    var connectivity = "checking";
    var lanOk = null;
    var slug = link.slug || "";
    var locationPromise = fetch(basePath + "api/location?slug=" + encodeURIComponent(slug), {
      cache: "no-store", credentials: "omit", signal: AbortSignal.timeout(8000)
    }).then(function (r) { return r.ok ? r.json() : null; }).then(function (data) {
      if (data && typeof data.home === "boolean") {
        location = data.home ? "home" : "away";
        var tried = data.tried || [];
        lanOk = tried.length ? !!tried[0].ok : null;
      }
    }).catch(function () {});
    var connectivityPromise = fetch(basePath + "api/connectivity", {
      cache: "no-store", credentials: "omit", signal: AbortSignal.timeout(8000)
    }).then(function (r) { return r.ok ? r.json() : null; }).then(function (data) {
      if (data && typeof data.online === "boolean") connectivity = data.online ? "online" : "offline";
    }).catch(function () {});
    await Promise.all([locationPromise, connectivityPromise]);
    if (location === "checking") location = "away";
    if (connectivity === "checking") connectivity = "offline";
    resolve({ location: location, connectivity: connectivity, lanOk: lanOk });
  }

  if (retry) retry.addEventListener("click", check);
  check();
})(window);
