use anyhow::{Context, Result};
use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_with::{
    formats::SpaceSeparator, serde_as, skip_serializing_none, OneOrMany, PickFirst,
    StringWithSeparator, TryFromInto,
};
use std::fs;

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Default, Debug)]
pub(crate) struct Compose {
    pub(crate) version: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) services: IndexMap<String, Service>,
    pub(crate) volumes: Option<IndexMap<String, Option<Volume>>>,
}

impl Compose {
    pub(crate) fn new() -> Self {
        Default::default()
    }
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub(crate) struct Service {
    pub(crate) cap_add: Option<Vec<String>>,
    pub(crate) cap_drop: Option<Vec<String>>,
    pub(crate) cgroup_parent: Option<String>,
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<SpaceSeparator, String>)>>")]
    pub(crate) command: Option<Vec<String>>,
    pub(crate) container_name: Option<String>,
    pub(crate) device_cgroup_rules: Option<String>,
    pub(crate) devices: Option<String>,
    #[serde_as(as = "Option<OneOrMany<_>>")]
    pub(crate) dns: Option<Vec<String>>,
    pub(crate) dns_opt: Option<Vec<String>>,
    #[serde_as(as = "Option<OneOrMany<_>>")]
    pub(crate) dns_search: Option<Vec<String>>,
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<SpaceSeparator, String>)>>")]
    pub(crate) entrypoint: Option<Vec<String>>,
    #[serde_as(as = "Option<OneOrMany<_>>")]
    pub(crate) env_file: Option<Vec<String>>,
    pub(crate) expose: Option<Vec<String>>,
    pub(crate) extra_hosts: Option<Vec<String>>,
    pub(crate) group_add: Option<Vec<String>>,
    pub(crate) hostname: Option<String>,
    pub(crate) image: String,
    pub(crate) init: Option<bool>,
    pub(crate) mac_address: Option<String>,
    pub(crate) network_mode: Option<String>,
    pub(crate) platform: Option<String>,
    #[serde_as(as = "Option<Vec<PickFirst<(_, TryFromInto<String>)>>>")]
    pub(crate) ports: Option<Vec<Port>>,
    pub(crate) privileged: Option<bool>,
    pub(crate) profiles: Option<Vec<String>>,
    pub(crate) pull_policy: Option<String>,
    pub(crate) read_only: Option<bool>,
    pub(crate) restart: Option<String>,
    pub(crate) user: Option<String>,
    pub(crate) userns_mode: Option<String>,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub(crate) struct Port {
    pub(crate) target: u32,
    pub(crate) published: Option<String>,
    pub(crate) host_ip: Option<String>,
    pub(crate) protocol: Option<String>,
}

impl TryFrom<String> for Port {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let parts = value.split(':').rev().collect::<Vec<_>>();

        Ok(Port {
            target: parts[0].parse().unwrap(),
            published: parts.get(1).map(|port| port.to_owned().to_owned()),
            host_ip: parts.get(2).map(|port| port.to_owned().to_owned()),
            protocol: None,
        })
    }
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub(crate) struct Volume {
    pub(crate) driver: Option<String>,
    pub(crate) driver_opts: Option<IndexMap<String, String>>,
    pub(crate) external: Option<bool>,
    pub(crate) name: Option<String>,
}

pub(crate) fn parse(paths: Option<Vec<String>>) -> Result<Compose> {
    let contents = match paths {
        Some(paths) => paths
            .into_iter()
            .map(|path| {
                fs::read_to_string(&path)
                    .with_context(|| format!("{path} not found"))
                    .map(|content| (path, content))
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => vec![(
            "compose.yaml".to_owned(),
            fs::read_to_string("compose.yaml")
                .or_else(|_| fs::read_to_string("compose.yml"))
                .or_else(|_| fs::read_to_string("docker-compose.yaml"))
                .or_else(|_| {
                    fs::read_to_string("docker-compose.yml").context("compose.yaml not found")
                })?,
        )],
    };
    let files = contents
        .into_iter()
        .map(|(path, content)| {
            let mut unused = IndexSet::new();

            serde_ignored::deserialize(serde_yaml::Deserializer::from_str(&content), |path| {
                unused.insert(path.to_string());
            })
            .with_context(|| format!("{path} does not follow the Compose specification"))
            .map(|file: Compose| (path, file, unused))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut combined_file = Compose::new();

    for (path, file, unused) in files {
        if !unused.is_empty() {
            eprintln!(
                "Warning: Unsupported/unknown attributes in {path}: {}",
                unused.into_iter().join(", ")
            );
        }

        combined_file.version = file.version;
        combined_file.name = file.name;
        combined_file.services.extend(file.services);

        match (&mut combined_file.volumes, file.volumes) {
            (Some(combined_volumes), Some(volumes)) => combined_volumes.extend(volumes),
            (combined_volumes, volumes) if combined_volumes.is_none() && volumes.is_some() => {
                *combined_volumes = volumes;
            }
            _ => {}
        }
    }

    Ok(combined_file)
}
