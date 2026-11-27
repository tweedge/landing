//! Install, service lifecycle, and status commands.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{bail, Result};
use serde_json::Value;

use crate::build::{build, BuildOptions};

pub fn xdg_dir(env_var: &str, fallback: &str) -> PathBuf {
    match std::env::var(env_var) {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => home_dir().join(fallback),
    }
}

fn home_dir() -> PathBuf {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => PathBuf::from(home),
        _ => PathBuf::from("."),
    }
}

pub struct Paths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub unit_dir: PathBuf,
    pub site_dir: PathBuf,
    pub server_json: PathBuf,
    pub unit_path: PathBuf,
    pub unit_name: String,
}

impl Paths {
    pub fn new() -> Self {
        let data_dir = xdg_dir("XDG_DATA_HOME", ".local/share").join("landing");
        let config_dir = xdg_dir("XDG_CONFIG_HOME", ".config").join("landing");
        let state_dir = xdg_dir("XDG_STATE_HOME", ".local/state").join("landing");
        let unit_dir = xdg_dir("XDG_CONFIG_HOME", ".config").join("systemd/user");
        let site_dir = data_dir.join("site");
        let server_json = data_dir.join("server.json");
        let unit_name = "landing.service".to_string();
        let unit_path = unit_dir.join(&unit_name);
        Self {
            data_dir,
            config_dir,
            state_dir,
            unit_dir,
            site_dir,
            server_json,
            unit_path,
            unit_name,
        }
    }
}

