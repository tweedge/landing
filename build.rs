use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=LANDING_VERSION_OVERRIDE");
    println!("cargo:rerun-if-changed=build.rs");
    let version = env::var("LANDING_VERSION_OVERRIDE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").unwrap_or_default());
    println!("cargo:rustc-env=LANDING_VERSION={version}");
}
