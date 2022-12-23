use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, bail, Context, Result};
use itertools::Itertools;
use once_cell::sync::Lazy;
use semver::Version;
use serde_json::Value;

static PODMAN_MIN_SUPPORTED_VERSION: Lazy<Version> = Lazy::new(|| Version::new(4, 3, 0));

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
        let value = serde_json::from_str::<Value>(&data)?;
        let version_str = value["Client"]["Version"]
            .as_str()
            .ok_or_else(|| anyhow!("Podman version not found"))?;
        let version = Version::parse(version_str)
            .with_context(|| anyhow!("Podman version \"{version_str}\" not recognised"))?;

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
}
