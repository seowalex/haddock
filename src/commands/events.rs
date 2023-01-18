use anyhow::Result;
use futures::TryStreamExt;
use indexmap::IndexSet;
use itertools::Itertools;

use crate::{
    compose::types::Compose,
    podman::{types::Container, Podman},
};

/// Receive real time events from containers
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    services: Vec<String>,

    /// Output events as a stream of JSON objects
    #[arg(long)]
    json: bool,
}

pub(crate) async fn run(args: Args, podman: &Podman, file: &Compose) -> Result<()> {
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
    let services = serde_json::from_str::<Vec<Container>>(&output)?
        .into_iter()
        .filter_map(|container| {
            container
                .labels
                .and_then(|labels| labels.service)
                .and_then(|service| {
                    if args.services.contains(&service)
                        || (args.services.is_empty() && file.services.keys().contains(&service))
                    {
                        Some(format!("label=io.podman.compose.service={service}"))
                    } else {
                        None
                    }
                })
        })
        .collect::<IndexSet<_>>();

    if !services.is_empty() {
        let mut output = podman.watch(
            ["events"]
                .into_iter()
                .chain(if args.json {
                    vec!["--format", "json"]
                } else {
                    vec![]
                })
                .chain(services.iter().flat_map(|service| ["--filter", service])),
        )?;

        while let Some(line) = output.try_next().await? {
            println!("{line}");
        }
    }

    Ok(())
}
