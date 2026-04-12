use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

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
    let cfg = pulse::config::load(&cli.config)
        .with_context(|| format!("failed to read config at {}", cli.config.display()))?;

    pulse::app::run(cfg).await
}
