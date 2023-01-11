use std::{env, mem};

use anyhow::{bail, Result};
use clap::{crate_version, ValueEnum};
use futures::{stream::FuturesUnordered, try_join, StreamExt, TryStreamExt};
use indexmap::IndexMap;
use itertools::Itertools;
use petgraph::{algo::tarjan_scc, graphmap::DiGraphMap, Direction};
use tokio::sync::{broadcast, Barrier};
use tokio_stream::wrappers::BroadcastStream;

use crate::{
    compose::{
        self,
        types::{Compose, FileReference, ServiceVolume, ServiceVolumeType},
    },
    config::Config,
    podman::Podman,
    progress::{Finish, Progress},
    utils::parse_key_val,
};

/// Create and start containers
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    /// Detached mode: Run containers in the background
    #[arg(short, long, conflicts_with_all = ["abort_on_container_exit", "attach", "attach_dependencies"])]
    detach: bool,

    /// Build images before starting containers
    #[arg(long, conflicts_with = "no_build")]
    build: bool,

    /// Don't build an image, even if it's missing
    #[arg(long, conflicts_with = "build")]
    no_build: bool,

    /// Pull image before running
    #[arg(long, value_enum, default_value_t = PullPolicy::Missing)]
    pull: PullPolicy,

    /// Remove containers for services not defined in the Compose file
    #[arg(long)]
    remove_orphans: bool,

    /// Scale SERVICE to NUM instances, overrides the `scale` setting in the Compose file if present
    #[arg(long, value_name = "SERVICE>=<NUM", value_parser = parse_key_val::<String, i32>)]
    scale: Vec<(String, i32)>,

    /// Produce monochrome output
    #[arg(long)]
    no_colour: bool,

    /// Don't print prefix in logs
    #[arg(long)]
    no_log_prefix: bool,

    /// Recreate containers even if their configuration and image haven't changed
    #[arg(long)]
    force_recreate: bool,

    /// If containers already exist, don't recreate them
    #[arg(long, conflicts_with_all = ["always_recreate_deps", "force_recreate"])]
    no_recreate: bool,

    /// Don't start the services after creating them
    #[arg(long)]
    no_start: bool,

    /// Stops all containers if any container was stopped
    #[arg(long)]
    abort_on_container_exit: bool,

    /// Return the exit code of the selected service container
    #[arg(long)]
    exit_code_from: Option<String>,

    /// Use this timeout in seconds for container shutdown when attached or when containers are already running
    #[arg(short, long, default_value_t = 10)]
    timeout: i32,

    /// Show timestamps
    #[arg(long)]
    timestamps: bool,

    /// Don't start linked services
    #[arg(long)]
    no_deps: bool,

    /// Recreate dependent containers
    #[arg(long)]
    always_recreate_deps: bool,

    /// Recreate anonymous volumes instead of retrieving data from the previous containers
    #[arg(short = 'V', long)]
    renew_anon_volumes: bool,

    /// Attach to dependent containers
    #[arg(long)]
    attach_dependencies: bool,

    /// Pull without printing progress information
    #[arg(long)]
    quiet_pull: bool,

    /// Attach to service output
    #[arg(long)]
    attach: Vec<String>,

    /// Wait for services to be running|healthy, implies detached mode
    #[arg(long, conflicts_with_all = ["abort_on_container_exit", "attach", "attach_dependencies"])]
    wait: bool,
}

#[derive(ValueEnum, Clone, Debug)]
enum PullPolicy {
    Always,
    Missing,
    Never,
    Newer,
}

async fn create_pod(
    podman: &Podman,
    progress: &Progress,
    config: &Config,
    file: &Compose,
    labels: &[String],
    name: &str,
) -> Result<()> {
    let spinner = progress.add_spinner(format!("Pod {name}"), "Creating");

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
            .await
            .finish_with_message(spinner, "Created")?;
    } else {
        spinner.finish_with_message("Exists");
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
) -> Result<()> {
    let dependencies = &file
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
        .map(|service| (service, broadcast::channel::<Vec<String>>(capacity).0))
        .collect::<IndexMap<_, _>>();
    let barrier = &Barrier::new(
        file.services
            .values()
            .map(|service| {
                service
                    .deploy
                    .as_ref()
                    .and_then(|deploy| deploy.replicas)
                    .or(service.scale)
                    .unwrap_or(1) as usize
            })
            .sum(),
    );

    file.services
        .iter()
        .map(|(service_name, service)| async move {
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
                    let spinner =
                        progress.add_spinner(format!("Container {container_name}"), "Creating");
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
                        let (global_args, args) = service.to_args();

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
                                    .chain(labels.iter().flat_map(|label| ["--label", label]))
                                    .chain(
                                        container_labels
                                            .iter()
                                            .flat_map(|label| ["--label", label]),
                                    )
                                    .chain(
                                        requirements
                                            .iter()
                                            .flat_map(|requirement| ["--requires", requirement]),
                                    )
                                    .chain(args.iter().map(AsRef::as_ref)),
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
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await
        .map(|_| ())
}

pub(crate) async fn run(args: Args, config: Config) -> Result<()> {
    let podman = Podman::new(&config);
    let mut file = compose::parse(&config, false)?;

    let dependencies = file
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
    let cycles = tarjan_scc(&dependencies)
        .into_iter()
        .filter(|component| component.len() > 1)
        .collect::<Vec<_>>();

    if !cycles.is_empty() {
        bail!(
            "Cycles found: {}",
            cycles
                .into_iter()
                .map(|component| format!("{} -> {}", component.iter().join(" -> "), component[0]))
                .join(", ")
        );
    }

    let name = file.name.as_ref().unwrap();
    let labels = [("version", crate_version!()), ("project", name)]
        .into_iter()
        .map(|label| format!("io.podman.compose.{}={}", label.0, label.1))
        .collect::<Vec<_>>();
    let progress = Progress::new(&config);
    let podman = podman.await?;

    try_join!(
        create_pod(&podman, &progress, &config, &file, &labels, name),
        create_networks(&podman, &progress, &file, &labels),
        create_volumes(&podman, &progress, &file, &labels),
        create_secrets(&podman, &progress, &file, &labels)
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

    let progress = Progress::new(&config);

    create_containers(&podman, &progress, &file, &labels, name).await?;

    progress.finish();

    Ok(())
}
