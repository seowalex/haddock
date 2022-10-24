use anyhow::Result;
use clap::{crate_name, crate_version, ValueEnum};
use serde_json::json;

/// Print version information
#[derive(clap::Args, Debug)]
pub(crate) struct Args {
    /// Format the output
    #[arg(short, long, value_enum, default_value_t = Format::Pretty)]
    format: Format,
    /// Show only the version number
    #[arg(long)]
    short: bool,
}

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq, Debug)]
enum Format {
    Pretty,
    Json,
}

pub(crate) fn run(args: Args) -> Result<()> {
    if args.short {
        println!(crate_version!());
    } else {
        match args.format {
            Format::Pretty => println!("{} {}", crate_name!(), crate_version!()),
            Format::Json => println!("{}", json!({ "version": crate_version!() })),
        }
    }

    Ok(())
}
