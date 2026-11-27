//! Landing: a local-first homelab start page.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use landing::{build, lifecycle, serve};

#[derive(Parser)]
#[command(name = "landing", version = env!("LANDING_VERSION"), about = "Local-first homelab start page")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Build the static site and single-file export.
    Build {
        #[arg(long)]
        refresh_favicons: bool,
    },
    /// Build, install to XDG paths, start, and healthcheck (default).
    Install {
        #[arg(long)]
        refresh_favicons: bool,
    },
    /// Start the installed service.
    Start,
    /// Stop the service.
    Stop,
    /// Rebuild, reinstall, and restart.
    Restart {
        #[arg(long)]
        refresh_favicons: bool,
    },
    /// Show service state and run a single healthcheck.
    Status,
    /// Request the health path; non-zero exit on failure.
    Health,
    /// Show recent logs.
    Logs,
    /// Stop and remove installed files (config kept unless --purge).
    Uninstall {
        #[arg(long)]
        purge: bool,
    },
    /// Write a starter config/landing.yml from the embedded example.
    Init {
        #[arg(long)]
        force: bool,
    },
    /// Build twice and compare output trees.
    CheckIdempotent,
    /// Run the local static server (used by the systemd unit).
    Serve {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        config: PathBuf,
    },
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::exit(2);
        }
    }
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    let ctx = lifecycle::Context::discover()?;
    match cli.command {
        None | Some(Command::Install { .. }) => {
            let refresh = matches!(
                cli.command,
                Some(Command::Install {
                    refresh_favicons: true
                })
            );
            lifecycle::cmd_install(&ctx, refresh)
        }
        Some(Command::Build { refresh_favicons }) => lifecycle::cmd_build(&ctx, refresh_favicons),
        Some(Command::Start) => lifecycle::cmd_start(&ctx),
        Some(Command::Stop) => lifecycle::cmd_stop(&ctx),
        Some(Command::Restart { refresh_favicons }) => {
            lifecycle::cmd_restart(&ctx, refresh_favicons)
        }
        Some(Command::Status) => lifecycle::cmd_status(&ctx),
        Some(Command::Health) => lifecycle::cmd_health(&ctx),
        Some(Command::Logs) => lifecycle::cmd_logs(&ctx),
        Some(Command::Uninstall { purge }) => lifecycle::cmd_uninstall(&ctx, purge),
        Some(Command::Init { force }) => lifecycle::cmd_init(&ctx, force),
        Some(Command::CheckIdempotent) => Ok(if build::check_idempotent(&ctx.config, &ctx.root)? {
            0
        } else {
            1
        }),
        Some(Command::Serve { root, config }) => {
            serve::serve(serve::ServeOptions {
                root,
                config_path: config,
            })?;
            Ok(0)
        }
    }
}
