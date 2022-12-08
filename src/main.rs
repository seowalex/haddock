mod commands;
mod compose;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, next_display_order = None)]
struct Args {
    #[command(subcommand)]
    command: commands::Command,
    #[command(flatten)]
    flags: Flags,
}

#[derive(clap::Args, Debug)]
struct Flags {
    /// Project name
    #[arg(short, long)]
    project_name: Option<String>,
    /// Compose configuration files
    #[arg(short, long)]
    file: Option<Vec<String>>,
    /// Specify a profile to enable
    #[arg(long)]
    profile: Option<Vec<String>>,
    /// Specify an alternate environment file
    #[arg(long)]
    env_file: Option<String>,
    /// Specify an alternate working directory
    #[arg(long)]
    project_directory: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    commands::run(args.command, args.flags)?;

    Ok(())
}
