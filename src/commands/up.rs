use std::env;

use anyhow::{bail, Result};
use clap::{crate_version, ValueEnum};
use futures::{stream::FuturesUnordered, try_join, TryStreamExt};
use itertools::Itertools;
use petgraph::{algo::tarjan_scc, graphmap::DiGraphMap};
use tap::Tap;

use crate::{
    compose::{
        self,
        types::{Compose, Condition},
    },
    config::Config,
    podman::Podman,
    progress::Progress,
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

    /// Return the exit code of the selected service container
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
    progress.header.inc_length(1);

    let spinner = progress.add_spinner();

    spinner.set_prefix(format!("Pod {name}"));
    spinner.set_message("Creating");

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
            .tap(|result| {
                spinner.finish_with_message(match result {
                    Ok(_) => "Created",
                    Err(_) => "Error",
                })
            })?;

        spinner.finish_with_message("Created");
    } else {
        spinner.finish_with_message("Exists");
    }

    progress.header.inc(1);

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
            progress.header.inc_length(1);

            let name = network.name.as_ref().unwrap();
            let spinner = progress.add_spinner();

            spinner.set_prefix(format!("Network {name}"));
            spinner.set_message("Creating");

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
                    .tap(|result| {
                        spinner.finish_with_message(match result {
                            Ok(_) => "Created",
                            Err(_) => "Error",
                        })
                    })?;
            } else {
                spinner.finish_with_message("Exists");
            }

            progress.header.inc(1);

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
            progress.header.inc_length(1);

            let name = volume.name.as_ref().unwrap();
            let spinner = progress.add_spinner();

            spinner.set_prefix(format!("Volume {name}"));
            spinner.set_message("Creating");

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
                    .tap(|result| {
                        spinner.finish_with_message(match result {
                            Ok(_) => "Created",
                            Err(_) => "Error",
                        })
                    })?;
            } else {
                spinner.finish_with_message("Exists");
            }

            progress.header.inc(1);

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
            progress.header.inc_length(1);

            let name = secret.name.as_ref().unwrap();
            let spinner = progress.add_spinner();

            spinner.set_prefix(format!("Secret {name}"));
            spinner.set_message("Creating");

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
                    .tap(|result| {
                        spinner.finish_with_message(match result {
                            Ok(_) => "Created",
                            Err(_) => "Error",
                        })
                    })?;
            } else {
                spinner.finish_with_message("Exists");
            }

            progress.header.inc(1);

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
    let mut dependencies = DiGraphMap::new();

    for (to, service) in &file.services {
        dependencies.extend(
            service
                .depends_on
                .iter()
                .map(|(from, dependency)| (from.as_str(), to.as_str(), dependency.condition)),
        );
        dependencies.extend(
            service
                .links
                .iter()
                .map(|(from, _)| (from.as_str(), to.as_str(), Condition::Started)),
        );
    }

    let (nodes, cycles): (Vec<_>, Vec<_>) = tarjan_scc(&dependencies)
        .into_iter()
        .partition(|component| component.len() == 1);

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
    let podman = podman.await?;
    let width = (name.len() + 4)
        .max(
            file.networks
                .values()
                .map(|network| network.name.as_ref().map_or(0, String::len))
                .max()
                .unwrap_or_default()
                + 8,
        )
        .max(
            file.volumes
                .values()
                .map(|volume| volume.name.as_ref().map_or(0, String::len))
                .max()
                .unwrap_or_default()
                + 7,
        )
        .max(
            file.secrets
                .values()
                .map(|secret| secret.name.as_ref().map_or(0, String::len))
                .max()
                .unwrap_or_default()
                + 7,
        );
    let progress = Progress::new(&config, width);

    try_join!(
        create_pod(&podman, &progress, &config, &file, &labels, name),
        create_networks(&podman, &progress, &file, &labels),
        create_volumes(&podman, &progress, &file, &labels),
        create_secrets(&podman, &progress, &file, &labels)
    )?;

    Ok(())
}
