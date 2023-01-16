use anyhow::Result;
use futures::{stream::FuturesUnordered, TryFutureExt, TryStreamExt};
use itertools::Itertools;

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
    let name = file.name.as_ref().unwrap();

    let output = podman
        .force_run([
            "ps",
            "--all",
            "--format",
            "json",
            "--filter",
            "status=running",
            "--filter",
            &format!("pod={name}"),
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
                        || (args.services.is_empty() && file.services.keys().contains(&service))
                    {
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
                        .map_ok(move |output| (container, output))
                })
                .collect::<FuturesUnordered<_>>()
                .try_collect::<Vec<_>>()
                .await?
                .into_iter()
                .map(|(container, output)| format!("{container}\n{output}"))
                .join("\n")
        );
    }

    Ok(())
}
