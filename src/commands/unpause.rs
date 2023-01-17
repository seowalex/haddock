use anyhow::Result;
use futures::{stream::FuturesUnordered, TryStreamExt};
use itertools::Itertools;

use crate::{
    compose::types::Compose,
    config::Config,
    podman::{types::Container, Podman},
    progress::{Finish, Progress},
};

/// Unpause services
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    services: Vec<String>,
}

async fn unpause_containers(
    podman: &Podman,
    progress: &Progress,
    containers: &[String],
) -> Result<()> {
    containers
        .iter()
        .map(|container| async move {
            let spinner = progress.add_spinner(format!("Container {container}"), "Unpausing");

            podman
                .run(["unpause", container])
                .await
                .finish_with_message(spinner, "Unpaused")
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
            "status=paused",
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
                    if args.services.contains(&service)
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

        unpause_containers(podman, &progress, &containers).await?;

        progress.finish();
    }

    Ok(())
}
