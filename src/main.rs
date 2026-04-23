use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

/// one terminal window for all your local dev servers.
#[derive(Parser, Debug)]
#[command(
    name = "pulse",
    version,
    about = "one terminal window for all your local dev servers",
    long_about = None,
)]
struct Cli {
    /// path to pulse.toml
    #[arg(short, long, default_value = "pulse.toml")]
    config: PathBuf,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// scan current dir and draft a pulse.toml
    Init {
        /// overwrite existing pulse.toml if present
        #[arg(long)]
        force: bool,
    },
    /// list processes currently listening on tcp ports
    Ports,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("PULSE_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let cli = Cli::parse();

    match cli.cmd {
        Some(Cmd::Init { force }) => run_init(force),
        Some(Cmd::Ports) => run_ports(),
        None => {
            let cfg = pulse::config::load(&cli.config)
                .with_context(|| format!("failed to read config at {}", cli.config.display()))?;
            pulse::app::run_with_path(cfg, Some(cli.config)).await
        }
    }
}

fn run_init(force: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let target = cwd.join("pulse.toml");
    if target.exists() && !force {
        anyhow::bail!("pulse.toml already exists. pass --force to overwrite");
    }
    let sugg = pulse::discover::scan(&cwd);
    let draft = pulse::discover::render_draft(&sugg);
    std::fs::write(&target, draft)?;
    if sugg.is_empty() {
        println!("nothing detected. wrote a placeholder pulse.toml anyway");
    } else {
        println!("wrote pulse.toml with {} suggested service(s):", sugg.len());
        for s in &sugg {
            println!("  {:<16}  {}  (from {})", s.name, s.cmd, s.source);
        }
        println!("\nreview, tweak, then run `pulse`");
    }
    Ok(())
}

fn run_ports() -> Result<()> {
    let list = pulse::ports::listeners();
    if list.is_empty() {
        println!("no LISTEN sockets found (or lsof unavailable)");
        return Ok(());
    }
    println!("{:<6} {:<18} PID", "PORT", "COMMAND");
    for e in list {
        println!("{:<6} {:<18} {}", e.port, e.command, e.pid);
    }
    Ok(())
}
