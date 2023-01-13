use anyhow::Result;
use clap::ValueEnum;
use indexmap::IndexSet;
use itertools::Itertools;

use crate::{
    compose,
    config::Config,
    podman::{types::Container, Podman},
};

/// List containers
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    services: Vec<String>,

    /// Format the output
    #[arg(long, value_enum, default_value_t = Format::Table)]
    format: Format,

    /// Filter services by a property
    #[arg(long)]
    filter: Vec<String>,

    /// Filter services by status
    #[arg(long)]
    status: Vec<Status>,

    /// Only display IDs
    #[arg(short, long)]
    quiet: bool,

    /// Display services
    #[arg(long = "services")]
    service: bool,

    /// Show all stopped containers (including those created by the run command)
    #[arg(short, long)]
    all: bool,
}

#[derive(ValueEnum, PartialEq, Clone, Debug)]
enum Format {
    Table,
    Json,
}

#[derive(ValueEnum, Clone, Debug)]
enum Status {
    Created,
    Exited,
    Paused,
    Running,
    Unknown,
}

pub(crate) async fn run(args: Args, config: Config) -> Result<()> {
    let podman = Podman::new(&config).await?;
    let file = compose::parse(&config, false)?;
    let name = file.name.as_ref().unwrap();

    if args.quiet {
        print!(
            "{}",
            podman
                .run(
                    [
                        "ps",
                        "--quiet",
                        "--no-trunc",
                        "--filter",
                        &format!("pod={name}")
                    ]
                    .into_iter()
                    .chain(if args.all { vec!["--all"] } else { vec![] }),
                )
                .await?
        );
    } else if args.service {
        let output = podman
            .run(
                ["ps", "--format", "json", "--filter", &format!("pod={name}")]
                    .into_iter()
                    .chain(if args.all { vec!["--all"] } else { vec![] }),
            )
            .await?;
        let services = serde_json::from_str::<Vec<Container>>(&output)?
            .into_iter()
            .filter_map(|container| {
                container
                    .labels
                    .and_then(|labels| labels.service)
                    .filter(|service| {
                        args.services.contains(service)
                            || (args.services.is_empty() && file.services.keys().contains(service))
                    })
            })
            .collect::<IndexSet<_>>();

        for service in services {
            println!("{service}");
        }
    } else {
        let statuses = args
            .status
            .into_iter()
            .map(|status| format!("status={status:?}").to_ascii_lowercase())
            .collect::<Vec<_>>();

        print!(
            "{}",
            podman
                .run(
                    ["ps", "--filter", &format!("pod={name}")]
                        .into_iter()
                        .chain(if args.all { vec!["--all"] } else { vec![] })
                        .chain(if args.format == Format::Json {
                            vec!["--format", "json"]
                        } else {
                            vec![]
                        })
                        .chain(args.filter.iter().flat_map(|filter| ["--filter", filter]))
                        .chain(statuses.iter().flat_map(|status| ["--filter", status]))
                )
                .await?
        );
    }

    Ok(())
}
