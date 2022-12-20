use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use figment::{
    providers::{Env, Serialized},
    Figment,
};
use itertools::iproduct;
use once_cell::sync::Lazy;
use path_absolutize::Absolutize;

use crate::Flags;

static COMPOSE_FILE_NAMES: Lazy<Vec<String>> = Lazy::new(|| {
    iproduct!(["compose", "docker-compose"], ["yaml", "yml"])
        .map(|name| format!("{}.{}", name.0, name.1))
        .collect()
});

#[derive(Default, Debug)]
pub(crate) struct Config {
    pub(crate) project_name: Option<String>,
    pub(crate) files: Vec<PathBuf>,
    pub(crate) profiles: Vec<String>,
    pub(crate) project_directory: PathBuf,
    pub(crate) ignore_orphans: bool,
}

fn find(directory: &Path, files: &[String]) -> Result<PathBuf> {
    let paths = files
        .iter()
        .map(|file| directory.join(file))
        .collect::<Vec<_>>();

    for path in paths {
        if path.is_file() {
            return Ok(path);
        }
    }

    if let Some(parent) = directory.parent() {
        find(parent, files)
    } else {
        bail!("Compose file not found in the working directory or its parent directories");
    }
}

fn resolve(flags: &Flags) -> Result<Config> {
    let current_dir = env::current_dir()?;
    let flags = Figment::new()
        .merge(Env::prefixed("COMPOSE_").ignore(&["env_file", "project_directory"]))
        .merge(Serialized::defaults(flags))
        .extract::<Flags>()?;

    let files = if let Some(files) = flags.file {
        files
            .into_iter()
            .map(|file| {
                if file.as_os_str() == "-" {
                    Ok(file)
                } else {
                    file.absolutize_from(&current_dir)
                        .map(|file| file.to_path_buf())
                }
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let file = find(
            flags.project_directory.as_ref().unwrap_or(&current_dir),
            &COMPOSE_FILE_NAMES,
        )?;

        let override_file = file.with_extension(format!(
            "override.{}",
            file.extension().unwrap().to_string_lossy()
        ));

        if override_file.is_file() {
            vec![&file, &override_file]
        } else {
            vec![&file]
        }
        .into_iter()
        .map(|file| {
            file.absolutize_from(&current_dir)
                .map(|file| file.to_path_buf())
        })
        .collect::<Result<Vec<_>, _>>()?
    };

    let project_directory = if let Some(dir) = flags.project_directory {
        dir.absolutize_from(&current_dir)?.to_path_buf()
    } else {
        files[0]
            .parent()
            .unwrap_or_else(|| Path::new("/"))
            .absolutize_from(&current_dir)?
            .to_path_buf()
    };

    Ok(Config {
        project_name: flags.project_name,
        files,
        profiles: flags.profile.unwrap_or_default(),
        project_directory,
        ignore_orphans: flags.ignore_orphans.unwrap_or_default(),
    })
}

pub(crate) fn load(flags: Flags) -> Result<Config> {
    let config = resolve(&flags)?;
    let env_file = flags
        .env_file
        .clone()
        .unwrap_or_else(|| config.project_directory.join(".env"));

    dotenvy::from_path(&env_file)
        .with_context(|| anyhow!("{} not found", env_file.display()))
        .or_else(|err| {
            if flags.env_file.is_some() {
                Err(err)
            } else {
                Ok(())
            }
        })?;
    resolve(&flags)
}
