mod commands;
mod compose;
mod config;
mod utils;

use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_with::{
    formats::CommaSeparator, serde_as, skip_serializing_none, BoolFromInt, PickFirst,
    StringWithSeparator,
};
use std::{env, path::PathBuf};

use commands::Command;
use utils::PathSeparator;

#[derive(Parser, Debug)]
#[command(version, about, next_display_order = None)]
struct Args {
    #[command(subcommand)]
    command: Command,

    #[command(flatten)]
    flags: Flags,
}

#[skip_serializing_none]
#[serde_as]
#[derive(clap::Args, Serialize, Deserialize, Debug)]
pub(crate) struct Flags {
    /// Project name
    #[arg(short, long)]
    pub(crate) project_name: Option<String>,

    /// Compose configuration files
    #[arg(short, long)]
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<PathSeparator, PathBuf>)>>")]
    pub(crate) file: Option<Vec<PathBuf>>,

    /// Specify a profile to enable
    #[arg(long)]
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<CommaSeparator, String>)>>")]
    #[serde(rename = "profiles")]
    pub(crate) profile: Option<Vec<String>>,

    /// Specify an alternate environment file
    #[arg(long)]
    pub(crate) env_file: Option<PathBuf>,

    /// Specify an alternate working directory
    #[arg(long)]
    pub(crate) project_directory: Option<PathBuf>,

    #[arg(skip)]
    pub(crate) path_separator: Option<String>,

    #[arg(skip)]
    #[serde_as(as = "Option<PickFirst<(_, BoolFromInt)>>")]
    pub(crate) ignore_orphans: Option<bool>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let config = config::load(args.flags)?;

    env::set_current_dir(&config.project_directory)?;
    commands::run(args.command, config)?;

    Ok(())
}
