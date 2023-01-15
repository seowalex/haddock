use anyhow::{anyhow, bail, Result};
use itertools::Itertools;

use crate::{
    compose,
    config::Config,
    podman::{types::Container, Podman},
    utils::parse_colon_delimited,
};

/// Copy files/folders between a service container and the local filesystem
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    #[arg(value_parser = parse_colon_delimited::<String, String>)]
    source: (Option<String>, String),

    #[arg(value_parser = parse_colon_delimited::<String, String>)]
    destination: (Option<String>, String),

    /// Index of the container if there are multiple instances of a service
    #[arg(long, default_value_t = 1)]
    index: usize,

    /// Archive mode (copy all uid/gid information)
    #[arg(short, long)]
    archive: bool,
}

pub(crate) async fn run(args: Args, config: &Config) -> Result<()> {
    match (&args.source.0, &args.destination.0) {
        (Some(_), Some(_)) => bail!("Copying between services is not supported"),
        (None, None) => bail!("Unknown copy direction"),
        _ => {}
    }

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
    let containers = serde_json::from_str::<Vec<Container>>(&output)?
        .into_iter()
        .filter_map(|mut container| {
            container.labels.and_then(|labels| {
                labels.service.and_then(|service| {
                    if args
                        .source
                        .0
                        .as_ref()
                        .map(|source| *source == service)
                        .unwrap_or_default()
                        || args
                            .destination
                            .0
                            .as_ref()
                            .map(|destination| *destination == service)
                            .unwrap_or_default()
                    {
                        container
                            .names
                            .pop_front()
                            .and_then(|name| labels.container_number.map(|n| (service, (n, name))))
                    } else {
                        None
                    }
                })
            })
        })
        .into_group_map();

    let [source, destination] = [args.source.0, args.destination.0].map(|service| {
        service
            .map(|service| {
                containers
                    .get(&service)
                    .ok_or_else(|| anyhow!("No container found for service \"{service}\""))
                    .and_then(|containers| {
                        containers
                            .iter()
                            .find_map(|(n, name)| if *n == args.index { Some(name) } else { None })
                            .ok_or_else(|| {
                                anyhow!(
                                    "Service \"{service}\" is not running container #{}",
                                    args.index
                                )
                            })
                    })
            })
            .transpose()
    });

    podman
        .run(
            ["cp"]
                .into_iter()
                .chain(if args.archive {
                    vec!["--archive"]
                } else {
                    vec![]
                })
                .chain([
                    format!(
                        "{}{}",
                        source?
                            .map(|container| format!("{container}:"))
                            .unwrap_or_default(),
                        args.source.1
                    )
                    .as_str(),
                    format!(
                        "{}{}",
                        destination?
                            .map(|container| format!("{container}:"))
                            .unwrap_or_default(),
                        args.destination.1
                    )
                    .as_str(),
                ]),
        )
        .await?;

    Ok(())
}
