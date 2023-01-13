pub(crate) mod types;

use std::{ffi::OsStr, path::PathBuf, pin::Pin, process::Stdio};

use anyhow::{anyhow, bail, Context, Error, Result};
use futures::{
    stream::{self, select},
    Stream, StreamExt, TryStreamExt,
};
use itertools::Itertools;
use once_cell::sync::Lazy;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use tokio_stream::wrappers::LinesStream;

use self::types::Version;
use crate::config::Config;

static PODMAN_MIN_SUPPORTED_VERSION: Lazy<semver::Version> =
    Lazy::new(|| semver::Version::new(4, 3, 0));

pub(crate) struct Podman {
    project_directory: PathBuf,
    dry_run: bool,
}

impl Podman {
    pub(crate) async fn new(config: &Config) -> Result<Self> {
        let podman = Self {
            project_directory: config.project_directory.clone(),
            dry_run: config.dry_run,
        };
        let output = podman.force_run(["version", "--format", "json"]).await?;
        let version = serde_json::from_str::<Version>(&output)
            .with_context(|| anyhow!("Podman version not recognised"))?
            .client
            .version;

        if version < *PODMAN_MIN_SUPPORTED_VERSION {
            bail!(
                "Only Podman {} and above is supported: version {version} found",
                *PODMAN_MIN_SUPPORTED_VERSION
            );
        }

        Ok(podman)
    }

    fn command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new("podman");
        command.current_dir(&self.project_directory).args(args);

        command
    }

    pub(crate) async fn run<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        if self.dry_run {
            println!(
                "`podman {}`",
                args.into_iter()
                    .map(|arg| arg.as_ref().to_string_lossy().to_string())
                    .join(" "),
            );

            Ok(String::new())
        } else {
            self.force_run(args).await
        }
    }

    pub(crate) async fn force_run<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.command(args);

        let output = command.output().await.with_context(|| {
            anyhow!(
                "`{} {}` cannot be executed",
                command.as_std().get_program().to_string_lossy(),
                command
                    .as_std()
                    .get_args()
                    .map(OsStr::to_string_lossy)
                    .join(" ")
            )
        })?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(
                anyhow!("{}", String::from_utf8_lossy(&output.stderr)).context(anyhow!(
                    "`{} {}` returned an error",
                    command.as_std().get_program().to_string_lossy(),
                    command
                        .as_std()
                        .get_args()
                        .map(OsStr::to_string_lossy)
                        .join(" ")
                )),
            )
        }
    }

    pub(crate) fn watch<I, S>(&self, args: I) -> Result<Pin<Box<dyn Stream<Item = Result<String>>>>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        if self.dry_run {
            println!(
                "`podman {}`",
                args.into_iter()
                    .map(|arg| arg.as_ref().to_string_lossy().to_string())
                    .join(" "),
            );

            Ok(stream::empty().boxed())
        } else {
            let child = self
                .command(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            let stdout = BufReader::new(child.stdout.unwrap()).lines();
            let stderr = BufReader::new(child.stderr.unwrap()).lines();

            Ok(select(LinesStream::new(stdout), LinesStream::new(stderr))
                .map_err(Error::from)
                .boxed())
        }
    }
}
