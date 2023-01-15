use std::fmt::{self, Display, Formatter};

use anyhow::Result;
use clap::ValueEnum;
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

impl Display for Status {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format!("{self:?}").to_ascii_lowercase())
    }
}

pub(crate) async fn run(args: Args, config: &Config) -> Result<()> {
    let podman = Podman::new(config).await?;
    let file = compose::parse(config, false)?;
    let name = file.name.as_ref().unwrap();

    let filters = args
        .status
        .into_iter()
        .map(|status| format!("status={status}"))
        .chain(args.filter)
        .collect::<Vec<_>>();
    let output = podman
        .force_run(
            ["ps", "--format", "json", "--filter", &format!("pod={name}")]
                .into_iter()
                .chain(if args.all { vec!["--all"] } else { vec![] })
                .chain(filters.iter().flat_map(|filter| ["--filter", filter])),
        )
        .await?;
    let containers = serde_json::from_str::<Vec<Container>>(&output)?
        .into_iter()
        .filter_map(|container| {
            container
                .labels
                .as_ref()
                .and_then(|labels| labels.service.clone())
                .and_then(|service| {
                    if args.services.contains(&service)
                        || (args.services.is_empty() && file.services.keys().contains(&service))
                    {
                        Some((service, container))
                    } else {
                        None
                    }
                })
        })
        .into_group_map();

    if args.quiet {
        for container in containers.into_values().flatten() {
            println!("{}", container.id);
        }
    } else if args.service {
        for service in containers.into_keys() {
            println!("{service}");
        }
    } else {
        let filters = if containers.is_empty() {
            vec![String::from("name=")]
                .into_iter()
                .chain(filters)
                .collect::<Vec<_>>()
        } else {
            containers
                .into_values()
                .flatten()
                .filter_map(|container| {
                    container.names.front().map(|name| format!("name=^{name}$"))
                })
                .chain(filters)
                .collect::<Vec<_>>()
        };

        print!(
            "{}",
            podman
                .run(
                    ["ps", "--filter", &format!("pod={name}")]
                        .into_iter()
                        .chain(if args.all { vec!["--all"] } else { vec![] })
                        .chain([
                            "--format",
                            match args.format {
                                Format::Table =>
                                    "table {{.Names}} {{.ID}} {{.Image}} {{.Command}} {{.CreatedHuman}} {{.Status}} {{.Ports}}",
                                Format::Json => "json",
                            }
                        ])
                        .chain(filters.iter().flat_map(|filter| ["--filter", filter]))
                )
                .await?
        );
    }

    Ok(())
}
