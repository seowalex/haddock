use std::{iter::repeat_with, path::PathBuf};

use anyhow::{anyhow, Result};
use atty::Stream;
use clap::crate_version;
use fastrand::Rng;

use crate::{
    commands::{create, start},
    compose::types::{
        parse_port, parse_service_volume, Compose, FileReference, Port, Service, ServiceVolume,
        ServiceVolumeType,
    },
    config::Config,
    podman::Podman,
    utils::{parse_key_val, parse_key_val_opt},
};

/// Run a one-off command on a service
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    service: String,
    command: String,
    args: Vec<String>,

    /// Run container in background and print container ID
    #[arg(short, long)]
    detach: bool,

    /// Set environment variables
    #[arg(short, long, value_parser = parse_key_val_opt::<String, String>)]
    env: Vec<(String, Option<String>)>,

    /// Add or override a label
    #[arg(short, long, value_parser = parse_key_val::<String, String>)]
    label: Vec<(String, String)>,

    /// Automatically remove the container when it exits
    #[arg(long)]
    rm: bool,

    /// Disable pseudo-TTY allocation
    #[arg(short = 'T', long = "no-TTY", default_value_t = !atty::is(Stream::Stdout))]
    no_tty: bool,

    /// Assign a name to the container
    #[arg(long)]
    name: Option<String>,

    /// Run as specified username or uid
    #[arg(short, long)]
    user: Option<String>,

    /// Working directory inside the container
    #[arg(short, long)]
    workdir: Option<PathBuf>,

    /// Override the entrypoint of the image
    #[arg(long)]
    entrypoint: Option<String>,

    /// Don't start linked services
    #[arg(long)]
    no_deps: bool,

    /// Bind mount a volume
    #[arg(short, long, value_parser = parse_service_volume)]
    volume: Vec<ServiceVolume>,

    /// Publish a container's port(s) to the host
    #[arg(short, long, value_parser = parse_port, conflicts_with = "service_ports")]
    publish: Vec<Port>,

    /// Use the service's network useAliases in the network(s) the container connects to
    #[arg(long)]
    use_aliases: bool,

    /// Run command with the service's ports enabled and mapped to the host
    #[arg(long, conflicts_with = "publish")]
    service_ports: bool,

    /// Remove containers for services not defined in the Compose file
    #[arg(long)]
    remove_orphans: bool,
}

