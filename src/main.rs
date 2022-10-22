use clap::{crate_name, crate_version, Parser, Subcommand, ValueEnum};
use serde_json::json;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
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

fn main() {
    let args = Args::parse();

    match args.command {
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
}
