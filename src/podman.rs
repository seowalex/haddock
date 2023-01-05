mod types;

use std::{ffi::OsStr, fmt::Write, path::PathBuf, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressState, ProgressStyle};
use itertools::Itertools;
use once_cell::sync::Lazy;
use tokio::process::Command;

use self::types::Version;
use crate::config::Config;

static PODMAN_MIN_SUPPORTED_VERSION: Lazy<semver::Version> =
    Lazy::new(|| semver::Version::new(4, 3, 0));
static SPINNER_STYLE: Lazy<ProgressStyle> = Lazy::new(|| {
    ProgressStyle::with_template(
        " {spinner:.blue} {prefix:.blue} {wide_msg:.blue} {elapsed:.blue} ",
    )
    .unwrap()
    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "⠿"])
    .with_key("elapsed", |state: &ProgressState, w: &mut dyn Write| {
        write!(w, "{:.1}s", state.elapsed().as_secs_f64()).unwrap()
    })
});

#[derive(Debug)]
pub(crate) struct Podman {
    pub(crate) progress: MultiProgress,
    project_directory: PathBuf,
    verbose: bool,
}

impl Podman {
    pub(crate) async fn new(config: &Config) -> Result<Self> {
        let podman = Podman {
            progress: MultiProgress::new(),
            project_directory: config.project_directory.clone(),
            verbose: config.verbose,
        };
        let output = podman.run(["version", "--format", "json"]).await?;
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

    pub(crate) async fn run<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new("podman");
        command.current_dir(&self.project_directory).args(args);

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

        if self.verbose {
            self.progress
                .println(format!(
                    "`{} {}`",
                    command.as_std().get_program().to_string_lossy(),
                    command
                        .as_std()
                        .get_args()
                        .map(OsStr::to_string_lossy)
                        .join(" "),
                ))
                .unwrap();
        }

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

    pub(crate) fn add_spinner(&self) -> ProgressBar {
        let spinner = self.progress.add(ProgressBar::new(0));

        spinner.enable_steady_tick(Duration::from_millis(100));
        spinner.set_style(SPINNER_STYLE.clone());

        spinner
    }
}
