mod convert;
mod version;

use anyhow::Result;
use clap::Subcommand;
use docker_compose_types::Compose;

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Create and start containers
    Up,
    /// Stop and remove containers, networks
    Down,
    Convert(convert::Args),
    Version(version::Args),
}

pub(crate) fn run(command: Command, file: Compose) -> Result<()> {
    match command {
        Command::Up => todo!(),
        Command::Down => todo!(),
        Command::Convert(args) => convert::run(args, file),
        Command::Version(args) => version::run(args),
    }?;

    Ok(())
}
