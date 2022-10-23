mod commands;

use anyhow::{bail, Context, Result};
use clap::Parser;
use docker_compose_types::{Compose, ComposeFile, TopLevelVolumes};
use std::fs;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: commands::Command,

    /// Compose configuration files
    #[arg(short, long)]
    file: Option<Vec<String>>,
}

fn parse_files(paths: Option<Vec<String>>) -> Result<Compose> {
    let contents = match paths {
        Some(paths) => paths
            .into_iter()
            .map(|path| {
                fs::read_to_string(&path)
                    .with_context(|| format!("{} not found", path))
                    .map(|content| (path, content))
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => vec![(
            "compose.yaml".to_owned(),
            fs::read_to_string("compose.yaml")
                .or_else(|_| fs::read_to_string("compose.yml"))
                .or_else(|_| fs::read_to_string("docker-compose.yaml"))
                .or_else(|_| {
                    fs::read_to_string("docker-compose.yml").context("compose.yaml not found")
                })?,
        )],
    };
    let files = contents
        .into_iter()
        .map(|(path, content)| {
            serde_yaml::from_str::<ComposeFile>(&content)
                .with_context(|| format!("{} does not follow the Compose specification", path))
                .map(|file| (path, file))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut combined_file = Compose::new();

    for (path, file) in files {
        if let ComposeFile::V2Plus(file) = file {
            combined_file.version = file.version;
            combined_file.service = file.service;
            combined_file.extensions.extend(file.extensions);

            match (&mut combined_file.services, file.services) {
                (Some(combined_services), Some(services)) => combined_services.0.extend(services.0),
                (combined_services, services)
                    if combined_services.is_none() && services.is_some() =>
                {
                    *combined_services = services;
                }
                _ => {}
            }

            match (&mut combined_file.volumes, file.volumes) {
                (
                    Some(TopLevelVolumes::CV(combined_volumes)),
                    Some(TopLevelVolumes::CV(volumes)),
                ) => combined_volumes.0.extend(volumes.0),
                (
                    Some(TopLevelVolumes::Labelled(combined_volumes)),
                    Some(TopLevelVolumes::Labelled(volumes)),
                ) => combined_volumes.0.extend(volumes.0),
                (combined_volumes, volumes) if combined_volumes.is_none() && volumes.is_some() => {
                    *combined_volumes = volumes;
                }
                (_, None) => {}
                _ => bail!(
                    "{} uses a different volumes syntax from the other Compose files",
                    path
                ),
            }

            match (&mut combined_file.networks, file.networks) {
                (Some(combined_networks), Some(networks)) => combined_networks.0.extend(networks.0),
                (combined_networks, networks)
                    if combined_networks.is_none() && networks.is_some() =>
                {
                    *combined_networks = networks;
                }
                _ => {}
            }
        } else {
            bail!("{} does not follow the latest Compose specification", path);
        }
    }

    Ok(combined_file)
}

fn main() -> Result<()> {
    let args = Args::parse();
    let file = parse_files(args.file)?;

    commands::run(args.command, file)?;

    Ok(())
}
