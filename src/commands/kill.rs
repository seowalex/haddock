use anyhow::Result;
use futures::{stream::FuturesUnordered, TryStreamExt};
use itertools::Itertools;

use crate::{
    compose::types::Compose,
    config::Config,
    podman::{types::Container, Podman},
    progress::{Finish, Progress},
};

/// Force stop service containers
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    services: Vec<String>,

    /// Remove containers for services not defined in the Compose file
    #[arg(long)]
    remove_orphans: bool,

    /// SIGNAL to send to the container
    #[arg(short, long, default_value_t = String::from("SIGKILL"))]
    signal: String,
}

async fn kill_containers(
    podman: &Podman,
    progress: &Progress,
    containers: &[String],
    args: Args,
) -> Result<()> {
    let args = &args;

    containers
        .iter()
        .map(|container| async move {
            let spinner = progress.add_spinner(format!("Container {container}"), "Killing");

            podman
                .run(["kill", "--signal", &args.signal, container])
                .await
                .finish_with_message(spinner, "Killed")
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await
        .map(|_| ())
}

pub(crate) async fn run(
    args: Args,
    podman: &Podman,
    file: &Compose,
    config: &Config,
) -> Result<()> {
    let output = podman
        .force_run([
            "ps",
            "--all",
            "--format",
            "json",
            "--filter",
            "status=running",
            "--filter",
            &format!("pod={}", file.name.as_ref().unwrap()),
        ])
        .await?;
    let containers = serde_json::from_str::<Vec<Container>>(&output)?
        .into_iter()
        .filter_map(|mut container| {
            container
                .labels
                .and_then(|labels| labels.service)
                .and_then(|service| {
                    if args.remove_orphans
                        || args.services.contains(&service)
                        || (args.services.is_empty() && file.services.keys().contains(&service))
                    {
                        container.names.pop_front()
                    } else {
                        None
                    }
                })
        })
        .collect::<Vec<_>>();

    if !containers.is_empty() {
        let progress = Progress::new(config);

        kill_containers(podman, &progress, &containers, args).await?;

        progress.finish();
    }

    Ok(())
}
