//! Integration tests for the Rust build pipeline.

use std::path::{Path, PathBuf};

use landing::{build, config};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn example_config() -> PathBuf {
    root().join("config/landing.example.yml")
}

fn temp_out(tag: &str) -> PathBuf {
    let unique = format!(
        "landing-test-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    std::env::temp_dir().join(unique)
}

fn build_to(out: &Path) -> build::BuildResult {
    build::build(build::BuildOptions {
        config_path: example_config(),
        out_dir: Some(out.to_path_buf()),
        single_file: Some(true),
        quiet: true,
        refresh_favicons: false,
        root: root(),
    })
    .expect("build should succeed")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn example_config_parses_and_validates() {
    let yaml = read(&example_config());
    let cfg = config::parse(&yaml).expect("example config parses");
    config::validate(&cfg).expect("example config validates");
    assert_eq!(cfg.site.title, "Landing");
    assert!(cfg.groups.iter().any(|g| g.id == "home"));
    let ha = cfg
        .groups
        .iter()
        .flat_map(|g| g.links.iter())
        .find(|l| l.id == "ha")
        .expect("home assistant link present");
    assert_eq!(ha.slug, "ha");
    assert_eq!(ha.url, "https://ha.example.com/");
    assert_eq!(ha.lan_url, "http://ha.lan/");
}

#[test]
fn build_produces_expected_artifacts() {
    let out = temp_out("artifacts");
    build_to(&out);
    for rel in [
        "site/index.html",
        "site/styles.css",
        "site/app.js",
        "site/sw.js",
        "site/manifest.webmanifest",
        "site/404.html",
        "site/robots.txt",
        "site/_headers",
        "site/Caddyfile.example",
        "site/nginx.conf.example",
        "site/go/ha/index.html",
        "landing.html",
        "config.resolved.json",
        "server.json",
        "build-info.json",
        "checksums.txt",
    ] {
        assert!(out.join(rel).is_file(), "missing {rel}");
    }
    let index = read(&out.join("site/index.html"));
    assert!(index.contains("data-link-id=\"ha\""));
    assert!(index.contains("data-group-id=\"home\""));
    assert!(index.contains("http://ha.lan/"));
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn build_is_idempotent() {
    let first = temp_out("idem-a");
    let second = temp_out("idem-b");
    build_to(&first);
    build_to(&second);
    let a = build::hash_tree(&first).unwrap();
    let b = build::hash_tree(&second).unwrap();
    assert_eq!(a, b, "two builds must produce identical output");
    let _ = std::fs::remove_dir_all(&first);
    let _ = std::fs::remove_dir_all(&second);
}

#[test]
fn noindex_governs_robots() {
    let out = temp_out("robots");
    build_to(&out);
    let robots = read(&out.join("site/robots.txt"));
    assert!(robots.contains("Disallow: /"));
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn redirect_pages_have_no_landing_ui() {
    let out = temp_out("redirect");
    build_to(&out);
    let page = read(&out.join("site/go/ha/index.html"));
    assert!(!page.contains("landing-groups"));
    assert!(!page.contains("app.js"));
    assert!(page.contains("Opening"));
    assert!(page.contains("Go to Landing"));
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn single_file_is_self_contained() {
    let out = temp_out("single");
    build_to(&out);
    let single = read(&out.join("landing.html"));
    assert!(single.contains("<style>"));
    assert!(single.contains("<script>"));
    assert!(!single.contains("src=\"/app.js"));
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn server_json_lists_probe_targets() {
    let out = temp_out("server");
    build_to(&out);
    let server: serde_json::Value = serde_json::from_str(&read(&out.join("server.json"))).unwrap();
    let lan = server["probe"]["lan"].as_array().unwrap();
    assert!(lan.iter().any(|e| e["url"] == "http://ha.lan/"));
    let internet = server["probe"]["internet"].as_array().unwrap();
    assert!(internet.iter().any(|u| u == "https://example.org/"));
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn csp_present_and_excluded_on_go_pages() {
    let out = temp_out("csp");
    build_to(&out);
    let headers = read(&out.join("site/_headers"));
    assert!(headers.contains("Content-Security-Policy:"));
    assert!(headers.contains("Permissions-Policy:"));
    // Redirect pages carry their own meta CSP, not a server header.
    let page = read(&out.join("site/go/ha/index.html"));
    assert!(page.contains("Content-Security-Policy"));
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn invalid_config_is_rejected() {
    let yaml = "version: 1\nsite:\n  title: ''\ngroups:\n  - id: home\n    links:\n      - id: a\n        url: https://a.example/\n";
    let cfg = config::parse(yaml).expect("parses");
    assert!(config::validate(&cfg).is_err());
}

#[test]
fn reserved_slug_is_rejected() {
    let yaml = "version: 1\nsite:\n  title: T\ngroups:\n  - id: home\n    links:\n      - id: go\n        slug: go\n        url: https://a.example/\n";
    let cfg = config::parse(yaml).unwrap();
    assert!(config::validate(&cfg).is_err());
}

#[test]
fn duplicate_slug_is_rejected() {
    let yaml = "version: 1\nsite:\n  title: T\ngroups:\n  - id: home\n    links:\n      - id: a\n        slug: same\n        url: https://a.example/\n      - id: b\n        slug: same\n        url: https://b.example/\n";
    let cfg = config::parse(yaml).unwrap();
    assert!(config::validate(&cfg).is_err());
}

#[test]
fn config_hash_is_stable() {
    let yaml = read(&example_config());
    let a = config::parse(&yaml).unwrap();
    let b = config::parse(&yaml).unwrap();
    assert_eq!(
        build::config_hash(&a).unwrap(),
        build::config_hash(&b).unwrap()
    );
}

#[test]
fn icon_tables_are_consistent() {
    // Rust and JS icon tables must expose the same names, or the client falls
    // back to letter tiles for icons the server knows about.
    let rust_names: std::collections::BTreeSet<&str> = landing::icons::ICONS
        .iter()
        .map(|(name, _)| *name)
        .collect();
    let js = landing::templates::ICONS_JS;
    let mut js_names = std::collections::BTreeSet::new();
    for line in js.lines() {
        let trimmed = line.trim_start();
        if let Some((key, _)) = trimmed.split_once(':') {
            let key = key.trim().trim_matches('\'').trim_matches('"');
            if !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
                && trimmed.contains('<')
            {
                js_names.insert(key);
            }
        }
    }
    let missing: Vec<_> = rust_names.difference(&js_names).collect();
    assert!(
        missing.is_empty(),
        "icons missing from src/js/icons.js: {missing:?}"
    );

    // Bodies must match too, or server-rendered HTML and client-rendered cards
    // can show different glyphs for the same icon.
    for (name, body) in landing::icons::ICONS {
        for key in [format!("{name}:"), format!("'{name}':")] {
            if let Some(start) = js.find(&key) {
                let rest = &js[start + key.len()..];
                if let Some(open) = rest.find('\'') {
                    let after = &rest[open + 1..];
                    if let Some(close) = after.find('\'') {
                        assert_eq!(
                            &after[..close],
                            *body,
                            "icon body drift for {name}: Rust and JS differ"
                        );
                    }
                }
                break;
            }
        }
    }
}

#[test]
fn embedded_js_is_structurally_balanced() {
    // Catches a truncated/broken asset that would make the whole bundle fail to
    // parse in the browser (and silently drop all interactivity).
    for (name, js) in [
        ("core.js", landing::templates::CORE_JS),
        ("icons.js", landing::templates::ICONS_JS),
        ("site.js", landing::templates::SITE_JS),
        ("redirect.js", landing::templates::REDIRECT_JS),
        ("sw.js", landing::templates::SW_JS),
    ] {
        let braces = js.matches('{').count() as i64 - js.matches('}').count() as i64;
        let brackets = js.matches('[').count() as i64 - js.matches(']').count() as i64;
        let parens = js.matches('(').count() as i64 - js.matches(')').count() as i64;
        assert_eq!(braces, 0, "{name} has unbalanced braces");
        assert_eq!(brackets, 0, "{name} has unbalanced brackets");
        assert_eq!(parens, 0, "{name} has unbalanced parentheses");
    }
}

#[test]
fn site_noindex_false_reenables_indexing() {
    // site.noindex is the documented control; build.noindex must default false
    // so it does not silently override an explicit site.noindex: false.
    let yaml = "version: 1\nsite:\n  title: T\n  noindex: false\ngroups:\n  - id: home\n    links:\n      - id: a\n        url: https://a.example/\n";
    let cfg = config::parse(yaml).unwrap();
    config::validate(&cfg).unwrap();
    assert!(!cfg.site.noindex);
    assert!(!cfg.build.noindex);
}

#[test]
fn server_json_reports_probe_flag_and_single_probe_object() {
    let out = temp_out("server-probe-flag");
    build_to(&out);
    let server: serde_json::Value = serde_json::from_str(&read(&out.join("server.json"))).unwrap();
    assert_eq!(server["server_probe"], serde_json::json!(true));
    // Only the nested probe object exists; no stale flat duplicates.
    assert!(server.get("probe_internet").is_none());
    assert!(server.get("captive_portal").is_none());
    let _ = std::fs::remove_dir_all(&out);
}
