use std::{
    collections::VecDeque,
    env,
    fmt::{self, Display, Formatter},
};

use anyhow::{bail, Result};
use clap::{crate_version, ValueEnum};
use futures::{stream::FuturesUnordered, try_join, StreamExt, TryStreamExt};
use heck::AsKebabCase;
use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use petgraph::{algo::has_path_connecting, graphmap::DiGraphMap, Direction};
use tokio::sync::{broadcast, Barrier};
use tokio_stream::wrappers::BroadcastStream;

use crate::{
    commands::down,
    compose::types::{Compose, FileReference, ServiceVolume, ServiceVolumeType},
    config::Config,
    podman::{types::Pod, Podman},
    progress::{Finish, Progress},
    utils::Digest,
};

/// Creates containers for a service
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    pub(crate) services: Vec<String>,

    /// Pull image before running
    #[arg(long, value_enum)]
    pub(crate) pull: Option<PullPolicy>,

    /// Recreate containers even if their configuration and image haven't changed
    #[arg(long, conflicts_with_all = ["services", "no_recreate"])]
    pub(crate) force_recreate: bool,

    /// If containers already exist, don't recreate them
    #[arg(long)]
    pub(crate) no_recreate: bool,

    /// Remove containers for services not defined in the Compose file
    #[arg(long)]
    pub(crate) remove_orphans: bool,
}

#[derive(ValueEnum, Clone, Debug)]
pub(crate) enum PullPolicy {
    Always,
    Missing,
    Never,
    Newer,
}

impl Display for PullPolicy {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", AsKebabCase(format!("{self:?}")))
    }
}

async fn create_pod(
    podman: &Podman,
    config: &Config,
    file: &Compose,
    labels: &[String],
) -> Result<()> {
    let name = file.name.as_ref().unwrap();

    if podman.force_run(["pod", "exists", name]).await.is_err() {
        let pod_labels = [
            (
                "project.working-dir",
                config.project_directory.to_string_lossy().as_ref(),
            ),
            (
                "project.config-files",
                &config
                    .files
                    .iter()
                    .map(|file| file.to_string_lossy())
                    .join(","),
            ),
            (
                "project.environment-file",
                config.env_file.to_string_lossy().as_ref(),
            ),
            ("config-hash", &file.digest()),
        ]
        .into_iter()
        .map(|label| format!("io.podman.compose.{}={}", label.0, label.1))
        .collect::<Vec<_>>();

        podman
            .run(
                ["pod", "create", "--share", "none"]
                    .into_iter()
                    .chain(labels.iter().flat_map(|label| ["--label", label]))
                    .chain(pod_labels.iter().flat_map(|label| ["--label", label]))
                    .chain([name.as_ref()]),
            )
            .await?;
    }

    Ok(())
}

