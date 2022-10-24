use anyhow::Result;
use clap::ValueEnum;
use docker_compose_types::{Compose, TopLevelVolumes};
use itertools::Itertools;
use std::{collections::HashSet, fs};

/// Converts the compose file to platform's canonical format
#[derive(clap::Args, Debug)]
#[command(alias = "config")]
pub(crate) struct Args {
    /// Format the output
    #[arg(long, value_enum, default_value_t = Format::Yaml)]
    format: Format,
    /// Only validate the configuration, don't print anything
    #[arg(short, long)]
    quiet: bool,
    /// Print the service names, one per line
    #[arg(long)]
    services: bool,
    /// Print the volume names, one per line
    #[arg(long)]
    volumes: bool,
    /// Print the profile names, one per line
    #[arg(long)]
    profiles: bool,
    /// Print the image names, one per line
    #[arg(long)]
    images: bool,
    /// Save to file (default to stdout)
    #[arg(short, long)]
    output: Option<String>,
}

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq, Debug)]
enum Format {
    Yaml,
    Json,
}

pub(crate) fn run(args: Args, file: Compose) -> Result<()> {
    if args.services {
        if let Some(services) = file.services {
            for service in services.0 {
                println!("{}", service.0);
            }
        }
    } else if args.volumes {
        match file.volumes {
            Some(TopLevelVolumes::CV(volumes)) => {
                for volume in &volumes.0 {
                    println!("{}", volume.0);
                }
            }
            Some(TopLevelVolumes::Labelled(volumes)) => {
                for volume in &volumes.0 {
                    println!("{}", volume.0);
                }
            }
            None => {}
        }
    } else if args.profiles {
        if let Some(services) = file.services {
            let mut all_profiles = HashSet::new();

            for service in services.0 {
                if let Some(profiles) = service.1.and_then(|service| service.profiles) {
                    all_profiles.extend(profiles);
                }
            }

            for profile in all_profiles.into_iter().sorted() {
                println!("{profile}");
            }
        }
    } else if args.images {
        if let Some(services) = file.services {
            for service in services.0 {
                if let Some(image) = service.1.and_then(|service| service.image) {
                    println!("{image}");
                }
            }
        }
    } else {
        match args.format {
            Format::Yaml => {
                let contents = serde_yaml::to_string(&file)?;

                if !args.quiet && args.output.is_none() {
                    print!("{contents}");
                }

                if let Some(path) = args.output {
                    fs::write(path, contents)?;
                }
            }
            Format::Json => {
                let contents = serde_json::to_string_pretty(&file)?;

                if !args.quiet && args.output.is_none() {
                    println!("{contents}");
                }

                if let Some(path) = args.output {
                    fs::write(path, contents + "\n")?;
                }
            }
        };
    }

    Ok(())
}
