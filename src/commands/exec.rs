use anyhow::{anyhow, Result};
use atty::Stream;

use crate::{
    compose::types::Compose,
    podman::{types::Container, Podman},
};

/// Execute a command in a running container
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    service: String,
    command: String,
    args: Vec<String>,

    /// Detached mode: Run command in the background
    #[arg(short, long)]
    detach: bool,

    /// Set environment variables
    #[arg(short, long)]
    env: Vec<String>,

    /// Index of the container if there are multiple instances of a service
    #[arg(long, default_value_t = 1)]
    index: usize,

    /// Give extended privileges to the process
    #[arg(long)]
    privileged: bool,

    /// Run the command as this user
    #[arg(short, long)]
    user: Option<String>,

    /// Disable pseudo-TTY allocation
    #[arg(short = 'T', long = "no-TTY", default_value_t = !atty::is(Stream::Stdout))]
    no_tty: bool,

    /// Path to workdir directory for this command
    #[arg(short, long)]
    workdir: Option<String>,
}

pub(crate) async fn run(args: Args, podman: &Podman, file: &Compose) -> Result<()> {
    let name = file.name.as_ref().unwrap();

    let output = podman
        .force_run([
            "ps",
            "--all",
            "--format",
            "json",
            "--filter",
            &format!("pod={name}"),
        ])
        .await?;
    let container = serde_json::from_str::<Vec<Container>>(&output)?
        .into_iter()
        .find_map(|mut container| {
            container.labels.and_then(|labels| {
                if labels
                    .service
                    .map(|service| args.service == service)
                    .unwrap_or_default()
                    && labels
                        .container_number
                        .map(|n| args.index == n)
                        .unwrap_or_default()
                {
                    container.names.pop_front()
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| {
            anyhow!(
                "Service \"{}\" is not running container #{}",
                args.service,
                args.index
            )
        })?;

    podman
        .attach(
            ["exec", "--interactive"]
                .into_iter()
                .chain(if args.detach {
                    vec!["--detach"]
                } else {
                    vec![]
                })
                .chain(args.env.iter().map(AsRef::as_ref))
                .chain(if args.privileged {
                    vec!["--privileged"]
                } else {
                    vec![]
                })
                .chain(if let Some(user) = args.user.as_ref() {
                    vec!["--user", user]
                } else {
                    vec![]
                })
                .chain(if args.no_tty { vec![] } else { vec!["--tty"] })
                .chain(if let Some(workdir) = args.workdir.as_ref() {
                    vec!["--workdir", workdir]
                } else {
                    vec![]
                })
                .chain([container, args.command].iter().map(AsRef::as_ref))
                .chain(args.args.iter().map(AsRef::as_ref)),
        )
        .await
}
