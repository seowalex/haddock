use anyhow::Result;
use futures::{stream::FuturesUnordered, TryFutureExt, TryStreamExt};

use crate::{
    compose::types::Compose,
    podman::{types::Container, Podman},
};

/// Display the running processes
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    services: Vec<String>,
}

pub(crate) async fn run(args: Args, podman: &Podman, file: &Compose) -> Result<()> {
    let output = podman
        .force_run([
            "ps",
            "--all",
            "--format",
            "json",
            "--filter",
            "status=running",
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
                    if args.services.is_empty() || args.services.contains(&service) {
                        container.names.pop_front()
                    } else {
                        None
                    }
                })
        })
        .collect::<Vec<_>>();

    if !containers.is_empty() {
        print!(
            "{}",
            containers
                .iter()
                .map(|container| {
                    podman
                        .run(["top", container])
                        .map_ok(move |output| format!("{container}\n{output}"))
                })
                .collect::<FuturesUnordered<_>>()
                .try_collect::<Vec<_>>()
                .await?
                .join("\n")
        );
    }

    Ok(())
}
