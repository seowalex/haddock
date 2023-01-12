mod convert;
mod down;
mod pause;
mod rm;
mod stop;
mod unpause;
mod up;
mod version;

use anyhow::Result;
use clap::Subcommand;

use crate::config::Config;

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    Up(up::Args),
    Down(down::Args),
    Rm(rm::Args),
    Stop(stop::Args),
    Pause(pause::Args),
    Unpause(unpause::Args),
    Convert(convert::Args),
    Version(version::Args),
}

pub(crate) async fn run(command: Command, config: Config) -> Result<()> {
    match command {
        Command::Up(args) => up::run(args, config).await?,
        Command::Down(args) => down::run(args, config).await?,
        Command::Rm(args) => rm::run(args, config).await?,
        Command::Stop(args) => stop::run(args, config).await?,
        Command::Pause(args) => pause::run(args, config).await?,
        Command::Unpause(args) => unpause::run(args, config).await?,
        Command::Convert(args) => convert::run(args, config)?,
        Command::Version(args) => version::run(args),
    };

    Ok(())
}
