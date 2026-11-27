//! Theme CSS generation, mirroring `scripts/build.py::theme_css`.

use crate::config::Config;

/// Parse `#rgb` or `#rrggbb` into an RGB tuple.
pub fn hex_to_rgb(value: &str) -> Option<(u8, u8, u8)> {
    let value = value.trim();
    let expanded;
    let value = if value.len() == 4 && value.starts_with('#') {
        expanded = format!(
            "#{}{}{}{}{}{}",
            &value[1..2],
            &value[1..2],
            &value[2..3],
            &value[2..3],
            &value[3..4],
            &value[3..4]
        );
        expanded.as_str()
    } else {
        value
    };
    if value.len() != 7 || !value.starts_with('#') {
        return None;
    }
    let r = u8::from_str_radix(&value[1..3], 16).ok()?;
    let g = u8::from_str_radix(&value[3..5], 16).ok()?;
    let b = u8::from_str_radix(&value[5..7], 16).ok()?;
    Some((r, g, b))
}

/// Pick a readable text color for a given background.
pub fn contrast_color(value: &str) -> String {
    match hex_to_rgb(value) {
        None => "#ffffff".into(),
        Some((r, g, b)) => {
            let luminance = (0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64) / 255.0;
            if luminance > 0.55 {
                "#08111f".into()
            } else {
                "#ffffff".into()
            }
        }
    }
}

fn fmt_float(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

const SHADOWS_LIGHT: &str = "--shadow-sm: 0 1px 2px rgba(74,45,32,.06), 0 1px 1px rgba(74,45,32,.03);--shadow-md: 0 10px 24px rgba(74,45,32,.10), 0 2px 6px rgba(74,45,32,.05);--shadow-lg: 0 24px 60px rgba(74,45,32,.18), 0 8px 20px rgba(74,45,32,.08);";
const SHADOWS_DARK: &str = "--shadow-sm: 0 1px 2px rgba(0,0,0,.35);--shadow-md: 0 12px 28px rgba(0,0,0,.4);--shadow-lg: 0 30px 70px rgba(0,0,0,.55);";

fn block(p: &crate::config::Palette, shadows: &str, accent_fallback: &str) -> String {
    format!(
        "--bg: {};--surface: {};--text: {};--muted: {};--border: {};--accent: {};--accent2: {};--success: {};--warning: {};--danger: {};--accent-contrast: {};{}",
        p.bg,
        p.surface,
        p.text,
        p.muted,
        p.border,
        p.accent,
        p.accent2,
        p.success,
        p.warning,
        p.danger,
        contrast_color(if p.accent.is_empty() { accent_fallback } else { &p.accent }),
        shadows
    )
}

/// Build the theme + group CSS block embedded ahead of `main.css`.
pub fn theme_css(config: &Config, background_url: Option<&str>) -> String {
    let theme = &config.theme;
    let layout = &config.layout;
    let accent = if theme.accent.is_empty() {
        theme.colors.dark.accent.clone()
    } else {
        theme.accent.clone()
    };
    let space = match theme.density.as_str() {
        "compact" => "12px",
        "spacious" => "20px",
        _ => "16px",
    };
    let font = if theme.font.is_empty() || theme.font == "system" {
        "system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif"
    } else {
        theme.font.as_str()
    };
    let bg_image = match background_url {
        Some(url) => format!("url(\"{url}\")"),
        None => "none".into(),
    };
    let bg = &theme.background;

    let base = format!(
        ":root{{{}--accent-ring: color-mix(in srgb, var(--accent) 22%, transparent);--surface-hover: color-mix(in srgb, var(--surface) 88%, var(--text) 12%);--border-strong: color-mix(in srgb, var(--border) 60%, var(--text) 40%);--tag-bg: color-mix(in srgb, var(--text) 7%, transparent);--state-bg: color-mix(in srgb, var(--text) 6%, transparent);--radius: {}px;--columns: {};--max-width: {}px;--groups-max-width: {}px;--space: {};--font: {};--bg-image: {};--bg-overlay: {};--bg-blur: {}px;--bg-brightness: {};--bg-position: {};--bg-size: {};}}",
        block(&theme.colors.light, SHADOWS_LIGHT, &accent),
        theme.radius,
        layout.columns,
        layout.max_width,
        layout.groups_max_width,
        space,
        font,
        bg_image,
        if bg_image != "none" && !bg.overlay.is_empty() { bg.overlay.as_str() } else { "transparent" },
        bg.blur,
        fmt_float(bg.brightness),
        bg.position,
        bg.size,
    );
    let dark_block = format!(
        "[data-theme=\"dark\"]{{{}}}",
        block(&theme.colors.dark, SHADOWS_DARK, &accent)
    );
    let auto_block = format!(
        "@media (prefers-color-scheme: dark){{[data-theme=\"auto\"]{{{}}}}}",
        block(&theme.colors.dark, SHADOWS_DARK, &accent)
    );

    let mut group_styles = Vec::new();
    for group in &config.groups {
        let color = group.color.clone().unwrap_or_else(|| accent.clone());
        group_styles.push(format!(
            "[data-group-id=\"{}\"]{{--group-accent:{color};}}",
            group.id
        ));
        for link in &group.links {
            group_styles.push(format!(
                "[data-link-id=\"{}\"]{{--card-accent:{color};}}",
                link.id
            ));
            if !link.icon_background.is_empty() {
                group_styles.push(format!(
                    "[data-link-id=\"{}\"] .card-icon{{background:{};}}",
                    link.id, link.icon_background
                ));
            }
        }
    }

    [base, dark_block, auto_block, group_styles.join("\n")].join("\n")
}
