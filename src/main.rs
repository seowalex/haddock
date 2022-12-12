#![warn(clippy::pedantic)]

mod commands;
mod compose;
mod config;

use anyhow::Result;
use clap::Parser;

use commands::Command;
use config::Config;

#[derive(Parser, Debug)]
#[command(version, about, next_display_order = None)]
struct Args {
    #[command(subcommand)]
    command: Command,

    #[command(flatten)]
    config: Config,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let config = config::load(&args.config)?;

    commands::run(args.command, config)?;

    Ok(())
}
