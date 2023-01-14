mod convert;
mod cp;
mod down;
mod events;
mod images;
mod kill;
mod pause;
mod ps;
mod rm;
mod stop;
mod top;
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
    Kill(kill::Args),
    Pause(pause::Args),
    Unpause(unpause::Args),
    Cp(cp::Args),
    Events(events::Args),
    Top(top::Args),
    Ps(ps::Args),
    Images(images::Args),
    Convert(convert::Args),
    Version(version::Args),
}

pub(crate) async fn run(command: Command, config: Config) -> Result<()> {
    match command {
        Command::Up(args) => up::run(args, config).await?,
        Command::Down(args) => down::run(args, config).await?,
        Command::Rm(args) => rm::run(args, config).await?,
        Command::Stop(args) => stop::run(args, config).await?,
        Command::Kill(args) => kill::run(args, config).await?,
        Command::Pause(args) => pause::run(args, config).await?,
        Command::Unpause(args) => unpause::run(args, config).await?,
        Command::Cp(args) => cp::run(args, config).await?,
        Command::Events(args) => events::run(args, config).await?,
        Command::Top(args) => top::run(args, config).await?,
        Command::Ps(args) => ps::run(args, config).await?,
        Command::Images(args) => images::run(args, config).await?,
        Command::Convert(args) => convert::run(args, config)?,
        Command::Version(args) => version::run(args),
    };

    Ok(())
}
