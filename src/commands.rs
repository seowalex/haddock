automod::dir!("src/commands");

use anyhow::Result;
use clap::Subcommand;

use crate::{compose, config::Config, podman::Podman};

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    #[command(flatten)]
    ExtCommand(ExtCommand),

    Convert(convert::Args),
    Version(version::Args),
}

#[derive(Subcommand, Debug)]
pub(crate) enum ExtCommand {
    Down(down::Args), //Includes one-offs if --remove-orphans
    Create(create::Args),
    Rm(rm::Args),
    Start(start::Args),
    Stop(stop::Args),
    Restart(restart::Args),
    Kill(kill::Args), // Includes one-offs
    Pause(pause::Args),
    Unpause(unpause::Args),
    Run(run::Args),
    Exec(exec::Args),
    Cp(cp::Args),
    Events(events::Args),
    Logs(logs::Args),
    Ps(ps::Args),         // Includes one-offs if --all
    Top(top::Args),       // Includes one-offs
    Images(images::Args), // Includes one-offs
    Port(port::Args),
    Ls(ls::Args),
}

pub(crate) async fn run(command: Command, config: Config) -> Result<()> {
    match command {
        Command::ExtCommand(command) => {
            let podman = Podman::new(&config).await?;
            let file = compose::parse(&config, false)?;

            match command {
                ExtCommand::Down(args) => down::run(args, &podman, &file, &config).await,
                ExtCommand::Create(args) => create::run(args, &podman, &file, &config).await,
                ExtCommand::Rm(args) => rm::run(args, &podman, &file, &config).await,
                ExtCommand::Start(args) => start::run(args, &podman, &file, &config).await,
                ExtCommand::Stop(args) => stop::run(args, &podman, &file, &config).await,
                ExtCommand::Restart(args) => restart::run(args, &podman, &file, &config).await,
                ExtCommand::Kill(args) => kill::run(args, &podman, &file, &config).await,
                ExtCommand::Pause(args) => pause::run(args, &podman, &file, &config).await,
                ExtCommand::Unpause(args) => unpause::run(args, &podman, &file, &config).await,
                ExtCommand::Run(args) => run::run(args, &podman, &file, &config).await,
                ExtCommand::Exec(args) => exec::run(args, &podman, &file).await,
                ExtCommand::Cp(args) => cp::run(args, &podman, &file).await,
                ExtCommand::Events(args) => events::run(args, &podman, &file).await,
                ExtCommand::Logs(args) => logs::run(args, &podman, &file).await,
                ExtCommand::Ps(args) => ps::run(args, &podman, &file).await,
                ExtCommand::Top(args) => top::run(args, &podman, &file).await,
                ExtCommand::Images(args) => images::run(args, &podman, &file).await,
                ExtCommand::Port(args) => port::run(args, &podman, &file).await,
                ExtCommand::Ls(args) => ls::run(args, &podman).await,
            }?
        }
        Command::Convert(args) => convert::run(args, &config)?,
        Command::Version(args) => version::run(args),
    }

    Ok(())
}
