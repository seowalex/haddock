use anyhow::{bail, Context, Result};
use clap::{crate_name, crate_version, Parser, Subcommand, ValueEnum};
use docker_compose_types::{Compose, ComposeFile, TopLevelVolumes};
use serde_json::json;
use std::fs;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,

    /// Compose configuration files
    #[arg(short, long)]
    file: Option<Vec<String>>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Converts the compose file to platform's canonical format
    #[command(alias = "config")]
    Convert {
        /// Format the output
        #[arg(long, value_enum, default_value_t = ConvertFormat::Yaml)]
        format: ConvertFormat,
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
    },
    /// Print version information
    Version {
        /// Format the output
        #[arg(short, long, value_enum, default_value_t = VersionFormat::Pretty)]
        format: VersionFormat,
        /// Show only the version number
        #[arg(long)]
        short: bool,
    },
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum ConvertFormat {
    Yaml,
    Json,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum VersionFormat {
    Pretty,
    Json,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let contents = match args.file {
        Some(paths) => paths
            .iter()
            .map(|path| {
                fs::read_to_string(path)
                    .with_context(|| format!("{} not found", path))
                    .map(|content| (path.to_string(), content))
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => vec![(
            "compose.yaml".to_string(),
            fs::read_to_string("compose.yaml")
                .or_else(|_| fs::read_to_string("compose.yml"))
                .or_else(|_| fs::read_to_string("docker-compose.yaml"))
                .or_else(|_| {
                    fs::read_to_string("docker-compose.yml").context("compose.yaml not found")
                })?,
        )],
    };
    let files = contents
        .iter()
        .map(|(path, content)| {
            serde_yaml::from_str::<ComposeFile>(content)
                .with_context(|| format!("{} does not follow the Compose specification", path))
                .map(|file| (path, file))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut combined_file = Compose::new();

    for (path, file) in &files {
        if let ComposeFile::V2Plus(file) = file {
            combined_file.version = file.version.to_owned();
            combined_file.service = file.service.to_owned();
            combined_file.extensions.extend(file.extensions.to_owned());

            match (&mut combined_file.services, &file.services) {
                (Some(combined_services), Some(services)) => {
                    combined_services.0.extend(services.to_owned().0)
                }
                (None, _) => combined_file.services = file.services.to_owned(),
                _ => {}
            }

            match (&mut combined_file.volumes, &file.volumes) {
                (
                    Some(TopLevelVolumes::CV(combined_volumes)),
                    Some(TopLevelVolumes::CV(volumes)),
                ) => combined_volumes.0.extend(volumes.to_owned().0),
                (
                    Some(TopLevelVolumes::Labelled(combined_volumes)),
                    Some(TopLevelVolumes::Labelled(volumes)),
                ) => combined_volumes.0.extend(volumes.to_owned().0),
                (None, _) => combined_file.volumes = file.volumes.to_owned(),
                (Some(..), None) => {}
                _ => bail!(
                    "{} uses a different volumes syntax from the other Compose files",
                    path
                ),
            }

            match (&mut combined_file.networks, &file.networks) {
                (Some(combined_networks), Some(networks)) => {
                    combined_networks.0.extend(networks.to_owned().0)
                }
                (None, _) => combined_file.networks = file.networks.to_owned(),
                _ => {}
            }
        } else {
            bail!("{} does not follow the latest Compose specification", path);
        }
    }

    match args.command {
        Command::Convert {
            format,
            quiet,
            services,
            volumes,
            profiles,
            images,
        } => {
            if services {
                if let Some(services) = combined_file.services {
                    for service in services.0 {
                        println!("{}", service.0);
                    }
                }
            } else if volumes {
                if let Some(volumes) = combined_file.volumes {
                    match volumes {
                        TopLevelVolumes::CV(volumes) => {
                            for volume in volumes.0 {
                                println!("{}", volume.0);
                            }
                        }
                        TopLevelVolumes::Labelled(volumes) => {
                            for volume in volumes.0 {
                                println!("{}", volume.0);
                            }
                        }
                    }
                }

                todo!();
            } else if profiles {
                todo!();
            } else if images {
                todo!();
            } else {
                match format {
                    ConvertFormat::Yaml => {
                        if !quiet {
                            print!("{}", serde_yaml::to_string(&combined_file)?);
                        }
                    }
                    ConvertFormat::Json => {
                        if !quiet {
                            print!("{}", serde_json::to_string_pretty(&combined_file)?);
                        }
                    }
                };
            }
        }
        Command::Version { format, short } => {
            if short {
                println!(crate_version!());
            } else {
                match format {
                    VersionFormat::Pretty => println!("{} {}", crate_name!(), crate_version!()),
                    VersionFormat::Json => println!("{}", json!({ "version": crate_version!() })),
                }
            }
        }
    }

    Ok(())
}
