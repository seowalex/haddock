mod convert;
mod version;

use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Create and start containers
    Up,
    /// Stop and remove containers, networks
    Down,
    Convert(convert::Args),
    Version(version::Args),
}

pub(crate) fn run(command: Command, paths: Option<Vec<String>>) -> Result<()> {
    match command {
        Command::Up => todo!(),
        Command::Down => todo!(),
        Command::Convert(args) => convert::run(args, paths),
        Command::Version(args) => version::run(args),
    }?;

    Ok(())
}