async fn run_container(
    podman: &Podman,
    file: &Compose,
    service: &Service,
    args: Args,
) -> Result<()> {
    let project_name = file.name.as_ref().unwrap();
    let rng = Rng::new();
    let id = hex::encode(repeat_with(|| rng.u8(..)).take(6).collect::<Vec<_>>());
    let container_name = format!("{project_name}_{}_run_{id}", args.service);

    let requirements = if args.no_deps {
        Vec::new()
    } else {
        service
            .depends_on
            .keys()
            .filter_map(|service_name| {
                file.services.get(service_name).map(|service| {
                    (1..=service
                        .deploy
                        .as_ref()
                        .and_then(|deploy| deploy.replicas)
                        .or(service.scale)
                        .unwrap_or(1))
                        .map(move |i| {
                            service
                                .container_name
                                .clone()
                                .unwrap_or_else(|| format!("{project_name}_{service_name}_{i}"))
                        })
                })
            })
            .flatten()
            .collect::<Vec<_>>()
    };

    let labels = [
        ("version", crate_version!()),
        ("project", project_name),
        ("service", &args.service),
        ("oneoff", "true"),
    ]
    .into_iter()
    .map(|label| format!("io.podman.compose.{}={}", label.0, label.1))
    .collect::<Vec<_>>();
    let pull_policy = service.pull_policy.as_ref().map(ToString::to_string);

    let networks = service
        .networks
        .iter()
        .map(|(name, network)| {
            let name = file.networks[name].name.clone().unwrap();
            let mut network = network
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default();

            if let Some(mac_address) = service.mac_address.as_ref() {
                if network.is_empty() {
                    network = format!(":mac={mac_address}");
                } else {
                    network = format!("{network},mac={mac_address}");
                }
            }

            format!("{name}{network}")
        })
        .collect::<Vec<_>>();
    let volumes = service
        .volumes
        .iter()
        .flat_map(|volume| {
            let volume = match &volume.r#type {
                ServiceVolumeType::Volume(Some(source)) => ServiceVolume {
                    r#type: ServiceVolumeType::Volume(file.volumes[source].name.clone()),
                    ..volume.clone()
                },
                _ => volume.clone(),
            };

            [
                String::from(match volume.r#type {
                    ServiceVolumeType::Volume(_) | ServiceVolumeType::Bind(_) => "--volume",
                    ServiceVolumeType::Tmpfs => "--tmpfs",
                }),
                volume.to_string(),
            ]
        })
        .collect::<Vec<_>>();
    let secrets = service
        .secrets
        .iter()
        .map(|secret| {
            FileReference {
                source: file.secrets[&secret.source].name.clone().unwrap(),
                ..secret.clone()
            }
            .to_string()
        })
        .collect::<Vec<_>>();

    let (global_args, service_args) = service.to_args();

    podman
        .attach(
            global_args
                .iter()
                .map(AsRef::as_ref)
                .chain([
                    "run",
                    "--interactive",
                    "--pod",
                    project_name,
                    "--name",
                    &container_name,
                ])
                .chain(
                    requirements
                        .iter()
                        .flat_map(|requirement| ["--requires", requirement]),
                )
                .chain(labels.iter().flat_map(|label| ["--label", label]))
                .chain(if let Some(pull_policy) = &pull_policy {
                    vec!["--pull", pull_policy]
                } else {
                    vec![]
                })
                .chain(networks.iter().flat_map(|network| ["--network", network]))
                .chain(volumes.iter().map(AsRef::as_ref))
                .chain(secrets.iter().flat_map(|secret| ["--secret", secret]))
                .chain(if args.detach {
                    vec!["--detach"]
                } else {
                    vec![]
                })
                .chain(if args.rm { vec!["--rm"] } else { vec![] })
                .chain(if args.no_tty { vec![] } else { vec!["--tty"] })
                .chain(service_args.iter().map(AsRef::as_ref)),
        )
        .await
}

pub(crate) async fn run(
    args: Args,
    podman: &Podman,
    file: &Compose,
    config: &Config,
) -> Result<()> {
    let service = file
        .services
        .get(&args.service)
        .ok_or_else(|| anyhow!("No such service: \"{}\"", args.service))?;
    let services = service.depends_on.keys().cloned().collect::<Vec<_>>();

    if !args.no_deps {
        create::run(
            create::Args {
                services: services.clone(),
                pull: None,
                force_recreate: false,
                no_recreate: false,
                remove_orphans: args.remove_orphans,
            },
            podman,
            file,
            config,
        )
        .await?;

        start::run(start::Args { services }, podman, file, config).await?;
    }

    let mut service = service.clone();

    service.command = vec![args.command.clone()];
    service.command.extend(args.args.clone());

    service.environment.extend(args.env.clone());
    service.labels.extend(args.label.clone());
    service.container_name = args.name.clone().or(service.container_name);
    service.user = args.user.clone().or(service.user);
    service.working_dir = args.workdir.clone().or(service.working_dir);
    service.entrypoint = args
        .entrypoint
        .as_ref()
        .map(|entrypoint| shell_words::split(entrypoint))
        .transpose()?
        .unwrap_or(service.entrypoint);
    service.volumes.extend(args.volume.clone());
    service.ports = if args.service_ports {
        service.ports
    } else {
        args.publish.clone()
    };

    if !args.use_aliases {
        for network in service.networks.values_mut() {
            if let Some(network) = network {
                network.aliases.clear();
            }
        }
    }

    run_container(podman, file, &service, args).await?;

    Ok(())
}
