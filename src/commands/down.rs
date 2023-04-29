use anyhow::Result;
use futures::{future::try_join3, stream::FuturesUnordered, try_join, TryStreamExt};
use itertools::Itertools;

use crate::{
    commands::{
        rm::{self, remove_containers},
        stop::{self, stop_containers},
    },
    compose::types::Compose,
    config::Config,
    podman::{
        types::{Container, Network, Volume},
        Podman,
    },
    progress::{Finish, Progress},
};

/// Stop and remove containers, networks
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    /// Remove containers for services not defined in the Compose file
    #[arg(long)]
    pub(crate) remove_orphans: bool,

    /// Specify a shutdown timeout in seconds [default: 10]
    #[arg(short, long)]
    pub(crate) timeout: Option<u32>,

    /// Remove any anonymous volumes attached to containers
    #[arg(short, long)]
    pub(crate) volumes: bool,

    /// Remove images used by services
    #[arg(long)]
    pub(crate) rmi: bool,
}

async fn remove_networks(podman: &Podman, progress: &Progress, networks: &[String]) -> Result<()> {
    networks
        .iter()
        .map(|network| async move {
            let spinner = progress.add_spinner(format!("Network {network}"), "Removing");

            podman
                .run(["network", "rm", network])
                .await
                .finish_with_message(spinner, "Removed")
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await
        .map(|_| ())
}

async fn remove_volumes(podman: &Podman, progress: &Progress, volumes: &[String]) -> Result<()> {
    volumes
        .iter()
        .map(|volume| async move {
            let spinner = progress.add_spinner(format!("Volume {volume}"), "Removing");

            podman
                .run(["volume", "rm", volume])
                .await
                .finish_with_message(spinner, "Removed")
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
    let name = file.name.as_ref().unwrap();

    let (containers, networks, volumes) = try_join3(
        podman.force_run(
            [
                "ps",
                "--all",
                "--format",
                "json",
                "--filter",
                &format!("pod={name}"),
            ]
            .into_iter()
            .chain(if args.remove_orphans {
                vec![]
            } else {
                vec!["--filter", "label=io.podman.compose.oneoff=false"]
            }),
        ),
        podman.force_run([
            "network",
            "ls",
            "--format",
            "json",
            "--filter",
            &format!("label=io.podman.compose.project={name}"),
        ]),
        podman.force_run([
            "volume",
            "ls",
            "--format",
            "json",
            "--filter",
            &format!("label=io.podman.compose.project={name}"),
        ]),
    )
    .await?;

    let mut containers = serde_json::from_str::<Vec<Container>>(&containers)?
        .into_iter()
        .filter_map(|mut container| {
            container
                .labels
                .and_then(|labels| labels.service)
                .and_then(|service| container.names.pop_front().map(|name| (service, name)))
        })
        .into_group_map();
    let all_containers = containers.len();
    containers.retain(|service, _| args.remove_orphans || file.services.keys().contains(&service));

    let networks = serde_json::from_str::<Vec<Network>>(&networks)?
        .into_iter()
        .filter_map(|network| {
            if args.remove_orphans
                || file
                    .networks
                    .values()
                    .filter_map(|network| network.name.as_ref())
                    .contains(&network.name)
            {
                Some(network.name)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let volumes = serde_json::from_str::<Vec<Volume>>(&volumes)?
        .into_iter()
        .filter_map(|volume| {
            if args.remove_orphans
                || file
                    .volumes
                    .values()
                    .filter_map(|volume| volume.name.as_ref())
                    .contains(&volume.name)
            {
                Some(volume.name)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if !containers.is_empty() {
        let progress = Progress::new(config);

        stop_containers(
            podman,
            &progress,
            file,
            &containers,
            stop::Args {
                services: Vec::new(),
                timeout: args.timeout,
            },
        )
        .await?;

        progress.finish();

        let progress = Progress::new(config);

        remove_containers(
            podman,
            &progress,
            file,
            &containers,
            rm::Args {
                services: Vec::new(),
                force: true,
                stop: false,
                volumes: args.volumes,
            },
        )
        .await?;

        progress.finish();
    }

    if all_containers == containers.len() {
        podman.run(["pod", "rm", "--ignore", name]).await?;
    }

    if !networks.is_empty() || (args.volumes && !volumes.is_empty()) || args.rmi {
        let progress = Progress::new(config);

        try_join!(
            remove_networks(podman, &progress, &networks),
            async {
                if args.volumes {
                    remove_volumes(podman, &progress, &volumes).await?;
                }

                Ok(())
            },
            async {
                if args.rmi {
                    let spinner = progress.add_spinner("Images", "Removing");

                    podman
                        .run([
                            "image",
                            "prune",
                            "--force",
                            "--filter",
                            &format!("label=io.podman.compose.project={name}"),
                        ])
                        .await
                        .finish_with_message(spinner, "Removed")?;
                }

                Ok(())
            }
        )?;

        progress.finish();
    }

    Ok(())
}
