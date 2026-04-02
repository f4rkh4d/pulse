use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "pulse", version, about)]
struct Cli {}

fn main() -> anyhow::Result<()> {
    let _ = Cli::parse();
    Ok(())
}
