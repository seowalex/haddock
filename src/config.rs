use anyhow::{anyhow, Context, Result};
use figment::{
    providers::{Env, Serialized},
    Figment,
};
use itertools::iproduct;
use once_cell::sync::Lazy;
use std::{
    env, fs,
    io::{Error, ErrorKind},
    path::{Path, PathBuf},
};

use crate::Flags;

static COMPOSE_FILE_NAMES: Lazy<Vec<String>> = Lazy::new(|| {
    iproduct!(["compose", "docker-compose"], ["yaml", "yml"])
        .map(|name| format!("{}.{}", name.0, name.1))
        .collect()
});

#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) project_name: Option<String>,
    pub(crate) files: Vec<String>,
    pub(crate) profiles: Vec<String>,
    pub(crate) project_directory: String,
    pub(crate) convert_windows_paths: bool,
    pub(crate) ignore_orphans: bool,
}

fn find(directory: &Path, files: &Vec<String>) -> Result<PathBuf> {
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
        Err(Error::new(
            ErrorKind::NotFound,
            "Compose file not found in the working directory or its parent directories",
        ))?
    }
}

fn resolve(flags: &Flags) -> Result<Config> {
    let flags = Figment::new()
        .merge(Env::prefixed("COMPOSE_").ignore(&["env_file", "project_directory"]))
        .merge(Serialized::defaults(flags))
        .extract::<Flags>()?;

    let files = if let Some(files) = flags.file {
        files
            .iter()
            .map(|file| {
                fs::canonicalize(file)
                    .with_context(|| anyhow!("{file} not found"))
                    .map(|file| file.to_string_lossy().to_string())
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let file = find(
            &flags
                .project_directory
                .as_ref()
                .map_or(env::current_dir()?, PathBuf::from),
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
            file.canonicalize()
                .with_context(|| anyhow!("{} not found", file.to_string_lossy()))
                .map(|file| file.to_string_lossy().to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
    };

    let project_directory = if let Some(dir) = &flags.project_directory {
        fs::canonicalize(dir)
            .with_context(|| anyhow!("{dir} not found"))?
            .to_string_lossy()
            .to_string()
    } else {
        let parent = Path::new(&files[0])
            .parent()
            .unwrap_or_else(|| Path::new("/"));

        parent
            .canonicalize()
            .with_context(|| anyhow!("{} not found", parent.to_string_lossy()))?
            .to_string_lossy()
            .to_string()
    };

    Ok(Config {
        project_name: flags.project_name,
        files,
        profiles: flags.profile.unwrap_or_default(),
        project_directory,
        convert_windows_paths: flags.convert_windows_paths.unwrap_or_default(),
        ignore_orphans: flags.ignore_orphans.unwrap_or_default(),
    })
}

pub(crate) fn load(flags: Flags) -> Result<Config> {
    let config = resolve(&flags)?;
    let env_file = flags.env_file.as_ref().map_or_else(
        || PathBuf::from(config.project_directory).join(".env"),
        PathBuf::from,
    );

    dotenvy::from_filename(env_file).ok();
    resolve(&flags)
}
