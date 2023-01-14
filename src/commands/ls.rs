use anyhow::Result;
use clap::ValueEnum;

use crate::{config::Config, podman::Podman};

/// List running compose projects
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    /// Format the output
    #[arg(long, value_enum, default_value_t = Format::Table)]
    format: Format,

    /// Only display IDs
    #[arg(short, long)]
    quiet: bool,

    /// Filter output based on conditions provided
    #[arg(long)]
    filter: Vec<String>,
}

#[derive(ValueEnum, PartialEq, Clone, Debug)]
enum Format {
    Table,
    Json,
}

pub(crate) async fn run(args: Args, config: Config) -> Result<()> {
    let podman = Podman::new(&config).await?;

    if args.quiet {
        print!(
            "{}",
            podman
                .run(
                    [
                        "pod",
                        "ps",
                        "--quiet",
                        "--filter",
                        "label=io.podman.compose.project"
                    ]
                    .into_iter()
                    .chain(args.filter.iter().flat_map(|filter| ["--filter", filter]))
                )
                .await?
        );
    } else {
        print!(
            "{}",
            podman
                .run(
                    ["pod", "ps", "--filter", "label=io.podman.compose.project"]
                        .into_iter()
                        .chain([
                            "--format",
                            match args.format {
                                Format::Table =>
                                    "table {{.Name}} {{.Status}} {{.Created}} {{.NumberOfContainers}}",
                                Format::Json => "json",
                            }
                        ])
                        .chain(args.filter.iter().flat_map(|filter| ["--filter", filter]))
                )
                .await?
        );
    }

    Ok(())
}
