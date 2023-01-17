use anyhow::Result;
use clap::ValueEnum;
use indexmap::IndexSet;

use crate::{
    compose::types::Compose,
    podman::{types::Container, Podman},
};

/// List images used by the created containers
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    services: Vec<String>,

    /// Format the output
    #[arg(long, value_enum, default_value_t = Format::Table)]
    format: Format,

    /// Only display IDs
    #[arg(short, long)]
    quiet: bool,
}

#[derive(ValueEnum, PartialEq, Clone, Debug)]
enum Format {
    Table,
    Json,
}

pub(crate) async fn run(args: Args, podman: &Podman, file: &Compose) -> Result<()> {
    let output = podman
        .force_run([
            "ps",
            "--all",
            "--format",
            "json",
            "--filter",
            &format!("pod={}", file.name.as_ref().unwrap()),
        ])
        .await?;
    let images = serde_json::from_str::<Vec<Container>>(&output)?
        .into_iter()
        .filter_map(|container| {
            container
                .labels
                .and_then(|labels| labels.service)
                .and_then(|service| {
                    if args.services.is_empty() || args.services.contains(&service) {
                        Some(container.image_id)
                    } else {
                        None
                    }
                })
        })
        .collect::<IndexSet<_>>();

    if args.quiet {
        for image in images {
            println!("{image}");
        }
    } else if !images.is_empty() {
        let filters = images
            .into_iter()
            .map(|image| format!("id={image}"))
            .collect::<Vec<_>>();

        print!(
            "{}",
            podman
                .run(
                    ["images"]
                        .into_iter()
                        .chain(if args.format == Format::Json {
                            vec!["--format", "json"]
                        } else {
                            vec![]
                        })
                        .chain(filters.iter().flat_map(|filter| ["--filter", filter]))
                )
                .await?
        );
    }

    Ok(())
}
