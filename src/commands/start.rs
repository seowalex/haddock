use anyhow::Result;
use futures::{stream::FuturesUnordered, TryStreamExt};
use indexmap::{IndexMap, IndexSet};
use petgraph::{algo::has_path_connecting, graphmap::DiGraphMap, Direction};
use tokio::sync::{broadcast, Barrier};

use crate::{
    compose::types::Compose,
    config::Config,
    podman::Podman,
    progress::{Finish, Progress},
};

/// Start services
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    pub(crate) services: Vec<String>,
}

async fn start_containers(
    podman: &Podman,
    progress: &Progress,
    file: &Compose,
    args: Args,
) -> Result<()> {
    let mut dependencies = file
        .services
        .iter()
        .flat_map(|(to, service)| {
            service
                .depends_on
                .keys()
                .chain(service.links.keys())
                .map(move |from| (from, to, ()))
        })
        .collect::<DiGraphMap<_, _>>();

    for service in file.services.keys() {
        dependencies.add_node(service);
    }

    if !args.services.is_empty() {
        for node in dependencies
            .nodes()
            .filter(|node| {
                args.services
                    .iter()
                    .all(|service| !has_path_connecting(&dependencies, node, service, None))
            })
            .collect::<Vec<_>>()
        {
            dependencies.remove_node(node);
        }
    }

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
    let txs = &dependencies
        .nodes()
        .map(|service| (service, broadcast::channel(capacity).0))
        .collect::<IndexMap<_, _>>();
    let barrier = &Barrier::new(
        file.services
            .iter()
            .filter_map(|(name, service)| {
                if dependencies.contains_node(name) {
                    Some(
                        service
                            .deploy
                            .as_ref()
                            .and_then(|deploy| deploy.replicas)
                            .or(service.scale)
                            .unwrap_or(1) as usize,
                    )
                } else {
                    None
                }
            })
            .sum(),
    );
    let dependencies = &dependencies;

    file.services
        .iter()
        .filter_map(|(service_name, service)| {
            if dependencies.contains_node(service_name) {
                Some(async move {
                    (1..=service
                        .deploy
                        .as_ref()
                        .and_then(|deploy| deploy.replicas)
                        .or(service.scale)
                        .unwrap_or(1))
                        .map(|i| async move {
                            let container_name =
                                service.container_name.clone().unwrap_or_else(|| {
                                    format!("{}_{service_name}_{i}", file.name.as_ref().unwrap())
                                });
                            let spinner = progress
                                .add_spinner(format!("Container {container_name}"), "Starting");
                            let mut rx = txs[service_name].subscribe();

                            barrier.wait().await;

                            for _ in
                                dependencies.neighbors_directed(service_name, Direction::Incoming)
                            {
                                rx.recv().await?;
                            }

                            podman
                                .run(["start", &container_name])
                                .await
                                .finish_with_message(spinner, "Started")
                        })
                        .collect::<FuturesUnordered<_>>()
                        .try_collect::<Vec<_>>()
                        .await?;

                    for dependent in dependencies.neighbors(service_name) {
                        txs[dependent].send(())?;
                    }

                    Ok(())
                })
            } else {
                None
            }
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
    if args.services.is_empty()
        || !args
            .services
            .iter()
            .collect::<IndexSet<_>>()
            .is_disjoint(&file.services.keys().collect::<IndexSet<_>>())
    {
        let progress = Progress::new(config);

        start_containers(podman, &progress, file, args).await?;

        progress.finish();
    }

    Ok(())
}
