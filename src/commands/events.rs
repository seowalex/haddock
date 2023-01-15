use anyhow::Result;
use futures::stream::TryStreamExt;
use indexmap::IndexSet;
use itertools::Itertools;

use crate::{
    compose,
    config::Config,
    podman::{types::Container, Podman},
};

/// Receive real time events from containers
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    services: Vec<String>,

    /// Output events as a stream of json objects
    #[arg(long)]
    json: bool,
}

pub(crate) async fn run(args: Args, config: &Config) -> Result<()> {
    let podman = Podman::new(config).await?;
    let file = compose::parse(config, false)?;
    let name = file.name.as_ref().unwrap();

    let output = podman
        .force_run([
            "ps",
            "--all",
            "--format",
            "json",
            "--filter",
            &format!("pod={name}"),
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

    Ok(())
}
