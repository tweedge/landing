//! Built-in SVG icon set, mirrored from the original Python build.

pub static ICONS: &[(&str, &str)] = &[
    (
        "home",
        r#"<path d="M3 10.5 12 3l9 7.5"/><path d="M5 9.5V21h14V9.5"/><path d="M9 21v-6h6v6"/>"#,
    ),
    (
        "router",
        r#"<rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 16h.01M11 16h.01M17 16h.01"/><path d="M7 13V9M12 13V7M17 13V9"/>"#,
    ),
    (
        "book",
        r#"<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z"/>"#,
    ),
    ("play", r#"<path d="m8 5 11 7-11 7Z"/>"#),
    (
        "camera",
        r#"<path d="M4 7h3l2-2h6l2 2h3v12H4Z"/><circle cx="12" cy="13" r="4"/>"#,
    ),
    (
        "music",
        r#"<path d="M9 18V5l10-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="16" cy="16" r="3"/>"#,
    ),
    (
        "server",
        r#"<rect x="3" y="4" width="18" height="6" rx="2"/><rect x="3" y="14" width="18" height="6" rx="2"/><path d="M7 7h.01M7 17h.01"/>"#,
    ),
    ("chart", r#"<path d="M4 20V10M10 20V4M16 20v-7M22 20H2"/>"#),
    (
        "shield",
        r#"<path d="M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6Z"/><path d="m9 12 2 2 4-4"/>"#,
    ),
    (
        "cloud",
        r#"<path d="M17.5 19H7a5 5 0 0 1-.8-9.9A6 6 0 0 1 18 8a5.5 5.5 0 0 1-.5 11Z"/>"#,
    ),
    (
        "link",
        r#"<path d="M10 13a5 5 0 0 0 7.1 0l2-2a5 5 0 0 0-7.1-7.1l-1.1 1.1"/><path d="M14 11a5 5 0 0 0-7.1 0l-2 2A5 5 0 0 0 12 20.1l1.1-1.1"/>"#,
    ),
    (
        "globe",
        r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.6 4 5.7 4 9s-1.5 6.4-4 9c-2.5-2.6-4-5.7-4-9s1.5-6.4 4-9Z"/>"#,
    ),
    (
        "terminal",
        r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 9 3 3-3 3M13 15h4"/>"#,
    ),
    (
        "lock",
        r#"<rect x="4" y="10" width="16" height="11" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3"/>"#,
    ),
    ("download", r#"<path d="M12 3v12M7 10l5 5 5-5M4 21h16"/>"#),
    (
        "search",
        r#"<circle cx="11" cy="11" r="7"/><path d="m20 20-3.2-3.2"/>"#,
    ),
    (
        "sun",
        r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>"#,
    ),
    (
        "moon",
        r#"<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5Z"/>"#,
    ),
    (
        "refresh",
        r#"<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 4v7h-7"/>"#,
    ),
    ("check", r#"<path d="m5 12 4 4L19 6"/>"#),
    (
        "alert",
        r#"<path d="M12 3 2 21h20Z"/><path d="M12 9v5M12 17h.01"/>"#,
    ),
    (
        "wifi",
        r#"<path d="M5 12a10 10 0 0 1 14 0M8 15a6 6 0 0 1 8 0M11 18a2 2 0 0 1 2 0"/><circle cx="12" cy="21" r=".5"/>"#,
    ),
    (
        "external",
        r#"<path d="M14 3h7v7M10 14 21 3"/><path d="M21 14v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5"/>"#,
    ),
    ("menu", r#"<path d="M4 7h16M4 12h16M4 17h16"/>"#),
    (
        "cat",
        r#"<path d="M12 5c.67 0 1.35.09 2 .26 1.78-2 5.03-2.84 6.42-2.26 1.4.58-.42 7-.42 7 .57 1.07 1 2.24 1 3.44C21 17.9 16.97 21 12 21s-9-3.1-9-7.56c0-1.25.43-2.42 1-3.44 0 0-1.82-6.42-.42-7 1.39-.58 4.64.26 6.42 2.26A9.7 9.7 0 0 1 12 5Z"/><path d="M8 14v.5"/><path d="M16 14v.5"/><path d="M11.25 16.25h1.5L12 17l-.75-.75Z"/>"#,
    ),
    ("bookmark", r#"<path d="M6 4h12v17l-6-4-6 4Z"/>"#),
    (
        "help",
        r#"<circle cx="12" cy="12" r="9"/><path d="M9.1 9a3 3 0 0 1 5.8 1c0 2-3 2-3 4"/><path d="M12 17h.01"/>"#,
    ),
    (
        "copy",
        r#"<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/>"#,
    ),
    ("play", r#"<path d="m8 5 11 7-11 7Z"/>"#),
    (
        "film",
        r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 4v16M17 4v16M3 9h4M3 15h4M17 9h4M17 15h4"/>"#,
    ),
    (
        "library",
        r#"<path d="M4 4h4v16H4zM10 4h4v16h-4zM16 5l3.5 15"/>"#,
    ),
    ("download", r#"<path d="M12 3v12M7 10l5 5 5-5M4 21h16"/>"#),
    (
        "magnet",
        r#"<path d="M6 3v9a6 6 0 0 0 12 0V3M6 8h4M14 8h4"/>"#,
    ),
    (
        "box",
        r#"<path d="m12 3 8 4.5v9L12 21l-8-4.5v-9Z"/><path d="m4 7.5 8 4.5 8-4.5M12 12v9"/>"#,
    ),
    ("chart", r#"<path d="M4 20V10M10 20V4M16 20v-7M22 20H2"/>"#),
    ("activity", r#"<path d="M3 12h4l3 8 4-16 3 8h4"/>"#),
    (
        "image",
        r#"<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m3 17 5-5 4 4 3-3 6 6"/>"#,
    ),
    (
        "key",
        r#"<circle cx="7" cy="15" r="4"/><path d="m10 12 8-8 3 3-3 3 2 2"/>"#,
    ),
    (
        "git-branch",
        r#"<path d="M6 3v12"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="6" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/>"#,
    ),
    (
        "git",
        r#"<circle cx="12" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="18" r="3"/><path d="M12 9v3M6 15v-3h12v3"/>"#,
    ),
    (
        "home-automation",
        r#"<path d="M3 10.5 12 3l9 7.5"/><path d="M5 9.5V21h14V9.5"/><circle cx="12" cy="14" r="2.5"/>"#,
    ),
    (
        "cloud-upload",
        r#"<path d="M17.5 19H7a5 5 0 0 1-.8-9.9A6 6 0 0 1 18 8a5.5 5.5 0 0 1-.5 11Z"/><path d="M12 18v-6M9.5 14.5 12 12l2.5 2.5"/>"#,
    ),
    (
        "database",
        r#"<ellipse cx="12" cy="6" rx="8" ry="3"/><path d="M4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6"/><path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3"/>"#,
    ),
    (
        "dns",
        r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.6 4 5.7 4 9s-1.5 6.4-4 9c-2.5-2.6-4-5.7-4-9s1.5-6.4 4-9Z"/>"#,
    ),
    (
        "monitor",
        r#"<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>"#,
    ),
    ("code", r#"<path d="m8 7-5 5 5 5M16 7l5 5-5 5"/>"#),
    (
        "mail",
        r#"<rect x="3" y="5" width="18" height="14" rx="2"/><path d="m3 7 9 6 9-6"/>"#,
    ),
];

/// Return the raw inner SVG path for a named icon, if it exists.
pub fn body(name: &str) -> Option<&'static str> {
    ICONS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, body)| *body)
}

/// Wrap an icon body in the standard 24x24 stroked SVG element.
pub fn svg(name: &str) -> Option<String> {
    body(name).map(|body| {
        format!(
            "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{body}</svg>"
        )
    })
}

/// First letters of a label, used as the last-resort letter tile.
pub fn initials(label: &str) -> String {
    let parts: Vec<&str> = label.split_whitespace().collect();
    match parts.as_slice() {
        [] => "?".to_string(),
        [one] => one
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".into()),
        [first, .., last] => {
            let a = first.chars().next().unwrap_or('?');
            let b = last.chars().next().unwrap_or('?');
            format!("{}{}", a.to_uppercase(), b.to_uppercase())
        }
    }
}
