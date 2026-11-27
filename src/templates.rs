//! Template rendering via minijinja, using the embedded Jinja2 templates.

use std::collections::BTreeMap;

use anyhow::Result;
use minijinja::value::Value as MjValue;
use minijinja::{syntax::SyntaxConfig, Environment, Output, State};
use serde_json::Value;

pub const INDEX_TEMPLATE: &str = include_str!("../src/templates/index.html.j2");
pub const REDIRECT_TEMPLATE: &str = include_str!("../src/templates/redirect.html.j2");
pub const NOT_FOUND_TEMPLATE: &str = include_str!("../src/templates/404.html.j2");

pub const MAIN_CSS: &str = include_str!("../src/styles/main.css");
pub const CORE_JS: &str = include_str!("../src/js/core.js");
pub const ICONS_JS: &str = include_str!("../src/js/icons.js");
pub const SITE_JS: &str = include_str!("../src/js/site.js");
pub const REDIRECT_JS: &str = include_str!("../src/js/redirect.js");
pub const SW_JS: &str = include_str!("../src/js/sw.js");

/// Convert a serde_json value into a minijinja value.
fn to_mj(value: &Value) -> minijinja::Value {
    match value {
        Value::Null => minijinja::Value::from(()),
        Value::Bool(b) => minijinja::Value::from(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                minijinja::Value::from(i)
            } else if let Some(f) = n.as_f64() {
                minijinja::Value::from(f)
            } else {
                minijinja::Value::from(n.to_string())
            }
        }
        Value::String(s) => minijinja::Value::from(s.clone()),
        Value::Array(items) => minijinja::Value::from(items.iter().map(to_mj).collect::<Vec<_>>()),
        Value::Object(map) => {
            let mut obj = std::collections::BTreeMap::new();
            for (key, val) in map {
                obj.insert(key.clone(), to_mj(val));
            }
            minijinja::Value::from(obj)
        }
    }
}

fn env() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_syntax(
        SyntaxConfig::builder()
            .trim_blocks(true)
            .lstrip_blocks(true)
            .build()
            .unwrap(),
    );
    // Jinja2/MarkupSafe escapes only & < > ' " and leaves "/" alone; minijinja's
    // default also escapes "/" and uses &#x27;. Match Jinja2 byte-for-byte so
    // built output and CSP hashes stay identical to the reference build.
    env.set_formatter(|out: &mut Output, state: &mut State, value: &MjValue| {
        if matches!(state.auto_escape(), minijinja::AutoEscape::None) || value.is_safe() {
            out.write_str(&value.to_string())?;
            return Ok(());
        }
        let mut escaped = String::new();
        for ch in value.to_string().chars() {
            match ch {
                '&' => escaped.push_str("&amp;"),
                '<' => escaped.push_str("&lt;"),
                '>' => escaped.push_str("&gt;"),
                '"' => escaped.push_str("&#34;"),
                '\'' => escaped.push_str("&#39;"),
                other => escaped.push(other),
            }
        }
        out.write_str(&escaped)?;
        Ok(())
    });
    env.add_template("index.html.j2", INDEX_TEMPLATE).unwrap();
    env.add_template("redirect.html.j2", REDIRECT_TEMPLATE)
        .unwrap();
    env.add_template("404.html.j2", NOT_FOUND_TEMPLATE).unwrap();
    env
}

/// Render the main page. `ctx` must supply the variables the template expects.
pub fn render_index(ctx: Value) -> Result<String> {
    let env = env();
    let tmpl = env.get_template("index.html.j2")?;
    Ok(tmpl.render(to_mj(&ctx))?)
}

pub fn render_redirect(ctx: Value) -> Result<String> {
    let env = env();
    let tmpl = env.get_template("redirect.html.j2")?;
    Ok(tmpl.render(to_mj(&ctx))?)
}

pub fn render_404(ctx: Value) -> Result<String> {
    let env = env();
    let tmpl = env.get_template("404.html.j2")?;
    Ok(tmpl.render(to_mj(&ctx))?)
}

/// Helpers exposed to templates are not needed because the build precomputes
/// `icon_html`; this re-export keeps the module cohesive for future use.
#[allow(dead_code)]
pub fn empty_context() -> BTreeMap<String, Value> {
    BTreeMap::new()
}
