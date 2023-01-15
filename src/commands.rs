automod::dir!("src/commands");

use anyhow::Result;
use clap::Subcommand;

use crate::config::Config;

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    Up(up::Args),
    Down(down::Args),
    Create(create::Args),
    Rm(rm::Args),
    Start(start::Args),
    Stop(stop::Args),
    Restart(restart::Args),
    Kill(kill::Args),
    Pause(pause::Args),
    Unpause(unpause::Args),
    Cp(cp::Args),
    Events(events::Args),
    Top(top::Args),
    Ps(ps::Args),
    Images(images::Args),
    Ls(ls::Args),
    Port(port::Args),
    Convert(convert::Args),
    Version(version::Args),
}

pub(crate) async fn run(command: Command, config: Config) -> Result<()> {
    match command {
        Command::Up(args) => up::run(args, config).await?,
        Command::Down(args) => down::run(args, config).await?,
        Command::Create(args) => create::run(args, config).await?,
        Command::Rm(args) => rm::run(args, config).await?,
        Command::Start(args) => start::run(args, config).await?,
        Command::Stop(args) => stop::run(args, config).await?,
        Command::Restart(args) => restart::run(args, config).await?,
        Command::Kill(args) => kill::run(args, config).await?,
        Command::Pause(args) => pause::run(args, config).await?,
        Command::Unpause(args) => unpause::run(args, config).await?,
        Command::Cp(args) => cp::run(args, config).await?,
        Command::Events(args) => events::run(args, config).await?,
        Command::Top(args) => top::run(args, config).await?,
        Command::Ps(args) => ps::run(args, config).await?,
        Command::Images(args) => images::run(args, config).await?,
        Command::Ls(args) => ls::run(args, config).await?,
        Command::Port(args) => port::run(args, config).await?,
        Command::Convert(args) => convert::run(args, config)?,
        Command::Version(args) => version::run(args),
    };

    Ok(())
}
