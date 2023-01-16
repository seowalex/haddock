automod::dir!("src/commands");

use anyhow::Result;
use clap::Subcommand;

use crate::{compose, config::Config, podman::Podman};

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    Down(down::Args),
    Create(create::Args),
    Rm(rm::Args),
    Start(start::Args),
    Stop(stop::Args),
    Restart(restart::Args),
    Kill(kill::Args),
    Pause(pause::Args),
    Unpause(unpause::Args),
    Exec(exec::Args),
    Cp(cp::Args),
    Events(events::Args),
    Logs(logs::Args),
    Top(top::Args),
    Ps(ps::Args),
    Images(images::Args),
    Port(port::Args),
    Ls(ls::Args),
    Convert(convert::Args),
    Version(version::Args),
}

pub(crate) async fn run(command: Command, config: Config) -> Result<()> {
    match command {
        Command::Convert(args) => convert::run(args, &config)?,
        Command::Version(args) => version::run(args),

        command => {
            let podman = Podman::new(&config).await?;
            let file = compose::parse(&config, false)?;

            match command {
                Command::Down(args) => down::run(args, &podman, &file, &config).await,
                Command::Create(args) => create::run(args, &podman, &file, &config).await,
                Command::Rm(args) => rm::run(args, &podman, &file, &config).await,
                Command::Start(args) => start::run(args, &podman, &file, &config).await,
                Command::Stop(args) => stop::run(args, &podman, &file, &config).await,
                Command::Restart(args) => restart::run(args, &podman, &file, &config).await,
                Command::Kill(args) => kill::run(args, &podman, &file, &config).await,
                Command::Pause(args) => pause::run(args, &podman, &file, &config).await,
                Command::Unpause(args) => unpause::run(args, &podman, &file, &config).await,
                Command::Exec(args) => exec::run(args, &podman, &file).await,
                Command::Cp(args) => cp::run(args, &podman, &file).await,
                Command::Events(args) => events::run(args, &podman, &file).await,
                Command::Logs(args) => logs::run(args, &podman, &file).await,
                Command::Top(args) => top::run(args, &podman, &file).await,
                Command::Ps(args) => ps::run(args, &podman, &file).await,
                Command::Images(args) => images::run(args, &podman, &file).await,
                Command::Port(args) => port::run(args, &podman, &file).await,
                Command::Ls(args) => ls::run(args, &podman).await,
                _ => unreachable!(),
            }?
        }
    }

    Ok(())
}