impl Default for Paths {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Context {
    pub root: PathBuf,
    pub config: PathBuf,
    pub dist: PathBuf,
    pub paths: Paths,
}

impl Context {
    pub fn discover() -> Result<Self> {
        // The working directory is the project root when it contains the sample
        // config. Otherwise (a globally installed binary run from anywhere) the
        // project root is the current directory, and `init` writes here.
        let cwd = std::env::current_dir()?;
        let root = cwd;
        let paths = Paths::new();
        let config = if root.join("config/landing.yml").is_file() {
            root.join("config/landing.yml")
        } else {
            paths.config_dir.join("landing.yml")
        };
        let dist = root.join("dist");
        Ok(Self {
            root,
            config,
            dist,
            paths,
        })
    }
}

fn have_systemd() -> bool {
    Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn run_build(ctx: &Context, refresh_favicons: bool) -> Result<()> {
    build(BuildOptions {
        config_path: ctx.config.clone(),
        out_dir: Some(ctx.dist.clone()),
        single_file: None,
        quiet: false,
        refresh_favicons,
        root: ctx.root.clone(),
    })?;
    Ok(())
}

fn install_files(ctx: &Context) -> Result<Value> {
    if !ctx.dist.join("site/index.html").is_file() {
        bail!("build output missing; run `landing build`");
    }
    if let Some(parent) = ctx.paths.site_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    if ctx.paths.site_dir.exists() {
        fs::remove_dir_all(&ctx.paths.site_dir)?;
    }
    copy_tree(&ctx.dist.join("site"), &ctx.paths.site_dir)?;
    fs::copy(ctx.dist.join("server.json"), &ctx.paths.server_json)?;
    for name in ["config.resolved.json", "build-info.json"] {
        let src = ctx.dist.join(name);
        if src.is_file() {
            fs::copy(src, ctx.paths.data_dir.join(name))?;
        }
    }
    fs::create_dir_all(&ctx.paths.config_dir)?;
    if ctx.config.is_file() {
        fs::copy(&ctx.config, ctx.paths.config_dir.join("landing.yml"))?;
    }
    fs::create_dir_all(&ctx.paths.state_dir)?;
    let text = fs::read_to_string(&ctx.paths.server_json)?;
    Ok(serde_json::from_str(&text)?)
}

fn copy_tree(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dest.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &target)?;
        } else {
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

fn write_unit(ctx: &Context) -> Result<()> {
    fs::create_dir_all(&ctx.paths.unit_dir)?;
    let exe = std::env::current_exe()?;
    let unit = format!(
        "[Unit]\nDescription=Landing local start page\nAfter=network-online.target\n\n[Service]\nType=simple\nExecStart={} serve --root {} --config {}\nRestart=on-failure\nNoNewPrivileges=true\nPrivateTmp=true\n\n[Install]\nWantedBy=default.target\n",
        exe.display(),
        ctx.paths.site_dir.display(),
        ctx.paths.server_json.display()
    );
    fs::write(&ctx.paths.unit_path, unit)?;
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    Ok(())
}

fn start(ctx: &Context) -> Result<()> {
    if have_systemd() {
        write_unit(ctx)?;
        let status = Command::new("systemctl")
            .args(["--user", "enable", "--now", &ctx.paths.unit_name])
            .status()?;
        if !status.success() {
            bail!("failed to start {}", ctx.paths.unit_name);
        }
        return Ok(());
    }
    // Foreground fallback: spawn `serve` detached with a pid file.
    let pid_file = ctx.paths.state_dir.join("landing.pid");
    if let Ok(pid_text) = fs::read_to_string(&pid_file) {
        if let Ok(pid) = pid_text.trim().parse::<i32>() {
            if process_alive(pid) {
                println!("Landing is already running (foreground mode).");
                return Ok(());
            }
        }
    }
    fs::create_dir_all(&ctx.paths.state_dir)?;
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ctx.paths.state_dir.join("landing.log"))?;
    let err = log.try_clone()?;
    let exe = std::env::current_exe()?;
    let child = Command::new(exe)
        .args([
            "serve",
            "--root",
            &ctx.paths.site_dir.to_string_lossy(),
            "--config",
            &ctx.paths.server_json.to_string_lossy(),
        ])
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .spawn()?;
    fs::write(&pid_file, child.id().to_string())?;
    println!(
        "Started Landing in the background (pid {}); logs: {}",
        child.id(),
        ctx.paths.state_dir.join("landing.log").display()
    );
    Ok(())
}

fn process_alive(pid: i32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn stop(ctx: &Context) -> Result<()> {
    if have_systemd() && ctx.paths.unit_path.is_file() {
        let _ = Command::new("systemctl")
            .args(["--user", "stop", &ctx.paths.unit_name])
            .status();
        return Ok(());
    }
    let pid_file = ctx.paths.state_dir.join("landing.pid");
    if let Ok(pid_text) = fs::read_to_string(&pid_file) {
        if let Ok(pid) = pid_text.trim().parse::<i32>() {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
        let _ = fs::remove_file(&pid_file);
    } else {
        println!("Landing is not running.");
    }
    Ok(())
}

pub fn health(server: &Value, retries: u32) -> bool {
    let host = server
        .get("host")
        .and_then(|v| v.as_str())
        .unwrap_or("127.0.0.1");
    let port = server.get("port").and_then(|v| v.as_u64()).unwrap_or(8080);
    let path = server
        .get("health_path")
        .and_then(|v| v.as_str())
        .unwrap_or("/healthz");
    let url = format!("http://{host}:{port}{path}");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok();
    for _ in 0..retries {
        if let Some(client) = &client {
            if let Ok(response) = client.get(&url).send() {
                let code = response.status().as_u16();
                let body = response.text().unwrap_or_default();
                // Require a 2xx and a body that reports "ok", so a stale or
                // half-broken install cannot pass the check.
                if (200..300).contains(&code) {
                    let status_ok = serde_json::from_str::<Value>(&body)
                        .ok()
                        .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(|s| s == "ok"))
                        .unwrap_or(false);
                    if status_ok {
                        println!("Healthcheck OK: {url} -> {}", body.trim());
                        return true;
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    eprintln!("Healthcheck FAILED: {url}");
    false
}

fn read_server(ctx: &Context) -> Value {
    fs::read_to_string(&ctx.paths.server_json)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null)
}

/// Ensure the service is enabled and (re)started so it reloads server.json.
/// Unlike `start`, this always cycles an already-running service.
fn restart(ctx: &Context) -> Result<()> {
    if have_systemd() {
        write_unit(ctx)?;
        let _ = Command::new("systemctl")
            .args(["--user", "enable", &ctx.paths.unit_name])
            .status();
        let status = Command::new("systemctl")
            .args(["--user", "restart", &ctx.paths.unit_name])
            .status()?;
        if !status.success() {
            bail!("failed to restart {}", ctx.paths.unit_name);
        }
        return Ok(());
    }
    stop(ctx)?;
    start(ctx)
}

pub fn cmd_build(ctx: &Context, refresh_favicons: bool) -> Result<i32> {
    run_build(ctx, refresh_favicons)?;
    Ok(0)
}

pub fn cmd_install(ctx: &Context, refresh_favicons: bool) -> Result<i32> {
    run_build(ctx, refresh_favicons)?;
    let server = install_files(ctx)?;
    restart(ctx)?;
    if !health(&server, 20) {
        return Ok(1);
    }
    println!(
        "Landing installed and running at http://{}:{}/",
        server
            .get("host")
            .and_then(|v| v.as_str())
            .unwrap_or("127.0.0.1"),
        server.get("port").and_then(|v| v.as_u64()).unwrap_or(8080)
    );
    Ok(0)
}

pub fn cmd_start(ctx: &Context) -> Result<i32> {
    start(ctx)?;
    Ok(0)
}

pub fn cmd_stop(ctx: &Context) -> Result<i32> {
    stop(ctx)?;
    Ok(0)
}

pub fn cmd_restart(ctx: &Context, refresh_favicons: bool) -> Result<i32> {
    run_build(ctx, refresh_favicons)?;
    let server = install_files(ctx)?;
    restart(ctx)?;
    Ok(if health(&server, 20) { 0 } else { 1 })
}

pub fn cmd_status(ctx: &Context) -> Result<i32> {
    let server = read_server(ctx);
    let host = server
        .get("host")
        .and_then(|v| v.as_str())
        .unwrap_or("127.0.0.1");
    let port = server.get("port").and_then(|v| v.as_u64()).unwrap_or(8080);
    println!("Site:    {}", ctx.paths.site_dir.display());
    println!(
        "Config:  {}",
        ctx.paths.config_dir.join("landing.yml").display()
    );
    println!("URL:     http://{host}:{port}/");
    if have_systemd() && ctx.paths.unit_path.is_file() {
        let output = Command::new("systemctl")
            .args(["--user", "is-active", &ctx.paths.unit_name])
            .output();
        let state = output
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".into());
        println!("Service: {state} ({})", ctx.paths.unit_name);
    } else {
        let pid_file = ctx.paths.state_dir.join("landing.pid");
        let running = fs::read_to_string(pid_file)
            .ok()
            .and_then(|t| t.trim().parse::<i32>().ok())
            .map(process_alive)
            .unwrap_or(false);
        println!(
            "Service: {} (foreground mode)",
            if running { "running" } else { "stopped" }
        );
    }
    Ok(if health(&server, 1) { 0 } else { 1 })
}

pub fn cmd_health(ctx: &Context) -> Result<i32> {
    let server = read_server(ctx);
    Ok(if health(&server, 20) { 0 } else { 1 })
}

pub fn cmd_logs(ctx: &Context) -> Result<i32> {
    if have_systemd() && ctx.paths.unit_path.is_file() {
        let status = Command::new("journalctl")
            .args([
                "--user",
                "-u",
                &ctx.paths.unit_name,
                "-n",
                "100",
                "--no-pager",
            ])
            .status()?;
        return Ok(status.code().unwrap_or(0));
    }
    let log = ctx.paths.state_dir.join("landing.log");
    if log.is_file() {
        print!("{}", fs::read_to_string(log).unwrap_or_default());
        Ok(0)
    } else {
        eprintln!("No logs yet.");
        Ok(1)
    }
}

pub fn cmd_uninstall(ctx: &Context, purge: bool) -> Result<i32> {
    stop(ctx)?;
    if have_systemd() && ctx.paths.unit_path.is_file() {
        let _ = Command::new("systemctl")
            .args(["--user", "disable", &ctx.paths.unit_name])
            .status();
        let _ = fs::remove_file(&ctx.paths.unit_path);
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
    }
    let _ = fs::remove_dir_all(&ctx.paths.data_dir);
    let _ = fs::remove_dir_all(&ctx.paths.state_dir);
    if purge {
        let _ = fs::remove_dir_all(&ctx.paths.config_dir);
    }
    println!(
        "Landing uninstalled.{}",
        if purge { "" } else { " Configuration kept." }
    );
    Ok(0)
}

/// Write a starter config if one does not exist.
pub fn cmd_init(ctx: &Context, force: bool) -> Result<i32> {
    let target = ctx.root.join("config/landing.yml");
    if target.exists() && !force {
        println!("{} already exists; not overwriting.", target.display());
        return Ok(0);
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&target, crate::TEMPLATE_CONFIG)?;
    println!("Wrote {}", target.display());
    Ok(0)
}
