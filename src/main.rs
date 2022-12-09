mod commands;
mod compose;

use anyhow::Result;
use clap::Parser;
use figment::{
    providers::{Env, Serialized},
    Figment,
};
use serde::{Deserialize, Serialize};
use serde_with::{
    formats::{CommaSeparator, Separator},
    serde_as, skip_serializing_none, BoolFromInt, PickFirst, StringWithSeparator,
};
use std::{
    env,
    path::{Path, PathBuf},
};

#[derive(Parser, Debug)]
#[command(version, about, next_display_order = None)]
struct Args {
    #[command(subcommand)]
    command: commands::Command,

    #[command(flatten)]
    config: Config,
}

struct PathSeparator;

impl Separator for PathSeparator {
    fn separator() -> &'static str {
        Box::leak(
            env::var("COMPOSE_PATH_SEPARATOR")
                .unwrap_or_else(|_| String::from(":"))
                .into_boxed_str(),
        )
    }
}

#[skip_serializing_none]
#[serde_as]
#[derive(clap::Args, Serialize, Deserialize, Debug)]
struct Config {
    /// Project name
    #[arg(short, long)]
    project_name: Option<String>,

    /// Compose configuration files
    #[arg(short, long)]
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<PathSeparator, String>)>>")]
    file: Option<Vec<String>>,

    /// Specify a profile to enable
    #[arg(long = "profile")]
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<CommaSeparator, String>)>>")]
    profiles: Option<Vec<String>>,

    /// Specify an alternate environment file
    #[arg(long)]
    env_file: Option<String>,

    /// Specify an alternate working directory
    #[arg(long)]
    project_directory: Option<String>,

    #[arg(skip)]
    #[serde_as(as = "Option<PickFirst<(_, BoolFromInt)>>")]
    convert_windows_paths: Option<bool>,

    #[arg(skip)]
    path_separator: Option<String>,

    #[arg(skip)]
    #[serde_as(as = "Option<PickFirst<(_, BoolFromInt)>>")]
    ignore_orphans: Option<bool>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut config = Figment::new()
        .merge(Env::prefixed("COMPOSE_"))
        .merge(Serialized::defaults(&args.config))
        .extract::<Config>()?;

    if let Some(file) = &config.file {
        config.project_directory = file.first().and_then(|file| {
            Path::new(file)
                .parent()
                .and_then(|dir| dir.to_str().map(String::from))
        });
    }

    dotenvy::from_filename(
        config
            .env_file
            .to_owned()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                config
                    .project_directory
                    .to_owned()
                    .map(PathBuf::from)
                    .unwrap_or_default()
                    .join(Path::new(".env"))
            }),
    )
    .ok();

    let config = Figment::new()
        .merge(Env::prefixed("COMPOSE_"))
        .merge(Serialized::defaults(args.config))
        .extract::<Config>()?;

    commands::run(args.command, config)?;

    Ok(())
}
