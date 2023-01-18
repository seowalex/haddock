use std::process;

use anyhow::Result;
use futures::{stream::FuturesUnordered, TryStreamExt};
use itertools::Itertools;
use tokio::{select, signal};

use crate::{
    commands::{
        create::{self, PullPolicy},
        logs, start, stop,
    },
    compose::types::Compose,
    config::Config,
    podman::{types::Container, Podman},
    progress::{Finish, Progress},
};

/// Create and start containers
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    services: Vec<String>,

    /// Detached mode: Run containers in the background
    #[arg(short, long, conflicts_with_all = ["attach", "attach_dependencies"])]
    detach: bool,

    /// Pull image before running
    #[arg(long, value_enum)]
    pull: Option<PullPolicy>,

    /// Remove containers for services not defined in the Compose file
    #[arg(long)]
    remove_orphans: bool,

    /// Produce monochrome output
    #[arg(long)]
    no_colour: bool,

    /// Don't print prefix in logs
    #[arg(long)]
    no_log_prefix: bool,

    /// Recreate containers even if their configuration and image haven't changed
    #[arg(long, conflicts_with = "no_recreate")]
    force_recreate: bool,

    /// If containers already exist, don't recreate them
    #[arg(long, conflicts_with = "force_recreate")]
    no_recreate: bool,

    /// Don't start the services after creating them
    #[arg(long)]
    no_start: bool,

    /// Use this timeout in seconds for container shutdown when attached or when containers are already running
    #[arg(short, long, default_value_t = 10)]
    timeout: u32,

    /// Show timestamps
    #[arg(long)]
    timestamps: bool,

    /// Attach to dependent containers
    #[arg(long)]
    attach_dependencies: bool,

    /// Attach to service output
    #[arg(long)]
    attach: Vec<String>,

    /// Don't attach to specified service
    #[arg(long)]
    no_attach: Vec<String>,

    /// Wait for services to be running|healthy, implies detached mode
    #[arg(long, conflicts_with_all = ["attach", "attach_dependencies"])]
    wait: bool,
}

async fn wait_containers(
    podman: &Podman,
    progress: &Progress,
    containers: &[String],
) -> Result<()> {
    containers
        .iter()
        .map(|container| async move {
            let spinner = progress.add_spinner(format!("Container {container}"), "Waiting");

            podman
                .run(["wait", "--condition", "running", container])
                .await
                .finish_with_message(spinner, "Running")
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
    create::run(
        create::Args {
            services: args.services.clone(),
            pull: args.pull,
            force_recreate: args.force_recreate,
            no_recreate: args.no_recreate,
            remove_orphans: args.remove_orphans,
        },
        podman,
        file,
        config,
    )
    .await?;

    if !args.no_start {
        start::run(
            start::Args {
                services: args.services.clone(),
            },
            podman,
            file,
            config,
        )
        .await?;

        if args.wait || !args.detach {
            let output = podman
                .force_run([
                    "ps",
                    "--all",
                    "--format",
                    "json",
                    "--filter",
                    "label=io.podman.compose.oneoff=false",
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
                                || (args.services.is_empty()
                                    && file.services.keys().contains(&service))
                            {
                                container.names.pop_front()
                            } else {
                                None
                            }
                        })
                })
                .collect::<Vec<_>>();

            if args.wait {
                let progress = Progress::new(config);

                wait_containers(podman, &progress, &containers).await?;

                progress.finish();
            } else {
                let mut services = if args.attach_dependencies {
                    file.services.keys().cloned().collect()
                } else if !args.attach.is_empty() {
                    args.attach
                } else if !args.services.is_empty() {
                    args.services
                } else {
                    file.services.keys().cloned().collect()
                };

                services.retain(|service| !args.no_attach.contains(service));

                eprintln!("Attaching to {}", containers.join(", "));

                select! {
                    biased;

                    _ = signal::ctrl_c() => {
                        eprintln!("Gracefully stopping... (press Ctrl+C again to force)");

                        stop::run(
                            stop::Args {
                                services: Vec::new(),
                                timeout: args.timeout,
                            },
                            podman,
                            file,
                            config,
                        )
                        .await?;

                        process::exit(130);
                    }
                    _ = logs::run(
                        logs::Args {
                            services,
                            follow: true,
                            since: None,
                            until: None,
                            no_color: args.no_colour,
                            no_log_prefix: args.no_log_prefix,
                            timestamps: args.timestamps,
                            tail: Some(0),
                        },
                        podman,
                        file,
                    ) => {}
                };
            }
        }
    }

    Ok(())
}
