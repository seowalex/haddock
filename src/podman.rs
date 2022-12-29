pub(crate) mod types;

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, bail, Context, Result};
use itertools::Itertools;
use once_cell::sync::Lazy;

use self::types::Version;

static PODMAN_MIN_SUPPORTED_VERSION: Lazy<semver::Version> =
    Lazy::new(|| semver::Version::new(4, 3, 0));

#[derive(Debug)]
pub(crate) struct Podman {
    project_directory: PathBuf,
}

impl Podman {
    pub(crate) fn new(project_directory: &Path) -> Result<Self> {
        let mut command = Command::new("podman");

        command
            .current_dir(project_directory)
            .args(["version", "--format", "json"]);

        let output = command
            .output()
            .with_context(|| {
                anyhow!(
                    "`{} {}` cannot be executed",
                    command.get_program().to_string_lossy(),
                    command.get_args().map(OsStr::to_string_lossy).join(" ")
                )
            })?
            .stdout;
        let data = String::from_utf8_lossy(&output);
        let version = serde_json::from_str::<Version>(&data)
            .with_context(|| anyhow!("Podman version not recognised"))?
            .client
            .version;

        if version < *PODMAN_MIN_SUPPORTED_VERSION {
            bail!(
                "Only Podman {} and above is supported: version {version} found",
                *PODMAN_MIN_SUPPORTED_VERSION
            );
        }

        Ok(Podman {
            project_directory: project_directory.to_path_buf(),
        })
    }

    pub(crate) fn run<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new("podman");
        command.current_dir(&self.project_directory).args(args);

        command
    }

    pub(crate) fn output<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.run(args);
        let output = command
            .output()
            .with_context(|| {
                anyhow!(
                    "`{} {}` cannot be executed",
                    command.get_program().to_string_lossy(),
                    command.get_args().map(OsStr::to_string_lossy).join(" ")
                )
            })?
            .stdout;

        Ok(String::from_utf8_lossy(&output).to_string())
    }
}
