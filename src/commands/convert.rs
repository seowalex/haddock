use anyhow::Result;
use clap::ValueEnum;
use indexmap::IndexSet;
use itertools::Itertools;
use std::{collections::HashSet, fs};

use crate::compose;

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

pub(crate) fn run(args: Args, paths: Option<Vec<String>>) -> Result<()> {
    let file = compose::parse(paths)?;

    if args.services {
        for (name, _) in file.services {
            println!("{name}");
        }
    } else if args.volumes {
        if let Some(volumes) = file.volumes {
            for (name, _) in volumes {
                println!("{name}");
            }
        }
    } else if args.profiles {
        let mut all_profiles = IndexSet::new();

        for (_, service) in file.services {
            if let Some(profiles) = service.profiles {
                all_profiles.extend(profiles);
            }
        }

        for profile in all_profiles {
            println!("{profile}");
        }
    } else if args.images {
        for (_, service) in file.services {
            println!("{}", service.image);
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
                let mut contents = serde_json::to_string_pretty(&file)?;
                contents.push('\n');

                if !args.quiet && args.output.is_none() {
                    print!("{contents}");
                }

                if let Some(path) = args.output {
                    fs::write(path, contents)?;
                }
            }
        }
    }

    Ok(())
}
