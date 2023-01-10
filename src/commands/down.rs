use anyhow::Result;
use futures::try_join;

use crate::{
    compose,
    config::Config,
    podman::Podman,
    progress::{Finish, Progress},
};

/// Stop and remove containers, networks
#[derive(clap::Args, Debug)]
#[command(next_display_order = None)]
pub(crate) struct Args {
    /// Specify a shutdown timeout in seconds
    #[arg(short, long, default_value_t = 10)]
    timeout: i32,

    /// Remove named volumes declared in the `volumes` section of the Compose file
    #[arg(short, long)]
    volumes: bool,

    /// Remove images used by services
    #[arg(long)]
    rmi: bool,
}

pub(crate) async fn run(args: Args, config: Config) -> Result<()> {
    let podman = Podman::new(&config);
    let file = compose::parse(&config, false)?;
    let name = file.name.as_ref().unwrap();
    let progress = Progress::new(&config);
    let podman = podman.await?;
    let spinner = progress.add_spinner(format!("Pod {name}"), "Removing");

    podman
        .run([
            "pod",
            "rm",
            "--force",
            "--time",
            &args.timeout.to_string(),
            name,
        ])
        .await
        .finish_with_message(spinner, "Removed")?;

    progress.finish();

    if args.volumes || args.rmi {
        let progress = Progress::new(&config);

        try_join!(
            async {
                if args.volumes {
                    let spinner = progress.add_spinner("Volumes", "Removing");

                    podman
                        .run([
                            "volume",
                            "prune",
                            "--force",
                            "--filter",
                            &format!("label=project={name}"),
                        ])
                        .await
                        .finish_with_message(spinner, "Removed")?;
                }

                anyhow::Ok(())
            },
            async {
                if args.rmi {
                    let spinner = progress.add_spinner("Images", "Removing");

                    podman
                        .run([
                            "image",
                            "prune",
                            "--force",
                            "--filter",
                            &format!("label=project={name}"),
                        ])
                        .await
                        .finish_with_message(spinner, "Removed")?;
                }

                anyhow::Ok(())
            }
        )?;

        progress.finish();
    }

    Ok(())
}
