use anyhow::Result;
use console::Style;
use futures::stream::{select_all, TryStreamExt};
use itertools::Itertools;

use crate::{
    compose::types::Compose,
    podman::{types::Container, Podman},
};

/// View output from containers
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    services: Vec<String>,

    /// Follow log output
    #[arg(short, long)]
    follow: bool,

    /// Show logs since timestamp (e.g. 2013-01-02T13:23:37Z) or relative (e.g. 42m for 42 minutes)
    #[arg(long)]
    since: Option<String>,

    /// Show logs before a timestamp (e.g. 2013-01-02T13:23:37Z) or relative (e.g. 42m for 42 minutes)
    #[arg(long)]
    until: Option<String>,

    /// Produce monochrome output
    #[arg(long)]
    no_color: bool,

    /// Don't print prefix in logs
    #[arg(long)]
    no_log_prefix: bool,

    /// Show timestamps
    #[arg(short, long)]
    timestamps: bool,

    /// Number of lines to show from the end of the logs for each container
    #[arg(long)]
    tail: Option<u32>,
}

pub(crate) async fn run(args: Args, podman: &Podman, file: &Compose) -> Result<()> {
    let name = file.name.as_ref().unwrap();
    let tail = args.tail.map(|tail| tail.to_string());

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
    let width = containers.iter().map(String::len).max().unwrap_or_default();
    let colours = ["cyan", "yellow", "green", "magenta", "blue"];

    let mut output = select_all(
        containers
            .into_iter()
            .enumerate()
            .map(|(i, container)| {
                podman
                    .watch(
                        ["logs"]
                            .into_iter()
                            .chain(if args.follow {
                                vec!["--follow"]
                            } else {
                                vec![]
                            })
                            .chain(if let Some(since) = args.since.as_ref() {
                                vec!["--since", since]
                            } else {
                                vec![]
                            })
                            .chain(if let Some(until) = args.until.as_ref() {
                                vec!["--until", until]
                            } else {
                                vec![]
                            })
                            .chain(if args.timestamps {
                                vec!["--timestamps"]
                            } else {
                                vec![]
                            })
                            .chain(if let Some(tail) = tail.as_ref() {
                                vec!["--tail", tail]
                            } else {
                                vec![]
                            })
                            .chain([container.as_str()]),
                    )
                    .map(|stream| {
                        let i = i % (colours.len() * 2);

                        let style = if args.no_color {
                            Style::new()
                        } else {
                            if i < colours.len() {
                                Style::from_dotted_str(colours[i])
                            } else {
                                Style::from_dotted_str(&format!(
                                    "{}.bright",
                                    colours[i - colours.len()]
                                ))
                            }
                        };

                        stream.map_ok(move |line| {
                            if args.no_log_prefix {
                                line
                            } else {
                                format!(
                                    "{} {line}",
                                    style.apply_to(format!("{container:width$}  |"))
                                )
                            }
                        })
                    })
            })
            .collect::<Result<Vec<_>>>()?,
    );

    while let Some(line) = output.try_next().await? {
        println!("{line}");
    }

    Ok(())
}
