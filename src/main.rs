use anyhow::{bail, Context, Result};
use clap::{crate_name, crate_version, Parser, Subcommand, ValueEnum};
use docker_compose_types::ComposeFile;
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
    Convert,
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
enum VersionFormat {
    Pretty,
    Json,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let contents = match args.file {
        Some(paths) => paths
            .iter()
            .map(|path| fs::read_to_string(path).with_context(|| format!("{} not found", path)))
            .collect::<Result<Vec<_>, _>>()?,
        None => vec![fs::read_to_string("compose.yaml")
            .or_else(|_| fs::read_to_string("compose.yml"))
            .or_else(|_| fs::read_to_string("docker-compose.yaml"))
            .or_else(|_| {
                fs::read_to_string("docker-compose.yml").context("compose.yaml not found")
            })?],
    };
    let files = contents
        .iter()
        .map(|content| serde_yaml::from_str::<ComposeFile>(content))
        .collect::<Result<Vec<_>, _>>()?;

    if files
        .iter()
        .any(|file| !matches!(file, ComposeFile::V2Plus(_)))
    {
        bail!("Only the latest Compose specification is supported");
    }

    match args.command {
        Command::Convert => {
            println!("{:#?}", files);
        }
        Command::Version { format, short } => {
            if short {
                println!(crate_version!());
            } else {
                match format {
                    VersionFormat::Pretty => {
                        println!("{} {}", crate_name!(), crate_version!());
                    }
                    VersionFormat::Json => {
                        println!("{}", json!({ "version": crate_version!() }))
                    }
                }
            }
        }
    }

    Ok(())
}