async fn create_networks(
    podman: &Podman,
    progress: &Progress,
    file: &Compose,
    labels: &[String],
) -> Result<()> {
    file.networks
        .values()
        .map(|network| async {
            let name = network.name.as_ref().unwrap();
            let spinner = progress.add_spinner(format!("Network {name}"), "Creating");

            if podman.force_run(["network", "exists", name]).await.is_err() {
                if network.external.unwrap_or_default() {
                    bail!("External network \"{name}\" not found");
                }

                let network_labels = [("network", name)]
                    .into_iter()
                    .map(|label| format!("io.podman.compose.{}={}", label.0, label.1))
                    .collect::<Vec<_>>();

                podman
                    .run(
                        ["network", "create"]
                            .into_iter()
                            .chain(labels.iter().flat_map(|label| ["--label", label]))
                            .chain(network_labels.iter().flat_map(|label| ["--label", label]))
                            .chain(network.to_args().iter().map(AsRef::as_ref)),
                    )
                    .await
                    .finish_with_message(spinner, "Created")?;
            } else {
                spinner.finish_with_message("Exists");
            }

            Ok(())
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await
        .map(|_| ())
}

async fn create_volumes(
    podman: &Podman,
    progress: &Progress,
    file: &Compose,
    labels: &[String],
) -> Result<()> {
    file.volumes
        .values()
        .map(|volume| async {
            let name = volume.name.as_ref().unwrap();
            let spinner = progress.add_spinner(format!("Volume {name}"), "Creating");

            if podman.force_run(["volume", "exists", name]).await.is_err() {
                if volume.external.unwrap_or_default() {
                    bail!("External volume \"{name}\" not found");
                }

                let volume_labels = [("volume", name)]
                    .into_iter()
                    .map(|label| format!("io.podman.compose.{}={}", label.0, label.1))
                    .collect::<Vec<_>>();

                podman
                    .run(
                        ["volume", "create"]
                            .into_iter()
                            .chain(labels.iter().flat_map(|label| ["--label", label]))
                            .chain(volume_labels.iter().flat_map(|label| ["--label", label]))
                            .chain(volume.to_args().iter().map(AsRef::as_ref)),
                    )
                    .await
                    .finish_with_message(spinner, "Created")?;
            } else {
                spinner.finish_with_message("Exists");
            }

            Ok(())
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await
        .map(|_| ())
}

async fn create_secrets(
    podman: &Podman,
    progress: &Progress,
    file: &Compose,
    labels: &[String],
) -> Result<()> {
    file.secrets
        .values()
        .map(|secret| async {
            let name = secret.name.as_ref().unwrap();
            let spinner = progress.add_spinner(format!("Secret {name}"), "Creating");

            if podman.force_run(["secret", "inspect", name]).await.is_err() {
                if secret.external.unwrap_or_default() {
                    bail!("External secret \"{name}\" not found");
                }

                let secret_labels = [("secret", name)]
                    .into_iter()
                    .map(|label| format!("io.podman.compose.{}={}", label.0, label.1))
                    .collect::<Vec<_>>();

                podman
                    .run(
                        ["secret", "create"]
                            .into_iter()
                            .chain(labels.iter().flat_map(|label| ["--label", label]))
                            .chain(secret_labels.iter().flat_map(|label| ["--label", label]))
                            .chain(secret.to_args().iter().map(AsRef::as_ref)),
                    )
                    .await
                    .finish_with_message(spinner, "Created")?;
            } else {
                spinner.finish_with_message("Exists");
            }

            Ok(())
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await
        .map(|_| ())
}

async fn create_containers(
    podman: &Podman,
    progress: &Progress,
    file: &Compose,
    labels: &[String],
    args: Args,
) -> Result<()> {
    let project_name = file.name.as_ref().unwrap();
    let mut dependencies = file
        .services
        .iter()
        .flat_map(|(to, service)| service.depends_on.keys().map(move |from| (from, to, ())))
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
        .map(|service| (service, broadcast::channel::<Vec<String>>(capacity).0))
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
    let args = &args;

    file.services
        .iter()
        .filter_map(|(service_name, service)| {
            if dependencies.contains_node(service_name) {
                Some(async move {
                    let container_names = (1..=service
                        .deploy
                        .as_ref()
                        .and_then(|deploy| deploy.replicas)
                        .or(service.scale)
                        .unwrap_or(1))
                        .map(|i| async move {
                            let container_name = service
                                .container_name
                                .clone()
                                .unwrap_or_else(|| format!("{project_name}_{service_name}_{i}"));
                            let spinner = progress
                                .add_spinner(format!("Container {container_name}"), "Creating");
                            let rx = txs[service_name].subscribe();

                            barrier.wait().await;

                            let dependencies = dependencies
                                .neighbors_directed(service_name, Direction::Incoming)
                                .count();
                            let requirements = BroadcastStream::new(rx)
                                .take(dependencies)
                                .try_concat()
                                .await?;

                            if podman
                                .force_run(["container", "exists", &container_name])
                                .await
                                .is_err()
                            {
                                let container_labels = [
                                    ("oneoff", "false"),
                                    ("service", service_name),
                                    ("container-number", &i.to_string()),
                                ]
                                .into_iter()
                                .map(|label| format!("io.podman.compose.{}={}", label.0, label.1))
                                .collect::<Vec<_>>();
                                let pull_policy =
                                    args.pull.as_ref().map(ToString::to_string).or_else(|| {
                                        service.pull_policy.as_ref().map(ToString::to_string)
                                    });

                                let networks = service
                                    .networks
                                    .iter()
                                    .map(|(name, network)| {
                                        let name = file.networks[name].name.clone().unwrap();
                                        let mut network = network
                                            .as_ref()
                                            .map(ToString::to_string)
                                            .unwrap_or_default();

                                        if let Some(mac_address) = service.mac_address.as_ref() {
                                            if network.is_empty() {
                                                network = format!(":mac={mac_address}");
                                            } else {
                                                network = format!("{network},mac={mac_address}");
                                            }
                                        }

                                        format!("{name}{network}")
                                    })
                                    .collect::<Vec<_>>();
                                let volumes = service
                                    .volumes
                                    .iter()
                                    .flat_map(|volume| {
                                        let volume = match &volume.r#type {
                                            ServiceVolumeType::Volume(Some(source)) => {
                                                ServiceVolume {
                                                    r#type: ServiceVolumeType::Volume(
                                                        file.volumes[source].name.clone(),
                                                    ),
                                                    ..volume.clone()
                                                }
                                            }
                                            _ => volume.clone(),
                                        };

                                        [
                                            String::from(match volume.r#type {
                                                ServiceVolumeType::Volume(_)
                                                | ServiceVolumeType::Bind(_) => "--volume",
                                                ServiceVolumeType::Tmpfs => "--tmpfs",
                                            }),
                                            volume.to_string(),
                                        ]
                                    })
                                    .collect::<Vec<_>>();
                                let secrets = service
                                    .secrets
                                    .iter()
                                    .map(|secret| {
                                        FileReference {
                                            source: file.secrets[&secret.source]
                                                .name
                                                .clone()
                                                .unwrap(),
                                            ..secret.clone()
                                        }
                                        .to_string()
                                    })
                                    .collect::<Vec<_>>();

                                let (global_args, service_args) = service.to_args();

                                podman
                                    .run(
                                        global_args
                                            .iter()
                                            .map(AsRef::as_ref)
                                            .chain([
                                                "create",
                                                "--pod",
                                                project_name,
                                                "--name",
                                                &container_name,
                                                "--network-alias",
                                                service_name,
                                            ])
                                            .chain(requirements.iter().flat_map(|requirement| {
                                                ["--requires", requirement]
                                            }))
                                            .chain(
                                                labels.iter().flat_map(|label| ["--label", label]),
                                            )
                                            .chain(
                                                container_labels
                                                    .iter()
                                                    .flat_map(|label| ["--label", label]),
                                            )
                                            .chain(if let Some(pull_policy) = &pull_policy {
                                                vec!["--pull", pull_policy]
                                            } else {
                                                vec![]
                                            })
                                            .chain(
                                                networks
                                                    .iter()
                                                    .flat_map(|network| ["--network", network]),
                                            )
                                            .chain(volumes.iter().map(AsRef::as_ref))
                                            .chain(
                                                secrets
                                                    .iter()
                                                    .flat_map(|secret| ["--secret", secret]),
                                            )
                                            .chain(service_args.iter().map(AsRef::as_ref)),
                                    )
                                    .await
                                    .finish_with_message(spinner, "Created")?;
                            } else {
                                spinner.finish_with_message("Exists");
                            }

                            anyhow::Ok(container_name)
                        })
                        .collect::<FuturesUnordered<_>>()
                        .try_collect::<Vec<_>>()
                        .await?;

                    for dependent in dependencies.neighbors(service_name) {
                        txs[dependent].send(container_names.clone())?;
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
    let name = file.name.as_ref().unwrap();
    let labels = [("version", crate_version!()), ("project", name)]
        .into_iter()
        .map(|label| format!("io.podman.compose.{}={}", label.0, label.1))
        .collect::<Vec<_>>();

    let output = podman
        .force_run([
            "pod",
            "ps",
            "--format",
            "json",
            "--filter",
            &format!("name=^{name}$"),
        ])
        .await?;
    let config_hash = serde_json::from_str::<VecDeque<Pod>>(&output)?
        .pop_front()
        .and_then(|pod| pod.labels.and_then(|labels| labels.config_hash));

    if args.force_recreate
        || (!args.no_recreate
            && config_hash
                .map(|config_hash| config_hash != file.digest())
                .unwrap_or_default())
    {
        down::run(
            down::Args {
                remove_orphans: args.remove_orphans,
                timeout: 10,
                volumes: true,
                rmi: false,
            },
            &podman,
            &file,
            config,
        )
        .await?;
    }

    let progress = Progress::new(config);

    try_join!(
        create_pod(&podman, config, &file, &labels),
        create_networks(&podman, &progress, &file, &labels),
        create_volumes(&podman, &progress, &file, &labels),
        create_secrets(&podman, &progress, &file, &labels),
    )?;

    progress.finish();

    if args.services.is_empty()
        || !args
            .services
            .iter()
            .collect::<IndexSet<_>>()
            .is_disjoint(&file.services.keys().collect::<IndexSet<_>>())
    {
        let progress = Progress::new(config);

        create_containers(&podman, &progress, &file, &labels, args).await?;

        progress.finish();
    }

    Ok(())
}
