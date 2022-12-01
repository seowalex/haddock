use anyhow::{anyhow, bail, Context, Result};
use byte_unit::Byte;
use humantime::{format_duration, parse_duration};
use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_with::{
    formats::SpaceSeparator, serde_as, serde_conv, skip_serializing_none, DisplayFromStr,
    DurationMicroSeconds, OneOrMany, PickFirst, StringWithSeparator,
};
use std::{convert::Infallible, fs, time::Duration};

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Default, Debug)]
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

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Service {
    pub(crate) blkio_config: Option<BlkioConfig>,
    pub(crate) cap_add: Option<Vec<String>>,
    pub(crate) cap_drop: Option<Vec<String>>,
    pub(crate) cgroup_parent: Option<String>,
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<SpaceSeparator, String>)>>")]
    pub(crate) command: Option<Vec<String>>,
    #[serde_as(as = "Option<Vec<PickFirst<(_, FileReferenceOrString)>>>")]
    pub(crate) configs: Option<Vec<FileReference>>,
    pub(crate) container_name: Option<String>,
    #[serde_as(as = "Option<PickFirst<(DurationMicroSeconds, DurationWithSuffix)>>")]
    pub(crate) cpu_period: Option<Duration>,
    #[serde_as(as = "Option<PickFirst<(DurationMicroSeconds, DurationWithSuffix)>>")]
    pub(crate) cpu_quota: Option<Duration>,
    #[serde_as(as = "Option<PickFirst<(DurationMicroSeconds, DurationWithSuffix)>>")]
    pub(crate) cpu_rt_period: Option<Duration>,
    #[serde_as(as = "Option<PickFirst<(DurationMicroSeconds, DurationWithSuffix)>>")]
    pub(crate) cpu_rt_runtime: Option<Duration>,
    pub(crate) cpu_shares: Option<i64>,
    pub(crate) cpus: Option<f32>,
    pub(crate) cpuset: Option<String>,
    #[serde_as(as = "Option<PickFirst<(_, DependsOnVec)>>")]
    pub(crate) depends_on: Option<IndexMap<String, Dependency>>,
    pub(crate) device_cgroup_rules: Option<Vec<String>>,
    pub(crate) devices: Option<Vec<String>>,
    #[serde_as(as = "Option<OneOrMany<_>>")]
    pub(crate) dns: Option<Vec<String>>,
    pub(crate) dns_opt: Option<Vec<String>>,
    #[serde_as(as = "Option<OneOrMany<_>>")]
    pub(crate) dns_search: Option<Vec<String>>,
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<SpaceSeparator, String>)>>")]
    pub(crate) entrypoint: Option<Vec<String>>,
    #[serde_as(as = "Option<OneOrMany<_>>")]
    pub(crate) env_file: Option<Vec<String>>,
    #[serde_as(as = "Option<PickFirst<(_, EnvironmentVec)>>")]
    pub(crate) environment: Option<IndexMap<String, Option<String>>>,
    pub(crate) expose: Option<Vec<String>>,
    pub(crate) extends: Option<Extends>,
    pub(crate) external_links: Option<Vec<String>>,
    pub(crate) extra_hosts: Option<Vec<String>>,
    pub(crate) group_add: Option<Vec<String>>,
    pub(crate) healthcheck: Option<Healthcheck>,
    pub(crate) hostname: Option<String>,
    pub(crate) image: Option<String>,
    pub(crate) init: Option<bool>,
    pub(crate) ipc: Option<String>,
    #[serde_as(as = "Option<PickFirst<(_, LabelsVec)>>")]
    pub(crate) labels: Option<IndexMap<String, String>>,
    pub(crate) links: Option<Vec<String>>,
    pub(crate) logging: Option<Logging>,
    pub(crate) mac_address: Option<String>,
    pub(crate) mem_limit: Option<Byte>,
    pub(crate) mem_reservation: Option<Byte>,
    pub(crate) mem_swappiness: Option<i64>,
    pub(crate) memswap_limit: Option<SwapLimit>,
    #[serde_as(as = "Option<PickFirst<(_, NetworksVec)>>")]
    pub(crate) networks: Option<IndexMap<String, Option<ServiceNetwork>>>,
    pub(crate) network_mode: Option<String>,
    pub(crate) oom_kill_disable: Option<bool>,
    pub(crate) oom_score_adj: Option<i64>,
    pub(crate) pid: Option<String>,
    pub(crate) pids_limit: Option<i64>,
    pub(crate) platform: Option<String>,
    #[serde_as(as = "Option<Vec<PickFirst<(_, PortOrString, PortOrU32)>>>")]
    pub(crate) ports: Option<Vec<Port>>,
    pub(crate) privileged: Option<bool>,
    pub(crate) profiles: Option<Vec<String>>,
    pub(crate) pull_policy: Option<PullPolicy>,
    pub(crate) read_only: Option<bool>,
    pub(crate) restart: Option<RestartPolicy>,
    pub(crate) runtime: Option<String>,
    #[serde_as(as = "Option<Vec<PickFirst<(_, FileReferenceOrString)>>>")]
    pub(crate) secrets: Option<Vec<FileReference>>,
    pub(crate) security_opt: Option<Vec<String>>,
    pub(crate) shm_size: Option<Byte>,
    #[serde_as(as = "Option<DurationWithSuffix>")]
    pub(crate) stop_grace_period: Option<Duration>,
    pub(crate) stop_signal: Option<String>,
    pub(crate) storage_opt: Option<IndexMap<String, String>>,
    #[serde_as(as = "Option<PickFirst<(_, SysctlsVec)>>")]
    pub(crate) sysctls: Option<IndexMap<String, String>>,
    #[serde_as(as = "Option<OneOrMany<_>>")]
    pub(crate) tmpfs: Option<Vec<String>>,
    pub(crate) tty: Option<bool>,
    pub(crate) ulimits: Option<IndexMap<String, ResourceLimit>>,
    pub(crate) user: Option<String>,
    pub(crate) userns_mode: Option<String>,
    #[serde_as(as = "Option<Vec<PickFirst<(_, ServiceVolumeOrString)>>>")]
    pub(crate) volumes: Option<Vec<ServiceVolume>>,
    pub(crate) volumes_from: Option<Vec<String>>,
    pub(crate) working_dir: Option<String>,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct BlkioConfig {
    pub(crate) weight: Option<u16>,
    pub(crate) weight_device: Option<Vec<WeightDevice>>,
    pub(crate) device_read_bps: Option<Vec<ThrottleDevice>>,
    pub(crate) device_write_bps: Option<Vec<ThrottleDevice>>,
    pub(crate) device_read_iops: Option<Vec<ThrottleDevice>>,
    pub(crate) device_write_iops: Option<Vec<ThrottleDevice>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct WeightDevice {
    pub(crate) path: String,
    pub(crate) weight: u16,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ThrottleDevice {
    pub(crate) path: String,
    pub(crate) rate: Byte,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Default, Debug)]
pub(crate) struct FileReference {
    pub(crate) source: String,
    pub(crate) target: Option<String>,
    pub(crate) uid: Option<String>,
    pub(crate) gid: Option<String>,
    #[serde_as(as = "Option<PickFirst<(_, DisplayFromStr)>>")]
    pub(crate) mode: Option<u32>,
}

serde_conv!(
    FileReferenceOrString,
    FileReference,
    |file_reference: &FileReference| { file_reference.source.to_owned() },
    |source| -> std::result::Result<_, Infallible> {
        Ok(FileReference {
            source,
            ..Default::default()
        })
    }
);

serde_conv!(
    DurationWithSuffix,
    Duration,
    |duration: &Duration| format_duration(*duration).to_string(),
    |duration: String| parse_duration(&duration)
);

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Dependency {
    pub(crate) condition: Condition,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum Condition {
    #[serde(rename = "service_started")]
    Started,
    #[serde(rename = "service_healthy")]
    Healthy,
    #[serde(rename = "service_completed_successfully")]
    CompletedSuccessfully,
}

serde_conv!(
    DependsOnVec,
    IndexMap<String, Dependency>,
    |dependencies: &IndexMap<String, Dependency>| dependencies.keys().cloned().collect::<Vec<_>>(),
    |dependencies: Vec<String>| -> std::result::Result<_, Infallible> {
        Ok(IndexMap::from_iter(dependencies.into_iter().map(
            |dependency| {
                (
                    dependency,
                    Dependency {
                        condition: Condition::Started,
                    },
                )
            },
        )))
    }
);

serde_conv!(
    EnvironmentVec,
    IndexMap<String, Option<String>>,
    |variables: &IndexMap<String, Option<String>>| {
        variables
            .iter()
            .map(|(key, value)| match value {
                Some(value) => format!("{key}={value}"),
                None => key.to_owned(),
            })
            .collect::<Vec<_>>()
    },
    |variables: Vec<String>| -> std::result::Result<_, Infallible> {
        Ok(IndexMap::from_iter(variables.into_iter().map(|variable| {
            let mut parts = variable.split('=');
            (
                parts.next().unwrap().to_owned(),
                parts.next().map(|part| part.to_owned()),
            )
        })))
    }
);

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Extends {
    pub(crate) service: String,
    pub(crate) file: Option<String>,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Healthcheck {
    #[serde_as(as = "Option<PickFirst<(_, StringWithSeparator::<SpaceSeparator, String>)>>")]
    pub(crate) test: Option<Vec<String>>,
    #[serde_as(as = "Option<DurationWithSuffix>")]
    pub(crate) interval: Option<Duration>,
    #[serde_as(as = "Option<DurationWithSuffix>")]
    pub(crate) timeout: Option<Duration>,
    #[serde_as(as = "Option<DurationWithSuffix>")]
    pub(crate) start_period: Option<Duration>,
    pub(crate) retries: Option<u64>,
    pub(crate) disable: Option<bool>,
}

serde_conv!(
    LabelsVec,
    IndexMap<String, String>,
    |variables: &IndexMap<String, String>| {
        variables
            .iter()
            .map(|(key, value)| {
                if value.is_empty() {
                    key.to_owned()
                } else {
                    format!("{key}={value}")
                }
            })
            .collect::<Vec<_>>()
    },
    |variables: Vec<String>| -> std::result::Result<_, Infallible> {
        Ok(IndexMap::from_iter(variables.into_iter().map(|variable| {
            let mut parts = variable.split('=');
            (
                parts.next().unwrap().to_owned(),
                parts.next().map(|part| part.to_owned()).unwrap_or_default(),
            )
        })))
    }
);

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Logging {
    pub(crate) driver: Option<String>,
    pub(crate) options: Option<IndexMap<String, String>>,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ServiceNetwork {
    pub(crate) aliases: Option<Vec<String>>,
    pub(crate) ipv4_address: Option<String>,
    pub(crate) ipv6_address: Option<String>,
    pub(crate) link_local_ips: Option<Vec<String>>,
    pub(crate) priority: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum SwapLimit {
    Limited(Byte),
    Unlimited(i8),
}

serde_conv!(
    NetworksVec,
    IndexMap<String, Option<ServiceNetwork>>,
    |networks: &IndexMap<String, Option<ServiceNetwork>>| networks.keys().cloned().collect::<Vec<_>>(),
    |networks: Vec<String>| -> std::result::Result<_, Infallible> {
        Ok(IndexMap::from_iter(networks.into_iter().map(|network| (network, None))))
    }
);

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Default, Debug)]
pub(crate) struct Port {
    #[serde_as(as = "PickFirst<(_, StringOrU16)>")]
    pub(crate) target: String,
    #[serde_as(as = "Option<PickFirst<(_, StringOrU16)>>")]
    pub(crate) published: Option<String>,
    pub(crate) host_ip: Option<String>,
    pub(crate) protocol: Option<String>,
}

serde_conv!(
    PortOrString,
    Port,
    |port: &Port| {
        let mut string = port.target.to_owned();

        match (&port.published, &port.host_ip) {
            (None, None) => {}
            (published, host_ip) => {
                string = format!("{}:{string}", published.to_owned().unwrap_or_default());

                if let Some(host_ip) = host_ip {
                    string = format!("{host_ip}:{string}");
                }
            }
        }

        if let Some(protocol) = &port.protocol {
            string = format!("{string}/{protocol}");
        }

        string
    },
    |port: String| -> std::result::Result<_, Infallible> {
        let mut parts = port.split(':').rev();
        let container_port = parts.next().unwrap();
        let mut container_parts = container_port.split('/');
        let target = container_parts.next().unwrap().to_owned();

        Ok(Port {
            target,
            published: parts.next().and_then(|part| {
                if part.is_empty() {
                    None
                } else {
                    Some(part.to_owned())
                }
            }),
            host_ip: parts.next().map(|part| part.to_owned()),
            protocol: container_parts.next().map(|part| part.to_owned()),
        })
    }
);

serde_conv!(
    PortOrU32,
    Port,
    |port: &Port| port.target.parse::<u32>().unwrap(),
    |target: u32| -> std::result::Result<_, Infallible> {
        Ok(Port {
            target: target.to_string(),
            ..Default::default()
        })
    }
);

serde_conv!(
    StringOrU16,
    String,
    |port: &String| port.parse::<u16>().unwrap(),
    |port: u16| -> std::result::Result<_, Infallible> { Ok(port.to_string()) }
);

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PullPolicy {
    Always,
    Never,
    Missing,
    Build,
    Newer,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RestartPolicy {
    No,
    Always,
    OnFailure,
    UnlessStopped,
}

serde_conv!(
    SysctlsVec,
    IndexMap<String, String>,
    |variables: &IndexMap<String, String>| {
        variables
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
    },
    |variables: Vec<String>| -> Result<_> {
        let variables = variables.into_iter().map(|variable| -> Result<_> {
            let mut parts = variable.split('=');
            let key = parts.next().unwrap().to_owned();
            let value = parts.next().map(|part| part.to_owned()).ok_or_else(|| anyhow!("value not defined for {key}"))?;

            Ok((key, value))
        }).collect::<Result<Vec<_>, _>>()?;

        Ok(IndexMap::from_iter(variables.into_iter()))
    }
);

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum ResourceLimit {
    Single(i32),
    Double { soft: i32, hard: i32 },
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ServiceVolume {
    pub(crate) r#type: ServiceVolumeType,
    pub(crate) source: Option<String>,
    pub(crate) target: String,
    pub(crate) read_only: Option<bool>,
    pub(crate) bind: Option<ServiceVolumeBind>,
    pub(crate) volume: Option<ServiceVolumeVolume>,
    pub(crate) tmpfs: Option<ServiceVolumeTmpfs>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServiceVolumeType {
    Volume,
    Bind,
    Tmpfs,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Default, Debug)]
pub(crate) struct ServiceVolumeBind {
    pub(crate) propagation: Option<String>,
    pub(crate) create_host_path: Option<bool>,
    pub(crate) selinux: Option<String>,
}

impl ServiceVolumeBind {
    pub(crate) fn new() -> Self {
        Default::default()
    }
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ServiceVolumeVolume {
    pub(crate) nocopy: Option<bool>,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ServiceVolumeTmpfs {
    pub(crate) size: Option<Byte>,
    #[serde_as(as = "Option<PickFirst<(_, DisplayFromStr)>>")]
    pub(crate) mode: Option<u32>,
}

serde_conv!(
    ServiceVolumeOrString,
    ServiceVolume,
    |_| {},
    |mount: String| -> Result<_> {
        let mut r#type = ServiceVolumeType::Volume;
        let mut source = None;
        let target;
        let mut read_only = None;
        let mut bind = None;
        let mut volume = None;
        let mut options = "";
        let parts = mount.split(':').collect::<Vec<_>>();

        match parts[..] {
            [dst] => {
                target = dst.to_owned();
            }
            [src, dst] if dst.starts_with('/') => {
                if src.starts_with('/') || src.starts_with('.') {
                    r#type = ServiceVolumeType::Bind;
                }

                source = Some(src.to_owned());
                target = dst.to_owned();
            }
            [dst, opts] => {
                target = dst.to_owned();
                options = opts;
            }
            [src, dst, opts] => {
                if src.starts_with('/') || src.starts_with('.') {
                    r#type = ServiceVolumeType::Bind;
                }

                source = Some(src.to_owned());
                target = dst.to_owned();
                options = opts;
            }
            _ => {
                bail!("too many colons in {mount}");
            }
        }

        let options = options.split(',');
        let mut unused = vec![];

        for option in options {
            match option {
                "rw" | "ro" => {
                    read_only = Some(option == "ro");
                }
                "shared" | "rshared" | "slave" | "rslave" | "private" | "rprivate"
                | "unbindable" | "runbindable" => {
                    bind.get_or_insert(ServiceVolumeBind::new()).propagation =
                        Some(option.to_owned());
                }
                "z" | "Z" => {
                    bind.get_or_insert(ServiceVolumeBind::new()).selinux = Some(option.to_owned());
                }
                "copy" | "nocopy" => {
                    volume = Some(ServiceVolumeVolume {
                        nocopy: Some(option == "nocopy"),
                    })
                }
                "" => {}
                _ => {
                    unused.push(option);
                }
            }
        }

        if !unused.is_empty() {
            eprintln!(
                "Warning: Unsupported/unknown mount options: {}",
                unused.into_iter().join(", ")
            );
        }

        Ok(ServiceVolume {
            r#type,
            source,
            target,
            read_only,
            bind,
            volume,
            tmpfs: None,
        })
    }
);

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Volume {
    pub(crate) driver: Option<String>,
    pub(crate) driver_opts: Option<IndexMap<String, String>>,
    pub(crate) external: Option<bool>,
    #[serde_as(as = "Option<PickFirst<(_, LabelsVec)>>")]
    pub(crate) labels: Option<IndexMap<String, String>>,
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

#[cfg(test)]
mod tests {
    use glob::glob;
    use std::fs;

    use super::Compose;

    #[test]
    fn serde_compose() {
        let mut all_succeeded = true;

        for entry in glob("tests/fixtures/**/*.y*ml")
            .expect("Failed to read glob pattern")
            .filter_map(Result::ok)
        {
            let contents = fs::read_to_string(&entry).unwrap();

            match serde_yaml::from_str::<Compose>(&contents) {
                Ok(_) => {}
                Err(e) => {
                    all_succeeded = false;
                    eprintln!("{}: {:?}", entry.display(), e);
                }
            }
        }

        assert!(all_succeeded);
    }
}
