use std::{
    collections::VecDeque,
    env,
    fmt::{self, Display, Formatter},
    mem,
};

use anyhow::{bail, Result};
use clap::{crate_version, ValueEnum};
use futures::{stream::FuturesUnordered, try_join, StreamExt, TryStreamExt};
use heck::AsKebabCase;
use indexmap::IndexMap;
use itertools::Itertools;
use petgraph::{algo::has_path_connecting, graphmap::DiGraphMap, Direction};
use tokio::sync::{broadcast, Barrier};
use tokio_stream::wrappers::BroadcastStream;

use crate::{
    commands::down,
    compose::{
        self,
        types::{Compose, FileReference, ServiceVolume, ServiceVolumeType},
    },
    config::Config,
    podman::{types::Pod, Podman},
    progress::{Finish, Progress},
    utils::Digest,
};

/// Creates containers for a service
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    services: Vec<String>,

    /// Build images before starting containers
    // #[arg(long, conflicts_with = "no_build")]
    // build: bool,

    /// Don't build an image, even if it's missing
    // #[arg(long, conflicts_with = "build")]
    // no_build: bool,

    /// Pull image before running
    #[arg(long, value_enum)]
    pull: Option<PullPolicy>,

    /// Recreate containers even if their configuration and image haven't changed
    #[arg(long, conflicts_with_all = ["services", "no_recreate"])]
    force_recreate: bool,

    /// If containers already exist, don't recreate them
    #[arg(long)]
    no_recreate: bool,

    /// Remove containers for services not defined in the Compose file
    #[arg(long)]
    remove_orphans: bool,
}

#[derive(ValueEnum, Clone, Debug)]
enum PullPolicy {
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
    name: &str,
) -> Result<()> {
    if podman.force_run(["pod", "exists", name]).await.is_err() {
        let pod_labels = [
            (
                "project.working_dir",
                config.project_directory.to_string_lossy().as_ref(),
            ),
            (
                "project.config_files",
                &config
                    .files
                    .iter()
                    .map(|file| file.to_string_lossy())
                    .join(","),
            ),
            (
                "project.environment_file",
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
                    .chain([name]),
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
    name: &str,
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

    if !args.services.is_empty() {
        let mut nodes = Vec::new();

        for node in dependencies.nodes() {
            if !args
                .services
                .iter()
                .any(|service| has_path_connecting(&dependencies, node, service, None))
            {
                nodes.push(node);
            }
        }

        for node in nodes {
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
    let txs = &file
        .services
        .keys()
        .filter_map(|service| {
            if dependencies.contains_node(service) {
                Some((service, broadcast::channel::<Vec<String>>(capacity).0))
            } else {
                None
            }
        })
        .collect::<IndexMap<_, _>>();
    let barrier = &Barrier::new(
        file.services
            .iter()
            .filter_map(|(service_name, service)| {
                if dependencies.contains_node(service_name) {
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
                    let names = (1..=service
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
                                    ("service", service_name),
                                    ("container-number", &i.to_string()),
                                ]
                                .into_iter()
                                .map(|label| format!("io.podman.compose.{}={}", label.0, label.1))
                                .collect::<Vec<_>>();
                                let (global_args, service_args) = service.to_args();
                                let pull_policy =
                                    args.pull.as_ref().map(ToString::to_string).or_else(|| {
                                        service.pull_policy.as_ref().and_then(|pull_policy| {
                                            if *pull_policy == compose::types::PullPolicy::Build {
                                                None
                                            } else {
                                                Some(pull_policy.to_string())
                                            }
                                        })
                                    });

                                podman
                                    .run(
                                        global_args
                                            .iter()
                                            .map(AsRef::as_ref)
                                            .chain(["create", "--pod", name])
                                            .chain(if service.container_name.is_none() {
                                                vec!["--name", &container_name]
                                            } else {
                                                vec![]
                                            })
                                            .chain(if let Some(pull_policy) = &pull_policy {
                                                vec!["--pull", pull_policy]
                                            } else {
                                                vec![]
                                            })
                                            .chain(
                                                labels.iter().flat_map(|label| ["--label", label]),
                                            )
                                            .chain(
                                                container_labels
                                                    .iter()
                                                    .flat_map(|label| ["--label", label]),
                                            )
                                            .chain(requirements.iter().flat_map(|requirement| {
                                                ["--requires", requirement]
                                            }))
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
                        txs[dependent].send(names.clone())?;
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

pub(crate) async fn run(args: Args, config: &Config) -> Result<()> {
    let podman = Podman::new(config).await?;
    let mut file = compose::parse(config, false)?;
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
        || config_hash
            .map(|config_hash| config_hash != file.digest())
            .unwrap_or_default()
    {
        down::run(
            down::Args {
                remove_orphans: args.remove_orphans,
                timeout: 10,
                volumes: true,
                rmi: false,
            },
            config,
        )
        .await?;
    }

    let progress = Progress::new(config);

    try_join!(
        create_pod(&podman, config, &file, &labels, name),
        create_networks(&podman, &progress, &file, &labels),
        create_volumes(&podman, &progress, &file, &labels),
        create_secrets(&podman, &progress, &file, &labels),
    )?;

    progress.finish();

    for service in file.services.values_mut() {
        service.networks = mem::take(&mut service.networks)
            .into_iter()
            .map(|(name, network)| (file.networks[&name].name.clone().unwrap(), network))
            .collect();

        service.volumes = mem::take(&mut service.volumes)
            .into_iter()
            .map(|volume| match volume.r#type {
                ServiceVolumeType::Volume(Some(source)) => ServiceVolume {
                    r#type: ServiceVolumeType::Volume(file.volumes[&source].name.clone()),
                    ..volume
                },
                _ => volume,
            })
            .collect();

        service.secrets = mem::take(&mut service.secrets)
            .into_iter()
            .map(|secret| FileReference {
                source: file.secrets[&secret.source].name.clone().unwrap(),
                ..secret
            })
            .collect();
    }

    let progress = Progress::new(config);

    create_containers(&podman, &progress, &file, &labels, name, args).await?;

    progress.finish();

    Ok(())
}
