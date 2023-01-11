use anyhow::Result;
use futures::{stream::FuturesUnordered, try_join, TryStreamExt};
use indexmap::IndexMap;
use itertools::Itertools;
use petgraph::{graphmap::DiGraphMap, Direction};
use serde_yaml::Value;
use tokio::sync::broadcast;

use crate::{
    compose::{self, types::Compose},
    config::Config,
    podman::Podman,
    progress::{Finish, Progress},
};

/// Stop and remove containers, networks
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    /// Remove containers for services not defined in the Compose file
    #[arg(long)]
    remove_orphans: bool,

    /// Specify a shutdown timeout in seconds
    #[arg(short, long, default_value_t = 10)]
    timeout: i32,

    /// Remove named volumes declared in the `volumes` section of the Compose file and anonymous volumes attached to containers
    #[arg(short, long)]
    volumes: bool,

    /// Remove images used by services
    #[arg(long)]
    rmi: bool,
}

async fn remove_networks(podman: &Podman, progress: &Progress, file: &Compose) -> Result<()> {
    file.networks
        .values()
        .map(|network| async {
            let name = network.name.as_ref().unwrap();
            let spinner = progress.add_spinner(format!("Network {name}"), "Removing");

            if podman.force_run(["network", "exists", name]).await.is_ok() {
                podman
                    .run(["network", "rm", name])
                    .await
                    .finish_with_message(spinner, "Removed")?;
            } else {
                spinner.finish_with_message("Removed");
            }

            Ok(())
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await
        .map(|_| ())
}

async fn remove_volumes(podman: &Podman, progress: &Progress, file: &Compose) -> Result<()> {
    file.volumes
        .values()
        .map(|volume| async {
            let name = volume.name.as_ref().unwrap();
            let spinner = progress.add_spinner(format!("Volume {name}"), "Removing");

            if podman.force_run(["volume", "exists", name]).await.is_ok() {
                podman
                    .run(["volume", "rm", name])
                    .await
                    .finish_with_message(spinner, "Removed")?;
            } else {
                spinner.finish_with_message("Removed");
            }

            Ok(())
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await
        .map(|_| ())
}

async fn remove_images(podman: &Podman, progress: &Progress, file: &Compose) -> Result<()> {
    file.services
        .values()
        .filter_map(|service| service.image.as_ref())
        .unique()
        .map(|name| async move {
            let spinner = progress.add_spinner(format!("Image {name}"), "Removing");

            podman
                .run(["rmi", "--ignore", name])
                .await
                .finish_with_message(spinner, "Removed")
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await
        .map(|_| ())
}

async fn remove_containers(
    args: &Args,
    podman: &Podman,
    progress: &Progress,
    file: &Compose,
    name: &str,
) -> Result<()> {
    let dependencies = &file
        .services
        .iter()
        .flat_map(|(from, service)| {
            service
                .depends_on
                .keys()
                .chain(service.links.keys())
                .map(move |to| (from, to, ()))
        })
        .collect::<DiGraphMap<_, _>>();
    let capacity = dependencies
        .nodes()
        .map(|node| {
            dependencies
                .neighbors_directed(node, Direction::Incoming)
                .count()
        })
        .max()
        .unwrap_or_default();
    let txs = &file
        .services
        .keys()
        .map(|service| (service, broadcast::channel(capacity).0))
        .collect::<IndexMap<_, _>>();

    file.services
        .iter()
        .map(|(service_name, service)| async move {
            (1..=service
                .deploy
                .as_ref()
                .and_then(|deploy| deploy.replicas)
                .or(service.scale)
                .unwrap_or(1))
                .map(|i| async move {
                    let container_name = service
                        .container_name
                        .clone()
                        .unwrap_or_else(|| format!("{name}_{service_name}_{i}"));
                    let spinner =
                        progress.add_spinner(format!("Container {container_name}"), "Removing");
                    let mut rx = txs[service_name].subscribe();

                    for _ in dependencies.neighbors_directed(service_name, Direction::Incoming) {
                        rx.recv().await?;
                    }

                    podman
                        .run(
                            [
                                "rm",
                                "--force",
                                "--time",
                                &args.timeout.to_string(),
                                "--filter",
                                &format!("pod={name}"),
                                "--filter",
                                &format!("name={container_name}"),
                            ]
                            .into_iter()
                            .chain(if args.volumes {
                                vec!["--volumes"]
                            } else {
                                vec![]
                            }),
                        )
                        .await
                        .finish_with_message(spinner, "Removed")
                })
                .collect::<FuturesUnordered<_>>()
                .try_collect::<Vec<_>>()
                .await?;

            for dependent in dependencies.neighbors(service_name) {
                txs[dependent].send(())?;
            }

            Ok(())
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await
        .map(|_| ())
}

pub(crate) async fn run(args: Args, config: Config) -> Result<()> {
    let podman = Podman::new(&config);
    let file = compose::parse(&config, false)?;
    let name = file.name.as_ref().unwrap();
    let podman = podman.await?;

    if args.remove_orphans {
        let progress = Progress::new(&config);
        let spinner = progress.add_spinner(format!("Pod {name}"), "Removing");

        podman
            .run([
                "pod",
                "rm",
                "--force",
                "--time",
                &args.timeout.to_string(),
                name,
            ])
            .await
            .finish_with_message(spinner, "Removed")?;

        progress.finish();

        let progress = Progress::new(&config);

        try_join!(
            async {
                let spinner = progress.add_spinner("Networks", "Removing");

                podman
                    .run([
                        "network",
                        "prune",
                        "--force",
                        "--filter",
                        &format!("label=io.podman.compose.project={name}"),
                    ])
                    .await
                    .finish_with_message(spinner, "Removed")
            },
            async {
                if args.volumes {
                    let spinner = progress.add_spinner("Volumes", "Removing");

                    podman
                        .run([
                            "volume",
                            "prune",
                            "--force",
                            "--filter",
                            &format!("label=io.podman.compose.project={name}"),
                        ])
                        .await
                        .finish_with_message(spinner, "Removed")?;
                }

                anyhow::Ok(())
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

                anyhow::Ok(())
            }
        )?;

        progress.finish();
    } else {
        let progress = Progress::new(&config);

        remove_containers(&args, &podman, &progress, &file, name).await?;

        let spinner = progress.add_spinner(format!("Pod {name}"), "Removing");
        let output = podman
            .force_run([
                "ps",
                "--all",
                "--format",
                "json",
                "--filter",
                &format!("pod={name}"),
            ])
            .await?;
        let containers = serde_json::from_str::<Vec<Value>>(&output)?;

        if containers.is_empty() {
            podman
                .run(["pod", "rm", "--ignore", name])
                .await
                .finish_with_message(spinner, "Removed")?;
        } else {
            spinner.finish_and_clear();
        }

        progress.finish();

        let progress = Progress::new(&config);

        try_join!(
            remove_networks(&podman, &progress, &file),
            async {
                if args.volumes {
                    remove_volumes(&podman, &progress, &file).await?;
                }

                anyhow::Ok(())
            },
            async {
                if args.rmi {
                    remove_images(&podman, &progress, &file).await?;
                }

                anyhow::Ok(())
            }
        )?;

        progress.finish();
    }

    Ok(())
}
