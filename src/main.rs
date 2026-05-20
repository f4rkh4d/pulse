use std::path::PathBuf;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};

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

    /// skip the startup banner
    #[arg(long)]
    quiet: bool,

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
    /// write a self-contained html snapshot of the current stack
    Share {
        /// output path. defaults to `pulse-snapshot-<ts>.html`
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// tail a single service's logs without launching the tui
    Logs {
        /// service name as declared in pulse.toml
        service: String,
        /// number of lines to show (then exit); 0 = follow forever
        #[arg(short, long, default_value_t = 0)]
        lines: usize,
    },
    /// theme utilities
    Theme {
        #[command(subcommand)]
        cmd: ThemeCmd,
    },
    /// print a shell completion script to stdout
    Completions {
        /// shell to generate for (bash, zsh, fish, powershell, elvish)
        shell: clap_complete::Shell,
    },
    /// drop into a subshell with a service's env, cwd
    Shell {
        /// service name as declared in pulse.toml
        service: String,
    },
}

#[derive(Subcommand, Debug)]
enum ThemeCmd {
    /// print the current defaults as a starter theme.toml
    Dump,
    /// show the resolved theme file path (if platform supports it)
    Path,
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
        Some(Cmd::Share { out }) => run_share(&cli.config, out),
        Some(Cmd::Logs { service, lines }) => run_logs(&cli.config, &service, lines).await,
        Some(Cmd::Theme { cmd }) => run_theme(cmd),
        Some(Cmd::Completions { shell }) => run_completions(shell),
        Some(Cmd::Shell { service }) => run_shell(&cli.config, &service),
        None => {
            if !cli.quiet {
                eprint!("{}", pulse::banner(env!("CARGO_PKG_VERSION")));
            }
            let cfg = pulse::config::load(&cli.config)
                .map_err(|e| pulse::errors::explain(e, &cli.config))?;
            pulse::app::run_with_path(cfg, Some(cli.config)).await
        }
    }
}

fn run_completions(shell: clap_complete::Shell) -> Result<()> {
    let mut cmd = Cli::command();
    let bin = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, bin, &mut std::io::stdout());
    Ok(())
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

fn run_share(cfg_path: &std::path::Path, out: Option<PathBuf>) -> Result<()> {
    let out_path = match out {
        Some(p) => p,
        None => {
            let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
            PathBuf::from(format!("pulse-snapshot-{ts}.html"))
        }
    };
    // try live: dial the most recent TUI's unix socket, read html back.
    if let Some(sock) = pulse::ipc::find_latest_socket_with_fallback() {
        if let Ok(html) = read_from_ipc(&sock) {
            std::fs::write(&out_path, html)?;
            println!("wrote {} (live state from running tui)", out_path.display());
            return Ok(());
        }
    }
    // fallback: configured-only snapshot.
    let cfg = pulse::config::load(cfg_path).map_err(|e| pulse::errors::explain(e, cfg_path))?;
    let services: Vec<pulse::service::Service> = cfg
        .services
        .into_iter()
        .map(pulse::service::Service::new)
        .collect();
    let tap_rings: Vec<Option<pulse::tap::SharedRing>> = services.iter().map(|_| None).collect();
    let snap = pulse::share::collect(&services, &tap_rings);
    let html = pulse::share::render(&snap);
    std::fs::write(&out_path, html)?;
    println!(
        "wrote {} (configured-only; no running tui found)",
        out_path.display()
    );
    Ok(())
}

fn read_from_ipc(path: &std::path::Path) -> std::io::Result<String> {
    use std::io::Read;
    use std::os::unix::net::UnixStream;
    let mut sock = UnixStream::connect(path)?;
    sock.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    let mut buf = String::new();
    sock.read_to_string(&mut buf)?;
    Ok(buf)
}

fn run_shell(cfg_path: &std::path::Path, service: &str) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let cfg = pulse::config::load(cfg_path).map_err(|e| pulse::errors::explain(e, cfg_path))?;
    let parent_env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let plan = pulse::shellcmd::plan(&cfg, service, &parent_env).map_err(anyhow::Error::msg)?;

    let shell_str = plan.shell.to_string_lossy().into_owned();
    let is_zsh = shell_str.ends_with("/zsh") || shell_str.ends_with("zsh");
    let is_bashlike =
        shell_str.ends_with("/bash") || shell_str.ends_with("bash") || shell_str.ends_with("sh");

    let mut cmd = std::process::Command::new(&plan.shell);
    cmd.current_dir(&plan.cwd);
    for (k, v) in &plan.env {
        cmd.env(k, v);
    }
    if is_zsh {
        let dir = pulse::shellcmd::prepare_zshrc(&plan.ps1)?;
        cmd.env("ZDOTDIR", &dir);
    } else if is_bashlike {
        cmd.env("PS1", &plan.ps1);
    }
    println!(
        "[pulse:{service}] dropping into {shell_str} in {}",
        plan.cwd.display()
    );
    // replace this process with the shell; errors only if execve itself fails
    let fail = cmd.exec();
    Err(anyhow::anyhow!("replacing process failed: {fail}"))
}

async fn run_logs(cfg_path: &std::path::Path, service: &str, lines: usize) -> Result<()> {
    let cfg = pulse::config::load(cfg_path).map_err(|e| pulse::errors::explain(e, cfg_path))?;
    let spec = cfg
        .services
        .iter()
        .find(|s| s.name == service)
        .ok_or_else(|| anyhow::anyhow!("no service named `{service}` in config"))?
        .clone();

    use tokio::sync::mpsc;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sc = pulse::supervisor::spawn_one(0, &spec, tx.clone()).await?;
    tokio::spawn(sc.watch(0, tx.clone()));

    let mut shown = 0usize;
    while let Some(ev) = rx.recv().await {
        if let pulse::supervisor::SupEvent::Log { line, origin, .. } = ev {
            let mark = match origin {
                pulse::service::Origin::Stderr => "!",
                pulse::service::Origin::System => "·",
                pulse::service::Origin::Stdout => " ",
            };
            println!("{mark} {line}");
            shown += 1;
            if lines > 0 && shown >= lines {
                return Ok(());
            }
        } else if let pulse::supervisor::SupEvent::Exited { code, .. } = ev {
            if let Some(c) = code {
                if c != 0 {
                    std::process::exit(c);
                }
            }
            return Ok(());
        }
    }
    Ok(())
}

fn run_theme(cmd: ThemeCmd) -> Result<()> {
    match cmd {
        ThemeCmd::Dump => {
            print!("{}", pulse::theme_file::dump_default());
        }
        ThemeCmd::Path => match pulse::theme_file::config_path() {
            Some(p) => println!("{}", p.display()),
            None => println!("(no config dir on this platform)"),
        },
    }
    Ok(())
}
