use std::env;

use anyhow::{bail, Result};
use clap::ValueEnum;
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

    podman
        .run([
            "pod",
            "create",
            "--share",
            "none",
            "--label",
            &format!("io.podman.compose.config-hash={}", file.digest()),
            &env::var("COMPOSE_PROJECT_NAME")?,
        ])
        .ok();

    for (_, network) in file.networks {
        if podman
            .run(["network", "exists", network.name.as_ref().unwrap()])
            .is_err()
        {
            if network.external.unwrap_or_default() {
                bail!(
                    "External network \"{}\" not found",
                    network.name.as_ref().unwrap()
                );
            }

            podman.run(
                ["network", "create"]
                    .into_iter()
                    .map(String::from)
                    .chain(network.to_args().into_iter()),
            )?;
        }
    }

    for (_, volume) in file.volumes {
        if podman
            .run(["volume", "exists", volume.name.as_ref().unwrap()])
            .is_err()
        {
            if volume.external.unwrap_or_default() {
                bail!(
                    "External volume \"{}\" not found",
                    volume.name.as_ref().unwrap()
                );
            }

            podman.run(
                ["volume", "create"]
                    .into_iter()
                    .map(String::from)
                    .chain(volume.to_args().into_iter()),
            )?;
        }
    }

    for (_, secret) in file.secrets {
        if podman
            .run(["secret", "inspect", secret.name.as_ref().unwrap()])
            .is_err()
        {
            if secret.external.unwrap_or_default() {
                bail!(
                    "External secret \"{}\" not found",
                    secret.name.as_ref().unwrap()
                );
            }

            podman.run(
                ["secret", "create"]
                    .into_iter()
                    .map(String::from)
                    .chain(secret.to_args().into_iter()),
            )?;
        }
    }

    Ok(())
}
