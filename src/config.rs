use anyhow::{anyhow, Context, Result};
use clap::Args;
use figment::{
    providers::{Env, Serialized},
    Figment,
};
use itertools::iproduct;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_with::{
    formats::{CommaSeparator, Separator},
    serde_as, skip_serializing_none, BoolFromInt, PickFirst, StringWithSeparator,
};
use std::{
    env, fs,
    io::{Error, ErrorKind},
    path::{Path, PathBuf},
};

static COMPOSE_FILE_NAMES: Lazy<Vec<String>> = Lazy::new(|| {
    iproduct!(["compose", "docker-compose"], ["yaml", "yml"])
        .map(|name| format!("{}.{}", name.0, name.1))
        .collect()
});

struct PathSeparator;

impl Separator for PathSeparator {
    fn separator() -> &'static str {
        Box::leak(
            env::var("COMPOSE_PATH_SEPARATOR")
                .unwrap_or_else(|_| {
                    String::from(if cfg!(unix) {
                        ":"
                    } else if cfg!(windows) {
                        ";"
                    } else {
                        unreachable!()
                    })
                })
                .into_boxed_str(),
        )
    }
}

#[skip_serializing_none]
#[serde_as]
#[derive(Args, Serialize, Deserialize, Debug)]
pub(crate) struct Config {
    /// Project name
    #[arg(short, long)]
    pub(crate) project_name: Option<String>,

    /// Compose configuration files
    #[arg(short, long)]
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<PathSeparator, String>)>>")]
    pub(crate) file: Option<Vec<String>>,

    /// Specify a profile to enable
    #[arg(long)]
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<CommaSeparator, String>)>>")]
    #[serde(rename = "profiles")]
    pub(crate) profile: Option<Vec<String>>,

    /// Specify an alternate environment file
    #[arg(long)]
    pub(crate) env_file: Option<String>,

    /// Specify an alternate working directory
    #[arg(long)]
    pub(crate) project_directory: Option<String>,

    #[arg(skip)]
    #[serde_as(as = "Option<PickFirst<(_, BoolFromInt)>>")]
    pub(crate) convert_windows_paths: Option<bool>,

    #[arg(skip)]
    pub(crate) path_separator: Option<String>,

    #[arg(skip)]
    #[serde_as(as = "Option<PickFirst<(_, BoolFromInt)>>")]
    pub(crate) ignore_orphans: Option<bool>,
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

fn resolve(config: &Config) -> Result<Config> {
    let mut config = Figment::new()
        .merge(Env::prefixed("COMPOSE_").ignore(&["env_file", "project_directory"]))
        .merge(Serialized::defaults(config))
        .extract::<Config>()?;

    if config.file.is_none() {
        let file = find(
            &config
                .project_directory
                .as_ref()
                .map_or(env::current_dir()?, PathBuf::from),
            &COMPOSE_FILE_NAMES,
        )?;

        let override_file = file.with_extension(format!(
            "override.{}",
            file.extension().unwrap().to_string_lossy()
        ));

        config.file = Some(
            if override_file.is_file() {
                vec![&file, &override_file]
            } else {
                vec![&file]
            }
            .into_iter()
            .map(|file| file.to_string_lossy().to_string())
            .collect(),
        );
    }

    if let Some(files) = &mut config.file {
        for file in files {
            *file = fs::canonicalize(&file)
                .with_context(|| anyhow!("{file} not found"))?
                .to_string_lossy()
                .to_string();
        }
    }

    if let Some(file) = config.file.as_ref().and_then(|files| files.first()) {
        let project_directory = config.project_directory.get_or_insert_with(|| {
            Path::new(file)
                .parent()
                .map(|parent| parent.to_string_lossy().to_string())
                .unwrap_or_default()
        });

        *project_directory = fs::canonicalize(&project_directory)
            .with_context(|| anyhow!("{project_directory} not found"))?
            .to_string_lossy()
            .to_string();
    }

    Ok(config)
}

pub(crate) fn load(config: &Config) -> Result<Config> {
    let resolved_config = resolve(config)?;
    let env_file = config.env_file.as_ref().map_or_else(
        || PathBuf::from(resolved_config.project_directory.unwrap()).join(".env"),
        PathBuf::from,
    );

    dotenvy::from_filename(env_file).ok();
    resolve(config)
}
