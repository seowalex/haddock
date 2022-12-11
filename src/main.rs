mod commands;
mod compose;
mod config;

use anyhow::Result;
use clap::Parser;

use config::Config;

#[derive(Parser, Debug)]
#[command(version, about, next_display_order = None)]
struct Args {
    #[command(subcommand)]
    command: commands::Command,

    #[command(flatten)]
    config: Config,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let config = config::parse(&args.config)?;

    println!("{:#?}", config);

    commands::run(args.command, config)?;

    Ok(())
}
