mod commands;
mod compose;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, next_display_order = None)]
struct Args {
    #[command(subcommand)]
    command: commands::Command,

    /// Compose configuration files
    #[arg(short, long)]
    file: Option<Vec<String>>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    commands::run(args.command, args.file)?;

    Ok(())
}
