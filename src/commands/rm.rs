use std::collections::HashMap;

use anyhow::Result;
use futures::{stream::FuturesUnordered, TryStreamExt};
use indexmap::IndexMap;
use inquire::Confirm;
use itertools::Itertools;
use petgraph::{graphmap::DiGraphMap, Direction};
use tokio::sync::{broadcast, Barrier};

use crate::{
    compose::types::Compose,
    config::Config,
    podman::{types::Container, Podman},
    progress::{Finish, Progress},
};

/// Removes stopped service containers
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    pub(crate) services: Vec<String>,

    /// Don't ask to confirm removal
    #[arg(short, long)]
    pub(crate) force: bool,

    /// Stop the containers, if required, before removing
    #[arg(short, long)]
    pub(crate) stop: bool,

    /// Remove any anonymous volumes attached to containers
    #[arg(short, long)]
    pub(crate) volumes: bool,
}

pub(crate) async fn remove_containers(
    podman: &Podman,
    progress: &Progress,
    file: &Compose,
    containers: &HashMap<String, Vec<String>>,
    args: Args,
) -> Result<()> {
    let dependencies = &file
        .services
        .iter()
        .filter(|(service, _)| containers.keys().contains(service))
        .flat_map(|(from, service)| {
            service
                .depends_on
                .keys()
                .chain(service.links.keys())
                .filter(|service| containers.keys().contains(service))
                .map(move |to| (from, to, ()))
        })
        .collect::<DiGraphMap<_, _>>();
    let capacity = dependencies
        .nodes()
        .map(|service| {
            dependencies
                .neighbors_directed(service, Direction::Incoming)
                .count()
        })
        .max()
        .unwrap_or_default()
        .max(1);
    let txs = &containers
        .keys()
        .map(|service| (service, broadcast::channel(capacity).0))
        .collect::<IndexMap<_, _>>();
    let barrier = &Barrier::new(containers.values().map(Vec::len).sum());

    containers
        .iter()
        .map(|(service, containers)| async move {
            containers
                .iter()
                .map(|container| async move {
                    let spinner =
                        progress.add_spinner(format!("Container {container}"), "Removing");
                    let mut rx = txs[service].subscribe();

                    barrier.wait().await;

                    for _ in dependencies.neighbors_directed(service, Direction::Incoming) {
                        rx.recv().await?;
                    }

                    podman
                        .run(
                            ["rm", "--depend", "--ignore"]
                                .into_iter()
                                .chain(if args.stop { vec!["--force"] } else { vec![] })
                                .chain(if args.volumes {
                                    vec!["--volumes"]
                                } else {
                                    vec![]
                                })
                                .chain([container.as_ref()]),
                        )
                        .await
                        .finish_with_message(spinner, "Removed")
                })
                .collect::<FuturesUnordered<_>>()
                .try_collect::<Vec<_>>()
                .await?;

            for dependent in dependencies.neighbors(service) {
                txs[dependent].send(())?;
            }

            Ok(())
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
                        container.names.pop_front().map(|name| (service, name))
                    } else {
                        None
                    }
                })
        })
        .into_group_map();

    if !containers.is_empty()
        && (args.force
            || Confirm::new(&format!(
                "Going to remove {}",
                containers.values().flatten().join(", ")
            ))
            .with_default(false)
            .prompt()?)
    {
        let progress = Progress::new(config);

        remove_containers(podman, &progress, file, &containers, args).await?;

        progress.finish();
    }

    Ok(())
}
