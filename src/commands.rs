mod convert;
mod version;

use anyhow::Result;
use clap::Subcommand;

use crate::config::Config;

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    Convert(convert::Args),
    Version(version::Args),
}

pub(crate) fn run(command: Command, config: Config) -> Result<()> {
    match command {
        Command::Convert(args) => convert::run(args, config),
        Command::Version(args) => version::run(args),
    }?;

    Ok(())
}
