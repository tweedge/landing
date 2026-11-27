//! Landing library: build engine, config, server, and lifecycle.

pub mod build;
pub mod config;
pub mod csp;
pub mod favicon;
pub mod icons;
pub mod lifecycle;
pub mod pwa;
pub mod serve;
pub mod templates;
pub mod theme;

/// The tracked example config, embedded so `landing init` can write it out
/// and so the binary always has a template without external files.
pub const TEMPLATE_CONFIG: &str = include_str!("../config/landing.example.yml");
