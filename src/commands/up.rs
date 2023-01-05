use std::env;

use anyhow::{bail, Result};
use clap::{crate_version, ValueEnum};
use itertools::Itertools;
use petgraph::{algo::tarjan_scc, graphmap::DiGraphMap};

use crate::{
    compose::{self, types::Condition},
    config::Config,
    podman::Podman,
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

pub(crate) fn run(args: Args, config: Config) -> Result<()> {
    let podman = Podman::new(&config)?;
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

    if podman.run(["pod", "exists", name]).is_err() {
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

        podman.run(
            ["pod", "create", "--share", "none"]
                .into_iter()
                .chain(labels.iter().flat_map(|label| ["--label", label]))
                .chain(pod_labels.iter().flat_map(|label| ["--label", label]))
                .chain([name.as_ref()]),
        )?;
    }

    for network in file.networks.into_values() {
        let name = network.name.as_ref().unwrap();

        if podman.run(["network", "exists", name]).is_err() {
            if network.external.unwrap_or_default() {
                bail!("External network \"{name}\" not found");
            }

            let network_labels = [("network", name)]
                .into_iter()
                .map(|label| format!("io.podman.compose.{}={}", label.0, label.1))
                .collect::<Vec<_>>();

            podman.run(
                ["network", "create"]
                    .into_iter()
                    .chain(labels.iter().flat_map(|label| ["--label", label]))
                    .chain(network_labels.iter().flat_map(|label| ["--label", label]))
                    .chain(network.to_args().iter().map(AsRef::as_ref)),
            )?;
        }
    }

    for volume in file.volumes.into_values() {
        let name = volume.name.as_ref().unwrap();

        if podman.run(["volume", "exists", name]).is_err() {
            if volume.external.unwrap_or_default() {
                bail!("External volume \"{name}\" not found");
            }

            let volume_labels = [("volume", name)]
                .into_iter()
                .map(|label| format!("io.podman.compose.{}={}", label.0, label.1))
                .collect::<Vec<_>>();

            podman.run(
                ["volume", "create"]
                    .into_iter()
                    .chain(labels.iter().flat_map(|label| ["--label", label]))
                    .chain(volume_labels.iter().flat_map(|label| ["--label", label]))
                    .chain(volume.to_args().iter().map(AsRef::as_ref)),
            )?;
        }
    }

    for secret in file.secrets.into_values() {
        let name = secret.name.as_ref().unwrap();

        if podman.run(["secret", "inspect", name]).is_err() {
            if secret.external.unwrap_or_default() {
                bail!("External secret \"{name}\" not found");
            }

            let secret_labels = [("secret", name)]
                .into_iter()
                .map(|label| format!("io.podman.compose.{}={}", label.0, label.1))
                .collect::<Vec<_>>();

            podman.run(
                ["secret", "create"]
                    .into_iter()
                    .chain(labels.iter().flat_map(|label| ["--label", label]))
                    .chain(secret_labels.iter().flat_map(|label| ["--label", label]))
                    .chain(secret.to_args().iter().map(AsRef::as_ref)),
            )?;
        }
    }

    Ok(())
}
